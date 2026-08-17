import AppKit
import SwiftUI

// Code-driven App Store screenshots, rendered natively with SwiftUI + ImageRenderer.
//
//   swift run studio                       # -> generated/<Language>/<Name>.png
//   swift run studio --publish             # -> ../<Language>/<Name>.png (ships)
//   LOCALES="fr de" SCREENS="share-securely" swift run studio
//
// Screenshots come from generated/shots/<platform>/<locale>/<screen>.png (from
// ./capture.sh) — transient build output, regenerated per run, never committed. The 3D
// device models + globe come from assets/. Run from the studio directory.

// Screen id -> output file basename (matches the existing App Store filenames).
let nameFor: [String: String] = [
    "choose-receivers": "Choose Receivers", "send-anywhere": "Send Anywhere",
    "share-securely": "Share Securely", "stay-private": "Stay private",
    "web-hero": "hero",
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
func writePNG(_ view: some View, to url: URL, transparent: Bool = false) throws {
    let renderer = ImageRenderer(content: view)
    renderer.scale = 1
    // The web hero has no background of its own; the page supplies it.
    renderer.isOpaque = !transparent
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

    // Target platform: canvas size + 3D device model. PLATFORM=iphone|ipad (default iphone).
    let platform = Platform(rawValue: env("PLATFORM", "iphone")) ?? .iphone
    let canvas = platform.canvas
    let model = platform.deviceModel(assets: base.appendingPathComponent("assets"))
    let device: DeviceRenderer? = FileManager.default.fileExists(atPath: model.url.path)
        ? SceneKitDeviceRenderer(model: model) : nil
    if device == nil {
        FileHandle.standardError.write(Data("warning: missing model \(model.url.lastPathComponent) — devices will be blank\n".utf8))
    }
    let specs = ScreenSpec.all(for: platform)

    var count = 0
    for loc in locales {
        guard let ls = strings.locales[loc] else {
            FileHandle.standardError.write(Data("warning: no strings for locale \(loc)\n".utf8)); continue
        }
        // iPhone -> <Language>/, iPad -> <Language>/iPad/ so the sets stay separate.
        var outDir = publish
            ? base.appendingPathComponent("../\(ls.folder)").standardized
            : base.appendingPathComponent("generated/\(ls.folder)")
        if !platform.outputSubfolder.isEmpty {
            outDir = outDir.appendingPathComponent(platform.outputSubfolder)
        }

        for scr in screens {
            guard let spec = specs[scr] else {
                FileHandle.standardError.write(Data("warning: skipping \(loc)/\(scr)\n".utf8)); continue
            }
            // Caption-less screens (the web hero) carry no strings.json entry.
            let blank = Caption(title: "", subtitle: "", encryption: nil, protection: nil)
            guard let caption = spec.showsCaption ? ls[scr] : blank else {
                FileHandle.standardError.write(Data("warning: no caption for \(loc)/\(scr)\n".utf8)); continue
            }

            let shotId = spec.shotId ?? scr
            let shotsDir = env("SHOTS_DIR", "generated/shots/\(platform.rawValue)")
            let shot = loadNSImage(base.appendingPathComponent("\(shotsDir)/\(loc)/\(shotId).png").path)?.0

            // Flat window layers come from their own platform's capture tree — the web
            // hero pairs a `mac` window shot with the `iphone` model in one composition.
            let windowShots: [NSImage?] = spec.windowShotIds.map { id in
                loadNSImage(base.appendingPathComponent("\(env("WINDOW_SHOTS_DIR", "generated/shots/mac"))/\(loc)/\(id).png").path)?.0
            }
            let hasDevice = spec.device != nil || !spec.devices.isEmpty
            let frame = ScreenFrame(spec: spec, caption: caption, globe: globe, shot: shot,
                                    windowShots: windowShots,
                                    device: hasDevice ? device : nil, canvas: canvas)
            let out = outDir.appendingPathComponent("\(nameFor[scr] ?? scr).png")
            try writePNG(frame, to: out, transparent: spec.bg == nil)
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
