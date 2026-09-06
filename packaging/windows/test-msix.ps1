[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Package,
    [switch]$EnableDeveloperMode
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$packagePath = (Resolve-Path -LiteralPath $Package).Path
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Import-Module "$PSScriptRoot/Packaging.psm1" -Force
$stage = Join-Path $repo ('build/windows/msix-test/' + [guid]::NewGuid().ToString('N'))
$layout = Join-Path $stage 'package'
$profile = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.vnidrop'
if (Get-AppxPackage -Name 'SudosyLabs.Vnidrop') { throw 'MSIX test requires an account without an installed VniDrop Store package' }
if (Test-Path -LiteralPath $profile) { throw 'MSIX notification tests require a clean account without an existing default VniDrop profile' }
$developerKey = 'HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/AppModelUnlock'
$oldDeveloperMode = Get-ItemPropertyValue -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue
if ($oldDeveloperMode -ne 1 -and !$EnableDeveloperMode) { throw 'Loose-package testing needs Developer Mode. Use an isolated Windows test machine.' }
Add-Type -Path "$PSScriptRoot/StandardUserProcess.cs"
$integrity = [StandardUserProcess]::IntegrityLevel($PID)
Write-Host "MSIX test process integrity: $integrity"
if ($integrity -ge 0x3000) {
    if (!$EnableDeveloperMode) { throw 'Notification activation must be tested from a non-elevated PowerShell process' }
    [void][IO.Directory]::CreateDirectory($stage)
    $log = Join-Path $stage 'activation.log'
    $script = @'
$ErrorActionPreference = 'Stop'
try {{ & '{0}' -Package '{1}' *> '{2}'; exit 0 }}
catch {{ $_ | Out-String | Add-Content -LiteralPath '{2}'; exit 1 }}
'@ -f $PSCommandPath.Replace("'", "''"), $packagePath.Replace("'", "''"), $log.Replace("'", "''")
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
    $child = $null
    try {
        if ($oldDeveloperMode -ne 1) {
            [void](New-Item -Path $developerKey -Force)
            Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value 1 -Type DWord
        }
        $child = [StandardUserProcess]::Start("$env:WINDIR/System32/WindowsPowerShell/v1.0/powershell.exe", "-NoProfile -NonInteractive -EncodedCommand $encoded", $repo)
        if (!$child.WaitForExit(240000)) { throw 'Non-elevated MSIX activation test timed out' }
        if ($child.ExitCode) { throw "Non-elevated MSIX activation test failed with exit code $($child.ExitCode)" }
    } finally {
        if ($child) {
            if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }
            $child.Dispose()
        }
        if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log | Write-Host }
        if ($oldDeveloperMode -ne 1) {
            if ($null -eq $oldDeveloperMode) { Remove-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue }
            else { Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value $oldDeveloperMode -Type DWord }
        }
    }
    return
}
if ($integrity -ne 0x2000) { throw "MSIX activation tests require medium integrity, got $integrity" }
$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin/10.0.26100.0/x64/MakeAppx.exe'
& $sdk unpack /p $packagePath /d $layout /o | Out-Null
if ($LASTEXITCODE) { throw 'MSIX test extraction failed' }
[xml]$testManifest = Get-Content -LiteralPath (Join-Path $layout 'AppxManifest.xml') -Raw
Assert-NotificationRegistration $testManifest
$notificationActivation = $testManifest.SelectSingleNode('//*[local-name()="ToastNotificationActivation"]')
$notificationClsid = [guid]$notificationActivation.GetAttribute('ToastActivatorCLSID')
# The SDK matches the COM activation argument exactly; cold activation uses the
# default profile on this clean account, without modifying the production manifest.
# Development registration uses the extracted payload; the Store upload remains unsigned and unchanged.
Remove-Item -LiteralPath (Join-Path $layout 'AppxBlockMap.xml') -Force
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VniDropPackageTest {
    [ComImport, Guid("53E31837-6600-4A81-9395-75CFFE746F94"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface INotificationActivationCallback {
        void Activate([MarshalAs(UnmanagedType.LPWStr)] string appId,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, IntPtr data, uint count);
    }
    public static void Notify(Guid clsid, string appId) {
        var callback = (INotificationActivationCallback)Activator.CreateInstance(Type.GetTypeFromCLSID(clsid));
        try { callback.Activate(appId, "", IntPtr.Zero, 0); }
        finally { Marshal.ReleaseComObject(callback); }
    }
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
$profileCreated = $false
try {
    [void][IO.Directory]::CreateDirectory($profile)
    $profileCreated = $true
    Set-Content -LiteralPath (Join-Path $profile 'windows-preferences.json') -Value '{"Username":"MSIX test","RelayMode":3}'
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
    $appIntegrity = [StandardUserProcess]::IntegrityLevel($processId)
    Write-Host "Packaged app integrity: $appIntegrity"
    if ($appIntegrity -ne 0x2000) { throw 'Packaged notification test launched an app without standard-user integrity' }
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
    $appId = $registered.PackageFamilyName + '!VniDrop'
    Write-Host 'Testing notification activation with the app running.'
    [VniDropPackageTest]::Notify($notificationClsid, $appId)
    $process.Refresh()
    if ($process.HasExited -or !$process.Responding) { throw 'Notification activation did not reach the running app' }
    [void]$process.CloseMainWindow()
    if (!$process.WaitForExit(15000)) { throw 'Packaged WinUI app did not shut down cleanly' }
    $process = $null
    Write-Host 'Testing notification activation after the app exits.'
    [VniDropPackageTest]::Notify($notificationClsid, $appId)
    $deadline.Restart()
    do {
        Start-Sleep -Milliseconds 200
        $instances = @(Get-Process VniDrop -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $layout 'VniDrop.exe') })
        if ($instances.Count -eq 1) { $process = $instances[0] }
    } while ((!$process -or $process.MainWindowHandle -eq 0) -and $deadline.Elapsed.TotalSeconds -lt 30)
    if (!$process -or $instances.Count -ne 1 -or $process.MainWindowHandle -eq 0 -or !$process.Responding) { throw 'Cold notification activation did not open one responsive WinUI window' }
    if ([StandardUserProcess]::IntegrityLevel($process.Id) -ne 0x2000) { throw 'Cold notification activation did not retain standard-user integrity' }
    [VniDropPackageTest]::Notify($notificationClsid, $appId)
    [void]$process.CloseMainWindow()
    if (!$process.WaitForExit(15000)) { throw 'Notification-activated app did not shut down cleanly' }
    $process = $null
    Write-Host 'PASS: MSIX registration, Store identity, .vnd declaration, WinUI resources, localized startup, and warm/cold notification COM activation.'
} finally {
    if ($process -and !$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    if ($registered) { Remove-AppxPackage -Package $registered.PackageFullName }
    if ($profileCreated -and (Test-Path -LiteralPath $profile)) {
        $resolvedProfile = (Resolve-Path -LiteralPath $profile).Path
        $retainedProfile = [IO.Path]::GetFullPath((Join-Path $stage 'profile'))
        if ($resolvedProfile -ne [IO.Path]::GetFullPath($profile) -or
            (Get-Item -LiteralPath $profile).Attributes -band [IO.FileAttributes]::ReparsePoint -or
            !$retainedProfile.StartsWith([IO.Path]::GetFullPath($stage) + [IO.Path]::DirectorySeparatorChar)) {
            throw 'Unsafe MSIX test profile cleanup path'
        }
        Move-Item -LiteralPath $resolvedProfile -Destination $retainedProfile
    }
    if ($oldDeveloperMode -ne 1 -and $EnableDeveloperMode) {
        if ($null -eq $oldDeveloperMode) { Remove-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue }
        else { Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value $oldDeveloperMode -Type DWord }
    }
}
