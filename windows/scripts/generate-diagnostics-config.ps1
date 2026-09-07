[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputPath,
    [string]$ProjectProperties,
    [string]$UserProperties
)
$ErrorActionPreference = 'Stop'
if (!$ProjectProperties) { $ProjectProperties = Join-Path $PSScriptRoot '../../gradle.properties' }
if (!$UserProperties) {
    $profileDirectory = [Environment]::GetFolderPath('UserProfile')
    if (!$profileDirectory) { $profileDirectory = $env:USERPROFILE }
    if ($profileDirectory) { $UserProperties = Join-Path $profileDirectory '.gradle/gradle.properties' }
}
$endpoint = [Environment]::GetEnvironmentVariable('VNIDROP_DIAGNOSTICS_ENDPOINT')
$key = [Environment]::GetEnvironmentVariable('VNIDROP_DIAGNOSTICS_INGEST_KEY')
if ($null -eq $endpoint -and $null -eq $key) {
    foreach ($path in @($ProjectProperties, $UserProperties)) {
        if (!$path -or !(Test-Path -LiteralPath $path)) { continue }
        foreach ($line in [IO.File]::ReadLines($path)) {
            if ($line -match '^\s*vnidrop\.diagnostics\.(endpoint|ingestKey)\s*=(.*)$') {
                if ($Matches[1] -eq 'endpoint') { $endpoint = $Matches[2].Trim() }
                else { $key = $Matches[2].Trim() }
            }
        }
    }
}
$endpoint = ([string]$endpoint).Trim()
$key = ([string]$key).Trim()
$required = [Environment]::GetEnvironmentVariable('VNIDROP_DIAGNOSTICS_REQUIRED')
if ($required -and $required -notin @('0', '1')) { throw 'VNIDROP_DIAGNOSTICS_REQUIRED must be 0 or 1.' }
if (!$endpoint -and !$key) {
    if ($required -eq '1') { throw 'Official Windows packages require a diagnostics endpoint and ingest key.' }
} else {
    $uri = $null
    if (![Uri]::TryCreate($endpoint, [UriKind]::Absolute, [ref]$uri) -or
        !$uri.Host -or $uri.UserInfo -or $uri.Query -or $uri.Fragment -or $endpoint -match '\s' -or
        ($uri.Scheme -ne 'https' -and !($uri.Scheme -eq 'http' -and $uri.IsLoopback -and $required -ne '1')) -or
        ($required -eq '1' -and $uri.IsLoopback) -or !$key -or $key.Length -gt 4096 -or $key -match '[\x00-\x1f\x7f]') {
        throw 'Diagnostics configuration requires an HTTPS endpoint and a valid ingest key; loopback HTTP is allowed only for development.'
    }
}
$output = [IO.Path]::GetFullPath($OutputPath)
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($output))
$json = @{ endpoint = $endpoint; ingestKey = $key } | ConvertTo-Json -Compress
if (!(Test-Path -LiteralPath $output) -or [IO.File]::ReadAllText($output) -cne $json) {
    [IO.File]::WriteAllText($output, $json, [Text.UTF8Encoding]::new($false))
}
