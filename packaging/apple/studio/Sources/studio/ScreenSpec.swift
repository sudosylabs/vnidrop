import SwiftUI

// Layout data for each marketing screen. Numbers are in pixels (the canvas renders
// at scale 1, so a value of 104 == 104px). Ported from the old screens.typ `screens`
// dict — tune these against the originals.

extension Color {
	// "#rrggbb"
	init(hex: String) {
		let s = hex.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
		let v = UInt64(s, radix: 16) ?? 0
		self.init(
			.sRGB,
			red: Double((v >> 16) & 0xff) / 255,
			green: Double((v >> 8) & 0xff) / 255,
			blue: Double(v & 0xff) / 255,
			opacity: 1)
	}
}

enum CaptionPlace { case top, bottom }
enum CaptionTheme { case dark, light }

struct GradientSpec {
	let stops: [String]  // hex colors, top→bottom / start→end
	let start: UnitPoint
	let end: UnitPoint

	var gradient: LinearGradient {
		LinearGradient(colors: stops.map { Color(hex: $0) }, startPoint: start, endPoint: end)
	}
}

// A 3D device layer. The screenshot is textured onto the model's screen and the phone
// is posed in real 3D (pitch/yaw/roll), positioned by its center on the canvas and
// sized by its upright height — matching how the originals were laid out.
struct Pose {
	var pitch: CGFloat = 0  // deg, about X (top tips away/toward viewer)
	var yaw: CGFloat = 0  // deg, about Y (turn left/right, reveals a side edge)
	var roll: CGFloat = 0  // deg, about Z (in-plane spin)
}

struct DeviceSpec {
	var height: CGFloat  // upright phone height in canvas px (drives scale)
	var cx: CGFloat  // phone center X on the 1284-wide canvas
	var cy: CGFloat  // phone center Y on the 2778-tall canvas
	var pose: Pose = Pose()
	var shadow: Bool = true  // soft contact shadow under the phone
	var blackScreen: Bool = false  // texture the screen solid black (ignore the shot)
}

struct GlobeSpec {
	var width: CGFloat
	var dx: CGFloat
	var dy: CGFloat
}

// An encrypted-data tunnel between two phones (stay-private). The tube passes THROUGH
// each waypoint in order, with the corners auto-rounded (Catmull-Rom), so an "up, right,
// up" routing is just: the bottom-phone point, a corner, a corner, the top-phone point.
// Generated natively (no asset).
struct RibbonSpec {
	var waypoints: [CGPoint]  // the tube runs through these, in order
	var width: CGFloat        // tube diameter
	var color: String         // hex (glow tint)
}

// A transfer route: two city markers (departure + arrival) on the globe joined by a
// glowing arc that swoops down and passes behind the phone — "send from Paris to LA,
// through your phone". Generated natively.
struct RouteSpec {
	var from: CGPoint  // departure marker (on the globe)
	var to: CGPoint  // arrival marker (on the globe)
	var c1: CGPoint  // Bézier controls — pull down to bulge the arc through the phone
	var c2: CGPoint
	var lineWidth: CGFloat = 12
	var color: String = "#a855f7"
}

// Stay-private redesign: a vertical encryption flow between two stacked phones —
// converging light beams (encryption) → a glowing padlock → a binary stream (protection).
struct BeamsSpec {
	var cx: CGFloat          // convergence x
	var y0: CGFloat          // beams start at the top phone's bottom edge
	var spread: CGFloat      // half-width the beams fan across at y0
	var cy: CGFloat          // converge to this point
	var count: Int = 22
	var color: String = "#9ec3ff"
}

struct LockSpec {
	var cx: CGFloat
	var cy: CGFloat
	var size: CGFloat
	var color: String = "#a9c9ff"
}

struct StreamSpec {
	var cx: CGFloat          // vertical binary stream centre
	var y0: CGFloat          // from (below the lock)
	var y1: CGFloat          // to (the bottom phone)
	var color: String = "#9ec3ff"
}

// Localized banner labels (CHIFFREMENT / PROTECTION), text taken from strings.json.
enum BannerKind { case encryption, protection }
struct Banner { var kind: BannerKind; var cx: CGFloat; var cy: CGFloat }

// A glowing purple orbit ring, drawn behind the globe (which occludes its top) and
// looping in front down toward the phone. Generated natively, no asset.
struct OrbitSpec {
	var cx: CGFloat
	var cy: CGFloat  // ellipse center on the canvas
	var w: CGFloat
	var h: CGFloat  // ellipse size
	var rotation: CGFloat = 0  // deg
	var lineWidth: CGFloat = 16
}

struct ScreenSpec {
	let id: String
	let bg: GradientSpec
	let captionPlace: CaptionPlace
	let captionTheme: CaptionTheme
	var globe: GlobeSpec? = nil
	var orbit: OrbitSpec? = nil
	var route: RouteSpec? = nil
	var device: DeviceSpec? = nil
	var devices: [DeviceSpec] = []  // multiple phones (e.g. stay-private)
	var ribbon: RibbonSpec? = nil
	var beams: BeamsSpec? = nil
	var lock: LockSpec? = nil
	var stream: StreamSpec? = nil
	var banners: [Banner] = []
	var headerBackdrop: Bool = false   // dark scrim behind the top caption
	// Which captured screenshot to texture (defaults to `id`). The hero reuses the
	// approval modal shot, matching the original.
	var shotId: String? = nil

	static let all: [String: ScreenSpec] = [
		// Straight-on hero, large and centered, showing the Share/QR sheet.
		"share-securely": ScreenSpec(
			id: "share-securely",
			bg: GradientSpec(stops: ["#f3edfc", "#e7dbf7"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .dark,
			device: DeviceSpec(
				height: 2300, cx: 642, cy: 1560,
				pose: Pose(pitch: 0, yaw: 0, roll: 0))
		),
		// Tilted hero: turned to reveal the right edge, top sloping down-right.
		"choose-receivers": ScreenSpec(
			id: "choose-receivers",
			bg: GradientSpec(stops: ["#f2ecfb", "#e6d9f6"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .dark,
			device: DeviceSpec(
				height: 2160, cx: 620, cy: 1620,
				pose: Pose(pitch: -9, yaw: -14, roll: 7))
		),
		// Strongly tilted, lying diagonally in the lower-left; globe + orbit above.
		"send-anywhere": ScreenSpec(
			id: "send-anywhere",
			bg: GradientSpec(stops: ["#e9ddf9", "#e5d6f6"], start: .top, end: .bottom),
			captionPlace: .bottom, captionTheme: .dark,
			globe: GlobeSpec(width: 1520, dx: -10, dy: -200),
			route: RouteSpec(
				from: CGPoint(x: 1284 - 289, y: 226),  // departure — Paris (Europe, right of globe)
				to: CGPoint(x: 211, y: 296),  // arrival — Los Angeles (upper-left of globe)
				c1: CGPoint(x: 1500, y: 1980),  // pull the arc down through the phone
				c2: CGPoint(x: -300, y: 1980),
				lineWidth: 12),
			device: DeviceSpec(
				height: 1550, cx: 581, cy: 1600,
				pose: Pose(pitch: 35, yaw: 35, roll: 15)),
			shotId: "share-securely"
		),
		// Two partial phones with black screens + a generated binary data ribbon.
		// Vertical encryption flow: two stacked centred phones, converging beams into a
		// glowing padlock, then a binary "protection" stream down to the receiver.
		"stay-private": ScreenSpec(
			id: "stay-private",
			bg: GradientSpec(
				stops: ["#241047", "#3a1e6b", "#7a5aa8", "#c9b6e6"],
				start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .light,
			devices: [
				DeviceSpec(
					height: 1500, cx: 642, cy: -20,     // top phone, only its lower part shows
					pose: Pose(roll: 180), shadow: false, blackScreen: true),
				DeviceSpec(
					height: 1500, cx: 642, cy: 2820,    // bottom phone, only its upper part shows
					pose: Pose(), shadow: false, blackScreen: true),
			],
			beams: BeamsSpec(cx: 642, y0: 700, spread: 150, cy: 1120, count: 22),
			lock: LockSpec(cx: 642, cy: 1330, size: 300),
			stream: StreamSpec(cx: 642, y0: 1520, y1: 2360),
			banners: [
				Banner(kind: .encryption, cx: 642, cy: 1120),
				Banner(kind: .protection, cx: 642, cy: 1620),
			],
			headerBackdrop: true
		),
	]
}

// Where the glass sits inside the STRAIGHT mockup image, as fractions of the mockup's
// own pixel size, plus the screen corner radius as a fraction of screen width. Measure
// once against assets/mockup-straight.png and set here.
struct MockupGeometry {
	var screenXFrac: CGFloat
	var screenYFrac: CGFloat
	var screenWFrac: CGFloat
	var screenHFrac: CGFloat
	var cornerFrac: CGFloat  // corner radius / screen width

	// Placeholder — tune against the real mockup export.
	static let straight = MockupGeometry(
		screenXFrac: 0.036, screenYFrac: 0.028,
		screenWFrac: 0.928, screenHFrac: 0.944,
		cornerFrac: 0.075
	)
}
