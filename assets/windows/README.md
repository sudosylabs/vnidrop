# Windows app icons

`app-icon.svg` is the source for the executable, window, installer and MSIX
artwork. Its transparent mark uses a tighter view box than the other platforms
so it stays legible at shell sizes. Keep the gap between its uprights transparent.

Regenerate the PNGs and multi-size ICO from the repository root with Node.js and
Sharp 0.35.4:

```powershell
npm install --no-save --prefix build/windows/icon-tools sharp@0.35.4
$env:NODE_PATH = "$PWD/build/windows/icon-tools/node_modules"
node assets/windows/generate-icons.cjs
pwsh windows/scripts/test-icons.ps1
```

The generator provides exact shell sizes and identical default, dark unplated
and light unplated artwork, plus 100%, 200% and 400% package scales. Transparency
lets the same purple mark work on light and dark surfaces. The native build runs
the icon regression checks with `-Test`.

After changing the artwork, inspect 16–48px icons on light and dark backgrounds,
then check a rebuilt app's taskbar, title bar, Start and Explorer icons. Keep
small details and the drop distinct without adding a background plate.

See Microsoft's [Windows app icon construction guidance](https://learn.microsoft.com/windows/apps/design/iconography/app-icon-construction).
