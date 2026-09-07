[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module "$PSScriptRoot/Packaging.psm1" -Force

$scenarios = @(
    @{ Name = 'local'; Actions = ''; Runner = ''; Skip = $false }
    @{ Name = 'GitHub-hosted'; Actions = 'true'; Runner = 'github-hosted'; Skip = $true }
    @{ Name = 'self-hosted'; Actions = 'true'; Runner = 'self-hosted'; Skip = $false }
    @{ Name = 'unknown GitHub runner'; Actions = 'true'; Runner = ''; Skip = $false }
    @{ Name = 'unrecognized runner'; Actions = 'true'; Runner = 'new-runner'; Skip = $false }
    @{ Name = 'outside GitHub Actions'; Actions = 'false'; Runner = 'github-hosted'; Skip = $false }
    @{ Name = 'missing GitHub context'; Actions = ''; Runner = 'github-hosted'; Skip = $false }
)
foreach ($scenario in $scenarios) {
    $reason = Get-ColdNotificationTestSkipReason -GitHubActions $scenario.Actions -RunnerEnvironment $scenario.Runner
    if ([bool]$reason -ne $scenario.Skip) {
        throw "Incorrect cold notification test policy for $($scenario.Name)"
    }
    if ($scenario.Skip -and ($reason -notmatch 'DCOM' -or $reason -notmatch 'test-msix.ps1')) {
        throw 'Skipped cold activation must explain the limitation and remaining acceptance command'
    }
}
Write-Host 'PASS: cold notification activation skips only on GitHub-hosted runners; local, self-hosted, and unknown environments retain the check.'
