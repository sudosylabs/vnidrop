param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VniDropSmokeInput {
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extraInfo);
    public static void Click(IntPtr window, int x, int y) {
        SetForegroundWindow(window);
        SetCursorPos(x, y);
        mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
        mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    }
}
'@
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$profile = Join-Path $repo ('build/windows/smoke/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $profile -Force | Out-Null
@{ RelayMode = 3; ReceiveDirectory = (Join-Path $profile 'received') } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $profile 'windows-preferences.json')
$invalidInvitation = Join-Path $profile 'invalid.vnd'
Set-Content -LiteralPath $invalidInvitation -Value 'invalid invitation fixture'
$appProcess = $null
$second = $null

function Wait-Until([scriptblock]$Predicate, [string]$Failure) {
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    while ($deadline.Elapsed.TotalSeconds -lt 25) {
        if (& $Predicate) { return }
        Start-Sleep -Milliseconds 100
    }
    throw $Failure
}
function Control([string]$Id) {
    $script:root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}
function Invoke-Control([string]$Id) {
    Wait-Until { $candidate = Control $Id; $candidate -and $candidate.Current.IsEnabled -and !$candidate.Current.IsOffscreen } "Control unavailable: $Id"
    $control = Control $Id
    if (!$control -or !$control.Current.IsEnabled) { throw "Control unavailable: $Id" }
    $control.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Select-Control([string]$Id) {
    (Control $Id).GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
}
function Set-ControlValue([string]$Id, [string]$Value) {
    (Control $Id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
}
function Click-Control([string]$Id) {
    Wait-Until { $candidate = Control $Id; $candidate -and $candidate.Current.IsEnabled -and !$candidate.Current.IsOffscreen } "Control unavailable: $Id"
    $bounds = (Control $Id).Current.BoundingRectangle
    if ($bounds.IsEmpty) { throw "Control has no clickable bounds: $Id" }
    [VniDropSmokeInput]::Click($appProcess.MainWindowHandle, [int]($bounds.X + $bounds.Width / 2), [int]($bounds.Y + $bounds.Height / 2))
}
try {
    $arguments = @('--profile', ('"' + $profile + '"'))
    $appProcess = Start-Process -FilePath $Executable -ArgumentList $arguments -PassThru
    Wait-Until {
        $appProcess.Refresh()
        if ($appProcess.HasExited) { throw "Native app exited before opening its window (exit $($appProcess.ExitCode))." }
        $null -ne $appProcess.MainWindowHandle -and $appProcess.MainWindowHandle -ne [IntPtr]::Zero
    } 'Native window did not open.'
    $script:root = [System.Windows.Automation.AutomationElement]::FromHandle($appProcess.MainWindowHandle)
    Wait-Until { $navigation = Control 'Navigation'; $navigation -and $navigation.Current.IsEnabled } 'Core startup failed: navigation never became available.'
    foreach ($id in @('NavSend', 'NavReceive', 'NavDevices')) {
        $control = Control $id
        if (!$control -or [string]::IsNullOrWhiteSpace($control.Current.Name)) { throw "Missing navigation resource: $id" }
    }
    Invoke-Control 'EmptyCreateTransfer'
    Wait-Until { Control 'ChooseFilesButton' } 'Send dialog did not open.'
    Invoke-Control 'CloseButton'
    Wait-Until { $null -eq (Control 'ChooseFilesButton') } 'Send dialog did not close.'
    Select-Control 'NavDevices'
    Select-Control 'SettingsItem'
    Wait-Until { Control 'Username' } 'Settings did not open.'
    Set-ControlValue 'Username' 'Windows UI smoke test'
    Start-Sleep -Milliseconds 150
    $savedUsername = (Control 'Username').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
    if ($savedUsername -ne 'Windows UI smoke test') { throw "Settings value was not applied through UI Automation: $savedUsername" }
    Click-Control 'SavePreferencesButton'
    try {
        Wait-Until { (Get-Content -Raw -LiteralPath (Join-Path $profile 'windows-preferences.json') | ConvertFrom-Json).Username -eq 'Windows UI smoke test' } 'Settings were not persisted.'
    } catch {
        $errorBar = Control 'ErrorBar'
        if ($errorBar -and $errorBar.Current.Name) { throw "Settings were not persisted: $($errorBar.Current.Name)" }
        throw
    }
    $second = Start-Process -FilePath $Executable -ArgumentList ($arguments + ('"' + $invalidInvitation + '"')) -PassThru
    Wait-Until { $second.Refresh(); $second.HasExited } 'Second activation did not redirect to the original instance.'
    Wait-Until { Control 'OpenInvitationButton' } 'File activation did not open the receive dialog.'
    Wait-Until { $dialogError = Control 'Error'; $dialogError -and !$dialogError.Current.IsOffscreen } 'Invalid invitation did not produce a visible error.'
    (Control 'OpenInvitationButton').SetFocus()
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    Wait-Until { $null -eq (Control 'OpenInvitationButton') } 'Receive dialog did not close.'
    Start-Sleep -Milliseconds 300
    $appProcess.CloseMainWindow() | Out-Null
    Wait-Until { $appProcess.Refresh(); $appProcess.HasExited } 'Native app did not shut down cleanly.'
    Write-Output 'PASS: native startup, resources, navigation, modal transfer flow, settings, single instance, file activation, and shutdown.'
    Write-Output "QA profile: $profile"
} finally {
    foreach ($ownedProcess in @($second, $appProcess)) {
        if ($ownedProcess) {
            $ownedProcess.Refresh()
            if (!$ownedProcess.HasExited -and $ownedProcess.Path -eq $Executable) { Stop-Process -Id $ownedProcess.Id -Force }
        }
    }
}
