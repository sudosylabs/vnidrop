import Foundation

// A 3D device model + the per-model quirks the renderer needs (screen material name,
// orientation fix so the screen faces the camera, optional front-glass mesh to hide,
// whether to tint the body graphite).
struct DeviceModel {
    var url: URL
    var screenMaterial: String
    var glassMaterial: String?     // nil = no separate glass mesh
    var bodyYaw: Double = 0        // deg about Y to face the screen toward the camera (-Z)
    var recolorBody: Bool = false  // graphite tint
    // Fraction of the square render the device's upright height fills. Lower = more margin
    // for wide 3/4 poses (a laptop's fanned base projects past a tight frame and clips).
    var fillFraction: CGFloat = 0.66
    // Overscan for auto-generated screen UVs: >0 shrinks the screenshot slightly so its
    // edges aren't hidden when the screen mesh is a touch larger than the visible display.
    var screenPad: CGFloat = 0

    static func iphone(_ assets: URL) -> DeviceModel {
        DeviceModel(url: assets.appendingPathComponent("iphone-17-pro-max.usdz"),
                    screenMaterial: "_7ProMax_Screen", glassMaterial: "glass",
                    bodyYaw: 0, recolorBody: true)
    }
    static func ipad(_ assets: URL) -> DeviceModel {
        // Screen mesh "Material_002" (display only; the bezel is real geometry). Front
        // faces +Z, so yaw 180° to face -Z. Standard UVs — no remap or bezel padding.
        DeviceModel(url: assets.appendingPathComponent("ipad-pro.usdz"),
                    screenMaterial: "Material_002", glassMaterial: nil,
                    bodyYaw: 180, recolorBody: true)
    }
    static func macbook(_ assets: URL) -> DeviceModel {
        // Open-laptop model; display mesh "screen_black" (no UVs — auto-generated). Front
        // faces +Z, so yaw 180° to face -Z.
        DeviceModel(url: assets.appendingPathComponent("macbook-air.usdz"),
                    screenMaterial: "screen_black", glassMaterial: nil,
                    bodyYaw: 180, recolorBody: false, fillFraction: 0.46, screenPad: 0.008)
    }
}

// An App Store target: canvas size, device model, capture simulator, output subfolder.
enum Platform: String {
    case iphone, ipad, mac, web

    var canvas: CGSize {
        switch self {
        case .iphone: CGSize(width: 1284, height: 2778)   // 6.5" iPhone
        case .ipad:   CGSize(width: 2064, height: 2752)   // 13" iPad Pro (matches the M5 sim)
        case .mac:    CGSize(width: 2880, height: 1800)   // Mac App Store (16:10 landscape)
        case .web:    CGSize(width: 2400, height: 1920)   // landing-page hero, 5:4 @2x
        }
    }

    func deviceModel(assets: URL) -> DeviceModel {
        switch self {
        case .iphone: DeviceModel.iphone(assets)
        case .ipad:   DeviceModel.ipad(assets)
        case .mac:    DeviceModel.macbook(assets)
        // The hero's Mac is a flat window layer, so the only 3D model is the phone.
        case .web:    DeviceModel.iphone(assets)
        }
    }

    // Simulator used by capture.sh (informational here; capture reads its own env).
    var simulator: String {
        switch self {
        case .iphone: "iPhone 17 Pro Max"
        case .ipad:   "iPad Pro 13-inch (M5)"
        case .mac:    "My Mac"
        case .web:    "iPhone 17 Pro Max"
        }
    }

    // Output goes under <Language>/<subfolder> so the sets stay separate.
    var outputSubfolder: String {
        switch self {
        case .iphone: "iPhone"
        case .ipad:   "iPad"
        case .mac:    "Mac"
        case .web:    "Web"
        }
    }
}
