import SwiftUI
import SceneKit
import AppKit

// Full 3D device layer: textures the app screenshot onto the real iPhone .usdz screen
// mesh and renders it with SceneKit off-screen. Slots behind the same DeviceRenderer
// protocol as the 2D compositor — ScreenFrame is unchanged. Tilt is baked into the
// scene (a real camera + perspective), not faked.

struct SceneKitDeviceRenderer: DeviceRenderer {
    let model: DeviceModel

    var supersample: CGFloat = 2              // render big, SwiftUI downscales for clean edges

    @MainActor
    func view(shot: NSImage?, spec: DeviceSpec) -> AnyView {
        // Render into a square (so any pose fits without clipping); the upright phone
        // occupies `heightFraction` of it, so displaySide maps that to spec.height.
        let displaySide = spec.height / model.fillFraction
        let renderSide = min(displaySide * supersample, 3200)
        guard let img = render(shot: shot, spec: spec, side: renderSide) else {
            return AnyView(Color.clear.frame(width: displaySide, height: displaySide))
        }
        return AnyView(Image(nsImage: img).resizable().frame(width: displaySide, height: displaySide))
    }

    @MainActor
    private func render(shot: NSImage?, spec: DeviceSpec, side: CGFloat) -> NSImage? {
        guard let scene = try? SCNScene(url: model.url),
              let device = MTLCreateSystemDefaultDevice() else { return nil }
        let root = scene.rootNode
        let d2r = Double.pi / 180

        // Orient the model so its screen faces the camera (-Z). Some models (iPad) have
        // the screen on ±X, so a per-model yaw fix is applied first.
        let oriented = SCNNode()
        for child in root.childNodes { oriented.addChildNode(child) }
        oriented.eulerAngles = SCNVector3(0, model.bodyYaw * d2r, 0)

        // Re-parent under a pivot so the pose rotates it about its own centre. Compute the
        // (oriented) bounding box before applying the pose. rootNode.boundingBox aggregates
        // the big ancestor unit-scale correctly (flattenedClone collapses these models).
        let pivot = SCNNode()
        pivot.addChildNode(oriented)
        root.addChildNode(pivot)
        let (bmin, bmax) = pivot.boundingBox
        let center = SCNVector3((bmin.x + bmax.x) / 2, (bmin.y + bmax.y) / 2, (bmin.z + bmax.z) / 2)
        let height = CGFloat(bmax.y - bmin.y)
        pivot.pivot = SCNMatrix4MakeTranslation(center.x, center.y, center.z)
        pivot.position = center
        pivot.eulerAngles = SCNVector3(spec.pose.pitch * d2r, spec.pose.yaw * d2r, spec.pose.roll * d2r)

        // Texture the screenshot onto the screen mesh, shown flat and full-brightness.
        // The model's screen carries a wavy normal map (a "screen protector" look) that
        // ripples the image — clear it so the app content stays crisp, as Apple requires.
        // Some screen meshes ship with no UV coordinates (an image can't map onto them —
        // it renders as a flat colour). Generate planar UVs from the vertex positions.
        addPlanarUVsToScreen(in: pivot, material: model.screenMaterial)

        // Texture EVERY material with the screen name (some models split the display into
        // several meshes that share the material name); setting only the first leaves the
        // rest white.
        // A thin black margin baked around the shot so the window's own corners clear the
        // display's rounded edge — done in the image (not via UV overscan) so the clamped
        // border samples black instead of smearing the capture's rounded-corner pixels.
        let screenShot = (shot != nil && model.screenPad > 0) ? matted(shot!, pad: model.screenPad) : shot
        for mat in materials(named: model.screenMaterial, in: pivot) where spec.blackScreen || shot != nil {
            mat.diffuse.contents = spec.blackScreen ? NSColor.black : screenShot
            mat.lightingModel = .constant
            mat.normal.contents = nil
            mat.emission.contents = nil
            mat.metalness.contents = NSNumber(value: 0)
            mat.roughness.contents = NSNumber(value: 1)
            mat.isDoubleSided = false
            mat.diffuse.wrapS = .clamp
            mat.diffuse.wrapT = .clamp
        }

        // Hide the front glass mesh (if any) — it has its own normal-mapped waviness that
        // reflects as swirls over the screen. We keep the crisp display instead.
        if let glass = model.glassMaterial { hideMeshes(withMaterial: glass, in: pivot) }

        // Recolor a silver body to graphite (iPhone); models already dark (iPad) skip it.
        if model.recolorBody { recolorBody(in: pivot) }

        // Camera on the screen side (front = -Z), oriented by hand: look(at:) renders
        // nothing here, but a straight 180° yaw does. Framed so an upright phone height
        // fills `heightFraction` of the square canvas (leaving margin for the pose).
        let fovV = 18.0
        let cam = SCNCamera()
        cam.usesOrthographicProjection = false
        cam.fieldOfView = fovV
        cam.projectionDirection = .vertical
        // zFar must comfortably exceed the camera distance; models differ hugely in unit
        // scale (iPhone ~16 units tall, iPad ~300), so keep this large.
        cam.zNear = 0.01; cam.zFar = 100_000
        // HDR + subtle bloom so bright specular highlights on the metal/glass glow like a
        // real product shot instead of clipping flat.
        cam.wantsHDR = true
        cam.wantsExposureAdaptation = false
        cam.bloomThreshold = 0.9
        cam.bloomIntensity = 0.25
        cam.bloomBlurRadius = 6
        let camNode = SCNNode(); camNode.camera = cam
        let dist = Double(height) / (2 * Double(model.fillFraction) * tan(fovV / 2 * .pi / 180))
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
                // Never tint the screen — its material name may not contain "screen"
                // (the iPad's is just "Material").
                if m.name == model.screenMaterial { continue }
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

    // Give the screen mesh planar UVs (mapped over its two largest-extent axes) when it
    // has none, so a screenshot texture maps across the display.
    private func addPlanarUVsToScreen(in node: SCNNode, material name: String) {
        guard let g = node.geometry,
              g.materials.contains(where: { ($0.name ?? "").caseInsensitiveCompare(name) == .orderedSame }),
              g.sources(for: .texcoord).isEmpty,
              let vsrc = g.sources(for: .vertex).first else {
            node.childNodes.forEach { addPlanarUVsToScreen(in: $0, material: name) }
            return
        }
        // Read vertex positions.
        var pos: [(Float, Float, Float)] = []
        let stride = vsrc.dataStride, off = vsrc.dataOffset
        vsrc.data.withUnsafeBytes { (p: UnsafeRawBufferPointer) in
            for i in 0..<vsrc.vectorCount {
                let b = off + i * stride
                pos.append((p.load(fromByteOffset: b, as: Float.self),
                            p.load(fromByteOffset: b + 4, as: Float.self),
                            p.load(fromByteOffset: b + 8, as: Float.self)))
            }
        }
        let xs = pos.map(\.0), ys = pos.map(\.1), zs = pos.map(\.2)
        let ext = [xs.max()! - xs.min()!, ys.max()! - ys.min()!, zs.max()! - zs.min()!]
        // Screen plane = the two axes with the largest extent (drop the thin normal axis).
        let normalAxis = ext.firstIndex(of: ext.min()!)!
        let planeAxes = [0, 1, 2].filter { $0 != normalAxis }        // [u-axis, v-axis]
        func comp(_ p: (Float, Float, Float), _ a: Int) -> Float { a == 0 ? p.0 : (a == 1 ? p.1 : p.2) }
        let ua = planeAxes[0], va = planeAxes[1]
        let umin = [xs, ys, zs][ua].min()!, urange = max(1e-6, ext[ua])
        let vmin = [xs, ys, zs][va].min()!, vrange = max(1e-6, ext[va])
        // Plain [0,1] planar mapping — the screenshot fills the mesh exactly. Any margin the
        // display needs is baked into the image (see `matted`), not added here, so the clamped
        // edge stays black instead of smearing the capture's corner pixels.
        let uvs: [CGPoint] = pos.map {
            CGPoint(x: CGFloat((comp($0, ua) - umin) / urange),
                    y: CGFloat(1 - (comp($0, va) - vmin) / vrange))   // flip V for top-left origin
        }
        let uvSource = SCNGeometrySource(textureCoordinates: uvs)
        let newGeo = SCNGeometry(sources: g.sources(for: .vertex) + g.sources(for: .normal) + [uvSource],
                                 elements: g.elements)
        newGeo.materials = g.materials
        node.geometry = newGeo
        node.childNodes.forEach { addPlanarUVsToScreen(in: $0, material: name) }
    }

    // Return the shot centred on a black canvas `pad` larger on each side, so texturing it
    // leaves a thin black border around the window inside the display's rounded corners.
    private func matted(_ shot: NSImage, pad: CGFloat) -> NSImage {
        let s = shot.size
        let canvas = NSSize(width: s.width * (1 + 2 * pad), height: s.height * (1 + 2 * pad))
        let out = NSImage(size: canvas)
        out.lockFocus()
        NSColor.black.setFill()
        NSRect(origin: .zero, size: canvas).fill()
        shot.draw(in: NSRect(x: s.width * pad, y: s.height * pad, width: s.width, height: s.height))
        out.unlockFocus()
        return out
    }

    private func materials(named name: String, in node: SCNNode) -> [SCNMaterial] {
        var out = (node.geometry?.materials ?? []).filter {
            ($0.name ?? "").caseInsensitiveCompare(name) == .orderedSame
        }
        for c in node.childNodes { out += materials(named: name, in: c) }
        return out
    }
}
