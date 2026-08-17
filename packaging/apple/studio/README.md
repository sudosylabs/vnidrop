# App Store screenshot studio

Code-driven, fully local App Store screenshots — composed natively with **SwiftUI +
`ImageRenderer`** and **SceneKit** (no Typst, no ImageMagick). Each marketing screen is a
SwiftUI view (gradient + 3D device + captions + generated artwork) rendered off-screen to
an exact-size PNG. Captions come from `strings.json` (9 locales); the app screenshots on
the device screens are captured from the real app.

## Requirements
- Xcode / Swift toolchain (macOS 26+). That's it.

## Run
```sh
swift run studio                      # all locales × screens -> generated/<Language>/
swift run studio --publish            # -> ../<Language>/ (ships to App Store)
LOCALES="fr de" SCREENS="share-securely" swift run studio        # subset
PLATFORM=ipad swift run studio        # iPad set -> generated/<Language>/iPad/
PLATFORM=mac swift run studio         # Mac set  -> generated/<Language>/Mac/
```
`PLATFORM` must match between `capture.sh` and `swift run studio` (they read the same
`generated/shots/<platform>/` tree).
Run from this directory (it reads `strings.json` and `assets/` relative to cwd).

## Full pipeline
```sh
./capture.sh        # real localized app screens -> generated/shots/<platform>/<locale>/
swift run studio    # composite everything -> generated/<Language>/[iPad/]
./generate.sh       # capture + composite in one shot (forwards PLATFORM / --publish)
```
`capture.sh` drives the app into each screen via the DEBUG `-VniScreenshot` launch
argument (see `apple/VniDrop/App/ScreenshotSupport.swift`), once per locale via
`-AppleLanguages`, in dark mode. Screenshots are transient (git-ignored, regenerated per
run) — during layout iteration, capture once then re-run `swift run studio` on its own.
- **iPhone / iPad** run in the simulator (9:41 status-bar override) via `simctl`.
- **Mac** has no simulator: `capture.sh` builds the native app, launches the binary
  directly, sizes its window with `osascript`, and grabs it with `screencapture`. Grant
  the terminal **Accessibility + Screen Recording** permission the first time.

## Web hero
The landing-page hero is a separate target: a flat macOS window with the 3D iPhone
overlapping its lower-right, rendered on a **transparent** background so the site
supplies the backdrop. The Mac stays flat (not SceneKit) because its UI text is the
message and perspective would foreshorten it into mush at landing-page widths.

```sh
LOCALES="en" SCENARIOS="compose:web-mac"          PLATFORM=mac   ./capture.sh
LOCALES="en" SCENARIOS="receive-connect:web-iphone" PLATFORM=iphone SETTLE=8 ./capture.sh
LOCALES="en" SCREENS="web-hero" PLATFORM=web SHOTS_DIR="generated/shots/iphone" swift run studio
```
`PLATFORM=web` reads the phone shot from `SHOTS_DIR` and the window shot from
`WINDOW_SHOTS_DIR` (default `generated/shots/mac`), so one composition mixes two
device classes. Output: `generated/<Language>/Web/hero.png`.

`SETTLE` (seconds, default 4) is how long capture.sh waits before grabbing — raise it
for scenarios that present a sheet, or the capture catches it mid-slide.

## Platforms
`PLATFORM=iphone` (default), `ipad`, `mac`, or `web`. Each `Platform`
(`Sources/studio/Platform.swift`) carries its canvas size, capture target, output
subfolder, and a `DeviceModel` — the `.usdz`, its screen material, the orientation fix
(yaw so the screen faces the camera), optional front-glass mesh to hide, whether to tint
the body graphite, a `fillFraction` (how much of the frame the device fills; lower for
wide 3/4 poses that would otherwise clip), and a `screenPad` (black margin baked around
the shot so the window clears the display's rounded corners). Adding a device is a new
`DeviceModel` + `Platform` case + a layout set. (The studio is Apple-only today; the
same SwiftUI/SceneKit approach is intended to extend to Android and Windows later.)

## Device rendering
`SceneKitDeviceRenderer` textures the captured screenshot onto the model's screen mesh and
renders it off-screen. Curvature, bezel and body are the model's real geometry; poses
(pitch/yaw/roll) and a studio environment (IBL + bloom) are applied in the scene. It
auto-generates planar `[0,1]` UVs for screen meshes that ship without them, and mattes the
shot onto a slightly larger black canvas (`screenPad`) so the display edge stays black
instead of smearing the capture's corner pixels.

## Assets (`assets/`)
- `iphone-17-pro-max.usdz`, `ipad-pro.usdz`, `macbook-air.usdz` — the 3D devices
  (committed; CC BY 4.0, see `ATTRIBUTION.md`).
- `globe.png` — for the send-anywhere hero (committed).
- `generated/shots/<platform>/<locale>/*.png` — captured app screens (transient).

## Layout & tuning
- `Sources/studio/ScreenSpec.swift` — per-platform layouts (`iphone` / `ipad` / `mac`):
  gradient, device pose/position/size, globe + route (send-anywhere), the encryption flow
  (stay-private: `beams` → `lock` → `stream` + `banners`), caption placement. `beams` and
  `stream` take a `horizontal` flag for side-by-side devices (used by Mac stay-private).
- `Sources/studio/ScreenFrame.swift` — the composition (layer order, caption fitting,
  generated artwork).

## Screens
`share-securely`, `choose-receivers`, `send-anywhere` (globe + Paris→LA route arc,
reuses the share screenshot), `stay-private` (encryption beams → padlock → binary
protection stream, with localized CHIFFREMENT/PROTECTION banners; two stacked phones on
iPhone/iPad, two side-by-side laptops with a horizontal flow on Mac).
