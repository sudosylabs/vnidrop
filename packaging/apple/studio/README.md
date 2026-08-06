# App Store screenshot studio

Code-driven, fully local App Store screenshots. **Typst** composes each frame
(gradient background + blank device mockup + app screenshot + localized caption)
and renders it to an exact-size PNG. Captions come from `strings.json` (9 locales);
app screenshots come from the real app (see capture, step 3).

## Requirements
- `typst` (0.15+) — installed.
- `imagemagick` — only for the tilted hero shot's perspective warp (step 2). `brew install imagemagick`.

## Render
```sh
./build.sh                                  # all locales × screens -> generated/<Language>/
./build.sh --publish                        # -> ../<Language>/ (ships to App Store)
LOCALES="fr de" SCREENS="share-securely" ./build.sh
```
Default writes to `generated/` (safe). `--publish` overwrites the real language folders.

Single frame while tuning:
```sh
typst compile screens.typ out.png --input locale=fr --input screen=share-securely --ppi 72
```

## Files
- `strings.json` — captions per locale (first-pass translations; review before shipping).
- `screens.typ` — the composition + per-screen layout data (`screens` dict). Tweak numbers, re-run.
- `build.sh` — loops locales × screens; maps locale→folder and screen→filename.
- `assets/mockup-straight.png` — blank straight device (real alpha, black screen).
- `assets/mockup-rotated.png` — blank tilted device (for the hero shot).
- `assets/globe.png` — circle-clipped in Typst (the export has a baked checkerboard, not true alpha).
- `assets/shots/<locale>/<screen>.png` — captured app screenshots (optional; placeholder if absent).

## How the screenshot gets into the phone
The mockup's screen glass is a rectangle. `screens.typ` places the screenshot inset
into that rectangle (`sx`/`sy`/`sr` fractions) with the bezel framing it. Measure those
fractions once against `mockup-straight.png`.

## Full pipeline
```sh
./capture.sh     # real localized app screens -> assets/shots/<locale>/<screen>.png
./build.sh       # composite everything -> generated/<Language>/
./build.sh --publish   # when happy -> ships to ../<Language>/
```
`capture.sh` drives the app into each screen via the DEBUG `-VniScreenshot` launch
argument (see `apple/VniDrop/App/ScreenshotSupport.swift`), once per locale via
`-AppleLanguages`, in dark mode with a 9:41 status bar. Screen ⇄ scenario map:
`share-securely`→`share`, `choose-receivers`→`approval`, `send-anywhere`→`transfer-details`.

## Status
1. ✅ Typst + JSON + straight-mockup compositing.
2. ✅ Tilted hero (`send-anywhere`): `warp.sh` perspective-warps the screenshot onto the
   rotated mockup's screen quad (4 auto-detected corners); `build.sh` runs it automatically.
3. ✅ Transparent globe layer (placed directly — use the alpha export, not a flattened one).
4. ✅ Screenshot capture: `capture.sh` + the `#if DEBUG` fixture gateway, per locale, deterministic.
5. ⬜ Ribbon art layer for `stay-private` (export as its own transparent PNG).
6. ⬜ Layout polish: tune device positions/sizes in `screens.typ` against the originals.
