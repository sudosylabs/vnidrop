param(
    [ValidateSet('Debug','Release')][string]$Configuration = 'Debug',
    [switch]$SkipCore,
    [switch]$Test,
    [switch]$Publish,
    [switch]$Run,
    [string]$ProfileDirectory
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
function Find-Tool([string]$Name, [string]$LocalPath) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $fallback = Join-Path $repo $LocalPath
    if (Test-Path -LiteralPath $fallback) { return $fallback }
    throw "Install $Name before building the native Windows app. See windows/README.md."
}
$dotnet = Find-Tool 'dotnet' 'build/windows/tools/dotnet/dotnet.exe'
$bun = Find-Tool 'bun' 'build/windows/tools/bun-windows-x64/bun.exe'
if ($dotnet.StartsWith((Join-Path $repo 'build'))) { $env:DOTNET_CLI_HOME = Join-Path $repo 'build/windows/tools/dotnet-home' }
$rustProfile = $Configuration.ToLowerInvariant()
Push-Location $repo
try {
    if (!$SkipCore) { & "$PSScriptRoot/build-core.ps1" -Profile $rustProfile }
    & $bun run localization/src/cli.ts generate
    if ($LASTEXITCODE) { throw 'Localization generation failed' }
    if ($Test) {
        & $bun run localization/src/cli.ts validate
        if ($LASTEXITCODE) { throw 'Localization validation failed' }
        & $bun test localization/src/lib/windows-resources.test.ts
        if ($LASTEXITCODE) { throw 'Localization tests failed' }
        Set-Location (Join-Path $repo 'windows')
        & $dotnet test VniDrop.Tests/VniDrop.Tests.csproj -c $Configuration "-p:RustProfile=$rustProfile"
        if ($LASTEXITCODE) { throw 'Windows bridge tests failed' }
    }
    Set-Location (Join-Path $repo 'windows')
    $arguments = @('build', 'VniDrop/VniDrop.csproj', '-c', $Configuration, "-p:RustProfile=$rustProfile")
    if ($Publish) { $arguments[0] = 'publish'; $arguments += @('-o', (Join-Path $repo 'build/windows/publish')) }
    & $dotnet @arguments
    if ($LASTEXITCODE) { throw 'Native Windows build failed' }
    if ($Publish) {
        foreach ($asset in @('VniDrop.exe', 'vnidrop_native.dll', 'VniDrop.pri', 'App.xbf', 'MainWindow.xbf', 'Views/TransfersPage.xbf')) {
            if (!(Test-Path -LiteralPath (Join-Path $repo "build/windows/publish/$asset"))) { throw "Published app is missing $asset" }
        }
    }
    if ($Run) {
        if (!$ProfileDirectory) { $ProfileDirectory = Join-Path $repo 'build/windows/dev-profile' }
        $executable = Join-Path $repo "windows/VniDrop/bin/$Configuration/net10.0-windows10.0.26100.0/win-x64/VniDrop.exe"
        if ($Publish) { $executable = Join-Path $repo 'build/windows/publish/VniDrop.exe' }
        Start-Process -FilePath $executable -ArgumentList @('--profile', ('"' + [IO.Path]::GetFullPath($ProfileDirectory) + '"'))
    }
} finally { Pop-Location }
