Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-NativeAppImage([string]$Path, [string]$Version) {
    foreach ($asset in @('VniDrop.exe', 'VniDrop.dll', 'VniDrop.Core.dll', 'vnidrop_native.dll',
        'VniDrop.runtimeconfig.json', 'coreclr.dll', 'hostfxr.dll', 'Microsoft.UI.Xaml.dll',
        'Microsoft.WindowsAppRuntime.dll', 'VniDrop.pri', 'App.xbf', 'MainWindow.xbf', 'Views/TransfersPage.xbf')) {
        $file = Join-Path $Path $asset
        if (!(Test-Path -LiteralPath $file -PathType Leaf) -or (Get-Item -LiteralPath $file).Length -eq 0) {
            throw "Native app image is missing $asset"
        }
    }
    $actualVersion = (Get-Item -LiteralPath (Join-Path $Path 'VniDrop.dll')).VersionInfo.ProductVersion.Split('+')[0]
    if ($actualVersion -ne $Version) { throw "Native app version $actualVersion does not match $Version" }
    $runtime = Get-Content -LiteralPath (Join-Path $Path 'VniDrop.runtimeconfig.json') -Raw | ConvertFrom-Json
    if (!$runtime.runtimeOptions.PSObject.Properties['includedFrameworks']) { throw 'The .NET runtime must be self-contained' }
    if ($runtime.runtimeOptions.configProperties.'System.Reflection.Metadata.MetadataUpdater.IsSupported' -ne $false) {
        throw 'The native app must be published in Release configuration'
    }
    $repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
    $releaseCore = Join-Path $repo 'target/x86_64-pc-windows-msvc/release/vnidrop.dll'
    if (!(Test-Path -LiteralPath $releaseCore) -or
        (Get-FileHash -LiteralPath $releaseCore).Hash -ne (Get-FileHash -LiteralPath (Join-Path $Path 'vnidrop_native.dll')).Hash) {
        throw 'The app image must contain the freshly built Release Rust library'
    }
}

function Get-PackagingDotnet {
    $command = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
    $env:DOTNET_ROOT = Join-Path $repo 'build/windows/tools/dotnet'
    $env:DOTNET_CLI_HOME = Join-Path $repo 'build/windows/tools/dotnet-home'
    $path = Join-Path $env:DOTNET_ROOT 'dotnet.exe'
    if (!(Test-Path -LiteralPath $path)) { throw 'Install the .NET 10 SDK before packaging.' }
    return $path
}

function Get-PackagingWix {
    $dotnet = Get-PackagingDotnet
    $repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
    $directory = Join-Path $repo 'build/windows/tools/wix'
    $wix = Join-Path $directory 'wix.exe'
    if (!(Test-Path -LiteralPath $wix)) {
        & $dotnet tool install wix --version 4.0.6 --tool-path $directory --allow-roll-forward | Out-Host
        if ($LASTEXITCODE) { throw 'WiX installation failed' }
    }
    $version = & $wix --version
    if ($LASTEXITCODE -or $version -notmatch '^4\.0\.6\+') { throw "Expected WiX 4.0.6, found $version" }
    $extension = Join-Path $directory '.wix/extensions/WixToolset.Bal.wixext/4.0.6/wixext4/WixToolset.Bal.wixext.dll'
    if (!(Test-Path -LiteralPath $extension)) {
        Push-Location $directory
        try {
            & $wix extension add WixToolset.Bal.wixext/4.0.6 | Out-Host
            if ($LASTEXITCODE) { throw 'WiX bootstrapper extension installation failed' }
        } finally { Pop-Location }
    }
    return @{ Executable = $wix; BalExtension = $extension }
}

Export-ModuleMember -Function Assert-NativeAppImage, Get-PackagingDotnet, Get-PackagingWix
