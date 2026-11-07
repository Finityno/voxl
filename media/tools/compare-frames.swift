// Memory-only CPU/Metal NV12 comparison experiment. No capture, UI or input.
import Foundation
import CoreVideo
import Metal
import Darwin

struct Failure: Error { let message: String }
func require(_ condition: Bool, _ message: String) throws { if !condition { throw Failure(message: message) } }
let byteLimit = 64 * 1024 * 1024

struct Signature: Equatable {
    let width: Int
    let height: Int
    let format: OSType
    let epoch: UInt64
    var bytes: Int { width * height * 3 / 2 }
}

func signature(_ image: CVPixelBuffer, epoch: UInt64, limit: Int) throws -> Signature {
    let width = CVPixelBufferGetWidth(image), height = CVPixelBufferGetHeight(image)
    let format = CVPixelBufferGetPixelFormatType(image)
    try require(width >= 2 && height >= 2 && width <= 8192 && height <= 8192 && width % 2 == 0 && height % 2 == 0, "unsupported geometry")
    try require(format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange || format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, "unsupported pixel format")
    let value = Signature(width: width, height: height, format: format, epoch: epoch)
    try require(value.bytes <= limit && limit > 0 && limit <= byteLimit, "snapshot byte bound")
    try require(CVPixelBufferGetPlaneCount(image) == 2 && CVPixelBufferGetWidthOfPlane(image, 0) == width && CVPixelBufferGetHeightOfPlane(image, 0) == height && CVPixelBufferGetWidthOfPlane(image, 1) == width / 2 && CVPixelBufferGetHeightOfPlane(image, 1) == height / 2, "unexpected NV12 planes")
    return value
}

struct Plane {
    let base: UnsafeRawPointer
    let stride: Int
    let rows: Int
    let width: Int
}
struct Planes: RandomAccessCollection {
    typealias Index = Int
    let y: Plane
    let uv: Plane
    var startIndex: Int { 0 }
    var endIndex: Int { 2 }
    subscript(index: Int) -> Plane { precondition(index == 0 || index == 1); return index == 0 ? y : uv }
}

func readPlanes<T>(_ image: CVPixelBuffer, _ signature: Signature, _ body: (Planes) throws -> T) throws -> T {
    try require(CVPixelBufferLockBaseAddress(image, .readOnly) == kCVReturnSuccess, "CPU read lock")
    let result: Result<T, Error> = Result {
        func plane(_ plane: Int) throws -> Plane {
            let rows = plane == 0 ? signature.height : signature.height / 2
            let stride = CVPixelBufferGetBytesPerRowOfPlane(image, plane)
            guard let base = CVPixelBufferGetBaseAddressOfPlane(image, plane) else { throw Failure(message: "missing plane address") }
            try require(stride >= signature.width, "short plane stride")
            return Plane(base: UnsafeRawPointer(base), stride: stride, rows: rows, width: signature.width)
        }
        return try body(Planes(y: plane(0), uv: plane(1)))
    }
    let status = CVPixelBufferUnlockBaseAddress(image, .readOnly)
    try require(status == kCVReturnSuccess, "CPU read unlock")
    return try result.get()
}

final class Bytes {
    let pointer: UnsafeMutableRawPointer
    let count: Int
    init(_ count: Int) throws {
        try require(count > 0 && count <= byteLimit, "owned byte bound")
        guard let pointer = malloc(count) else { throw Failure(message: "owned byte allocation") }
        self.pointer = pointer; self.count = count
    }
    deinit { free(pointer) }
}

struct Observation { let changed: Bool; let gpuMicros: Double? }
protocol Detector: AnyObject {
    var snapshotBytes: Int { get }
    var allocatedSnapshotBytes: Int { get }
    func observe(_ image: CVPixelBuffer, epoch: UInt64) throws -> Observation
    func reset()
}

final class CPUDetector: Detector {
    enum State { case empty; case snapshot(Signature, Bytes) }
    private var state: State = .empty
    let limit: Int
    init(limit: Int = byteLimit) { self.limit = limit }
    var snapshotBytes: Int { if case .snapshot(_, let bytes) = state { return bytes.count }; return 0 }
    var allocatedSnapshotBytes: Int { if case .snapshot(_, let bytes) = state { return malloc_size(bytes.pointer) }; return 0 }
    func reset() { state = .empty }
    func observe(_ image: CVPixelBuffer, epoch: UInt64) throws -> Observation {
        do {
            let current = try signature(image, epoch: epoch, limit: limit)
            let bytes: Bytes, initial: Bool
            switch state {
            case .snapshot(let previous, let owned) where previous == current: bytes = owned; initial = false
            default: bytes = try Bytes(current.bytes); initial = true
            }
            let changed = try readPlanes(image, current) { planes in
                var different = initial
                if !different {
                    var offset = 0
                    for plane in planes {
                        for row in 0..<plane.rows {
                            if memcmp(bytes.pointer.advanced(by: offset + row * plane.width), plane.base.advanced(by: row * plane.stride), plane.width) != 0 { different = true; break }
                        }
                        offset += plane.rows * plane.width
                        if different { break }
                    }
                }
                if different {
                    var offset = 0
                    for plane in planes {
                        for row in 0..<plane.rows { memcpy(bytes.pointer.advanced(by: offset + row * plane.width), plane.base.advanced(by: row * plane.stride), plane.width) }
                        offset += plane.rows * plane.width
                    }
                }
                return different
            }
            state = .snapshot(current, bytes)
            return Observation(changed: changed, gpuMicros: nil)
        } catch { state = .empty; throw error }
    }
}

let metalSource = #"""
#include <metal_stdlib>
using namespace metal;
struct Parameters { uint width; uint height; uint uv_width; uint uv_height; uint has_reference; };
kernel void compare_nv12(texture2d<float, access::read> y [[texture(0)]],
                         texture2d<float, access::read> uv [[texture(1)]],
                         device uchar *previous [[buffer(0)]],
                         device atomic_uint *changed [[buffer(1)]],
                         constant Parameters &p [[buffer(2)]],
                         uint index [[thread_position_in_grid]],
                         uint lane [[thread_index_in_threadgroup]],
                         uint simd_lane [[thread_index_in_simdgroup]]) {
    threadgroup atomic_uint local_changed;
    if (lane == 0) atomic_store_explicit(&local_changed, 0u, memory_order_relaxed);
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const uint y_count = p.width * p.height;
    const uint uv_count = p.uv_width * p.uv_height;
    bool different = false;
    if (index < y_count) {
        const uint2 point(index % p.width, index / p.width);
        const uchar value = uchar(round(y.read(point).r * 255.0f));
        different = p.has_reference == 0 || previous[index] != value;
        if (different) previous[index] = value;
    } else if (index - y_count < uv_count) {
        const uint pixel = index - y_count;
        const uint2 point(pixel % p.uv_width, pixel / p.uv_width);
        const float2 sample = uv.read(point).rg;
        const uchar2 value = uchar2(round(sample * 255.0f));
        const uint base = y_count + 2 * pixel;
        different = p.has_reference == 0 || previous[base] != value.r || previous[base + 1] != value.g;
        if (different) { previous[base] = value.r; previous[base + 1] = value.g; }
    }
    const bool simd_changed = simd_any(different);
    if (simd_lane == 0 && simd_changed) atomic_store_explicit(&local_changed, 1u, memory_order_relaxed);
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (lane == 0 && atomic_load_explicit(&local_changed, memory_order_relaxed) != 0)
        atomic_store_explicit(changed, 1u, memory_order_relaxed);
}
"""#

final class GPUContext {
    let device: MTLDevice
    let queue: MTLCommandQueue
    let pipeline: MTLComputePipelineState
    let cache: CVMetalTextureCache
    let result: MTLBuffer
    let startupMicros: Double
    init() throws {
        let start = DispatchTime.now().uptimeNanoseconds
        guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else { throw Failure(message: "Metal device or queue unavailable") }
        let options = MTLCompileOptions(); options.languageVersion = .version2_4
        let library = try device.makeLibrary(source: metalSource, options: options)
        guard let function = library.makeFunction(name: "compare_nv12") else { throw Failure(message: "Metal function unavailable") }
        let pipeline = try device.makeComputePipelineState(function: function)
        var cache: CVMetalTextureCache?
        try require(CVMetalTextureCacheCreate(nil, nil, device, nil, &cache) == kCVReturnSuccess, "Metal texture cache")
        guard let cache else { throw Failure(message: "Metal texture cache missing") }
        guard let result = device.makeBuffer(length: MemoryLayout<UInt32>.size, options: .storageModeShared) else { throw Failure(message: "GPU result allocation") }
        self.device = device; self.queue = queue; self.pipeline = pipeline; self.cache = cache; self.result = result
        self.startupMicros = Double(DispatchTime.now().uptimeNanoseconds - start) / 1000
    }
    func wait(_ command: MTLCommandBuffer, image: CVPixelBuffer?, wrappers: [CVMetalTexture]) throws {
        let done = DispatchSemaphore(value: 0)
        command.addCompletedHandler { _ in withExtendedLifetime((image, wrappers)) { _ = done.signal() } }
        command.commit()
        try require(done.wait(timeout: .now() + .seconds(2)) == .success, "GPU completion deadline; experiment must stop")
        try require(command.status == .completed, "GPU command did not complete successfully")
    }
}

final class GPUDetector: Detector {
    enum State { case empty; case snapshot(Signature, MTLBuffer) }
    private var state: State = .empty
    let context: GPUContext
    let limit: Int
    init(_ context: GPUContext, limit: Int = byteLimit) { self.context = context; self.limit = limit }
    var snapshotBytes: Int { if case .snapshot(_, let bytes) = state { return bytes.length }; return 0 }
    var allocatedSnapshotBytes: Int { if case .snapshot(_, let bytes) = state { return bytes.allocatedSize }; return 0 }
    func reset() { state = .empty; CVMetalTextureCacheFlush(context.cache, 0) }
    func observe(_ image: CVPixelBuffer, epoch: UInt64) throws -> Observation {
        do {
            let current = try signature(image, epoch: epoch, limit: limit)
            let reference: MTLBuffer, initial: Bool
            switch state {
            case .snapshot(let previous, let owned) where previous == current: reference = owned; initial = false
            default:
                guard let owned = context.device.makeBuffer(length: current.bytes, options: .storageModePrivate) else { throw Failure(message: "GPU snapshot allocation") }
                reference = owned; initial = true
            }
            let observation: Observation = try autoreleasepool {
                var wrappers: [CVMetalTexture] = []
                var textures: [MTLTexture] = []
                for plane in 0..<2 {
                    var wrapper: CVMetalTexture?
                    let status = CVMetalTextureCacheCreateTextureFromImage(nil, context.cache, image, nil, plane == 0 ? .r8Unorm : .rg8Unorm, plane == 0 ? current.width : current.width / 2, plane == 0 ? current.height : current.height / 2, plane, &wrapper)
                    guard status == kCVReturnSuccess, let wrapper, let texture = CVMetalTextureGetTexture(wrapper) else { throw Failure(message: "GPU plane mapping") }
                    wrappers.append(wrapper); textures.append(texture)
                }
                guard let command = context.queue.makeCommandBuffer(), let encoder = command.makeComputeCommandEncoder() else { throw Failure(message: "GPU command allocation") }
                let result = context.result
                result.contents().storeBytes(of: UInt32(0), as: UInt32.self)
                var parameters: [UInt32] = [UInt32(current.width), UInt32(current.height), UInt32(current.width / 2), UInt32(current.height / 2), initial ? 0 : 1]
                encoder.setComputePipelineState(context.pipeline)
                encoder.setTexture(textures[0], index: 0); encoder.setTexture(textures[1], index: 1)
                encoder.setBuffer(reference, offset: 0, index: 0); encoder.setBuffer(result, offset: 0, index: 1)
                parameters.withUnsafeMutableBytes { encoder.setBytes($0.baseAddress!, length: $0.count, index: 2) }
                let threads = min(256, context.pipeline.maxTotalThreadsPerThreadgroup)
                try require(threads > 0, "GPU threadgroup bound")
                let pixels = current.width * current.height + current.width * current.height / 4
                encoder.dispatchThreadgroups(MTLSize(width: (pixels + threads - 1) / threads, height: 1, depth: 1), threadsPerThreadgroup: MTLSize(width: threads, height: 1, depth: 1))
                encoder.endEncoding()
                try context.wait(command, image: image, wrappers: wrappers)
                let value = result.contents().load(as: UInt32.self)
                try require(value <= 1, "invalid GPU comparison flag")
                let elapsed = command.gpuEndTime - command.gpuStartTime
                return Observation(changed: value == 1, gpuMicros: elapsed.isFinite && elapsed > 0 ? elapsed * 1_000_000 : nil)
            }
            CVMetalTextureCacheFlush(context.cache, 0)
            state = .snapshot(current, reference)
            return observation
        } catch { state = .empty; CVMetalTextureCacheFlush(context.cache, 0); throw error }
    }
    func verifySnapshot(_ image: CVPixelBuffer) throws {
        guard case .snapshot(let current, let reference) = state,
              let output = context.device.makeBuffer(length: current.bytes, options: .storageModeShared),
              let command = context.queue.makeCommandBuffer(), let encoder = command.makeBlitCommandEncoder() else { throw Failure(message: "GPU snapshot oracle unavailable") }
        encoder.copy(from: reference, sourceOffset: 0, to: output, destinationOffset: 0, size: current.bytes); encoder.endEncoding()
        try context.wait(command, image: image, wrappers: [])
        try readPlanes(image, current) { planes in
            var offset = 0
            for plane in planes {
                for row in 0..<plane.rows { try require(memcmp(output.contents().advanced(by: offset + row * plane.width), plane.base.advanced(by: row * plane.stride), plane.width) == 0, "GPU byte conversion oracle mismatch") }
                offset += plane.rows * plane.width
            }
        }
    }
}

enum Pattern: String, CaseIterable {
    case unchanged, padding, lumaFirst, lumaLast, chromaUFirst, chromaVLast, fullChange
    var changed: Bool { self != .unchanged && self != .padding }
}

func makeImage(_ width: Int, _ height: Int, pattern: Pattern = .unchanged, format: OSType = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange) throws -> CVPixelBuffer {
    var image: CVPixelBuffer?
    let attributes: [String: Any] = [kCVPixelBufferIOSurfacePropertiesKey as String: [:], kCVPixelBufferMetalCompatibilityKey as String: true, kCVPixelBufferBytesPerRowAlignmentKey as String: pattern == .padding ? 512 : 128]
    try require(CVPixelBufferCreate(nil, width, height, format, attributes as CFDictionary, &image) == kCVReturnSuccess, "owned CVPixelBuffer creation")
    guard let image else { throw Failure(message: "owned CVPixelBuffer missing") }
    if format == kCVPixelFormatType_32BGRA { return image }
    let current = try signature(image, epoch: 1, limit: byteLimit)
    try require(CVPixelBufferLockBaseAddress(image, []) == kCVReturnSuccess, "source write lock")
    let result: Result<Void, Error> = Result {
        for plane in 0..<2 {
            guard let raw = CVPixelBufferGetBaseAddressOfPlane(image, plane) else { throw Failure(message: "source plane address") }
            let base = raw.assumingMemoryBound(to: UInt8.self), rows = plane == 0 ? height : height / 2
            let stride = CVPixelBufferGetBytesPerRowOfPlane(image, plane)
            try require(stride >= width, "source stride")
            for row in 0..<rows {
                memset(base.advanced(by: row * stride), pattern == .padding ? 0xa5 : 0x5a, stride)
                for column in 0..<width {
                    let index = row * width + column
                    let value = plane == 0 ? index % 256 : (index / 2 + (index % 2 == 0 ? 0 : 73)) % 256
                    base[row * stride + column] = UInt8(value) ^ (pattern == .fullChange ? 0xff : 0)
                }
            }
            let target: Int?
            switch (plane, pattern) {
            case (0, .lumaFirst): target = 0
            case (0, .lumaLast): target = width * rows - 1
            case (1, .chromaUFirst): target = 0
            case (1, .chromaVLast): target = width * rows - 1
            default: target = nil
            }
            if let target { base[(target / width) * stride + target % width] ^= 1 }
        }
    }
    let unlocked = CVPixelBufferUnlockBaseAddress(image, [])
    try require(unlocked == kCVReturnSuccess, "source write unlock"); try result.get()
    try require(current.bytes <= byteLimit, "source active byte bound")
    return image
}

func cpuMicros() throws -> Double {
    var usage = rusage(); try require(getrusage(RUSAGE_SELF, &usage) == 0, "process CPU counter unavailable")
    return Double(usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) * 1_000_000 + Double(usage.ru_utime.tv_usec + usage.ru_stime.tv_usec)
}
func percentile(_ values: [Double], _ rank: Double) -> Double { let ordered = values.sorted(); return ordered[max(0, Int(ceil(Double(ordered.count) * rank)) - 1)] }
func distribution(_ values: [Double]) -> [String: Any] { ["samples": values.count, "mean": values.reduce(0, +) / Double(values.count), "p50": percentile(values, 0.5), "p95": percentile(values, 0.95), "maximum": values.max() ?? 0] }

func check(_ detector: Detector) throws {
    let first = try makeImage(66, 34)
    try require(try detector.observe(first, epoch: 1).changed, "initial observation must establish a snapshot")
    try require(!(try detector.observe(first, epoch: 1).changed), "identical observation changed")
    for pattern in Pattern.allCases {
        detector.reset(); _ = try detector.observe(first, epoch: 1)
        let candidate = try makeImage(66, 34, pattern: pattern)
        if pattern == .padding { try require((0..<2).allSatisfy { CVPixelBufferGetBytesPerRowOfPlane(first, $0) != CVPixelBufferGetBytesPerRowOfPlane(candidate, $0) }, "padding oracle needs different strides") }
        for _ in 0..<2 {
            try require(try detector.observe(candidate, epoch: 1).changed == pattern.changed, "candidate equality oracle")
            try require(try detector.observe(first, epoch: 1).changed == pattern.changed, "snapshot update oracle")
        }
    }
    try require(try detector.observe(first, epoch: 2).changed, "epoch must reset snapshot")
    let other = try makeImage(32, 16)
    try require(try detector.observe(other, epoch: 2).changed, "geometry must reset snapshot")
    let range = try makeImage(32, 16, format: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
    try require(try detector.observe(range, epoch: 2).changed, "range must reset snapshot")
    let invalid = try makeImage(32, 16, format: kCVPixelFormatType_32BGRA)
    do { _ = try detector.observe(invalid, epoch: 2); throw Failure(message: "unsupported source accepted") }
    catch let failure as Failure { try require(failure.message == "unsupported pixel format", "unexpected unsupported-source error") }
    try require(try detector.observe(first, epoch: 1).changed, "failed observation must clear the snapshot")
    detector.reset(); try require(detector.snapshotBytes == 0, "reset retained a snapshot")
}

func measure(_ detector: Detector, backend: String, run: Int, first: CVPixelBuffer, candidate: CVPixelBuffer, pattern: Pattern, warmup: Int, samples: Int) throws -> [String: Any] {
    detector.reset(); _ = try detector.observe(first, epoch: 1)
    for index in 0..<warmup { let frame = index % 2 == 0 ? candidate : first; try require(try autoreleasepool { try detector.observe(frame, epoch: 1).changed } == pattern.changed, "warmup oracle") }
    var wall: [Double] = [], gpu: [Double] = []
    wall.reserveCapacity(samples); gpu.reserveCapacity(samples)
    let before = try cpuMicros()
    for index in 0..<samples {
        let frame = index % 2 == 0 ? candidate : first, start = DispatchTime.now().uptimeNanoseconds
        let value = try autoreleasepool { try detector.observe(frame, epoch: 1) }
        wall.append(Double(DispatchTime.now().uptimeNanoseconds - start) / 1000)
        try require(value.changed == pattern.changed, "observed equality oracle")
        if let elapsed = value.gpuMicros { gpu.append(elapsed) }
    }
    let cpu = try cpuMicros() - before
    try require(cpu.isFinite && cpu >= 0, "process CPU counter regressed")
    let value = try signature(first, epoch: 1, limit: byteLimit)
    return ["backend": backend, "withinProcessRepeat": run, "pattern": pattern.rawValue, "changed": pattern.changed,
        "width": value.width, "height": value.height, "activeBytes": value.bytes, "snapshotLogicalBytes": detector.snapshotBytes, "ownedSnapshotAllocationBytes": detector.allocatedSnapshotBytes,
        "referenceInputStrideBytes": (0..<2).map { CVPixelBufferGetBytesPerRowOfPlane(first, $0) }, "candidateInputStrideBytes": (0..<2).map { CVPixelBufferGetBytesPerRowOfPlane(candidate, $0) },
        "wallMicros": distribution(wall), "processCpuMicros": cpu, "processCpuMicrosPerCall": cpu / Double(samples),
        "gpuCommandMicros": gpu.isEmpty ? NSNull() as Any : distribution(gpu), "oraclesPassed": true]
}

func byteVectorCoverage(_ image: CVPixelBuffer) throws -> [Int] {
    let current = try signature(image, epoch: 1, limit: byteLimit)
    return try readPlanes(image, current) { planes in
        var values = [Set<UInt8>(), Set<UInt8>(), Set<UInt8>()]
        for (index, plane) in planes.enumerated() {
            let base = plane.base.assumingMemoryBound(to: UInt8.self)
            for row in 0..<plane.rows {
                for column in 0..<plane.width { values[index == 0 ? 0 : 1 + column % 2].insert(base[row * plane.stride + column]) }
            }
        }
        return values.map(\.count)
    }
}

@main struct Main {
    static func main() {
        do {
            let cpuOnly = CommandLine.arguments.contains("--cpu-only")
            let args = CommandLine.arguments.filter { $0 != "--cpu-only" }
            let samples = args.count > 1 ? Int(args[1]) ?? 0 : 60
            let runs = args.count > 2 ? Int(args[2]) ?? 0 : 3
            try require(args.count <= 3 && (20...300).contains(samples) && samples % 2 == 0 && (1...3).contains(runs), "compare-frames [even-samples 20...300] [within-process-repeats 1...3] [--cpu-only]")
            let context = !cpuOnly && MTLCreateSystemDefaultDevice() != nil ? try GPUContext() : nil
            let cpu = CPUDetector(), gpu = context.map { GPUDetector($0) }
            try check(cpu); if let gpu { try check(gpu) }
            let vector = try makeImage(64, 32), coverage = try byteVectorCoverage(vector)
            try require(coverage == [256, 256, 256], "byte vector does not cover every code in each component")
            if let gpu { _ = try gpu.observe(vector, epoch: 17); try gpu.verifySnapshot(vector) }
            let tinyCPU = CPUDetector(limit: 1024)
            var limited: [Detector] = [tinyCPU]; if let context { limited.append(GPUDetector(context, limit: 1024)) }
            for detector in limited {
                let small = try makeImage(32, 16); _ = try detector.observe(small, epoch: 1)
                do { _ = try detector.observe(vector, epoch: 1); throw Failure(message: "snapshot cap accepted") }
                catch let failure as Failure { try require(failure.message == "snapshot byte bound", "unexpected cap error") }
                try require(try detector.observe(small, epoch: 1).changed, "cap failure did not reset snapshot")
                detector.reset()
            }
            var cases: [[String: Any]] = []
            for run in 1...runs {
                for (width, height) in [(1112, 720), (1670, 1080), (3840, 2160)] {
                    let first = try makeImage(width, height)
                    for pattern in Pattern.allCases {
                        let candidate = try makeImage(width, height, pattern: pattern)
                        var backends: [(String, Detector)] = [("cpu", cpu)]
                        if let gpu { if run % 2 == 0 { backends.insert(("metal", gpu), at: 0) } else { backends.append(("metal", gpu)) } }
                        for (name, detector) in backends { cases.append(try autoreleasepool { try measure(detector, backend: name, run: run, first: first, candidate: candidate, pattern: pattern, warmup: 10, samples: samples) }) }
                    }
                }
            }
            cpu.reset(); gpu?.reset()
            var usage = rusage(); try require(getrusage(RUSAGE_SELF, &usage) == 0, "process RSS counter unavailable")
            let report: [String: Any] = ["schemaVersion": 1, "scope": "generated_nv12_memory_only_comparison", "productionAdmission": false,
                "device": context?.device.name as Any? ?? NSNull(), "unifiedMemory": context?.device.hasUnifiedMemory as Any? ?? NSNull(), "gpuStartupMicros": context?.startupMicros as Any? ?? NSNull(),
                "gpuAvailable": gpu != nil, "gpuSnapshotRoundTripPassed": gpu != nil, "cpuOraclesPassed": true, "gpuOraclesPassed": gpu != nil, "roundTripByteValuesPerComponent": coverage,
                "gpuUnavailableReason": context == nil ? (cpuOnly ? "explicit_cpu_control" : "no_metal_device") : NSNull() as Any,
                "snapshotLimitBytes": byteLimit, "samplesPerCase": samples, "withinProcessRepeats": runs, "warmupPerCase": 10,
                "maximumProcessResidentBytes": usage.ru_maxrss, "gpuSharedFlagAllocationBytes": context?.result.allocatedSize as Any? ?? NSNull(), "finalSnapshotBytes": cpu.snapshotBytes + (gpu?.snapshotBytes ?? 0), "cases": cases,
                "limits": "Alternating generated immutable inputs only; no capture/codec/transport/control, source colour/authority/freshness, installed-app resource, GPU utilization/power, or physical endpoints. GPU wall includes plane mapping, submission, completion wait, reference update and result read. CPU includes read locks, active-row comparison and reference update. Peak RSS covers the whole process and is not backend attribution. Snapshot bytes exclude input surfaces, drivers and helpers. No pixel files or suppression."]
            let bytes = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]); FileHandle.standardOutput.write(bytes); FileHandle.standardOutput.write(Data([10]))
        } catch {
            let report: [String: Any] = ["schemaVersion": 1, "scope": "generated_nv12_memory_only_comparison", "productionAdmission": false, "failed": true, "error": String(describing: error).prefix(1024).description]
            if let bytes = try? JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]) { FileHandle.standardOutput.write(bytes); FileHandle.standardOutput.write(Data([10])) }
            exit(1)
        }
    }
}
