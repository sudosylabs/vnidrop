import AppKit
import SwiftUI

// Code-driven App Store screenshots, rendered natively with SwiftUI + ImageRenderer.
//
//   swift run studio                       # -> generated/<Language>/<Name>.png
//   swift run studio --publish             # -> ../<Language>/<Name>.png (ships)
//   LOCALES="fr de" SCREENS="share-securely" swift run studio
//
// Screenshots come from generated/shots/<locale>/<screen>.png (from ./capture.sh),
// transient build output regenerated per run — never committed;
// the straight mockup + globe come from assets/. Run from the studio directory.

// Screen id -> output file basename (matches the existing App Store filenames).
let nameFor: [String: String] = [
    "choose-receivers": "Choose Receivers", "send-anywhere": "Send Anywhere",
    "share-securely": "Share Securely", "stay-private": "Stay private",
]

func env(_ key: String, _ fallback: String) -> String {
    let v = ProcessInfo.processInfo.environment[key]
    return (v?.isEmpty == false) ? v! : fallback
}

func loadNSImage(_ path: String) -> (NSImage, CGSize)? {
    guard let ns = NSImage(contentsOfFile: path) else { return nil }
    let size = ns.representations.first.map { CGSize(width: $0.pixelsWide, height: $0.pixelsHigh) } ?? ns.size
    return (ns, size)
}

@MainActor
func writePNG(_ view: some View, to url: URL) throws {
    let renderer = ImageRenderer(content: view)
    renderer.scale = 1
    guard let cg = renderer.cgImage else {
        throw NSError(domain: "studio", code: 1, userInfo: [NSLocalizedDescriptionKey: "render failed"])
    }
    let rep = NSBitmapImageRep(cgImage: cg)
    rep.size = NSSize(width: cg.width, height: cg.height)
    guard let data = rep.representation(using: .png, properties: [:]) else {
        throw NSError(domain: "studio", code: 2, userInfo: [NSLocalizedDescriptionKey: "png encode failed"])
    }
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                            withIntermediateDirectories: true)
    try data.write(to: url)
}

@MainActor
func run() throws {
    let args = CommandLine.arguments
    let publish = args.contains("--publish")
    let cwd = FileManager.default.currentDirectoryPath
    let base = URL(fileURLWithPath: cwd)

    let strings = try Strings.load(base.appendingPathComponent("strings.json"))

    let locales = env("LOCALES", "en fr de es it nl pl pt ru").split(separator: " ").map(String.init)
    let screens = env("SCREENS", "choose-receivers send-anywhere share-securely stay-private")
        .split(separator: " ").map(String.init)

    let globe = loadNSImage(base.appendingPathComponent("assets/globe.png").path).map { Image(nsImage: $0.0) }

    // Device layer: prefer the 3D iPhone model; fall back to the 2D mockup composite.
    // DEVICE=2d forces the flat compositor; DEVICE=3d requires the .usdz.
    let deviceMode = env("DEVICE", "auto")
    let usdz = base.appendingPathComponent("assets/iphone-17-pro-max.usdz")
    let hasModel = FileManager.default.fileExists(atPath: usdz.path)
    let mockup = loadNSImage(base.appendingPathComponent("assets/mockup-straight.png").path)

    let device: DeviceRenderer?
    if deviceMode != "2d" && hasModel {
        device = SceneKitDeviceRenderer(modelURL: usdz)
    } else if let mockup {
        device = ClipDeviceRenderer(mockup: mockup.0, mockupSize: mockup.1, geo: .straight)
    } else {
        device = nil
        FileHandle.standardError.write(Data("warning: no device model or mockup — devices will be blank\n".utf8))
    }

    var count = 0
    for loc in locales {
        guard let ls = strings.locales[loc] else {
            FileHandle.standardError.write(Data("warning: no strings for locale \(loc)\n".utf8)); continue
        }
        let outDir = publish
            ? base.appendingPathComponent("../\(ls.folder)").standardized
            : base.appendingPathComponent("generated/\(ls.folder)")

        for scr in screens {
            guard let spec = ScreenSpec.all[scr], let caption = ls[scr] else {
                FileHandle.standardError.write(Data("warning: skipping \(loc)/\(scr)\n".utf8)); continue
            }

            let shotId = spec.shotId ?? scr
            let shotsDir = env("SHOTS_DIR", "generated/shots")
            let shot = loadNSImage(base.appendingPathComponent("\(shotsDir)/\(loc)/\(shotId).png").path)?.0
            let hasDevice = spec.device != nil || !spec.devices.isEmpty
            let frame = ScreenFrame(spec: spec, caption: caption, globe: globe, shot: shot,
                                    device: hasDevice ? device : nil)
            let out = outDir.appendingPathComponent("\(nameFor[scr] ?? scr).png")
            try writePNG(frame, to: out)
            print("  ✅ \(out.path)")
            count += 1
        }
    }
    print("\nDone — \(count) screenshot(s)\(publish ? " (published)" : "").")
}

// ImageRenderer needs an AppKit context for text/font resolution.
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
do { try MainActor.assumeIsolated { try run() } }
catch { FileHandle.standardError.write(Data("error: \(error.localizedDescription)\n".utf8)); exit(1) }
