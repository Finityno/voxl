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
var encoderList: CFArray?
let encoderStatus = VTCopyVideoEncoderList(nil, &encoderList)
guard encoderStatus == noErr, let encoders = encoderList as? [[String: Any]] else {
    try? FileHandle.standardError.write(contentsOf: Data("encoder inventory failed with status \(encoderStatus)\n".utf8))
    exit(1)
}
let observations = codecs.map { name, type -> [String: Any] in
    let entries = encoders.filter { ($0[kVTVideoEncoderList_CodecType as String] as? NSNumber)?.uint32Value == type }
    let flags = entries.map { $0[kVTVideoEncoderList_IsHardwareAccelerated as String] as? Bool }
    let hardwareCount = flags.compactMap { $0 }.filter { $0 }.count
    let unknownCount = flags.filter { $0 == nil }.count
    let hardware: Any = hardwareCount > 0 ? true : unknownCount > 0 ? NSNull() : false
    return ["codec": name, "hardwareEncodeReported": hardware,
        "listedEncoderCount": entries.count, "listedHardwareEncoderCount": hardwareCount,
        "encodersWithoutHardwareFlag": unknownCount,
        "hardwareDecodeReported": VTIsHardwareDecodeSupported(type)]
}
let result: [String: Any] = [
    "schemaVersion": 1,
    "osVersion": ProcessInfo.processInfo.operatingSystemVersionString,
    "architecture": architecture,
    "codecs": observations,
    "scope": "VideoToolbox encoder-list metadata and hardware-decode flag on this host only. Missing encoder hardware flags remain unknown (null), unless another entry reports hardware. No session construction, encoded/decoded frames, exact profile/geometry/rate/color support, resource availability, receiver engine, performance or application admission is established. No listed hardware encoder does not rule out software or other APIs.",
]
do {
    let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys, .prettyPrinted])
    try FileHandle.standardOutput.write(contentsOf: data)
    try FileHandle.standardOutput.write(contentsOf: Data([10]))
} catch {
    try? FileHandle.standardError.write(contentsOf: Data("codec capability inventory failed\n".utf8))
    exit(1)
}
