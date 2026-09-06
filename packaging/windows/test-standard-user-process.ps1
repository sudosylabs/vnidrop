[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -Path "$PSScriptRoot/StandardUserProcess.cs"
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$stage = Join-Path $repo ('build/windows/standard-user-test/' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($stage)
$result = Join-Path $stage 'result.json'
$log = Join-Path $stage 'launcher.log'
$script = @'
$ErrorActionPreference = 'Stop'
Add-Type -Path '{0}'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
@{{
    Integrity = [StandardUserProcess]::IntegrityLevel($PID)
    User = $identity.User.Value
    Administrator = ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}} | ConvertTo-Json | Set-Content -LiteralPath '{1}'
exit 23
'@ -f ("$PSScriptRoot/StandardUserProcess.cs".Replace("'", "''")), $result.Replace("'", "''")
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
$child = [StandardUserProcess]::Start("$env:WINDIR/System32/WindowsPowerShell/v1.0/powershell.exe", "-NoProfile -NonInteractive -EncodedCommand $encoded", $repo, $log)
try {
    if (!$child.WaitForExit(30000)) { throw 'Standard-user process test timed out' }
    Write-Host "Child exit code: $($child.ExitCode)"
    $actual = Get-Content -LiteralPath $result -Raw | ConvertFrom-Json
    Write-Host "Standard-user child: $($actual | ConvertTo-Json -Compress)"
    if ($actual.Integrity -ne 0x2000 -or $actual.Administrator -or
        $actual.User -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value) {
        throw "Child must retain the same user, with medium integrity and no administrator membership: $($actual | ConvertTo-Json -Compress)"
    }
    if ($child.ExitCode -ne 23) { throw "Standard-user process lost the child exit code: $($child.ExitCode)" }
    Write-Host 'PASS: child has standard-user privileges, retains its identity and can write its result; exit code preserved.'
} finally {
    if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }
    $child.Dispose()
    if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log | Write-Host }
}
