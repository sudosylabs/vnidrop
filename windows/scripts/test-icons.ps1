[CmdletBinding()]
param([string]$RepositoryRoot = (Join-Path $PSScriptRoot '../..'))
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$repo = [IO.Path]::GetFullPath($RepositoryRoot)

function Assert-Mark([Drawing.Bitmap]$Image, [string]$Label) {
    if ($Image.Width -ne $Image.Height) { throw "${Label}: icon must be square" }
    # The gap between the uprights must expose the shell, including inside a rounded backplate.
    for ($y = [int]($Image.Height * 0.12); $y -lt $Image.Height * 0.3; $y++) {
        for ($x = [int]($Image.Width * 0.4); $x -lt $Image.Width * 0.6; $x++) {
            if ($Image.GetPixel($x, $y).A -ne 0) { throw "${Label}: the mark's negative space contains a background" }
        }
    }
    $top = $Image.Height
    $bottom = -1
    for ($y = 0; $y -lt $Image.Height; $y++) {
        for ($x = 0; $x -lt $Image.Width; $x++) {
            if ($Image.GetPixel($x, $y).A -lt 128) { continue }
            $top = [Math]::Min($top, $y)
            $bottom = [Math]::Max($bottom, $y)
            if ($x -eq 0 -or $y -eq 0 -or $x -eq $Image.Width - 1 -or $y -eq $Image.Height - 1) {
                throw "${Label}: artwork touches the icon boundary"
            }
        }
    }
    $height = ($bottom - $top + 1) / $Image.Height
    if ($height -lt 0.78 -or $height -gt 0.94) { throw "${Label}: mark is too small or lacks shell padding ($height)" }
}

$assets = Join-Path $repo 'packaging/windows/Assets'
$pngs = @(Join-Path $repo 'assets/windows/app-icon.png') + @(Get-ChildItem -LiteralPath $assets -Filter '*.png' | Select-Object -ExpandProperty FullName)
foreach ($file in $pngs) {
    $bitmap = [Drawing.Bitmap]::new($file)
    try { Assert-Mark $bitmap ([IO.Path]::GetFileName($file)) } finally { $bitmap.Dispose() }
}

$bytes = [IO.File]::ReadAllBytes((Join-Path $repo 'assets/windows/app-icon.ico'))
if ([BitConverter]::ToUInt16($bytes, 0) -ne 0 -or [BitConverter]::ToUInt16($bytes, 2) -ne 1) { throw 'Invalid ICO header' }
$sizes = @()
for ($index = 0; $index -lt [BitConverter]::ToUInt16($bytes, 4); $index++) {
    $entry = 6 + $index * 16
    $size = if ($bytes[$entry] -eq 0) { 256 } else { [int]$bytes[$entry] }
    $sizes += $size
    $length = [BitConverter]::ToUInt32($bytes, $entry + 8)
    $offset = [BitConverter]::ToUInt32($bytes, $entry + 12)
    $stream = [IO.MemoryStream]::new($bytes, $offset, $length)
    try {
        $bitmap = [Drawing.Bitmap]::new($stream)
        try {
            if ($bitmap.Width -ne $size) { throw "ICO ${size}px: incorrect frame dimensions" }
            Assert-Mark $bitmap "ICO ${size}px"
        } finally { $bitmap.Dispose() }
    } finally { $stream.Dispose() }
}
foreach ($size in @(16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256)) {
    if ($sizes -notcontains $size) { throw "ICO is missing the ${size}px frame" }
    foreach ($theme in @('', '_altform-unplated', '_altform-lightunplated')) {
        $file = Join-Path $assets "Square44x44Logo.targetsize-${size}${theme}.png"
        if (!(Test-Path -LiteralPath $file)) { throw "Missing shell theme asset: $file" }
        $bitmap = [Drawing.Bitmap]::new($file)
        try {
            if ($bitmap.Width -ne $size) { throw "${file}: incorrect target size" }
        } finally { $bitmap.Dispose() }
    }
}
Write-Host "PASS: $($pngs.Count) PNGs, $($sizes.Count) ICO frames, transparent negative space, mark sizing and shell theme variants."
