# Screenshot studio

Code-driven marketing images, fully local. Two products come out of here:

| Product | What it is | Output |
|---|---|---|
| **App Store screenshots** | Gradient + 3D device + localized caption, one set per platform | `generated/<Language>/<iPhone\|iPad\|Mac>/` |
| **Web hero** | Flat macOS window + 3D iPhone, transparent background | `generated/<Language>/Web/hero.png` |

Every image is a SwiftUI view rendered off-screen with `ImageRenderer`; devices are real
3D models rendered with SceneKit. **No Typst, no ImageMagick.** The app screens shown on
the devices are captured from the real app, driven into each state by a launch argument.

**Always run from this directory** — `strings.json` and `assets/` are read relative to cwd.

---

## First time

- Xcode / Swift toolchain (macOS 26+). That's it — no brew packages.
- **For Mac captures only:** grant your terminal **Accessibility** *and* **Screen
  Recording** in System Settings → Privacy & Security. You'll be prompted the first time;
  if you decline, captures come out as black rectangles.

---

## I want to…

### …regenerate the web hero

Two captures (different platforms) then one composite:

```sh
LOCALES="en" SCENARIOS="compose:web-mac"             PLATFORM=mac    ./capture.sh
LOCALES="en" SCENARIOS="receive-connect:web-iphone"  PLATFORM=iphone SETTLE=8 ./capture.sh
LOCALES="en" SCREENS="web-hero" PLATFORM=web SHOTS_DIR="generated/shots/iphone" swift run studio
```

→ `generated/English/Web/hero.png`, transparent background.

`SETTLE=8` matters: the connect sheet animates in, and the default 4s catches it
mid-slide with a half-risen sheet and a blurred backdrop.

### …regenerate the whole App Store set

```sh
./generate.sh                          # iPhone (default platform)
PLATFORM=ipad ./generate.sh
PLATFORM=mac  ./generate.sh
```

Slow — it rebuilds the app and drives it once per locale × screen (9 locales).

### …ship what I just made to the App Store

```sh
./generate.sh --publish                # -> ../<Language>/<Platform>/
```

Writes to `packaging/apple/<Language>/`, which is what gets uploaded. Without
`--publish` everything stays in `generated/` (git-ignored).

### …tweak a layout without waiting for captures

Capture once, then re-composite as often as you like — the shots persist on disk:

```sh
./capture.sh                           # once
swift run studio                       # after each edit to ScreenSpec.swift
```

This is the loop you want while nudging positions. A composite is ~1s; a capture pass
is minutes.

### …work on one screen in one language

```sh
LOCALES="fr" SCREENS="share-securely" swift run studio
```

---

## Environment variables

| Var | Default | Applies to | Meaning |
|---|---|---|---|
| `PLATFORM` | `iphone` | both | `iphone` · `ipad` · `mac` · `web` |
| `LOCALES` | all 9 | both | Space-separated: `en fr de es it nl pl pt ru` |
| `SCREENS` | all 4 App Store screens | studio | Which screens to composite |
| `SCENARIOS` | `share:share-securely approval:choose-receivers` | capture | `scenario:screen` pairs to capture |
| `SETTLE` | `4` | capture | Seconds to wait before grabbing. **Raise for sheets.** |
| `SHOTS_DIR` | `generated/shots/$PLATFORM` | both | Where device screenshots live |
| `WINDOW_SHOTS_DIR` | `generated/shots/mac` | studio | Where flat window shots live (web hero) |
| `SCREENSHOT_DEVICE` | per platform | capture | Override the simulator model |

**`PLATFORM` must match between `capture.sh` and `swift run studio`** — they read the
same `generated/shots/<platform>/` tree. Mismatch it and the studio silently renders
devices with blank or stale screens. The web hero is the deliberate exception: it reads
two trees at once.

---

## Scenarios

`capture.sh` drives the app into a state with the DEBUG `-VniScreenshot <scenario>`
launch argument — no UI automation. Scenarios are defined in
`apple/VniDrop/App/ScreenshotSupport.swift`; a fixture `CoreGateway` supplies
deterministic content so captures are stable and localized.

| Scenario | Stages | Used by |
|---|---|---|
| `share` | Share / QR panel | `share-securely`, `send-anywhere` |
| `approval` | Receive-request modal | `choose-receivers` |
| `transfer-details` | Transfer detail view | — |
| `compose` | Composer sheet, one file staged | web hero (Mac) |
| `receive-connect` | Receive + "How would you like to connect?" | web hero (iPhone) |

Captures require a **Debug** build — the fixture gateway is inside `#if DEBUG`.

Two things the screenshot path deliberately overrides, both truthful about shipping
behavior rather than the capture host:

- **QR and NFC report available.** The simulator has no camera or NFC radio, so a real
  capture greys out two of three connect methods that work on every supported iPhone.
- **macOS clears first responder.** AppKit makes the sheet's first text field key and
  selects its contents, which reads as a live edit in a marketing shot.

---

## Screens

| Screen | Notes |
|---|---|
| `share-securely` | Straight-on hero |
| `choose-receivers` | Tilted, reveals the right edge |
| `send-anywhere` | Globe + Paris→LA route arc; reuses the share capture via `shotId` |
| `stay-private` | Encryption beams → padlock → binary stream, localized banners. Two stacked phones (iPhone/iPad), two side-by-side laptops with horizontal flow (Mac). Black screens — no capture needed. |
| `web-hero` | Flat Mac window + 3D iPhone, transparent, no caption |

Output filenames come from `nameFor` in `main.swift` (e.g. `share-securely` →
`Share Securely.png`), matching the existing App Store filenames.

---

## Adding a screen

1. **New app state?** Add a case to `ScreenshotScenario` and drive it in `RootView`'s
   DEBUG `.task`, then add it to `SCENARIOS`.
2. Add a `ScreenSpec` to the right platform dictionary in `ScreenSpec.swift`.
3. Add its caption to `strings.json` for all 9 locales — unless `showsCaption: false`.
4. Add an output name to `nameFor` in `main.swift`.

Adding a **device** is a new `DeviceModel` + `Platform` case + a layout set.

---

## How it fits together

```
capture.sh ──> generated/shots/<platform>/<locale>/<screen>.png   (transient, git-ignored)
                     │
swift run studio ────┴──> generated/<Language>/<Platform>/<Name>.png
                              │
              --publish ──────┴──> ../<Language>/<Platform>/       (ships)
```

- **iPhone / iPad** capture in the simulator via `simctl`, with a 9:41 status bar
  override, dark mode, full battery.
- **Mac** has no simulator: `capture.sh` builds the native app, launches the binary
  directly (avoids LaunchServices `-1712` for DerivedData apps), sizes the window with
  `osascript`, and grabs the region with `screencapture`. The desktop shows through the
  window's rounded corners; the studio masks those to alpha when compositing.

### Layout & tuning

- `Sources/studio/ScreenSpec.swift` — per-platform layouts: gradient (`nil` = transparent),
  device pose/position/size, flat `windows`, globe + route, the encryption flow
  (`beams` → `lock` → `stream` + `banners`), caption placement.
- `Sources/studio/ScreenFrame.swift` — layer order, caption fitting, generated artwork.
- `Sources/studio/Platform.swift` — canvas size, device model, output subfolder.

### Device rendering

`SceneKitDeviceRenderer` textures the capture onto the model's screen mesh and renders
off-screen. Curvature, bezel and body are real geometry; poses and a studio environment
(IBL + bloom) come from the scene. It auto-generates planar `[0,1]` UVs for screen meshes
shipped without them, and mattes the shot onto a slightly larger black canvas (`screenPad`)
so the display edge stays black instead of smearing the capture's corner pixels.

The web hero's Mac is **deliberately not** a 3D model: there the app's UI text is the
message, and perspective foreshortening makes body copy unreadable at landing-page widths.

### Assets (`assets/`)

- `iphone-17-pro-max.usdz`, `ipad-pro.usdz`, `macbook-air.usdz` — 3D devices
  (committed; CC BY 4.0, see `ATTRIBUTION.md`)
- `globe.png` — send-anywhere hero (committed)
- `generated/` — all output and captures (git-ignored, regenerated per run)

The studio is Apple-only today; the same SwiftUI/SceneKit approach is intended to extend
to Android and Windows later.
