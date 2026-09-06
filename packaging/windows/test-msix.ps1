[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Package,
    [switch]$EnableDeveloperMode
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$packagePath = (Resolve-Path -LiteralPath $Package).Path
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$stage = Join-Path $repo ('build/windows/msix-test/' + [guid]::NewGuid().ToString('N'))
$layout = Join-Path $stage 'package'
$profile = Join-Path $stage 'profile with spaces'
[IO.Directory]::CreateDirectory($profile) | Out-Null
Set-Content -LiteralPath (Join-Path $profile 'windows-preferences.json') -Value '{"Username":"MSIX test","RelayMode":3}'
if (Get-AppxPackage -Name 'SudosyLabs.Vnidrop') { throw 'MSIX test requires an account without an installed VniDrop Store package' }
$developerKey = 'HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/AppModelUnlock'
$oldDeveloperMode = Get-ItemPropertyValue -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue
if ($oldDeveloperMode -ne 1 -and !$EnableDeveloperMode) { throw 'Loose-package testing needs Developer Mode. Use an isolated Windows test machine.' }
$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin/10.0.26100.0/x64/MakeAppx.exe'
& $sdk unpack /p $packagePath /d $layout /o | Out-Null
if ($LASTEXITCODE) { throw 'MSIX test extraction failed' }
# Development registration uses the extracted payload; the Store upload remains unsigned and unchanged.
Remove-Item -LiteralPath (Join-Path $layout 'AppxBlockMap.xml') -Force
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VniDropPackageTest {
    [ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IApplicationActivationManager {
        [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appId,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, out uint processId);
    }
    public static uint Launch(string appId, string arguments) {
        var manager = (IApplicationActivationManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")));
        try { uint processId; Marshal.ThrowExceptionForHR(manager.ActivateApplication(appId, arguments, 0, out processId)); return processId; }
        finally { Marshal.ReleaseComObject(manager); }
    }
}
'@
$registered = $null
$process = $null
try {
    if ($oldDeveloperMode -ne 1) {
        [void](New-Item -Path $developerKey -Force)
        Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value 1 -Type DWord
    }
    Add-AppxPackage -Register (Join-Path $layout 'AppxManifest.xml')
    $registered = Get-AppxPackage -Name 'SudosyLabs.Vnidrop'
    if (!$registered) { throw 'MSIX registration did not create the expected Store identity' }
    $manifest = Get-AppxPackageManifest -Package $registered.PackageFullName
    if (!$manifest.SelectSingleNode('//*[local-name()="FileType" and text()=".vnd"]')) { throw 'Installed MSIX is missing its invitation association' }
    $processId = [VniDropPackageTest]::Launch(($registered.PackageFamilyName + '!VniDrop'), ('--profile "' + $profile + '"'))
    $process = Get-Process -Id $processId
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    $createTransfer = $null
    $root = $null
    do {
        Start-Sleep -Milliseconds 200
        $process.Refresh()
        if ($process.HasExited) { throw 'Packaged WinUI app exited during startup' }
        if ($process.MainWindowHandle -ne 0) {
            $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
            $createTransfer = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'EmptyCreateTransfer'))
        }
    } while ((!$createTransfer -or $createTransfer.Current.IsOffscreen) -and $deadline.Elapsed.TotalSeconds -lt 30)
    if (!$createTransfer -or $createTransfer.Current.IsOffscreen -or !$createTransfer.Current.Name -or $createTransfer.Current.Name -match '^[a-z]+_[a-z_]+$') {
        if ($root) {
            $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
                Where-Object { !$_.Current.IsOffscreen -and $_.Current.Name } |
                Select-Object -First 30 | ForEach-Object { Write-Host ($_.Current.AutomationId + ': ' + $_.Current.Name) }
        }
        throw 'Packaged WinUI startup or localization did not load'
    }
    [void]$process.CloseMainWindow()
    if (!$process.WaitForExit(15000)) { throw 'Packaged WinUI app did not shut down cleanly' }
    $process = $null
    Write-Host 'PASS: MSIX registration, Store identity, .vnd declaration, packaged activation, WinUI resources and localized startup.'
} finally {
    if ($process -and !$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    if ($registered) { Remove-AppxPackage -Package $registered.PackageFullName }
    if ($oldDeveloperMode -ne 1 -and $EnableDeveloperMode) {
        if ($null -eq $oldDeveloperMode) { Remove-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue }
        else { Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value $oldDeveloperMode -Type DWord }
    }
}
