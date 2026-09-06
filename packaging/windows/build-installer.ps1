[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$AppImage,
    [Parameter(Mandatory)][string]$OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module "$PSScriptRoot/Packaging.psm1" -Force
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$version = (& "$PSScriptRoot/../version/resolve-version.ps1" -Field Json -VerifyTag | ConvertFrom-Json).productVersion
$appImagePath = (Resolve-Path -LiteralPath $AppImage).Path
Assert-NativeAppImage $appImagePath $version
$tools = Get-PackagingWix
$output = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($output) | Out-Null
$stage = Join-Path $output ('source-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($stage) | Out-Null

function Escape-Xml([string]$Value) { [Security.SecurityElement]::Escape($Value) }
function Get-PathId([string]$Path) {
    $hash = [Security.Cryptography.SHA256]::Create()
    try { return -join ($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($Path.ToLowerInvariant())) | ForEach-Object { $_.ToString('x2') }) }
    finally { $hash.Dispose() }
}

# jpackage derives this ProgId from the logical installation path (Java's nameUUIDFromBytes).
# Keeping it lets an existing Windows UserChoice continue to resolve after a Compose upgrade.
$md5 = [Security.Cryptography.MD5]::Create()
try { $bytes = $md5.ComputeHash([Text.Encoding]::UTF8.GetBytes('ProgId@installdir\vnd_vnidrop.exe')) }
finally { $md5.Dispose() }
$bytes[6] = ($bytes[6] -band 15) -bor 48
$bytes[8] = ($bytes[8] -band 63) -bor 128
$progId = 'progid' + (-join ($bytes | ForEach-Object { $_.ToString('x2') }))

$xml = [Text.StringBuilder]::new()
[void]$xml.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs"><Fragment>')
$components = [Collections.Generic.List[string]]::new()
$directories = @('') + @(Get-ChildItem -LiteralPath $appImagePath -Directory -Recurse | ForEach-Object { $_.FullName.Substring($appImagePath.Length + 1) })
foreach ($relative in $directories) {
    $directoryId = 'INSTALLFOLDER'
    if ($relative) {
        $directoryId = 'd' + (Get-PathId $relative).Substring(0, 32)
        $parent = [IO.Path]::GetDirectoryName($relative)
        $parentId = if ($parent) { 'd' + (Get-PathId $parent).Substring(0, 32) } else { 'INSTALLFOLDER' }
        [void]$xml.AppendLine('<DirectoryRef Id="' + $parentId + '"><Directory Id="' + $directoryId + '" Name="' + (Escape-Xml ([IO.Path]::GetFileName($relative))) + '" /></DirectoryRef>')
    }
    [void]$xml.AppendLine('<DirectoryRef Id="' + $directoryId + '">')
    $files = @(Get-ChildItem -LiteralPath (Join-Path $appImagePath $relative) -File | Where-Object Extension -ne '.pdb' | Sort-Object Name)
    foreach ($file in $files) {
        $path = $file.FullName.Substring($appImagePath.Length + 1)
        $id = (Get-PathId $path).Substring(0, 32)
        $fileId = if ($path -ieq 'VniDrop.exe') { 'VniDropExe' } else { 'f' + $id }
        $components.Add('c' + $id)
        $guid = [guid]::ParseExact((Get-PathId ('VniDrop.Native.File@' + $path)).Substring(0, 32), 'N').ToString('D')
        [void]$xml.AppendLine('<Component Id="c' + $id + '" Guid="' + $guid + '">')
        [void]$xml.AppendLine('<RegistryValue Root="HKCU" Key="Software\VniDrop\Installer\Files" Name="' + $id + '" Value="1" Type="integer" KeyPath="yes" />')
        [void]$xml.AppendLine('<File Id="' + $fileId + '" Source="' + (Escape-Xml $file.FullName) + '" />')
        [void]$xml.AppendLine('</Component>')
    }
    if ($relative) {
        $components.Add('c' + $directoryId)
        [void]$xml.AppendLine('<Component Id="c' + $directoryId + '" Guid="*"><CreateFolder />')
        [void]$xml.AppendLine('<RegistryValue Root="HKCU" Key="Software\VniDrop\Installer\Directories" Name="' + $directoryId + '" Value="1" Type="integer" KeyPath="yes" />')
        [void]$xml.AppendLine('<RemoveFolder Id="r' + $directoryId + '" On="uninstall" /></Component>')
    }
    [void]$xml.AppendLine('</DirectoryRef>')
}
[void]$xml.AppendLine('<ComponentGroup Id="NativeFiles">')
foreach ($id in $components) { [void]$xml.AppendLine('<ComponentRef Id="' + $id + '" />') }
[void]$xml.AppendLine('</ComponentGroup></Fragment></Wix>')
$filesSource = Join-Path $stage 'Files.wxs'
[IO.File]::WriteAllText($filesSource, $xml.ToString(), [Text.UTF8Encoding]::new($false))
$msi = Join-Path $output "VniDrop_${version}_x64.msi"
$exe = Join-Path $output "VniDrop_${version}_x64.exe"
& $tools.Executable build "$PSScriptRoot/installer.wxs" $filesSource -arch x64 -d "Version=$version" -d "Repo=$repo" -d "InvitationProgId=$progId" -o $msi -intermediateFolder $stage
if ($LASTEXITCODE) { throw 'Native MSI build failed' }
& $tools.Executable build "$PSScriptRoot/bundle.wxs" -arch x64 -ext $tools.BalExtension -d "Version=$version" -d "Repo=$repo" -d "Msi=$msi" -o $exe -intermediateFolder $stage
if ($LASTEXITCODE) { throw 'Native EXE installer build failed' }
Write-Host "Created $exe (current-user installation, no external runtime downloads)"
