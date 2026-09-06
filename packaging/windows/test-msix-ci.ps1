[CmdletBinding(DefaultParameterSetName = 'Package')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Package')][string]$Package,
    [Parameter(Mandatory, ParameterSetName = 'Launcher')][switch]$VerifyLauncher
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'This account-creating test is restricted to disposable GitHub-hosted runners.'
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$stage = Join-Path $repo ('build/windows/msix-user-test/' + [guid]::NewGuid().ToString('N'))
$fixture = Join-Path $stage 'fixture'
$scripts = Join-Path $fixture 'packaging/windows'
[void][IO.Directory]::CreateDirectory($scripts)
foreach ($file in @('test-msix.ps1', 'Packaging.psm1', 'StandardUserProcess.cs')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination $scripts
}
if (!$VerifyLauncher) { Copy-Item -LiteralPath (Resolve-Path -LiteralPath $Package).Path -Destination (Join-Path $fixture 'app.msix') }
$log = Join-Path $fixture 'activation.log'
$result = Join-Path $fixture 'launcher.json'
$name = 'vndtest' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$passwordBytes = New-Object byte[] 32
$random = [Security.Cryptography.RandomNumberGenerator]::Create()
try { $random.GetBytes($passwordBytes) } finally { $random.Dispose() }
$password = ConvertTo-SecureString ('Vn1!' + [Convert]::ToBase64String($passwordBytes)) -AsPlainText -Force
[Array]::Clear($passwordBytes, 0, $passwordBytes.Length)
$account = $null
$desktop = $null
$child = $null
$developerKey = 'HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/AppModelUnlock'
$oldDeveloperMode = Get-ItemPropertyValue -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue
try {
    $account = New-LocalUser -Name $name -Password $password -Description 'Temporary VniDrop package acceptance user'
    Add-LocalGroupMember -SID ([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')) -Member $account
    $acl = Get-Acl -LiteralPath $fixture
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($account.SID, 'Modify', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    Set-Acl -LiteralPath $fixture -AclObject $acl
    Add-Type -Path "$PSScriptRoot/TestUserDesktop.cs"
    Add-Type -Path "$PSScriptRoot/StandardUserProcess.cs"
    $desktop = [TestUserDesktop]::new($account.SID.Value)
    if (!$VerifyLauncher -and $oldDeveloperMode -ne 1) {
        [void](New-Item -Path $developerKey -Force)
        Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value 1 -Type DWord
    }
    $script = @'
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try {{
    Add-Type -Path '{0}/packaging/windows/StandardUserProcess.cs'
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $admin = ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    $integrity = [StandardUserProcess]::IntegrityLevel($PID)
    if ($identity.User.Value -ne '{1}' -or $admin -or $integrity -ne 8192) {{ throw 'The activation fixture must run as its standard test user.' }}
    @{{ User = $identity.User.Value; Integrity = $integrity; Administrator = $admin }} | ConvertTo-Json | Set-Content -LiteralPath '{2}'
    if ({3}) {{ exit 23 }}
    & '{0}/packaging/windows/test-msix.ps1' -Package '{0}/app.msix' *> '{4}'
    exit 0
}} catch {{ $_ | Out-String | Add-Content -LiteralPath '{4}'; exit 1 }}
'@ -f $fixture.Replace("'", "''"), $account.SID.Value, $result.Replace("'", "''"), ('$' + $VerifyLauncher.ToString().ToLowerInvariant()), $log.Replace("'", "''")
    # CreateProcessWithLogonW limits the command line to 1,024 characters.
    $entry = Join-Path $fixture 'run.ps1'
    Set-Content -LiteralPath $entry -Value $script -Encoding UTF8
    $credential = [Management.Automation.PSCredential]::new("$env:COMPUTERNAME\$name", $password)
    $child = Start-Process -FilePath "$env:WINDIR/System32/WindowsPowerShell/v1.0/powershell.exe" -ArgumentList ('-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $entry + '"') -Credential $credential -LoadUserProfile -WorkingDirectory $fixture -WindowStyle Hidden -PassThru
    $handle = $child.Handle
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    $seen = [Collections.Generic.HashSet[int]]::new()
    while (!$child.WaitForExit(1000)) {
        if ($deadline.Elapsed.TotalSeconds -ge 240) { throw 'Standard-account package test timed out' }
        if (!$VerifyLauncher) {
            Get-Process VniDrop -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($fixture + [IO.Path]::DirectorySeparatorChar) } | ForEach-Object {
                if ($seen.Add($_.Id)) { Write-Host "Package process $($_.Id), integrity $([StandardUserProcess]::IntegrityLevel($_.Id))" }
            }
        }
    }
    $child.Refresh()
    $expectedExit = if ($VerifyLauncher) { 23 } else { 0 }
    if ($child.ExitCode -ne $expectedExit) { throw "Standard-account package test exited with $($child.ExitCode), expected $expectedExit" }
    $actual = Get-Content -LiteralPath $result -Raw | ConvertFrom-Json
    if ($actual.User -ne $account.SID.Value -or $actual.Integrity -ne 8192 -or $actual.Administrator) { throw 'Standard-account launcher verification failed' }
    Write-Host 'PASS: package acceptance ran under an isolated standard Windows account.'
} finally {
    try {
        if ($child) {
            if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }
            $child.Dispose()
        }
        Get-Process VniDrop -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($fixture + [IO.Path]::DirectorySeparatorChar) } | Stop-Process -Force
        if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log | Write-Host }
    } finally {
        try { if ($desktop) { $desktop.Dispose() } }
        finally {
            try { if ($account) { Remove-LocalUser -SID $account.SID } }
            finally {
                $password.Dispose()
                if (!$VerifyLauncher -and $oldDeveloperMode -ne 1) {
                    if ($null -eq $oldDeveloperMode) { Remove-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue }
                    else { Set-ItemProperty -LiteralPath $developerKey -Name AllowDevelopmentWithoutDevLicense -Value $oldDeveloperMode -Type DWord }
                }
            }
        }
    }
}
