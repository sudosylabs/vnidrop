[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$module = Import-Module "$PSScriptRoot/Packaging.psm1" -Force -PassThru
$dotnet = Get-PackagingDotnet
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$testRoot = Join-Path $repo 'build/windows/packaging-tool-tests'
$stage = Join-Path $testRoot ([guid]::NewGuid().ToString('N'))
$moduleDirectory = Join-Path $stage 'packaging/windows'
[IO.Directory]::CreateDirectory($moduleDirectory) | Out-Null
$copy = Join-Path $moduleDirectory 'PackagingUnderTest.psm1'
Copy-Item -LiteralPath "$PSScriptRoot/Packaging.psm1" -Destination $copy
$oldPath = $env:PATH
$testModule = $null
try {
    $env:PATH = [IO.Path]::GetDirectoryName($dotnet) + [IO.Path]::PathSeparator + $env:PATH
    $testModule = Import-Module $copy -PassThru
    foreach ($scenario in @('fresh installation', 'existing installation')) {
        $result = @(& $testModule { Get-PackagingWix })
        if ($result.Count -ne 1 -or $result[0] -isnot [hashtable]) {
            throw "$scenario returned setup output alongside the tool descriptor ($($result.Count) objects)"
        }
        $tools = $result[0]
        foreach ($path in @($tools.Executable, $tools.BalExtension)) {
            if (!$path.StartsWith($stage + '\', [StringComparison]::OrdinalIgnoreCase) -or
                !(Test-Path -LiteralPath $path -PathType Leaf)) {
                throw "$scenario did not resolve an installed tool inside the isolated test directory"
            }
        }
        $version = & $tools.Executable --version
        if ($LASTEXITCODE -or $version -notmatch '^4\.0\.6\+') { throw "$scenario returned an unusable WiX executable" }
        Write-Host "PASS: $scenario returns one usable packaging tool descriptor."
    }
} finally {
    $env:PATH = $oldPath
    if ($testModule) { Remove-Module $testModule }
    Remove-Module $module
    $resolved = (Resolve-Path -LiteralPath $stage).Path
    if (!$resolved.StartsWith($testRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetFileName($resolved) -notmatch '^[a-f0-9]{32}$') {
        throw 'Unsafe packaging tool test cleanup path'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
