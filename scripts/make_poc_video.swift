import AppKit
import AVFoundation
import CoreVideo
import CoreGraphics
import Foundation

// ---- Config ----
let W = 1280, H = 720, FPS = 30, SECS = 2.5

struct Slide {
    let step: Int
    let lines: [String]
    let green: [String]
}

func s(_ step: Int, _ lines: [String], _ green: [String] = []) -> Slide {
    Slide(step: step, lines: lines, green: green)
}

let slides: [Slide] = [
    // 1 - START
    s(1, [
        "$ cargo run --example file_index_demo -- --dir /tmp/tuckdb_steps",
        "",
        "=== TuckDB-AI: Persistent vector DB from real files (incremental) ===",
        "",
        "Commands:",
        "  add <path>       upload a text file and index it",
        "  update <id> <path>  replace a file's content; only changed chunks re-embedded",
        "  del <doc_id>     remove a file's vectors",
        "  q <text>         semantic search",
    ]),

    // 2 - INSERT
    s(2, [
        "> add data/demo_cpp.txt",
        "",
        "> [add] doc 0 <- data/demo_cpp.txt: 11 new chunks embedded (0 -> 11 vectors)",
        "      vector DB saved to /tmp/tuckdb_steps/vector_db.tkdb",
        "",
        "(the file is split into 11 paragraphs = 11 chunks,",
        " every chunk is embedded, all 11 vectors are stored)",
    ], ["11 new chunks", "11 vectors"]),

    // 3 - CHECK
    s(3, [
        "> list",
        "",
        "  doc 0: data/demo_cpp.txt (11 chunks)",
        "  total vectors in vector DB: 11",
        "",
        "insert time: 0.22 ms   (11 chunks, 11 embeddings, 11 vectors)",
    ], ["11", "0.22 ms"]),

    // 4 - QUERY 1
    s(4, [
        "> q How do I manage ownership of dynamically allocated memory?",
        "",
        "  #1 score=0.437 [data/demo_cpp.txt chunk 1]",
        "     \"Ownership of dynamically allocated memory is best expressed with",
        "      smart pointers. std::unique_ptr is an exclusive owner...\"",
    ], ["0.437"]),

    // 5 - QUERY 2
    s(5, [
        "> q What happens when an object goes out of scope?",
        "",
        "  #1 score=0.296 [data/demo_cpp.txt chunk 2]",
        "     \"Resource Acquisition Is Initialization, RAII, means the lifetime of",
        "      a resource is tied to the lifetime of an object...\"",
        "",
        "> q What is runtime polymorphism?",
        "  #1 score=0.540 [data/demo_cpp.txt chunk 7]",
        "     \"Virtual functions provide runtime polymorphism through the vtable...\"",
    ], ["0.296", "0.540"]),

    // 6 - UPDATE (edit 1 paragraph)
    s(6, [
        "> update 0 data/demo_cpp_v2.txt",
        "",
        "> [update] doc 0 <- data/demo_cpp_v2.txt:",
        "    changed chunks = 1",
        "    re-embedded = 1",
        "    skipped = 10",
        "    work avoided = 90.9%",
        "",
        "(1 paragraph edited -> only that 1 chunk is re-embedded; 10 untouched)",
    ], ["1", "10", "90.9%"]),

    // 7 - VERIFY
    s(7, [
        "> list",
        "  doc 0: data/demo_cpp_v2.txt (11 chunks)",
        "  total vectors in vector DB: 11",
        "",
        "> q How do I manage ownership of dynamically allocated memory?",
        "  #1 score=0.440 [data/demo_cpp_v2.txt chunk 1]",
        "     \"Ownership of dynamically allocated memory is best expressed with",
        "      smart pointers. std::unique_ptr is an exclusive owner...\"",
    ], ["11", "0.440"]),

    // 8 - ADD + DELETE
    s(8, [
        "> add README.md",
        "> [add] doc 1 <- README.md: 381 new chunks embedded (11 -> 392 vectors)",
        "",
        "> del 1",
        "> [del] doc 1: removed 381 vectors directly (392 -> 11); unrelated untouched",
        "",
        "(add only processes the new file; delete only removes that file's vectors)",
    ], ["381", "untouched"]),

    // 9 - RESULT
    s(9, [
        "1 of 11 chunks changed:",
        "   naive            = 11 embeddings",
        "   lifecycle-aware  = 1 embedding   (90.9% work avoided)",
        "",
        "At scale: 500,000 chunks, 1,000 changed",
        "   naive            = 500,000 embeddings",
        "   lifecycle-aware  = 1,000 embeddings   (99.8% work avoided)",
        "",
        "Process what changed. Leave everything else untouched.",
    ], ["1 embedding", "99.8%", "90.9%"]),
]

let totalSlides = slides.count

func textColor(for line: String, green: [String]) -> NSColor {
    let cmd = NSColor(calibratedRed: 0.95, green: 0.97, blue: 1.0, alpha: 1)
    let out = NSColor(calibratedRed: 0.72, green: 0.78, blue: 0.88, alpha: 1)
    let dim = NSColor(calibratedRed: 0.55, green: 0.60, blue: 0.70, alpha: 1)
    let greenC = NSColor(calibratedRed: 0.28, green: 0.85, blue: 0.62, alpha: 1)
    if green.contains(where: { line.contains($0) }) { return greenC }
    if line.hasPrefix("$ ") || line.hasPrefix("> ") { return cmd }
    if line.hasPrefix("  ") { return out }
    return dim
}

// Render a slide into a CGImage (terminal-window style).
func renderCG(_ s: Slide) -> CGImage {
    let img = NSImage(size: NSSize(width: W, height: H))
    img.lockFocus()

    // outer background
    NSColor(calibratedRed: 0.02, green: 0.025, blue: 0.04, alpha: 1).setFill()
    NSRect(x: 0, y: 0, width: W, height: H).fill()

    // terminal window
    let win = NSRect(x: 48, y: 40, width: W - 96, height: H - 80)
    let path = NSBezierPath(roundedRect: win, xRadius: 14, yRadius: 14)
    NSColor(calibratedRed: 0.055, green: 0.062, blue: 0.085, alpha: 1).setFill()
    path.fill()
    NSColor(calibratedRed: 0.16, green: 0.20, blue: 0.28, alpha: 1).setStroke()
    path.lineWidth = 1.5
    path.stroke()

    // title bar
    let bar = NSBezierPath(roundedRect: NSRect(x: win.minX, y: win.maxY - 44, width: win.width, height: 44),
                           xRadius: 14, yRadius: 14)
    NSColor(calibratedRed: 0.086, green: 0.10, blue: 0.14, alpha: 1).setFill()
    bar.fill()
    NSRect(x: win.minX, y: win.maxY - 44, width: win.width, height: 30).fill() // flatten bottom of bar

    // dots
    NSColor(calibratedRed: 0.96, green: 0.42, blue: 0.42, alpha: 1).setFill()
    NSBezierPath(ovalIn: NSRect(x: win.minX + 22, y: win.maxY - 30, width: 12, height: 12)).fill()
    NSColor(calibratedRed: 0.98, green: 0.78, blue: 0.37, alpha: 1).setFill()
    NSBezierPath(ovalIn: NSRect(x: win.minX + 42, y: win.maxY - 30, width: 12, height: 12)).fill()
    NSColor(calibratedRed: 0.37, green: 0.83, blue: 0.52, alpha: 1).setFill()
    NSBezierPath(ovalIn: NSRect(x: win.minX + 62, y: win.maxY - 30, width: 12, height: 12)).fill()

    // window title + step
    ("tuckdb — vector DB demo" as NSString).draw(at: NSPoint(x: win.minX + 200, y: win.maxY - 28),
        withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 15, weight: .medium),
                         .foregroundColor: NSColor(calibratedRed: 0.6, green: 0.66, blue: 0.76, alpha: 1)])
    ("STEP \(s.step) / \(totalSlides)" as NSString).draw(at: NSPoint(x: win.maxX - 150, y: win.maxY - 28),
        withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 15, weight: .semibold),
                         .foregroundColor: NSColor(calibratedRed: 0.36, green: 0.58, blue: 1.0, alpha: 1)])

    // transcript
    var top: CGFloat = win.minY + 74
    func yFromBottom(_ t: CGFloat) -> CGFloat { CGFloat(H) - t }
    for line in s.lines {
        (line as NSString).draw(at: NSPoint(x: win.minX + 28, y: yFromBottom(top)),
            withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 19, weight: .regular),
                             .foregroundColor: textColor(for: line, green: s.green)])
        top += 28
    }

    img.unlockFocus()
    return img.cgImage(forProposedRect: nil, context: nil, hints: nil)!
}

// Copy a CGImage into a top-down BGRA pixel buffer for the video.
func toBuffer(_ cg: CGImage) -> CVPixelBuffer {
    var pb: CVPixelBuffer?
    let attrs: [String: Any] = [
        kCVPixelBufferCGImageCompatibilityKey as String: true,
        kCVPixelBufferCGBitmapContextCompatibilityKey as String: true,
    ]
    CVPixelBufferCreate(kCFAllocatorDefault, W, H, kCVPixelFormatType_32BGRA, attrs as CFDictionary, &pb)
    let buf = pb!
    CVPixelBufferLockBaseAddress(buf, [])
    let ctx = CGContext(
        data: CVPixelBufferGetBaseAddress(buf),
        width: W, height: H, bitsPerComponent: 8,
        bytesPerRow: CVPixelBufferGetBytesPerRow(buf),
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.premultipliedFirst.rawValue
    )!
    ctx.draw(cg, in: CGRect(x: 0, y: 0, width: CGFloat(W), height: CGFloat(H)))
    CVPixelBufferUnlockBaseAddress(buf, [])
    return buf
}

func pngData(_ buf: CVPixelBuffer) -> Data? {
    CVPixelBufferLockBaseAddress(buf, [])
    defer { CVPixelBufferUnlockBaseAddress(buf, []) }
    guard let ctx = CGContext(
        data: CVPixelBufferGetBaseAddress(buf),
        width: W, height: H, bitsPerComponent: 8,
        bytesPerRow: CVPixelBufferGetBytesPerRow(buf),
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.premultipliedFirst.rawValue
    ) else { return nil }
    guard let img = ctx.makeImage() else { return nil }
    let rep = NSBitmapImageRep(cgImage: img)
    return rep.representation(using: .png, properties: [:])
}

// preview frame for checking
let preview = toBuffer(renderCG(slides[0]))
if let d = pngData(preview) {
    try? d.write(to: URL(fileURLWithPath: "docs/preview_frame.png"))
    print("preview written: docs/preview_frame.png")
}

// ---- Encode frames into an MP4 ----
let url = URL(fileURLWithPath: "docs/poc_demo.mp4")
try? FileManager.default.removeItem(at: url)

let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
let settings: [String: Any] = [
    AVVideoCodecKey: AVVideoCodecType.h264,
    AVVideoWidthKey: W,
    AVVideoHeightKey: H,
]
let input = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
let adaptor = AVAssetWriterInputPixelBufferAdaptor(
    assetWriterInput: input,
    sourcePixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: Int(kCVPixelFormatType_32BGRA),
        kCVPixelBufferWidthKey as String: Int(W),
        kCVPixelBufferHeightKey as String: Int(H),
    ]
)
writer.add(input)
writer.startWriting()
writer.startSession(atSourceTime: .zero)

var frame = 0
for sl in slides {
    let frames = Int(Double(SECS) * Double(FPS))
    let cg = renderCG(sl)
    for _ in 0..<frames {
        while !input.isReadyForMoreMediaData { usleep(5000) }
        let time = CMTime(value: CMTimeValue(frame), timescale: CMTimeScale(FPS))
        adaptor.append(toBuffer(cg), withPresentationTime: time)
        frame += 1
    }
}
input.markAsFinished()
let sem = DispatchSemaphore(value: 0)
writer.finishWriting { sem.signal() }
sem.wait()

let secs = Double(frame) / Double(FPS)
print("done: \(url.path)")
print("frames: \(frame)  duration: \(String(format: "%.1f", secs))s  status: \(writer.status.rawValue)")
