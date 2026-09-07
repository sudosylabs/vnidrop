[CmdletBinding()]
param([string]$OutputDirectory = (Join-Path $PSScriptRoot '../../build/windows/legacy-release'))
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$directory = [IO.Path]::GetFullPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($directory)
$path = Join-Path $directory 'VniDrop_0.3.3_x64.exe'
# This is the final published Compose installer, including same-version migration coverage.
$expected = '668901f643c69225e4b18fb9284e7bc55d29f2c0eac51522297b26d36fed52ab'
if (!(Test-Path -LiteralPath $path)) {
    Invoke-WebRequest -Uri 'https://github.com/sudosylabs/vnidrop/releases/download/v0.3.3/VniDrop_0.3.3_x64.exe' -OutFile $path
}
if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $expected) {
    throw 'The published Compose installer does not match its pinned release checksum'
}
Write-Output $path
