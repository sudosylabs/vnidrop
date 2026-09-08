[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('vnidrop-diagnostics-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($fixture)
$project = Join-Path $fixture 'project.properties'
$user = Join-Path $fixture 'user.properties'
$output = Join-Path $fixture 'diagnostics.json'
$variables = @('VNIDROP_DIAGNOSTICS_ENDPOINT', 'VNIDROP_DIAGNOSTICS_INGEST_KEY', 'VNIDROP_DIAGNOSTICS_REQUIRED')
$previous = @{}
foreach ($name in $variables) { $previous[$name] = [Environment]::GetEnvironmentVariable($name) }
function Generate {
    & "$PSScriptRoot/generate-diagnostics-config.ps1" -OutputPath $output -ProjectProperties $project -UserProperties $user
    return (Get-Content -LiteralPath $output -Raw | ConvertFrom-Json)
}
function Expect-Failure {
    $failed = $false
    try { $null = Generate } catch { $failed = $true }
    if (!$failed) { throw 'Invalid diagnostics configuration was accepted.' }
}
try {
    foreach ($name in $variables) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
    $config = Generate
    if ($config.endpoint -or $config.ingestKey) { throw 'Development configuration should be empty.' }
    $env:VNIDROP_DIAGNOSTICS_REQUIRED = '1'
    Expect-Failure

    [IO.File]::WriteAllText($project, "vnidrop.diagnostics.endpoint=https://project.example.test`nvnidrop.diagnostics.ingestKey=project-fixture")
    [IO.File]::WriteAllText($user, "vnidrop.diagnostics.endpoint=https://user.example.test`nvnidrop.diagnostics.ingestKey=user-fixture")
    $config = Generate
    if ($config.endpoint -cne 'https://user.example.test' -or $config.ingestKey -cne 'user-fixture') { throw 'User properties did not override project properties.' }

    $env:VNIDROP_DIAGNOSTICS_ENDPOINT = 'https://env.example.test'
    Expect-Failure
    $env:VNIDROP_DIAGNOSTICS_INGEST_KEY = 'fixture-"quoted"-\key'
    $config = Generate
    if ($config.endpoint -cne $env:VNIDROP_DIAGNOSTICS_ENDPOINT -or $config.ingestKey -cne $env:VNIDROP_DIAGNOSTICS_INGEST_KEY) { throw 'Explicit environment configuration did not round-trip.' }
    foreach ($endpoint in @('http://example.test', 'https://user:password@example.test', 'https://example.test?query=1', 'https://example.test/#fragment', 'https://localhost', 'http://127.0.0.1:12345')) {
        $env:VNIDROP_DIAGNOSTICS_ENDPOINT = $endpoint
        Expect-Failure
    }
    $env:VNIDROP_DIAGNOSTICS_REQUIRED = '0'
    $config = Generate
    if ($config.endpoint -cne 'http://127.0.0.1:12345') { throw 'Development loopback configuration was rejected.' }
    $env:VNIDROP_DIAGNOSTICS_ENDPOINT = 'https://example.test'
    $env:VNIDROP_DIAGNOSTICS_INGEST_KEY = "fixture`r`nInjected: value"
    Expect-Failure
    $env:VNIDROP_DIAGNOSTICS_INGEST_KEY = 'fixture-key'
    $env:VNIDROP_DIAGNOSTICS_REQUIRED = 'invalid'
    Expect-Failure
    Write-Host 'PASS: diagnostics configuration requirements, overrides, JSON escaping and endpoint validation.'
} finally {
    foreach ($name in $variables) {
        if ($null -eq $previous[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $previous[$name]) }
    }
    $resolved = [IO.Path]::GetFullPath($fixture)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (!$resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetFileName($resolved) -notmatch '^vnidrop-diagnostics-[a-f0-9]{32}$' -or
        (Get-Item -LiteralPath $resolved).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
