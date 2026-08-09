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
}

// An App Store target: canvas size, device model, capture simulator, output subfolder.
enum Platform: String {
    case iphone, ipad

    var canvas: CGSize {
        switch self {
        case .iphone: CGSize(width: 1284, height: 2778)   // 6.5" iPhone
        case .ipad:   CGSize(width: 2064, height: 2752)   // 13" iPad Pro (matches the M5 sim)
        }
    }

    func deviceModel(assets: URL) -> DeviceModel {
        switch self {
        case .iphone: DeviceModel.iphone(assets)
        case .ipad:   DeviceModel.ipad(assets)
        }
    }

    // Simulator used by capture.sh (informational here; capture reads its own env).
    var simulator: String {
        switch self {
        case .iphone: "iPhone 17 Pro Max"
        case .ipad:   "iPad Pro 13-inch (M5)"
        }
    }

    // Output goes under <Language>/<subfolder> so iPhone/iPad sets stay separate.
    var outputSubfolder: String {
        switch self {
        case .iphone: ""
        case .ipad:   "iPad"
        }
    }
}
