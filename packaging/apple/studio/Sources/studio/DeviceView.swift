import SwiftUI
import AppKit

// The device is one swappable layer. `ClipDeviceRenderer` does a 2D composite
// (screenshot clipped into the straight mockup with iOS-correct `.continuous`
// curvature). `SceneKitDeviceRenderer` textures the shot onto a real iPhone .usdz.
// Both conform to DeviceRenderer, so ScreenFrame never changes.

protocol DeviceRenderer {
    // Produces the device as a SwiftUI view sized to `spec.width` (height follows the
    // device aspect), already framing `shot` and oriented per `spec`.
    @MainActor func view(shot: NSImage?, spec: DeviceSpec) -> AnyView
}

// 2D compositor: RoundedRectangle(.continuous) clip == the real iOS squircle.
struct ClipDeviceRenderer: DeviceRenderer {
    let mockup: NSImage
    let mockupSize: CGSize            // native px size of the mockup image
    let geo: MockupGeometry

    @MainActor
    func view(shot: NSImage?, spec: DeviceSpec) -> AnyView {
        // Fallback 2D path (DEVICE=2d). Authored in the mockup's native pixel space,
        // scaled to the target height, with the pose approximated by rotation effects.
        let mw = mockupSize.width, mh = mockupSize.height
        let sw = mw * geo.screenWFrac
        let sh = mh * geo.screenHFrac
        let sx = mw * geo.screenXFrac
        let sy = mh * geo.screenYFrac
        let radius = sw * geo.cornerFrac
        let displayWidth = spec.height * mw / mh

        let device = ZStack(alignment: .topLeading) {
            if let shot {
                Image(nsImage: shot)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: sw, height: sh)
                    .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
                    .offset(x: sx, y: sy)
            }
            Image(nsImage: mockup)
                .resizable()
                .frame(width: mw, height: mh)
        }
        .frame(width: mw, height: mh)
        .scaleEffect(displayWidth / mw, anchor: .topLeading)
        .frame(width: displayWidth, height: spec.height)

        return AnyView(device
            .rotation3DEffect(.degrees(spec.pose.yaw), axis: (x: 0, y: 1, z: 0), anchor: .center, perspective: 0.3)
            .rotation3DEffect(.degrees(spec.pose.pitch), axis: (x: 1, y: 0, z: 0), anchor: .center, perspective: 0.3)
            .rotationEffect(.degrees(spec.pose.roll)))
    }
}
