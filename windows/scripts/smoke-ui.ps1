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
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extraInfo);
    [DllImport("user32.dll", SetLastError=true)] static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    public static void Click(IntPtr window, int x, int y) {
        SetForegroundWindow(window);
        SetCursorPos(x, y);
        mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
        mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    }
    public static void Close(IntPtr window) {
        if (!PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero))
            throw new System.ComponentModel.Win32Exception();
    }
    public static void Hover(IntPtr window, int x, int y) { SetForegroundWindow(window); SetCursorPos(x, y); }
}
'@
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$profile = Join-Path $repo ('build/windows/smoke/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $profile -Force | Out-Null
@{ Username = 'S25'; RelayMode = 3; ReceiveDirectory = (Join-Path $profile 'received') } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $profile 'windows-preferences.json')
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
    $control = Control $Id
    if (!$control -or $control.Current.IsOffscreen -or $control.Current.BoundingRectangle.IsEmpty) {
        Invoke-Control 'PART_PaneToggleButton'
    }
    Click-Control $Id
}
function Set-ControlValue([string]$Id, [string]$Value) {
    Wait-Until { $candidate = Control $Id; $candidate -and $candidate.Current.IsEnabled -and !$candidate.Current.IsOffscreen } "Control unavailable: $Id"
    (Control $Id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
}
function Click-Control([string]$Id) {
    Wait-Until { $candidate = Control $Id; $candidate -and $candidate.Current.IsEnabled -and !$candidate.Current.IsOffscreen } "Control unavailable: $Id"
    $bounds = (Control $Id).Current.BoundingRectangle
    if ($bounds.IsEmpty) { throw "Control has no clickable bounds: $Id" }
    [VniDropSmokeInput]::Click($appProcess.MainWindowHandle, [int]($bounds.X + $bounds.Width / 2), [int]($bounds.Y + $bounds.Height / 2))
}
function Assert-ShortValueInline([string]$Id) {
    $row = Control $Id
    $title = $row.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'TitleText'))
    $value = $row.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'ValueText'))
    if (!$title -or !$value -or $value.Current.Name -ne 'S25') { throw 'The label/value regression fixture is missing.' }
    $titleBounds = $title.Current.BoundingRectangle
    $valueBounds = $value.Current.BoundingRectangle
    if ($valueBounds.Left -le $titleBounds.Left -or $valueBounds.Top -ge $titleBounds.Bottom -or $valueBounds.Bottom -le $titleBounds.Top) {
        throw "Short row value must stay beside its label: title=$titleBounds value=$valueBounds"
    }
}
function Assert-NoPageShortcutTooltip {
    $bounds = (Control 'Navigation').Current.BoundingRectangle
    [VniDropSmokeInput]::Hover($appProcess.MainWindowHandle, [int]($bounds.Right - 15), [int]($bounds.Top + $bounds.Height / 2))
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.Elapsed.TotalSeconds -lt 2) {
        $tooltips = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ToolTip))
        foreach ($tooltip in $tooltips) {
            if ($tooltip.Current.ProcessId -eq $appProcess.Id -and $tooltip.Current.Name -match 'Alt\+|Ctrl\+') {
                throw "Page-wide shortcut tooltip leaked into the UI: $($tooltip.Current.Name)"
            }
        }
        Start-Sleep -Milliseconds 100
    }
}
function Assert-PageHeading {
    $title = Control 'TitleText'
    if (!$title) { throw 'The page has no heading.' }
    $matches = $script:root.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.AndCondition]::new(
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Text),
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $title.Current.Name)))
    $visible = @($matches | Where-Object { !$_.Current.IsOffscreen })
    if ($visible.Count -ne 1) { throw "Expected one visible page heading, found $($visible.Count): $($title.Current.Name)" }
}
function Assert-HeaderAlignment {
    $title = (Control 'TitleText').Current.BoundingRectangle
    $subtitle = (Control 'SubtitleText').Current.BoundingRectangle
    if ($subtitle.IsEmpty -or [Math]::Abs($title.Left - $subtitle.Left) -gt 1 -or $subtitle.Top -lt $title.Bottom) {
        throw "Page title and description must share a left edge without overlap: title=$title subtitle=$subtitle"
    }
}
function Assert-ShellLayout([bool]$Expanded) {
    $titleBar = (Control 'AppTitleBar').Current.BoundingRectangle
    $back = (Control 'PART_BackButton').Current.BoundingRectangle
    $heading = (Control 'TitleText').Current.BoundingRectangle
    if ($titleBar.Height -lt 40 * $scale -or $back.Top -lt $titleBar.Top -or $back.Bottom -gt $titleBar.Bottom -or $heading.Top -lt $titleBar.Bottom) {
        throw "Back must sit inside the tall title bar, above the page: titlebar=$titleBar back=$back heading=$heading"
    }
    if ($heading.Top - $titleBar.Bottom -gt 48 * $scale) {
        throw 'Closed notification bars must not leave an empty band above the page heading.'
    }
    foreach ($id in @('NavigationViewBackButton', 'TogglePaneButton')) {
        $duplicate = Control $id
        if ($duplicate -and !$duplicate.Current.IsOffscreen -and !$duplicate.Current.BoundingRectangle.IsEmpty) {
            throw "Navigation must not duplicate the title-bar controls: $id"
        }
    }
    $toggle = Control 'PART_PaneToggleButton'
    $toggleVisible = $toggle -and !$toggle.Current.IsOffscreen -and !$toggle.Current.BoundingRectangle.IsEmpty
    $device = Control 'NavDevices'
    $deviceVisible = $device -and !$device.Current.IsOffscreen -and !$device.Current.BoundingRectangle.IsEmpty
    if ($toggleVisible -eq $Expanded -or $deviceVisible -ne $Expanded) {
        throw "Expected expanded sidebar=$Expanded, got toggle=$toggleVisible and devices=$deviceVisible"
    }
    if ($toggleVisible) {
        $bounds = $toggle.Current.BoundingRectangle
        if ($bounds.Top -lt $titleBar.Top -or $bounds.Bottom -gt $titleBar.Bottom) {
            throw 'The sidebar toggle must be inside the title bar.'
        }
    }
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
    $scale = [VniDropSmokeInput]::GetDpiForWindow($appProcess.MainWindowHandle) / 96.0
    [VniDropSmokeInput]::SetWindowPos($appProcess.MainWindowHandle, [IntPtr]::Zero, 0, 0, [int](1200 * $scale), [int](760 * $scale), 0x0040) | Out-Null
    Wait-Until { try { Assert-ShellLayout $true; return $true } catch { return $false } } 'Expanded shell layout failed.'
    if ((Control 'PART_BackButton').Current.IsEnabled) { throw 'Back must be disabled at a navigation root.' }
    foreach ($id in @('NavSend', 'NavReceive', 'NavDevices')) {
        $control = Control $id
        if (!$control -or [string]::IsNullOrWhiteSpace($control.Current.Name)) { throw "Missing navigation resource: $id" }
    }
    Assert-NoPageShortcutTooltip
    Invoke-Control 'EmptyCreateTransfer'
    Wait-Until { Control 'ChooseFilesButton' } 'Send dialog did not open.'
    Invoke-Control 'CloseButton'
    Wait-Until { $null -eq (Control 'ChooseFilesButton') } 'Send dialog did not close.'
    Select-Control 'NavDevices'
    Wait-Until { $title = Control 'TitleText'; $description = Control 'DescriptionText'; $title -and $title.Current.Name -eq (Control 'NavDevices').Current.Name -and $description -and !$description.Current.IsOffscreen } 'Devices did not show its empty state.'
    $savedHeading = Control 'SavedDevicesHeading'
    if ($savedHeading -and !$savedHeading.Current.IsOffscreen) { throw 'The saved-device list heading must only accompany a populated list.' }
    Select-Control 'SettingsItem'
    foreach ($width in @(1200, 800, 500)) {
        [VniDropSmokeInput]::SetWindowPos($appProcess.MainWindowHandle, [IntPtr]::Zero, 0, 0, [int]($width * $scale), [int](760 * $scale), 0x0040) | Out-Null
        Wait-Until { $bounds = $script:root.Current.BoundingRectangle; [Math]::Abs($bounds.Width - $width * $scale) -lt 20 } 'Window did not resize.'
        Wait-Until { try { Assert-ShortValueInline 'PreferencesRow'; return $true } catch { return $false } } "Short value wrapped below its label at $width effective pixels."
        Wait-Until { try { Assert-ShellLayout ($width -ge 1000); return $true } catch { return $false } } "Shell did not adapt at $width effective pixels."
    }
    Invoke-Control 'PART_PaneToggleButton'
    Wait-Until { $device = Control 'NavDevices'; $device -and !$device.Current.IsOffscreen } 'The title-bar toggle did not open the overlay sidebar.'
    Click-Control 'NavDevices'
    Wait-Until { $device = Control 'NavDevices'; $device -and $device.Current.IsOffscreen } 'Choosing a destination did not dismiss the overlay sidebar.'
    Select-Control 'SettingsItem'
    Wait-Until { Control 'PreferencesRow' } 'Settings is not reachable through the overlay sidebar.'
    Assert-NoPageShortcutTooltip
    foreach ($page in @(@('AppearanceRow', 'ThemeChoices'), @('NetworkRow', 'ModeChoices'), @('NotificationsRow', 'Notifications'))) {
        Invoke-Control $page[0]
        Wait-Until { (Control $page[1]) -and $null -eq (Control 'PreferencesRow') } "Settings page did not open: $($page[0])"
        if (!(Control 'PART_BackButton').Current.IsEnabled) { throw 'Title-bar Back must enable on a detail page.' }
        Assert-PageHeading
        if ($page[0] -eq 'NotificationsRow') {
            foreach ($width in @(1200, 800, 500)) {
                [VniDropSmokeInput]::SetWindowPos($appProcess.MainWindowHandle, [IntPtr]::Zero, 0, 0, [int]($width * $scale), [int](760 * $scale), 0x0040) | Out-Null
                Wait-Until { try { Assert-HeaderAlignment; return $true } catch { return $false } } "Notification header is misaligned at $width effective pixels."
            }
        }
        Invoke-Control 'PART_BackButton'
        Wait-Until { (Control 'PreferencesRow') -and $null -eq (Control $page[1]) } 'Settings did not return to its root.'
        if ((Control 'PART_BackButton').Current.IsEnabled) { throw 'Back stayed enabled after returning to the root.' }
    }
    [VniDropSmokeInput]::SetWindowPos($appProcess.MainWindowHandle, [IntPtr]::Zero, 0, 0, [int](1200 * $scale), [int](760 * $scale), 0x0040) | Out-Null
    Invoke-Control 'PreferencesRow'
    Wait-Until { Control 'DisplayNameTextBox' } 'Preferences did not open for keyboard navigation.'
    (Control 'DisplayNameTextBox').SetFocus()
    [System.Windows.Forms.SendKeys]::SendWait('%{LEFT}')
    Wait-Until { Control 'PreferencesRow' } 'Alt+Left stopped navigating back after hiding the page tooltip.'
    Invoke-Control 'PreferencesRow'
    Wait-Until { Control 'DisplayNameTextBox' } 'Preferences did not open.'
    Set-ControlValue 'DisplayNameTextBox' 'Windows UI smoke test'
    $savedUsername = (Control 'DisplayNameTextBox').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
    if ($savedUsername -ne 'Windows UI smoke test') { throw "Settings value was not applied through UI Automation: $savedUsername" }
    Click-Control 'SettingsItem'
    Wait-Until { (Control 'PreferencesRow') -and $null -eq (Control 'DisplayNameTextBox') } 'Selecting Settings again did not return to the settings root.'
    try {
        Wait-Until { (Get-Content -Raw -LiteralPath (Join-Path $profile 'windows-preferences.json') | ConvertFrom-Json).Username -eq 'Windows UI smoke test' } 'Settings were not persisted.'
    } catch {
        $errorBar = Control 'ErrorBar'
        if ($errorBar -and $errorBar.Current.Name) { throw "Settings were not persisted: $($errorBar.Current.Name)" }
        throw
    }
    Invoke-Control 'PreferencesRow'
    Wait-Until { Control 'DisplayNameTextBox' } 'Preferences did not reopen.'
    $reloadedUsername = (Control 'DisplayNameTextBox').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
    if ($reloadedUsername -ne 'Windows UI smoke test') { throw "Persisted settings did not reload: $reloadedUsername" }
    Set-ControlValue 'DisplayNameTextBox' 'Quick re-entry smoke test'
    Click-Control 'SettingsItem'
    Wait-Until { (Control 'PreferencesRow') -and $null -eq (Control 'DisplayNameTextBox') } 'Preferences did not navigate away for the re-entry check.'
    Invoke-Control 'PreferencesRow'
    Wait-Until { $field = Control 'DisplayNameTextBox'; $field -and $field.Current.IsEnabled } 'Preferences did not finish reconciling its pending write.'
    $reenteredUsername = (Control 'DisplayNameTextBox').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
    if ($reenteredUsername -ne 'Quick re-entry smoke test') { throw "Quick preference re-entry showed stale state: $reenteredUsername" }
    $second = Start-Process -FilePath $Executable -ArgumentList ($arguments + ('"' + $invalidInvitation + '"')) -PassThru
    Wait-Until { $second.Refresh(); $second.HasExited } 'Second activation did not redirect to the original instance.'
    Wait-Until { Control 'OpenInvitationButton' } 'File activation did not open the receive dialog.'
    Wait-Until { $dialogError = Control 'Error'; $dialogError -and !$dialogError.Current.IsOffscreen } 'Invalid invitation did not produce a visible error.'
    (Control 'OpenInvitationButton').SetFocus()
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    Wait-Until { $null -eq (Control 'OpenInvitationButton') } 'Receive dialog did not close.'
    Select-Control 'SettingsItem'
    Wait-Until { (Control 'PreferencesRow') -and $null -eq (Control 'DisplayNameTextBox') } 'Settings did not reopen after file activation.'
    Invoke-Control 'PreferencesRow'
    Wait-Until { Control 'DisplayNameTextBox' } 'Preferences did not reopen after file activation.'
    Set-ControlValue 'DisplayNameTextBox' 'Navigation close flush smoke test'
    Click-Control 'SettingsItem'
    Wait-Until { (Control 'PreferencesRow') -and $null -eq (Control 'DisplayNameTextBox') } 'Preferences did not navigate away before shutdown.'
    [VniDropSmokeInput]::Close($appProcess.MainWindowHandle)
    Wait-Until { $appProcess.Refresh(); $appProcess.HasExited } 'Native app did not flush a navigated-away preference save.'
    $navigatedUsername = (Get-Content -Raw -LiteralPath (Join-Path $profile 'windows-preferences.json') | ConvertFrom-Json).Username
    if ($navigatedUsername -ne 'Navigation close flush smoke test') { throw "Navigated-away settings were lost during shutdown: $navigatedUsername" }

    $appProcess = Start-Process -FilePath $Executable -ArgumentList $arguments -PassThru
    Wait-Until {
        $appProcess.Refresh()
        if ($appProcess.HasExited) { throw "Native app exited before the close-flush check (exit $($appProcess.ExitCode))." }
        $null -ne $appProcess.MainWindowHandle -and $appProcess.MainWindowHandle -ne [IntPtr]::Zero
    } 'Native window did not reopen for the close-flush check.'
    $script:root = [System.Windows.Automation.AutomationElement]::FromHandle($appProcess.MainWindowHandle)
    Wait-Until { $navigation = Control 'Navigation'; $navigation -and $navigation.Current.IsEnabled } 'Reopened app did not become ready.'
    Select-Control 'SettingsItem'
    Invoke-Control 'PreferencesRow'
    Wait-Until { Control 'DisplayNameTextBox' } 'Preferences did not reopen for the close-flush check.'
    Set-ControlValue 'DisplayNameTextBox' 'Close flush smoke test'
    [VniDropSmokeInput]::Close($appProcess.MainWindowHandle)
    Wait-Until { $appProcess.Refresh(); $appProcess.HasExited } 'Native app did not shut down cleanly.'
    $closedUsername = (Get-Content -Raw -LiteralPath (Join-Path $profile 'windows-preferences.json') | ConvertFrom-Json).Username
    if ($closedUsername -ne 'Close flush smoke test') { throw "Pending settings were lost during shutdown: $closedUsername" }
    Write-Output 'PASS: native startup, resources, empty devices, settings headings and responsive header alignment, responsive row values, shortcut tooltips and Alt+Left, navigation, modal transfer flow, settings autosave and both close-flush paths, single instance, file activation, and shutdown.'
    Write-Output "QA profile: $profile"
} catch {
    Write-Output ("FAIL: " + $_.ScriptStackTrace)
    if ($script:root) {
        $visible = $script:root.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition)
        for ($index = 0; $index -lt [Math]::Min($visible.Count, 80); $index++) {
            $element = $visible.Item($index)
            if (!$element.Current.IsOffscreen -and ($element.Current.Name -or $element.Current.AutomationId)) {
                Write-Output ("UI: {0} | {1} | {2}" -f $element.Current.ControlType.ProgrammaticName, $element.Current.AutomationId, $element.Current.Name)
            }
        }
    }
    throw
} finally {
    foreach ($ownedProcess in @($second, $appProcess)) {
        if ($ownedProcess) {
            $ownedProcess.Refresh()
            if (!$ownedProcess.HasExited -and $ownedProcess.Path -eq $Executable) { Stop-Process -Id $ownedProcess.Id -Force }
        }
    }
}
