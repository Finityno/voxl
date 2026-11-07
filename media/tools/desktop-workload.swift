// Offscreen generated coding workload. No screen capture, UI or input APIs.
// Menlo is read from the installed OS; no font or rendered image is bundled.
import Foundation
import CoreGraphics
import CoreText
import CryptoKit
import Darwin

struct Failure: Error { let message: String }
func require(_ condition: Bool, _ message: String) throws { if !condition { throw Failure(message: message) } }
func exclusive(_ path: URL) throws -> FileHandle {
    let fd = open(path.path, O_CREAT | O_EXCL | O_WRONLY, 0o600)
    try require(fd >= 0, "exclusive output creation failed")
    return FileHandle(fileDescriptor: fd, closeOnDealloc: true)
}
@MainActor func run() throws {
let args = CommandLine.arguments
try require(args.count == 2 || args.count == 4 || args.count == 5, "desktop-workload <new-output-directory> [width height [30|60]]")
let width = args.count >= 4 ? Int(args[2]) ?? 0 : 1920
let height = args.count >= 4 ? Int(args[3]) ?? 0 : 1080
try require(width >= 960 && height >= 540 && width <= 3840 && height <= 2160 && width % 2 == 0 && height % 2 == 0, "unsupported workload geometry")
let fps = args.count == 5 ? Int(args[4]) ?? 0 : 30
try require(fps == 30 || fps == 60, "unsupported workload cadence")
let warmup = fps, observed = fps*8, frames = warmup + observed, phaseFrames = fps*2
let destination = URL(fileURLWithPath: args[1], isDirectory: true)
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
let nv12File = try exclusive(destination.appendingPathComponent("reference.nv12"))
let thinFile = try exclusive(destination.appendingPathComponent("reference-thin.rgb"))
defer { try? nv12File.close(); try? thinFile.close() }
let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
// Let CoreGraphics own the backing allocation for the context's full lifetime.
// Its data pointer is borrowed only while this retained context remains live.
guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: colorSpace, bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue),
      let rgba = context.data else { throw Failure(message: "bitmap context unavailable") }
context.setShouldAntialias(true)
context.setShouldSmoothFonts(false)
let font = CTFontCreateWithName("Menlo-Regular" as CFString, 14, nil)
let smallFont = CTFontCreateWithName("Menlo-Regular" as CFString, 11, nil)
let actualFont = CTFontCopyPostScriptName(font) as String
try require(actualFont == "Menlo-Regular", "requested installed font is unavailable")
let palette: [[Double]] = [[0.90,0.90,0.92],[0.40,0.74,1.0],[1.0,0.40,0.60],[0.36,0.92,0.50],[1.0,0.77,0.32],[0.68,0.53,1.0]]
@MainActor func color(_ rgb: [Double]) -> CGColor { CGColor(colorSpace: colorSpace, components: rgb.map { CGFloat($0) } + [1])! }
@MainActor func rectangle(_ x: Int, _ y: Int, _ w: Int, _ h: Int, _ rgb: [Double]) {
    context.setFillColor(color(rgb)); context.fill(CGRect(x: x, y: height-y-h, width: w, height: h))
}
@MainActor func text(_ value: String, _ x: Int, _ y: Int, _ index: Int = 0, small: Bool = false) {
    let attributes: [NSAttributedString.Key: Any] = [NSAttributedString.Key(kCTFontAttributeName as String): small ? smallFont : font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): color(palette[index % palette.count]),
        NSAttributedString.Key(kCTLigatureAttributeName as String): 0]
    let line = CTLineCreateWithAttributedString(NSAttributedString(string: value, attributes: attributes))
    context.textPosition = CGPoint(x: x, y: height-y-14)
    CTLineDraw(line, context)
}
let code = ["use std::time::{Duration, Instant};", "", "struct View {", "    sequence: u64,", "    last_frame: Instant,", "    title: String,", "}", "", "impl View {", "    fn apply(&mut self, command: Command) -> Result<(), Error> {", "        if command.sequence <= self.sequence {", "            return Err(Error::Stale);", "        }", "        self.sequence = command.sequence;", "        self.last_frame = Instant::now();", "        Ok(())", "    }", "}", "", "// No input is sent by this generated workload.", "let status = view.title.trim();", "assert_eq!(width % 2, 0);", "let message = format!(\"frame {}\", sequence);", "return Ok((message, Duration::from_millis(16)));" ]
let thinY = height - 90, thinH = 56, textX = 180, textW = width - 200
var nv12 = [UInt8](repeating: 128, count: width * height * 3 / 2)
var nvHash = SHA256(), thinHash = SHA256()
var phases: [[String: Any]] = []
func quantize(_ value: Double) -> UInt8 { UInt8(min(255, max(0, value.rounded()))) }
for index in 0..<frames {
    let observation = max(0, index-warmup), phase = min(3, observation/phaseFrames), step = observation%phaseFrames
    let sequence = step*30/fps
    rectangle(0,0,width,height,[0.075,0.075,0.085])
    rectangle(0,0,width,32,[0.16,0.16,0.18]); text("Project   src / view.rs       Generated desktop workload",16,6)
    rectangle(0,32,160,height-56,[0.10,0.11,0.13])
    for (n,name) in ["src", "  main.rs", "  view.rs", "  transport.rs", "tests", "Cargo.toml"].enumerated() { text(name,12,52+n*24,n%3) }
    rectangle(164,36,width-172,height-138,[0.055,0.060,0.070])
    let scroll = phase == 2 ? step*120/fps : 0
    context.saveGState(); context.clip(to: CGRect(x: 166,y: 105,width: width-172,height: height-150))
    for row in 0..<(height/20+3) {
        let top = 54 + row*20 - scroll%20
        let line = row+scroll/20
        text(String(format:"%3d",line+1),170,top,0,small:true)
        text(code[line%code.count],210,top,line%palette.count)
    }
    if phase == 1 {
        let typing = "let decoded_frame = receiver.next().await?;"
        // Preserve the original wall-time typing schedule at either cadence.
        let count = min(typing.count,step*30*typing.count/(fps*59))
        rectangle(208,height-164,width-230,24,[0.055,0.060,0.070]);text(String(typing.prefix(count)),210,height-162,3)
        rectangle(210+count*8,height-160,1,15,[0.9,0.9,0.9])
    }
    context.restoreGState()
    // Static one-pixel colored strokes beside small antialiased text. Crop is
    // even-aligned and retained as original RGB, before 4:2:0 subsampling.
    rectangle(textX,thinY,textW,thinH,[0.055,0.060,0.070])
    text("thin syntax: fn main() { let value = 0123456789; } // rgb text",textX+8,thinY+3,1,small:true)
    text("AaBbCc []{}() <> != == -> &mut self   RED GREEN BLUE",textX+8,thinY+23,2,small:true)
    for n in 0..<32 {rectangle(width-190+n*5,thinY+4,1,44,palette[1+n%5])}
    if phase == 3 {
        // Keep the declared static reference strip unobscured at every
        // accepted geometry, including the minimum 960x540 fixture.
        let verticalSpan = max(1,min(130,thinY-130-196-12))
        let dx = 180+(step*330/fps)%(width-580), dy = 130+(step*90/fps)%verticalSpan
        rectangle(dx,dy,400,196,[0.25,0.26,0.29]); rectangle(dx+2,dy+28,396,166,[0.11,0.12,0.14])
        text("Generated task output",dx+12,dy+5,0)
        for n in 0..<6 {text("chunk \(n): ready  sequence=\(sequence)",dx+12,dy+42+n*21,3)}
    }
    rectangle(0,height-28,width,28,[0.08,0.19,0.28]);text("UTF-8    Rust    \(fps) fps    phase \(phase)",12,height-26,0,small:true)
    // A top-left red calibration pixel establishes raw memory orientation and
    // RGBA channel order without taking or inspecting an image.
    rectangle(0,0,2,2,[1,0,0])
    context.flush()
    let bytes = rgba.assumingMemoryBound(to: UInt8.self)
    try require(bytes[0] == 255 && bytes[1] == 0 && bytes[2] == 0 && bytes[3] == 255,"unexpected bitmap orientation/channel layout")
    for y in 0..<height {
        for x in 0..<width {
            let p = (y*width+x)*4
            let red = Double(bytes[p]), green = Double(bytes[p+1]), blue = Double(bytes[p+2])
            nv12[y*width+x] = quantize(0.2126*red+0.7152*green+0.0722*blue)
        }
    }
    for y in stride(from: 0,to: height,by: 2) {
        for x in stride(from: 0,to: width,by: 2) {
            var red = 0.0, green = 0.0, blue = 0.0
            for dy in 0..<2 {for dx in 0..<2 {let p = ((y+dy)*width+x+dx)*4;red += Double(bytes[p]);green += Double(bytes[p+1]);blue += Double(bytes[p+2])}}
            red /= 4;green /= 4;blue /= 4
            let luma = 0.2126*red+0.7152*green+0.0722*blue
            let p = width*height+(y/2)*width+x
            nv12[p] = quantize(128+(blue-luma)/1.8556);nv12[p+1] = quantize(128+(red-luma)/1.5748)
        }
    }
    let raw = Data(nv12);try nv12File.write(contentsOf: raw);nvHash.update(data: raw)
    var thin = [UInt8]();thin.reserveCapacity(textW*thinH*3)
    for y in thinY..<thinY+thinH {for x in textX..<textX+textW {let p = (y*width+x)*4;thin.append(bytes[p]);thin.append(bytes[p+1]);thin.append(bytes[p+2])}}
    let rgb = Data(thin);try thinFile.write(contentsOf: rgb);thinHash.update(data: rgb)
    if index == warmup {phases.append(["name":"still","firstFrame":index,"frames":phaseFrames])}
    if index > warmup && (index-warmup)%phaseFrames == 0 {phases.append(["name":["still","typing","scrolling","window-movement"][phase],"firstFrame":index,"frames":phaseFrames])}
}
try nv12File.synchronize();try thinFile.synchronize()
let hex: (SHA256.Digest) -> String = { $0.map { String(format:"%02x",$0) }.joined() }
let manifest: [String: Any] = ["schemaVersion":1,"generator":"CoreText offscreen coding workload v1","width":width,"height":height,"fps":fps,"warmupFrames":warmup,"observedFrames":observed,"totalFrames":frames,"pixelFormat":"nv12","pixelRange":"full","colorPrimaries":"bt709","transferFunction":"iec61966-2-1","ycbcrMatrix":"bt709","chromaSampling":"2x2 RGB box average then quantize","fontPostScriptName":actualFont,"fontPointSize":14,"smallFontPointSize":11,"fontURL":(CTFontCopyAttribute(font,kCTFontURLAttribute) as? URL)?.path ?? "unknown","phases":phases,"regions":["codeText":["x":textX,"y":48,"width":textW,"height":height-156],"thinColoredText":["x":textX,"y":thinY,"width":textW,"height":thinH]],"files":["reference.nv12":["bytes":width*height*3/2*frames,"sha256":hex(nvHash.finalize())],"reference-thin.rgb":["bytes":textW*thinH*3*frames,"sha256":hex(thinHash.finalize())]],"scope":"Generated pixels only. Original RGB thin region is retained before source 4:2:0 conversion. No screen, UI, input, codec or performance qualification."]
let json = try JSONSerialization.data(withJSONObject: manifest,options:[.prettyPrinted,.sortedKeys])
try json.write(to: destination.appendingPathComponent("manifest.json"),options:.withoutOverwriting)
print("generated \(frames) frames at \(width)x\(height); manifest: \(destination.appendingPathComponent("manifest.json").path)")
}

do {try run()}
catch {
    try? FileHandle.standardError.write(contentsOf: Data("desktop-workload: \(error)\n".utf8))
    exit(1)
}
