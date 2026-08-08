import SwiftUI
import SceneKit
import AppKit

// Full 3D device layer: textures the app screenshot onto the real iPhone .usdz screen
// mesh and renders it with SceneKit off-screen. Slots behind the same DeviceRenderer
// protocol as the 2D compositor — ScreenFrame is unchanged. Tilt is baked into the
// scene (a real camera + perspective), not faked.

struct SceneKitDeviceRenderer: DeviceRenderer {
    let modelURL: URL
    var screenMaterial = "_7ProMax_Screen"   // material on the screen mesh (from inspection)
    var glassMaterial = "glass"               // front-glass material (substring match)
    var heightFraction: CGFloat = 0.66        // upright phone height as fraction of the square render
    var supersample: CGFloat = 2              // render big, SwiftUI downscales for clean edges

    @MainActor
    func view(shot: NSImage?, spec: DeviceSpec) -> AnyView {
        // Render into a square (so any pose fits without clipping); the upright phone
        // occupies `heightFraction` of it, so displaySide maps that to spec.height.
        let displaySide = spec.height / heightFraction
        let renderSide = min(displaySide * supersample, 3200)
        guard let img = render(shot: shot, spec: spec, side: renderSide) else {
            return AnyView(Color.clear.frame(width: displaySide, height: displaySide))
        }
        return AnyView(Image(nsImage: img).resizable().frame(width: displaySide, height: displaySide))
    }

    @MainActor
    private func render(shot: NSImage?, spec: DeviceSpec, side: CGFloat) -> NSImage? {
        guard let scene = try? SCNScene(url: modelURL),
              let device = MTLCreateSystemDefaultDevice() else { return nil }
        let root = scene.rootNode

        // The USDZ carries a big unit scale on its ancestor chain, so the real model is
        // ~16 units tall. rootNode.boundingBox aggregates that correctly in world space
        // (flattenedClone collapses this model to nothing — do not use it here).
        let (bmin, bmax) = root.boundingBox
        let center = SCNVector3((bmin.x + bmax.x) / 2, (bmin.y + bmax.y) / 2, (bmin.z + bmax.z) / 2)
        let height = CGFloat(bmax.y - bmin.y)

        // Re-parent the model under a pivot so the pose rotates it about its own center
        // (children keep their own transforms; the pivot is identity + our rotation).
        let pivot = SCNNode()
        for child in root.childNodes { pivot.addChildNode(child) }
        root.addChildNode(pivot)
        pivot.pivot = SCNMatrix4MakeTranslation(center.x, center.y, center.z)
        pivot.position = center
        let d2r = Double.pi / 180
        pivot.eulerAngles = SCNVector3(spec.pose.pitch * d2r, spec.pose.yaw * d2r, spec.pose.roll * d2r)

        // Texture the screenshot onto the screen mesh, shown flat and full-brightness.
        // The model's screen carries a wavy normal map (a "screen protector" look) that
        // ripples the image — clear it so the app content stays crisp, as Apple requires.
        if let mat = material(named: screenMaterial, in: pivot), spec.blackScreen || shot != nil {
            mat.diffuse.contents = spec.blackScreen ? NSColor.black : shot
            mat.lightingModel = .constant
            mat.normal.contents = nil
            mat.emission.contents = nil
            mat.metalness.contents = NSNumber(value: 0)
            mat.roughness.contents = NSNumber(value: 1)
            mat.isDoubleSided = false
            mat.diffuse.wrapS = .clamp
            mat.diffuse.wrapT = .clamp
        }

        // Hide the front glass mesh — it has its own normal-mapped waviness that reflects
        // as swirls over the screen. We keep the crisp display instead.
        hideMeshes(withMaterial: glassMaterial, in: pivot)

        // Recolor the silver body to graphite (like Hardware.png): tint every body
        // material dark while keeping it metallic, so the rails still catch a sharp
        // highlight but the body reads dark instead of "lit up".
        recolorBody(in: pivot)

        // Camera on the screen side (front = -Z), oriented by hand: look(at:) renders
        // nothing here, but a straight 180° yaw does. Framed so an upright phone height
        // fills `heightFraction` of the square canvas (leaving margin for the pose).
        let fovV = 18.0
        let cam = SCNCamera()
        cam.usesOrthographicProjection = false
        cam.fieldOfView = fovV
        cam.projectionDirection = .vertical
        cam.zNear = 0.001; cam.zFar = 1000
        // HDR + subtle bloom so bright specular highlights on the metal/glass glow like a
        // real product shot instead of clipping flat.
        cam.wantsHDR = true
        cam.wantsExposureAdaptation = false
        cam.bloomThreshold = 0.9
        cam.bloomIntensity = 0.25
        cam.bloomBlurRadius = 6
        let camNode = SCNNode(); camNode.camera = cam
        let dist = Double(height) / (2 * Double(heightFraction) * tan(fovV / 2 * .pi / 180))
        camNode.position = SCNVector3(center.x, center.y, center.z - SCNFloat(dist))
        camNode.eulerAngles = SCNVector3(0, Double.pi, 0)
        root.addChildNode(camNode)

        lightRig(into: root, center: center)
        // Image-based lighting: a studio softbox environment gives the metal frame and
        // glass real reflections/highlights — this is what stops it looking plasticky.
        scene.lightingEnvironment.contents = Self.studioEnvironment
        scene.lightingEnvironment.intensity = 1.5

        let renderer = SCNRenderer(device: device, options: nil)
        renderer.scene = scene
        renderer.pointOfView = camNode
        renderer.autoenablesDefaultLighting = false
        let px = CGSize(width: side, height: side)
        // First snapshot primes the renderer and comes back empty; the second is real.
        _ = renderer.snapshot(atTime: 0, with: CGSize(width: 16, height: 16), antialiasingMode: .none)
        return renderer.snapshot(atTime: 0, with: px, antialiasingMode: .multisampling4X)
    }

    @MainActor
    private func lightRig(into root: SCNNode, center: SCNVector3) {
        func light(_ type: SCNLight.LightType, _ intensity: CGFloat, at pos: SCNVector3) {
            let l = SCNLight(); l.type = type; l.intensity = intensity; l.temperature = 6500
            l.castsShadow = false
            let n = SCNNode(); n.light = l; n.position = pos; n.look(at: center)
            root.addChildNode(n)
        }
        // Low ambient (the environment map does the fill); a crisp key for the highlight
        // streak down the frame, a fill from the other side, and a top rim so the upper
        // frame/bezel catches a highlight too instead of reading flat.
        light(.ambient, 180, at: center)
        light(.directional, 850, at: SCNVector3(center.x - 0.5, center.y + 0.6, center.z - 0.8))
        light(.directional, 320, at: SCNVector3(center.x + 0.7, center.y - 0.2, center.z - 0.6))
        light(.directional, 1100, at: SCNVector3(center.x - 0.15, center.y + 1.3, center.z - 0.5)) // top rim
        light(.spot, 900, at: SCNVector3(center.x + 0.2, center.y + 1.1, center.z - 0.7))          // top glint
    }

    // A procedural equirectangular studio environment: bright "ceiling" softbox fading to
    // a darker floor, so reflective surfaces show a gradient with a hot highlight band.
    static let studioEnvironment: NSImage = {
        let w = 1024, h = 512
        let img = NSImage(size: NSSize(width: w, height: h))
        img.lockFocus()
        let grad = NSGradient(colorsAndLocations:
            (NSColor(white: 1.0, alpha: 1), 0.0),      // top = ceiling (bright)
            (NSColor(white: 0.9, alpha: 1), 0.28),
            (NSColor(white: 0.5, alpha: 1), 0.55),
            (NSColor(white: 0.22, alpha: 1), 0.8),
            (NSColor(white: 0.1, alpha: 1), 1.0))       // bottom = floor (dark)
        grad?.draw(in: NSRect(x: 0, y: 0, width: w, height: h), angle: 90)
        // Two softbox bands (upper "ceiling" + lower "floor bounce") so reflective
        // surfaces catch a highlight at both the top and bottom of the frame.
        NSColor.white.setFill()
        NSBezierPath(roundedRect: NSRect(x: w / 4, y: Int(Double(h) * 0.66), width: w / 2, height: h / 8),
                     xRadius: 20, yRadius: 20).fill()
        NSColor(white: 0.75, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: w / 5, y: Int(Double(h) * 0.24), width: Int(Double(w) * 0.6), height: h / 10),
                     xRadius: 16, yRadius: 16).fill()
        img.unlockFocus()
        return img
    }()

    // Tint the phone body graphite. Skips the screen, hidden glass, and camera lenses.
    // Uses `multiply` so the metallic reflections survive (just darkened), giving a
    // space-black look with sharp rail highlights rather than bright silver.
    private func recolorBody(in node: SCNNode) {
        let skip = ["screen", "glass", "lens", "logo"]
        let graphite = NSColor(calibratedRed: 0.19, green: 0.19, blue: 0.21, alpha: 1)
        func walk(_ n: SCNNode) {
            for m in n.geometry?.materials ?? [] {
                let name = (m.name ?? "").lowercased()
                if skip.contains(where: { name.contains($0) }) { continue }
                m.multiply.contents = graphite
                // Sharper, brighter specular so the frame highlights "pop" like polished
                // metal instead of a soft satin.
                m.metalness.contents = NSNumber(value: 1.0)
                m.roughness.contents = NSNumber(value: 0.22)
            }
            n.childNodes.forEach(walk)
        }
        walk(node)
    }

    private func hideMeshes(withMaterial substring: String, in node: SCNNode) {
        if node.geometry?.materials.contains(where: { ($0.name ?? "").localizedCaseInsensitiveContains(substring) }) == true {
            node.isHidden = true
        }
        for c in node.childNodes { hideMeshes(withMaterial: substring, in: c) }
    }

    private func material(named name: String, in node: SCNNode) -> SCNMaterial? {
        if let m = node.geometry?.materials.first(where: {
            ($0.name ?? "").caseInsensitiveCompare(name) == .orderedSame
        }) { return m }
        for c in node.childNodes { if let m = material(named: name, in: c) { return m } }
        return nil
    }
}
