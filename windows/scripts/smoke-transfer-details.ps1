param([Parameter(Mandatory)][string]$Executable, [Parameter(Mandatory)][string]$ProfileDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TransferDetailsInput {
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
}
'@
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$ProfileDirectory = (Resolve-Path -LiteralPath $ProfileDirectory).Path
$appProcess = $null
function Wait-Until([scriptblock]$Predicate, [string]$Failure) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.Elapsed.TotalSeconds -lt 25) {
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
    (Control $Id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Resize-Window([int]$Width, [int]$Height) {
    [TransferDetailsInput]::SetWindowPos($appProcess.MainWindowHandle, [IntPtr]::Zero, 0, 0, [int]($Width * $scale), [int]($Height * $scale), 0x0040) | Out-Null
    Wait-Until { $bounds = $script:root.Current.BoundingRectangle; [Math]::Abs($bounds.Height - $Height * $scale) -lt 20 -and [Math]::Abs($bounds.Width - $Width * $scale) -lt 20 } 'Window did not resize.'
}
function Assert-Summary {
    $count = Control 'FileCountText'
    $size = Control 'TransferSizeText'
    $access = Control 'AccessText'
    if ($count.Current.Name -ne '22' -or $size.Current.Name -notmatch '^22[.,]0 KB$') { throw 'The fixture must show separate file count and transfer size values.' }
    $countBounds = $count.Current.BoundingRectangle
    $sizeBounds = $size.Current.BoundingRectangle
    $accessBounds = $access.Current.BoundingRectangle
    if ([Math]::Abs($countBounds.Top - $sizeBounds.Top) -gt 1 -or $countBounds.Right -ge $sizeBounds.Left -or $accessBounds.Top -le $countBounds.Bottom) {
        throw "Metadata must use two columns with access below: count=$countBounds size=$sizeBounds access=$accessBounds"
    }
    foreach ($id in @('StopTransferButton', 'DeleteTransferButton')) {
        $row = Control $id
        $label = $row.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'TitleText'))
        $rowBounds = $row.Current.BoundingRectangle
        $labelBounds = $label.Current.BoundingRectangle
        if ([Math]::Abs(($rowBounds.Top + $rowBounds.Height / 2) - ($labelBounds.Top + $labelBounds.Height / 2)) -gt $scale) {
            throw "Action label must be centered with its icon: $id row=$rowBounds label=$labelBounds"
        }
    }
}
try {
    $appProcess = Start-Process -FilePath $Executable -ArgumentList @('--profile', ('"' + $ProfileDirectory + '"')) -PassThru
    Wait-Until { $appProcess.Refresh(); if ($appProcess.HasExited) { throw 'The fixture profile must not already be open.' }; $appProcess.MainWindowHandle -ne [IntPtr]::Zero } 'Native window did not open.'
    $script:root = [System.Windows.Automation.AutomationElement]::FromHandle($appProcess.MainWindowHandle)
    Wait-Until { (Control 'Transfers') -and (Control 'NameText') } 'Transfer list did not load.'
    $scale = [TransferDetailsInput]::GetDpiForWindow($appProcess.MainWindowHandle) / 96.0
    Resize-Window 800 760
    $list = Control 'Transfers'
    $item = $list.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem))
    $item.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-Until { Control 'FileCountText' } 'Transfer details did not open.'
    foreach ($width in @(1200, 800, 500)) {
        Resize-Window $width 760
        Wait-Until { try { Assert-Summary; return $true } catch { $script:layoutFailure = $_.Exception.Message; return $false } } "Transfer summary is misaligned at $width effective pixels."
    }
    foreach ($id in @('TransferActivityButton', 'TransferReceiversButton')) {
        Invoke-Control $id
        Wait-Until { Control 'DialogListViewport' } 'History dialog did not open.'
        Invoke-Control 'PART_BackButton'
        foreach ($height in @(760, 500, 760)) {
            Resize-Window 500 $height
            $viewport = Control 'DialogListViewport'
            $scroll = $viewport.GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
            Wait-Until { $viewport.Current.BoundingRectangle.Height -le [Math]::Min(440 * $scale, $height * $scale * 0.6) } 'History viewport exceeded its height cap after resizing.'
            if (!$scroll.Current.VerticallyScrollable -or $scroll.Current.HorizontallyScrollable) { throw 'Long history must scroll vertically inside the dialog.' }
            $outerScroll = (Control 'ContentScrollViewer').GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
            if ($outerScroll.Current.VerticallyScrollable) { throw 'The dialog title must not scroll with the list.' }
            $closeBefore = (Control 'CloseButton').Current.BoundingRectangle
            $scroll.SetScrollPercent(-1, 100)
            Wait-Until { $scroll.Current.VerticalScrollPercent -ge 99 } 'Could not reach the last history item.'
            $closeAfter = (Control 'CloseButton').Current.BoundingRectangle
            if ($closeBefore -ne $closeAfter -or (Control 'CloseButton').Current.IsOffscreen) { throw 'The dialog close button moved or disappeared while scrolling.' }
            $scroll.SetScrollPercent(-1, 0)
        }
        Invoke-Control 'CloseButton'
        Wait-Until { $null -eq (Control 'DialogListViewport') } 'History dialog did not close.'
        if (!(Control 'FileCountText') -or !(Control 'PART_BackButton').Current.IsEnabled) {
            throw 'Title-bar Back must not navigate the page underneath an open dialog.'
        }
    }
    $appProcess.CloseMainWindow() | Out-Null
    Wait-Until { Control 'PrimaryButton' } 'The active fixture did not show a close confirmation.'
    if ((Control 'AppTitleBar').Current.IsEnabled -or (Control 'PART_PaneToggleButton').Current.IsEnabled) {
        throw 'Title-bar navigation must disable while closing.'
    }
    Invoke-Control 'CloseButton'
    Wait-Until { (Control 'AppTitleBar').Current.IsEnabled -and (Control 'PART_PaneToggleButton').Current.IsEnabled } 'Cancelling close did not restore title-bar navigation.'
    Write-Output 'PASS: transfer metadata, action alignment, bounded history scrolling, modal Back protection, and title-bar state during close and cancel.'
} catch {
    Write-Output ("FAIL: " + $_.ScriptStackTrace)
    if ($script:layoutFailure) { Write-Output $script:layoutFailure }
    throw
} finally {
    if ($appProcess) {
        $appProcess.Refresh()
        if (!$appProcess.HasExited -and $appProcess.Path -eq $Executable) {
            $appProcess.CloseMainWindow() | Out-Null
            if (!$appProcess.WaitForExit(1500)) {
                $confirm = Control 'PrimaryButton'
                if ($confirm) { $confirm.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
                if (!$appProcess.WaitForExit(5000)) { Stop-Process -Id $appProcess.Id -Force }
            }
        }
    }
}
