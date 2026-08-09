import SwiftUI
import AppKit

// The device is a swappable layer behind this protocol; `SceneKitDeviceRenderer` is the
// current implementation (textures the app screenshot onto a real .usdz model).
protocol DeviceRenderer {
    // Produces the device as a SwiftUI view sized to `spec.height` (width follows the
    // device aspect), already framing `shot` and posed per `spec`.
    @MainActor func view(shot: NSImage?, spec: DeviceSpec) -> AnyView
}
