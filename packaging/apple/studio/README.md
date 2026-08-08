# App Store screenshot studio

Code-driven, fully local App Store screenshots — composed natively with **SwiftUI +
`ImageRenderer`** (no Typst, no ImageMagick). Each frame is a SwiftUI view (gradient
background + globe + device with the app screenshot + localized caption) rendered
off-screen to an exact-size PNG. Captions come from `strings.json` (9 locales); app
screenshots come from the real app (see capture, below).

Why SwiftUI: the screen curvature is the real iOS squircle
(`RoundedRectangle(style: .continuous)`), SF Pro resolves for free, and the hero tilt
is a native `.rotation3DEffect` (real perspective) — no mask-guessing or warp math.

## Requirements
- Xcode / Swift toolchain (macOS 14+). That's it.

## Render
```sh
swift run studio                         # all locales × screens -> generated/<Language>/
swift run studio --publish               # -> ../<Language>/ (ships to App Store)
LOCALES="fr de" SCREENS="share-securely" swift run studio   # subset
```
Default writes to `generated/` (safe). `--publish` overwrites the real language folders.
Run from this directory (it reads `strings.json` and `assets/` relative to cwd).

## Full pipeline
```sh
./capture.sh        # real localized app screens -> assets/shots/<locale>/<screen>.png
swift run studio    # composite everything -> generated/<Language>/
swift run studio --publish   # when happy -> ships to ../<Language>/
```
`capture.sh` drives the app into each screen via the DEBUG `-VniScreenshot` launch
argument (see `apple/VniDrop/App/ScreenshotSupport.swift`), once per locale via
`-AppleLanguages`, in dark mode with a 9:41 status bar.

## Device rendering (3D by default)
The device is a real iPhone 17 Pro Max `.usdz` (`assets/iphone-17-pro-max.usdz`, CC BY
4.0 — see `assets/ATTRIBUTION.md`). `SceneKitDeviceRenderer` textures the app screenshot
onto the screen mesh and renders it off-screen with SceneKit. The screen curvature,
bezel and Dynamic Island are the model's real geometry; the hero tilt is a real camera
perspective (`spec.device.tilt`), not a warp.

Model-specific quirks handled in `SceneKitDeviceRenderer.swift` (re-derive if the model
changes — see the inspection scripts approach in git history):
- **Warm-up render**: `SCNRenderer.snapshot` returns empty on its first call; we render
  a throwaway 16×32 first, then the real frame.
- **Bounding box**: use `rootNode.boundingBox` (the model is ~16 units tall via an
  ancestor scale). `flattenedClone()` collapses this model to nothing — don't use it.
- **Camera orientation**: front is −Z; `look(at:)` renders nothing, so the camera is
  yawed 180° by hand (`eulerAngles`).
- **Crisp screen**: the screen material has a wavy normal map and there's a front glass
  mesh with its own waviness — both ripple the image. We clear the screen normal and
  hide the glass mesh so the app content stays flat/crisp (App Store requirement).

Set `DEVICE=2d` to fall back to the flat mockup compositor (`ClipDeviceRenderer` +
`assets/mockup-straight.png`, `.continuous` clip) if you ever want the non-3D path.

## Assets (git-ignored except the model — you supply the rest)
- `assets/iphone-17-pro-max.usdz` — the 3D device (committed, with `ATTRIBUTION.md`).
- `assets/globe.png` — transparent globe for the hero.
- `assets/shots/<locale>/<screen>.png` — captured app screenshots (from `capture.sh`).
- `assets/mockup-straight.png` — only needed for the `DEVICE=2d` fallback.

## Layout & tuning
- `Sources/studio/ScreenSpec.swift` — per-screen layout (gradient stops, caption
  placement, globe + device position/size, hero tilt). Numbers are in pixels at scale 1.
- `MockupGeometry.straight` in the same file — where the glass sits inside
  `mockup-straight.png` (fractions of the mockup) + corner radius. **Measure once**
  against your mockup export and set these; placeholders are approximate.
- `Sources/studio/ScreenFrame.swift` — the composition (layer order, caption styling).
- `Sources/studio/DeviceView.swift` — the device layer. `ClipDeviceRenderer` does the
  2D `.continuous` clip today; a `SceneKitDeviceRenderer` (texture the shot onto a real
  iPhone `.usdz`, render with `SCNRenderer.snapshot()`) can drop in behind the same
  `DeviceRenderer` protocol for full 3D — nothing else changes.

## Status
1. ✅ SwiftUI compositor: gradient + caption + globe + device, exact 1284×2778, headless.
2. ✅ Real 3D iPhone: screenshot textured onto the `.usdz` screen mesh (SceneKit), crisp.
3. ✅ Real hero tilt via 3D camera perspective (replaces `warp.sh` + rotated mockup).
4. ✅ Screenshot capture: `capture.sh` + the `#if DEBUG` fixture gateway, per locale.
5. ⬜ Tune device position/size/tilt per screen in `ScreenSpec.swift` against the originals
   (esp. `send-anywhere`: caption overlaps the phone; globe layer needs its asset).
6. ⬜ Ribbon art layer for `stay-private`.
7. ✅ 2D fallback (`DEVICE=2d`) retained for the flat mockup path.
