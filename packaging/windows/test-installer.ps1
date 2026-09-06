[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$AppImage,
    [Parameter(Mandatory)][string]$InstallerDirectory,
    [switch]$Install,
    [switch]$MsiOnly
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($MsiOnly -and !$Install) { throw '-MsiOnly requires -Install' }
Import-Module "$PSScriptRoot/Packaging.psm1" -Force
$version = (& "$PSScriptRoot/../version/resolve-version.ps1" -Field Json | ConvertFrom-Json).productVersion
$image = (Resolve-Path -LiteralPath $AppImage).Path
$directory = (Resolve-Path -LiteralPath $InstallerDirectory).Path
$msi = Join-Path $directory "VniDrop_${version}_x64.msi"
$exe = Join-Path $directory "VniDrop_${version}_x64.exe"
$stage = Join-Path $directory ('test-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($stage) | Out-Null
$tools = Get-PackagingWix
function Assert([bool]$Condition, [string]$Message) { if (!$Condition) { throw $Message } }
function Query($Database, [string]$Sql, [int]$Columns = 1) {
    $view = $Database.OpenView($Sql)
    try {
        [void]$view.Execute()
        while ($record = $view.Fetch()) {
            $row = @(); for ($i = 1; $i -le $Columns; $i++) { $row += $record.StringData($i) }
            ,$row
        }
    } finally { [void]$view.Close() }
}
function Run-Installer([string]$Path, [string[]]$Arguments) {
    $process = Start-Process -FilePath $Path -ArgumentList $Arguments -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(180000)) { throw "Installer timed out: $Path. Inspect $stage before retrying." }
    Assert ($process.ExitCode -in @(0, 3010)) "Installer failed ($($process.ExitCode)); logs: $stage"
}

& $tools.Executable burn extract $exe -o (Join-Path $stage 'bundle') -oba (Join-Path $stage 'bootstrapper')
if ($LASTEXITCODE) { throw 'EXE extraction failed' }
$embeddedMsi = @(Get-ChildItem -LiteralPath (Join-Path $stage 'bundle') -Recurse -Filter '*.msi')
Assert ($embeddedMsi.Count -eq 1) 'The EXE must embed exactly one MSI'
Assert ((Get-FileHash -LiteralPath $msi).Hash -eq (Get-FileHash -LiteralPath $embeddedMsi[0].FullName).Hash) 'EXE contains a different MSI'
& $tools.Executable msi decompile $msi -x (Join-Path $stage 'payload') -o (Join-Path $stage 'package.wxs')
if ($LASTEXITCODE) { throw 'MSI extraction failed' }
[xml]$source = Get-Content -LiteralPath (Join-Path $stage 'package.wxs') -Raw
$files = @($source.SelectNodes('//*[local-name()="File"]'))
$expected = @(Get-ChildItem -LiteralPath $image -Recurse -File | Where-Object Extension -ne '.pdb')
Assert ($files.Count -eq $expected.Count) 'MSI payload file count differs from the published app'
$expectedHashes = @($expected | ForEach-Object { (Get-FileHash -LiteralPath $_.FullName).Hash } | Sort-Object)
$actualHashes = @($files | ForEach-Object { (Get-FileHash -LiteralPath (Join-Path $stage ('payload/File/' + $_.GetAttribute('Id')))).Hash } | Sort-Object)
Assert (($expectedHashes -join ';') -eq ($actualHashes -join ';')) 'MSI payload differs from the published app'

$installer = New-Object -ComObject WindowsInstaller.Installer
$database = $installer.OpenDatabase($msi, 0)
$properties = @{}
foreach ($row in Query $database 'SELECT `Property`, `Value` FROM `Property`' 2) { $properties[$row[0]] = $row[1] }
$upgradeCode = '{E08E256E-2F07-479E-8AA9-4898D424F6C5}'
Assert ($properties['UpgradeCode'] -eq $upgradeCode) 'MSI must retain the Compose upgrade identity'
Assert ($properties['ProductVersion'] -eq $version) 'MSI version differs from the release'
foreach ($row in Query $database 'SELECT `Language` FROM `Upgrade`') {
    Assert (!$row[0]) 'Upgrades must recognize every legacy installer language'
}
Assert (!$properties['ALLUSERS']) 'MSI must install for the current user'
Assert ($database.SummaryInformation(0).Property(7) -match '^x64;') 'MSI must target x64'
$registry = @(Query $database 'SELECT `Root`, `Key`, `Name`, `Value` FROM `Registry`' 4)
Assert (@($registry | Where-Object { $_[0] -ne '1' }).Count -eq 0) 'Installer registry entries must be current-user only'
$commands = @($registry | Where-Object { $_[1] -match '^Software\\Classes\\progid[0-9a-f]{32}\\shell\\open\\command$' })
Assert ($commands.Count -eq 1 -and $commands[0][3] -eq '"[#VniDropExe]" "%1"') 'Invitation command must quote the executable and file argument'
$progId = $commands[0][1].Split('\')[2]
Assert (@($registry | Where-Object { $_[1] -match 'UserChoice' -or ($_[1] -eq 'Software\Classes\.vnd' -and $_[2] -eq '') }).Count -eq 0) 'Installer must not replace file defaults'
Assert (@($registry | Where-Object { $_[1] -eq 'Software\Classes\.vnd\OpenWithProgids' -and $_[2] -eq $progId }).Count -eq 1) 'Open With registration is missing'
$sequence = @{}
foreach ($row in Query $database 'SELECT `Action`, `Sequence` FROM `InstallExecuteSequence`' 2) { $sequence[$row[0]] = [int]$row[1] }
Assert ($sequence['RemoveExistingProducts'] -gt $sequence['InstallInitialize'] -and $sequence['RemoveExistingProducts'] -lt $sequence['InstallFiles']) 'Upgrade removal must run inside the rollback transaction before installing new files'
$productCode = $properties['ProductCode']
[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($database)
Write-Host 'PASS: embedded MSI, every published file hash, per-user scope, upgrade identity, version, architecture, and file association.'
if (!$Install) { return }

$related = @($installer.RelatedProducts($upgradeCode))
$installFolder = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'VniDrop'
Assert ($related.Count -eq 0 -and !(Test-Path -LiteralPath $installFolder)) 'Install tests require a clean Windows account with no existing VniDrop installation'
Assert (!(Test-Path -LiteralPath "HKCU:/Software/Classes/$progId")) 'Install tests must not overwrite an existing VniDrop handler'
$defaultBeforeKey = Get-Item -LiteralPath 'HKCU:/Software/Classes/.vnd' -ErrorAction SilentlyContinue
$defaultBefore = if ($defaultBeforeKey) { $defaultBeforeKey.GetValue('') } else { $null }
$choiceKey = 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Explorer/FileExts/.vnd/UserChoice'
$choiceBefore = Get-ItemProperty -LiteralPath $choiceKey -ErrorAction SilentlyContinue | Select-Object ProgId, Hash | ConvertTo-Json -Compress
$legacyMsi = Join-Path $stage 'legacy.msi'
$marker = Join-Path $stage 'compose-runtime.txt'
Set-Content -LiteralPath $marker -Value 'Legacy Compose installation fixture'
$legacySource = @'
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="VniDrop legacy test fixture" Manufacturer="Sudosy Labs" Version="0.0.1" Language="1036" UpgradeCode="E08E256E-2F07-479E-8AA9-4898D424F6C5" Scope="perUser">
    <MediaTemplate EmbedCab="yes" />
    <StandardDirectory Id="LocalAppDataFolder"><Directory Id="INSTALLDIR" Name="VniDrop">
      <Component Id="LegacyFile" Guid="E55A09C9-4821-4AE6-A0A6-C69325C2702F">
        <File Source="$(var.Marker)" />
        <RegistryValue Root="HKCU" Key="Software\VniDrop\InstallerTest" Name="Legacy" Type="integer" Value="1" KeyPath="yes" />
        <RemoveFolder Id="RemoveLegacyDirectory" On="uninstall" />
      </Component>
    </Directory></StandardDirectory>
    <Feature Id="DefaultFeature"><ComponentRef Id="LegacyFile" /></Feature>
  </Package>
</Wix>
'@
$legacySourcePath = Join-Path $stage 'legacy.wxs'
Set-Content -LiteralPath $legacySourcePath -Value $legacySource
& $tools.Executable build $legacySourcePath -arch x64 -d "Marker=$marker" -o $legacyMsi
if ($LASTEXITCODE) { throw 'Legacy fixture build failed' }
$installed = $false
$legacyInstalled = $false
$app = $null
try {
    Run-Installer msiexec.exe @('/i', ('"' + $legacyMsi + '"'), '/qn', '/norestart', '/l*v', ('"' + (Join-Path $stage 'legacy-install.log') + '"'))
    $legacyInstalled = $true
    Assert (Test-Path -LiteralPath (Join-Path $installFolder 'compose-runtime.txt')) 'Legacy fixture was not installed'
    if ($MsiOnly) {
        Run-Installer msiexec.exe @('/i', ('"' + $msi + '"'), '/qn', '/norestart', '/l*v', ('"' + (Join-Path $stage 'native-install.log') + '"'))
    } else {
        Run-Installer $exe @('/install', '/quiet', '/norestart', '/log', ('"' + (Join-Path $stage 'native-install.log') + '"'))
    }
    $installed = $true
    Assert (!(Test-Path -LiteralPath (Join-Path $installFolder 'compose-runtime.txt'))) 'Upgrade left the old runtime installed'
    $related = @($installer.RelatedProducts($upgradeCode))
    Assert ($related.Count -eq 1 -and $related[0] -eq $productCode) 'Upgrade did not replace the legacy MSI'
    $legacyInstalled = $false
    Assert-NativeAppImage $installFolder $version
    $installedCommand = (Get-Item -LiteralPath "HKCU:/Software/Classes/$progId/shell/open/command").GetValue('')
    Assert ($installedCommand -eq ('"' + (Join-Path $installFolder 'VniDrop.exe') + '" "%1"')) 'Installed handler points at the wrong executable'
    $profile = Join-Path $stage 'profile with spaces'
    [IO.Directory]::CreateDirectory($profile) | Out-Null
    Set-Content -LiteralPath (Join-Path $profile 'windows-preferences.json') -Value '{"Username":"Installer test","RelayMode":3}'
    $app = Start-Process -FilePath (Join-Path $installFolder 'VniDrop.exe') -ArgumentList @('--profile', ('"' + $profile + '"')) -PassThru
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    do {
        Start-Sleep -Milliseconds 200
        $app.Refresh()
    } while (!$app.HasExited -and $app.MainWindowHandle -eq 0 -and $deadline.Elapsed.TotalSeconds -lt 30)
    Assert (!$app.HasExited -and $app.MainWindowHandle -ne 0 -and $app.Responding) 'Installed WinUI app did not show a responsive window'
    [void]$app.CloseMainWindow()
    Assert ($app.WaitForExit(15000)) 'Installed app did not shut down cleanly'
    $app = $null
    if ($MsiOnly) {
        Run-Installer msiexec.exe @('/x', ('"' + $msi + '"'), '/qn', '/norestart', '/l*v', ('"' + (Join-Path $stage 'native-uninstall.log') + '"'))
    } else {
        Run-Installer $exe @('/uninstall', '/quiet', '/norestart', '/log', ('"' + (Join-Path $stage 'native-uninstall.log') + '"'))
    }
    $installed = $false
    $legacyInstalled = $false
    Assert (!(Test-Path -LiteralPath $installFolder)) 'Uninstall left installed files behind'
    Assert (!(Test-Path -LiteralPath "HKCU:/Software/Classes/$progId/shell/open/command")) 'Uninstall left a dead invitation handler'
    Assert (Test-Path -LiteralPath (Join-Path $profile 'windows-preferences.json')) 'Uninstall removed user data'
    $defaultAfterKey = Get-Item -LiteralPath 'HKCU:/Software/Classes/.vnd' -ErrorAction SilentlyContinue
    $defaultAfter = if ($defaultAfterKey) { $defaultAfterKey.GetValue('') } else { $null }
    Assert ($defaultBefore -eq $defaultAfter) 'Installation changed the existing file default'
    $choiceAfter = Get-ItemProperty -LiteralPath $choiceKey -ErrorAction SilentlyContinue | Select-Object ProgId, Hash | ConvertTo-Json -Compress
    Assert ($choiceBefore -eq $choiceAfter) 'Installation changed Windows UserChoice'
    Write-Host 'PASS: legacy upgrade, installed WinUI launch, handler routing, uninstall, retained profile and unchanged user defaults.'
    if ($MsiOnly) { Write-Warning 'MSI-only diagnostic run: EXE installation has not been verified.' }
} finally {
    if ($app -and !$app.HasExited) { $app.Kill(); $app.WaitForExit() }
    if ($installed) {
        if ($MsiOnly) { Run-Installer msiexec.exe @('/x', ('"' + $msi + '"'), '/qn', '/norestart') }
        else { Run-Installer $exe @('/uninstall', '/quiet', '/norestart') }
    }
    if ($legacyInstalled) { Run-Installer msiexec.exe @('/x', ('"' + $legacyMsi + '"'), '/qn', '/norestart') }
}
