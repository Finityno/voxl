// Native capability inventory only; no session, pixel buffer, UI or input.
import Foundation
import CoreMedia
import VideoToolbox

let codecs: [(String, CMVideoCodecType)] = [
    ("h264", kCMVideoCodecType_H264),
    ("hevc", kCMVideoCodecType_HEVC),
    ("av1", 0x61763031),
]
#if arch(arm64)
let architecture = "arm64"
#elseif arch(x86_64)
let architecture = "x86_64"
#else
let architecture = "unknown"
#endif
let observations = codecs.map { name, type -> [String: Any] in
    ["codec": name, "hardwareEncodeReported": VTIsHardwareEncodeSupported(type),
     "hardwareDecodeReported": VTIsHardwareDecodeSupported(type)]
}
let result: [String: Any] = [
    "schemaVersion": 1,
    "osVersion": ProcessInfo.processInfo.operatingSystemVersionString,
    "architecture": architecture,
    "codecs": observations,
    "scope": "VideoToolbox capability flags on this host only. No session construction, encoded/decoded frames, exact profile/geometry/rate/color support, receiver engine, performance or application admission is established. A missing hardware flag does not rule out software support.",
]
do {
    let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys, .prettyPrinted])
    try FileHandle.standardOutput.write(contentsOf: data)
    try FileHandle.standardOutput.write(contentsOf: Data([10]))
} catch {
    try? FileHandle.standardError.write(contentsOf: Data("codec capability inventory failed\n".utf8))
    exit(1)
}
