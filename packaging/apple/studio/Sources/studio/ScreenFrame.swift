import SwiftUI

// The full 1284x2778 marketing frame: gradient + globe + device + caption.
// Placement mirrors the old screens.typ: images are pinned top-left and pushed by
// (dx, dy); captions are centered and pinned to the top or bottom edge.

struct ScreenFrame: View {
    let spec: ScreenSpec
    let caption: Caption
    let globe: Image?
    let shot: NSImage?
    let device: DeviceRenderer?
    var canvas: CGSize = CGSize(width: 1284, height: 2778)

    private var allDevices: [DeviceSpec] { spec.device.map { [$0] } ?? spec.devices }

    private var titleColor: Color { spec.captionTheme == .light ? .white : Color(hex: "#1b1226") }
    private var subColor: Color { spec.captionTheme == .light ? Color(hex: "#f3ecfb") : Color(hex: "#2c2138") }

    var body: some View {
        let cw = canvas.width, ch = canvas.height

        ZStack(alignment: .topLeading) {
            spec.bg.gradient

            if let g = spec.globe, let globe {
                globe.resizable().scaledToFit()
                    .frame(width: g.width)
                    .saturation(0.82)
                    .brightness(-0.06)
                    .offset(x: g.dx, y: g.dy)
            }

            // Route sits on the globe but behind the phone, so it appears to pass through.
            if let rt = spec.route { routeLayer(rt) }

            // Encryption flow (behind the phones so beams/stream tuck into them).
            if let b = spec.beams { beamsLayer(b) }
            if let s = spec.stream { streamLayer(s) }

            if let device {
                // The renderer bakes the pose into the 3D scene; we place each phone by
                // its center on the canvas and add a soft contact shadow.
                ForEach(Array(allDevices.enumerated()), id: \.offset) { _, d in
                    device.view(shot: shot, spec: d)
                        .shadow(color: d.shadow ? .black.opacity(0.28) : .clear,
                                radius: 60, x: 0, y: 34)
                        .position(x: d.cx, y: d.cy)
                }
            }

            // Padlock + banner labels sit on top of the flow.
            if let l = spec.lock { lockLayer(l) }
            ForEach(Array(spec.banners.enumerated()), id: \.offset) { _, b in
                bannerLayer(b)
            }

            captionLayer
        }
        // Pin to top-leading: oversized layers (e.g. the globe frame is wider than the
        // canvas) must be clipped from the origin, NOT re-centered — otherwise the whole
        // composition, captions included, shifts left by (contentWidth - cw) / 2.
        .frame(width: cw, height: ch, alignment: .topLeading)
        .clipped()
    }

    static func bezier(_ t: Double, _ p0: CGPoint, _ c1: CGPoint, _ c2: CGPoint, _ p1: CGPoint) -> CGPoint {
        let u = 1 - t
        let a = u * u * u, b = 3 * u * u * t, c = 3 * u * t * t, d = t * t * t
        return CGPoint(x: a * p0.x + b * c1.x + c * c2.x + d * p1.x,
                       y: a * p0.y + b * c1.y + c * c2.y + d * p1.y)
    }
    static func bezierTangent(_ t: Double, _ p0: CGPoint, _ c1: CGPoint, _ c2: CGPoint, _ p1: CGPoint) -> CGPoint {
        let u = 1 - t
        let a = 3 * u * u, b = 6 * u * t, c = 3 * t * t
        return CGPoint(x: a * (c1.x - p0.x) + b * (c2.x - c1.x) + c * (p1.x - c2.x),
                       y: a * (c1.y - p0.y) + b * (c2.y - c1.y) + c * (p1.y - c2.y))
    }

    // Transfer route: a glowing arc from the departure city to the arrival city, with a
    // pulsing marker at each end. Drawn on the globe, behind the phone.
    private func routeLayer(_ r: RouteSpec) -> some View {
        let color = Color(hex: r.color)
        let path = Path { p in
            p.move(to: r.from)
            p.addCurve(to: r.to, control1: r.c1, control2: r.c2)
        }
        return ZStack {
            path.stroke(color.opacity(0.55), style: StrokeStyle(lineWidth: r.lineWidth * 2.6, lineCap: .round))
                .blur(radius: 22)
            path.stroke(
                LinearGradient(colors: [Color(hex: "#c98bff"), Color(hex: "#7c3aed"), Color(hex: "#c98bff")],
                               startPoint: .topTrailing, endPoint: .bottomLeading),
                style: StrokeStyle(lineWidth: r.lineWidth, lineCap: .round))
            path.stroke(.white.opacity(0.85), style: StrokeStyle(lineWidth: r.lineWidth * 0.35, lineCap: .round))
                .blur(radius: 1)
            marker(at: r.from, color)
            marker(at: r.to, color)
        }
        .frame(width: canvas.width, height: canvas.height)
    }

    private func marker(at pt: CGPoint, _ color: Color) -> some View {
        ZStack {
            Circle().fill(color.opacity(0.5)).frame(width: 60, height: 60).blur(radius: 14)
            Circle().fill(color).frame(width: 30, height: 30)
            Circle().fill(.white).frame(width: 14, height: 14)
        }
        .position(pt)
    }

    // Converging encryption beams: thin glowing lines fanning from the top phone's edge
    // down to a single convergence point (above the lock).
    private func beamsLayer(_ b: BeamsSpec) -> some View {
        let color = Color(hex: b.color)
        return Canvas { ctx, _ in
            for i in 0..<b.count {
                let f = b.count == 1 ? 0 : Double(i) / Double(b.count - 1) - 0.5   // -0.5…0.5
                // Fan across the cross axis from the start line, converging on (cx, cy).
                let start = b.horizontal
                    ? CGPoint(x: b.y0, y: b.cy + CGFloat(f) * 2 * b.spread)
                    : CGPoint(x: b.cx + CGFloat(f) * 2 * b.spread, y: b.y0)
                var path = Path()
                path.move(to: start)
                path.addLine(to: CGPoint(x: b.cx, y: b.cy))
                let op = 0.25 + 0.35 * (1 - abs(f) * 2)   // brighter toward the centre beam
                ctx.stroke(path, with: .color(color.opacity(op)),
                           style: StrokeStyle(lineWidth: 2.2, lineCap: .round))
            }
        }
        .frame(width: canvas.width, height: canvas.height)
        .blur(radius: 0.6)
        .shadow(color: color.opacity(0.7), radius: 10)
    }

    // Vertical binary "protection" stream from the lock down to the receiving phone.
    private func streamLayer(_ s: StreamSpec) -> some View {
        let color = Color(hex: s.color)
        return Canvas { ctx, _ in
            let spacing = 44.0
            let count = max(1, Int((s.y1 - s.y0) / spacing))
            for i in 0...count {
                let t = Double(i) / Double(count)
                let along = s.y0 + (s.y1 - s.y0) * t
                let at = s.horizontal ? CGPoint(x: along, y: s.cx) : CGPoint(x: s.cx, y: along)
                let bit = (i % 3 == 0) ? "0" : "1"
                // fade in near the lock and out near the receiving device
                let op = 0.5 + 0.5 * sin(Double.pi * t)
                var res = ctx.resolve(Text(bit)
                    .font(.system(size: 30, weight: .semibold, design: .monospaced)))
                res.shading = .color(color.opacity(op))
                ctx.draw(res, at: at)
            }
        }
        .frame(width: canvas.width, height: canvas.height)
        .shadow(color: color.opacity(0.6), radius: 8)
    }

    // Glowing padlock (SF Symbol) — the encryption focal point.
    private func lockLayer(_ l: LockSpec) -> some View {
        let color = Color(hex: l.color)
        return Image(systemName: "lock.fill")
            .font(.system(size: l.size, weight: .regular))
            .foregroundStyle(
                LinearGradient(colors: [.white, color], startPoint: .top, endPoint: .bottom))
            .shadow(color: color.opacity(0.9), radius: 30)
            .shadow(color: color.opacity(0.6), radius: 60)
            .position(x: l.cx, y: l.cy)
    }

    // Localized banner label (CHIFFREMENT / PROTECTION) from strings.json.
    private func bannerLayer(_ b: Banner) -> some View {
        let text = (b.kind == .encryption ? caption.encryption : caption.protection) ?? ""
        return Text(text)
            .font(.system(size: 48, weight: .bold))
            .tracking(3)
            .foregroundStyle(Color(hex: "#e7dcf7"))
            .position(x: b.cx, y: b.cy)
    }

    @ViewBuilder
    private func captionBlock(_ titleSize: CGFloat, _ subSize: CGFloat) -> some View {
        let content = VStack(spacing: 10) {
            Text(caption.title)
                .font(.system(size: titleSize, weight: .heavy))
                .foregroundStyle(titleColor)
            Text(caption.subtitle)
                .font(.system(size: subSize, weight: .semibold))
                .foregroundStyle(subColor)
        }
        .multilineTextAlignment(.center)
        .frame(width: canvas.width - (spec.headerBackdrop ? 300 : 160))

        if spec.headerBackdrop {
            // The panel is the caption's background, so it grows with the content — long
            // translations (Russian, etc.) get a taller/wider backdrop automatically.
            let shape = RoundedRectangle(cornerRadius: 48, style: .continuous)
            content
                .padding(.horizontal, 56)
                .padding(.vertical, 44)
                .background(shape.fill(Color(hex: "#180a30").opacity(0.82))
                    .overlay(shape.stroke(Color.white.opacity(0.16), lineWidth: 1.5)))
                .shadow(color: .black.opacity(0.35), radius: 24, y: 10)
        } else {
            content
        }
    }

    private var captionLayer: some View {
        let top = spec.captionPlace == .top
        // Keep the size for short captions, but shrink long translations (e.g. Russian /
        // Polish / Portuguese wrap both lines) so the taller block still fits its band and
        // never overlaps the phone. ViewThatFits picks the largest variant that fits.
        // The top band is shorter than the bottom one because those screens' phones are
        // large and start high, leaving less room above them.
        let region: CGFloat = top ? (spec.headerBackdrop ? 560 : 290) : 360
        let fitted = ViewThatFits(in: .vertical) {
            captionBlock(104, 60)
            captionBlock(92, 54)
            captionBlock(82, 48)
            captionBlock(72, 44)
        }
        .frame(width: canvas.width, height: region, alignment: top ? .top : .bottom)

        return fitted
            .padding(top ? .top : .bottom, top ? 96 : 110)
            .frame(width: canvas.width, height: canvas.height,
                   alignment: top ? .top : .bottom)
    }
}
