// App Store screenshot composition.
// Rendered per (locale, screen) via:
//   typst compile screens.typ out.png --input locale=fr --input screen=share-securely --ppi 72
// Canvas is 1284x2778pt -> at 72ppi that's exactly 1284x2778px.

#let strings = json("strings.json")

// ---- inputs (with dev-friendly defaults) --------------------------------
#let locale = sys.inputs.at("locale", default: "en")
#let screen-id = sys.inputs.at("screen", default: "share-securely")
// A pre-framed device PNG (mockup + screenshot, produced by frame.sh), or "none"
// to show the empty mockup. Screen curvature comes from the mockup mask, not Typst.
#let device-image = sys.inputs.at("device", default: "none")

#let cap = strings.locales.at(locale).at(screen-id)

// ---- canvas -------------------------------------------------------------
#let CW = 1284pt
#let CH = 2778pt

// ---- per-screen layout data --------------------------------------------
#let screens = (
  "choose-receivers": (
    bg: gradient.linear(angle: 160deg, rgb("#f1eafc"), rgb("#e6d8fb"), rgb("#d9c4f7")),
    text: (place: "top", theme: "dark"),
    device: (mockup: "straight", w: 860pt, dx: 212pt, dy: 560pt, rot: 4deg),
  ),
  "share-securely": (
    bg: gradient.linear(angle: 165deg, rgb("#efe7fb"), rgb("#e3d3f8"), rgb("#d5bff4")),
    text: (place: "top", theme: "dark"),
    device: (mockup: "straight", w: 820pt, dx: 232pt, dy: 470pt, rot: 0deg),
  ),
  "send-anywhere": (
    bg: gradient.linear(dir: ttb, rgb("#e9ddf9"), rgb("#ddc9f4")),
    text: (place: "bottom", theme: "dark"),
    globe: (w: 1180pt, dx: 52pt, dy: -160pt),
    device: (mockup: "rotated", w: 900pt, dx: 90pt, dy: 980pt, rot: 0deg),
  ),
  "stay-private": (
    bg: gradient.linear(dir: ttb, rgb("#2a0f4d"), rgb("#6b3fa0"), rgb("#e9dcf8"), rgb("#ddc7f4")),
    text: (place: "top", theme: "light"),
  ),
)

#let cfg = screens.at(screen-id)

#set page(width: CW, height: CH, margin: 0pt, fill: cfg.bg)
#set text(font: ("SF NS", "Helvetica Neue")) // "SF NS" is macOS San Francisco (= SF Pro)

// ---- compose ------------------------------------------------------------

// globe (transparent PNG)
#if "globe" in cfg [
  #place(top + left, dx: cfg.globe.dx, dy: cfg.globe.dy,
    image("assets/globe.png", width: cfg.globe.w))
]

// device: a pre-framed PNG (frame.sh) when available, else the empty mockup.
#if "device" in cfg [
  #let d = cfg.device
  #let img = if device-image != "none" { device-image } else { "assets/mockup-" + d.mockup + ".png" }
  #place(top + left, dx: d.dx, dy: d.dy,
    rotate(d.rot, origin: center, image(img, width: d.w)))
]

// caption
#let theme-color = if cfg.text.theme == "light" { white } else { rgb("#1b1226") }
#let sub-color = if cfg.text.theme == "light" { rgb("#f3ecfb") } else { rgb("#2c2138") }
#let caption = align(center)[
  #text(size: 104pt, weight: 800, fill: theme-color)[#cap.title]
  #v(10pt, weak: true)
  #text(size: 60pt, weight: 600, fill: sub-color)[#cap.subtitle]
]
#if cfg.text.place == "top" [
  #place(top + center, dy: 96pt, box(width: CW - 160pt, caption))
] else [
  #place(bottom + center, dy: -220pt, box(width: CW - 160pt, caption))
]
