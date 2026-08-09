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
```
Run from this directory (it reads `strings.json` and `assets/` relative to cwd).

## Full pipeline
```sh
./capture.sh        # real localized app screens -> generated/shots/<platform>/<locale>/
swift run studio    # composite everything -> generated/<Language>/[iPad/]
./generate.sh       # capture + composite in one shot (forwards PLATFORM / --publish)
```
`capture.sh` drives the app into each screen via the DEBUG `-VniScreenshot` launch
argument (see `apple/VniDrop/App/ScreenshotSupport.swift`), once per locale via
`-AppleLanguages`, in dark mode with a 9:41 status bar. Screenshots are transient
(git-ignored, regenerated per run) — during layout iteration, capture once then re-run
`swift run studio` on its own.

## Platforms
`PLATFORM=iphone` (default) or `ipad`. Each `Platform` (`Sources/studio/Platform.swift`)
carries its canvas size, capture simulator, output subfolder, and a `DeviceModel` — the
`.usdz`, its screen material, the orientation fix (yaw so the screen faces the camera),
optional front-glass mesh to hide, and whether to tint the body graphite. Adding a device
(e.g. Mac) is a new `DeviceModel` + `Platform` case + a layout set.

## Device rendering
`SceneKitDeviceRenderer` textures the captured screenshot onto the model's screen mesh and
renders it off-screen. Curvature, bezel and body are the model's real geometry; poses
(pitch/yaw/roll) and a studio environment (IBL + bloom) are applied in the scene. It
auto-generates planar UVs for screen meshes that ship without them.

## Assets (`assets/`)
- `iphone-17-pro-max.usdz`, `ipad-pro.usdz` — the 3D devices (committed; CC BY 4.0, see
  `ATTRIBUTION.md`).
- `globe.png` — for the send-anywhere hero (committed).
- `generated/shots/<platform>/<locale>/*.png` — captured app screens (transient).

## Layout & tuning
- `Sources/studio/ScreenSpec.swift` — per-platform layouts (`iphone` / `ipad`): gradient,
  device pose/position/size, globe + route (send-anywhere), the encryption flow
  (stay-private: `beams` → `lock` → `stream` + `banners`), caption placement.
- `Sources/studio/ScreenFrame.swift` — the composition (layer order, caption fitting,
  generated artwork).

## Screens
`share-securely`, `choose-receivers`, `send-anywhere` (globe + Paris→LA route arc,
reuses the share screenshot), `stay-private` (two stacked phones + encryption beams →
padlock → binary protection stream, with localized CHIFFREMENT/PROTECTION banners).
