Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$stage = Join-Path $repo ('build/windows/standard-user-test/' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($stage)
$source = Get-Content "$PSScriptRoot/StandardUserProcess.cs" -Raw
foreach ($scenario in @('UnchangedHigh', 'PrivilegesRemovedHigh', 'AdministratorsRemovedHigh', 'UnchangedMedium', 'AdministratorsRemovedMedium')) {
    $class = 'Probe' + $scenario
    $code = $source.Replace('StandardUserProcess', $class)
    if ($scenario.StartsWith('Unchanged')) {
        $code = $code.Replace('CreateRestrictedToken(existing, 1, 1, ref disabled', 'CreateRestrictedToken(existing, 0, 0, ref disabled')
    } elseif ($scenario -eq 'PrivilegesRemovedHigh') {
        $code = $code.Replace('CreateRestrictedToken(existing, 1, 1, ref disabled', 'CreateRestrictedToken(existing, 1, 0, ref disabled')
    }
    if ($scenario.EndsWith('High')) {
        $code = $code.Replace('if (!SetTokenInformation(restricted, 25, ref label, Marshal.SizeOf(label) + bytes.Length))', 'if (false)')
    }
    Add-Type -TypeDefinition $code -IgnoreWarnings
    $type = $class -as [type]
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('whoami.exe /groups; exit 23'))
    $log = Join-Path $stage ($scenario + '.log')
    $child = $null
    try {
        $child = $type::Start("$env:WINDIR/System32/WindowsPowerShell/v1.0/powershell.exe", "-NoProfile -NonInteractive -EncodedCommand $encoded", $repo, $log)
        if (!$child.WaitForExit(30000)) { throw 'Diagnostic child timed out' }
        Write-Host "$scenario exit: $($child.ExitCode)"
    } catch { Write-Host "$scenario failed: $_" }
    finally {
        if ($child) { if (!$child.HasExited) { $child.Kill() }; $child.Dispose() }
        if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log | Write-Host }
    }
}
