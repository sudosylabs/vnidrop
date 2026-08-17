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

// A flat window screenshot layer — no 3D. Used by the web hero, where the macOS
// window carries the message and must stay pixel-legible; SceneKit perspective
// would foreshorten the body text into mush at landing-page widths.
struct WindowSpec {
	var width: CGFloat  // rendered width in canvas px (height follows the shot's aspect)
	var cx: CGFloat
	var cy: CGFloat
	// The capture is a screen region, so the desktop shows through the window's
	// rounded corners. Masking with the same radius drops those pixels to alpha.
	var cornerRadius: CGFloat = 20
	var shadow: Bool = true
}

struct GlobeSpec {
	var width: CGFloat
	var dx: CGFloat
	var dy: CGFloat
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
	var cx: CGFloat  // convergence x
	var y0: CGFloat  // start-line coordinate: the y the beams fan from (x when horizontal)
	var spread: CGFloat  // half-extent the beams fan across at the start line
	var cy: CGFloat  // converge to this point
	var count: Int = 22
	var color: String = "#9ec3ff"
	// Horizontal: beams fan across y from a vertical start line and converge left→right
	// (for side-by-side devices) instead of fanning across x from a horizontal line.
	var horizontal: Bool = false
}

struct LockSpec {
	var cx: CGFloat
	var cy: CGFloat
	var size: CGFloat
	var color: String = "#a9c9ff"
}

struct StreamSpec {
	var cx: CGFloat  // cross-axis centre (x for a vertical stream, y for a horizontal one)
	var y0: CGFloat  // along-axis start (below the lock; right of the lock when horizontal)
	var y1: CGFloat  // along-axis end (the receiving device)
	var color: String = "#9ec3ff"
	var horizontal: Bool = false  // flow left→right instead of top→bottom
}

// Localized banner labels (CHIFFREMENT / PROTECTION), text taken from strings.json.
enum BannerKind { case encryption, protection }
struct Banner {
	var kind: BannerKind
	var cx: CGFloat
	var cy: CGFloat
}

struct ScreenSpec {
	let id: String
	// nil renders no background at all, leaving the canvas transparent (web hero).
	let bg: GradientSpec?
	let captionPlace: CaptionPlace
	let captionTheme: CaptionTheme
	// The web hero has no marketing caption — the app UI is the message.
	var showsCaption: Bool = true
	// Flat (non-3D) window screenshots, drawn under the 3D devices.
	var windows: [WindowSpec] = []
	// Which captured shot each window textures, parallel to `windows`.
	var windowShotIds: [String] = []
	var globe: GlobeSpec? = nil
	var route: RouteSpec? = nil
	var device: DeviceSpec? = nil
	var devices: [DeviceSpec] = []  // multiple phones (e.g. stay-private)
	var beams: BeamsSpec? = nil
	var lock: LockSpec? = nil
	var stream: StreamSpec? = nil
	var banners: [Banner] = []
	var headerBackdrop: Bool = false  // dark scrim behind the top caption
	// Which captured screenshot to texture (defaults to `id`). The hero reuses the
	// approval modal shot, matching the original.
	var shotId: String? = nil

	static func all(for platform: Platform) -> [String: ScreenSpec] {
		switch platform {
		case .iphone: iphone
		case .ipad: ipad
		case .mac: mac
		case .web: web
		}
	}

	// Landing-page hero: the macOS composer window with the iPhone overlapping its
	// lower-right, on transparent background. Same composition as the original hero;
	// the Mac stays flat so its UI text is legible at web widths.
	static let web: [String: ScreenSpec] = [
		"web-hero": ScreenSpec(
			id: "web-hero",
			bg: nil,
			captionPlace: .top, captionTheme: .light,
			showsCaption: false,
			windows: [WindowSpec(width: 1960, cx: 1060, cy: 800, cornerRadius: 22)],
			windowShotIds: ["web-mac"],
			device: DeviceSpec(
				height: 1300, cx: 1800, cy: 1150,
				pose: Pose(pitch: 0, yaw: 0, roll: 0)),
			shotId: "web-iphone"
		)
	]

	static let iphone: [String: ScreenSpec] = [
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
					height: 1500, cx: 642, cy: -20,  // top phone, only its lower part shows
					pose: Pose(roll: 180), shadow: false, blackScreen: true),
				DeviceSpec(
					height: 1500, cx: 642, cy: 2820,  // bottom phone, only its upper part shows
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

	// iPad Pro layouts (canvas 2048x2732, centre x = 1024). First pass — same
	// compositions as iPhone, retuned for the squarer, larger canvas.
	static let ipad: [String: ScreenSpec] = [
		"share-securely": ScreenSpec(
			id: "share-securely",
			bg: GradientSpec(stops: ["#f3edfc", "#e7dbf7"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .dark,
			device: DeviceSpec(height: 1950, cx: 1024, cy: 1520, pose: Pose())
		),
		"choose-receivers": ScreenSpec(
			id: "choose-receivers",
			bg: GradientSpec(stops: ["#f2ecfb", "#e6d9f6"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .dark,
			device: DeviceSpec(
				height: 1950, cx: 1000, cy: 1460, pose: Pose(pitch: -9, yaw: -14, roll: 7))
		),
		"send-anywhere": ScreenSpec(
			id: "send-anywhere",
			bg: GradientSpec(stops: ["#e9ddf9", "#e5d6f6"], start: .top, end: .bottom),
			captionPlace: .bottom, captionTheme: .dark,
			globe: GlobeSpec(width: 2000, dx: 24, dy: -240),
			route: RouteSpec(
				from: CGPoint(x: 1342, y: 323), to: CGPoint(x: 315, y: 423),
				c1: CGPoint(x: 2100, y: 2000), c2: CGPoint(x: -100, y: 2000), lineWidth: 14),
			device: DeviceSpec(
				height: 1180, cx: 1000, cy: 1720, pose: Pose(pitch: 12, yaw: 18, roll: -11)),
			shotId: "share-securely"
		),
		"stay-private": ScreenSpec(
			id: "stay-private",
			bg: GradientSpec(
				stops: ["#241047", "#3a1e6b", "#7a5aa8", "#c9b6e6"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .light,
			devices: [
				DeviceSpec(
					height: 1300, cx: 1024, cy: 120, pose: Pose(roll: 180), shadow: false,
					blackScreen: true),
				DeviceSpec(
					height: 1300, cx: 1024, cy: 2760, pose: Pose(), shadow: false, blackScreen: true
				),
			],
			beams: BeamsSpec(cx: 1024, y0: 720, spread: 200, cy: 1220, count: 24),
			lock: LockSpec(cx: 1024, cy: 1400, size: 320),
			stream: StreamSpec(cx: 1024, y0: 1600, y1: 2360),
			banners: [
				Banner(kind: .encryption, cx: 1024, cy: 1200),
				Banner(kind: .protection, cx: 1024, cy: 1680),
			],
			headerBackdrop: true
		),
	]

	// MacBook layouts (canvas 2880x1800, landscape, centre x = 1440). First pass.
	static let mac: [String: ScreenSpec] = [
		"share-securely": ScreenSpec(
			id: "share-securely",
			bg: GradientSpec(stops: ["#f3edfc", "#e7dbf7"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .dark,
			device: DeviceSpec(height: 1500, cx: 1440, cy: 1110, pose: Pose())
		),
		"choose-receivers": ScreenSpec(
			id: "choose-receivers",
			bg: GradientSpec(stops: ["#f2ecfb", "#e6d9f6"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .dark,
			device: DeviceSpec(height: 1500, cx: 1440, cy: 1110, pose: Pose())
		),
		// Globe backdrop in the upper half, route arcing through a centred laptop, caption
		// at the bottom. Reuses the share capture (matches the other platforms).
		"send-anywhere": ScreenSpec(
			id: "send-anywhere",
			bg: GradientSpec(stops: ["#e9ddf9", "#e5d6f6"], start: .top, end: .bottom),
			captionPlace: .bottom, captionTheme: .light,
			globe: GlobeSpec(width: 1320, dx: 780, dy: -150),
			route: RouteSpec(
				from: CGPoint(x: 1650, y: 222), to: CGPoint(x: 976, y: 274),
				c1: CGPoint(x: 5200, y: 1150), c2: CGPoint(x: -2000, y: 1150), lineWidth: 13),
			device: DeviceSpec(height: 1300, cx: 1440, cy: 960, pose: Pose()),
			headerBackdrop: true,
			shotId: "share-securely"
		),
		// Vertical encryption flow between two stacked laptops (same structure as the phone
		// layouts, retuned for the landscape canvas).
		"stay-private": ScreenSpec(
			id: "stay-private",
			bg: GradientSpec(
				stops: ["#241047", "#3a1e6b", "#7a5aa8", "#c9b6e6"], start: .top, end: .bottom),
			captionPlace: .top, captionTheme: .light,
			// Two laptops on the left and right; encryption flows horizontally between them:
			// beams converge left→lock, a binary stream runs lock→right.
			devices: [
				DeviceSpec(
					height: 1040, cx: 120, cy: 1000, pose: Pose(), shadow: false, blackScreen: true),
				DeviceSpec(
					height: 1040, cx: 2760, cy: 1000, pose: Pose(), shadow: false, blackScreen: true
				),
			],
			beams: BeamsSpec(cx: 1290, y0: 760, spread: 300, cy: 1000, count: 26, horizontal: true),
			lock: LockSpec(cx: 1440, cy: 1000, size: 300),
			stream: StreamSpec(cx: 1000, y0: 1600, y1: 2140, horizontal: true),
			banners: [
				Banner(kind: .encryption, cx: 940, cy: 1360),
				Banner(kind: .protection, cx: 1960, cy: 1360),
			]
		),
	]
}
