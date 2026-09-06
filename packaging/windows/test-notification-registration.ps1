[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module "$PSScriptRoot/Packaging.psm1" -Force
[xml]$source = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'AppxManifest.xml') -Raw
Assert-NotificationRegistration $source
foreach ($mutation in @('missing-activation', 'wrong-class', 'wrong-executable', 'missing-argument')) {
    [xml]$manifest = $source.OuterXml
    $activation = $manifest.SelectSingleNode('//*[@Category="windows.toastNotificationActivation"]')
    $server = $manifest.SelectSingleNode('//*[local-name()="ExeServer"]')
    switch ($mutation) {
        'missing-activation' { [void]$activation.ParentNode.RemoveChild($activation) }
        'wrong-class' { $server.FirstChild.SetAttribute('Id', [guid]::NewGuid().ToString()) }
        'wrong-executable' { $server.SetAttribute('Executable', 'missing.exe') }
        'missing-argument' { $server.SetAttribute('Arguments', '') }
    }
    $rejected = $false
    try { Assert-NotificationRegistration $manifest } catch { $rejected = $true }
    if (!$rejected) { throw "Notification regression was accepted: $mutation" }
}
Write-Host 'PASS: notification activation, matching COM server, executable routing and activation arguments.'
