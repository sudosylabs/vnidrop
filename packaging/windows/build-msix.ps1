[CmdletBinding()]
param(
	[Parameter(Mandatory)]
	[string] $AppImage,

	[Parameter(Mandatory)]
	[string] $DirectInstaller,

	[Parameter(Mandatory)]
	[string] $OutputDirectory,

	[string] $WindowsSdkVersion = "10.0.26100.0"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
Import-Module "$PSScriptRoot/Packaging.psm1" -Force

function Assert-Condition {
	param(
		[bool] $Condition,
		[string] $Message
	)

	if (-not $Condition) {
		throw $Message
	}
}

function Invoke-Checked {
	param(
		[string] $FilePath,
		[string[]] $Arguments
	)

	& $FilePath @Arguments
	if ($LASTEXITCODE -ne 0) {
		throw "$FilePath failed with exit code $LASTEXITCODE"
	}
}

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
	throw "MSIX packaging must run on Windows"
}

$versionResolver = Join-Path $PSScriptRoot "..\version\resolve-version.ps1"
$versionInfoJson = & $versionResolver -Field Json -VerifyTag
$versionInfo = $versionInfoJson | ConvertFrom-Json
$Version = [string] $versionInfo.productVersion
$packageVersion = [string] $versionInfo.windowsPackageVersion

$appImagePath = (Resolve-Path -LiteralPath $AppImage).Path
Assert-Condition (Test-Path -LiteralPath $appImagePath -PathType Container) "App image not found: $AppImage"
Assert-NativeAppImage $appImagePath $Version

$directInstallerSourcePath = (Resolve-Path -LiteralPath $DirectInstaller).Path
Assert-Condition (Test-Path -LiteralPath $directInstallerSourcePath -PathType Leaf) "Direct installer not found: $DirectInstaller"
Assert-Condition ([System.IO.Path]::GetExtension($directInstallerSourcePath) -eq ".exe") "The direct installer must be an EXE"
Assert-Condition ((Get-Item -LiteralPath $directInstallerSourcePath).Length -gt 0) "The direct installer is empty"
$directInstallerSignature = Get-AuthenticodeSignature -LiteralPath $directInstallerSourcePath
Assert-Condition ($directInstallerSignature.Status -eq [System.Management.Automation.SignatureStatus]::NotSigned) "The direct installer must be unsigned"

$programFilesX86 = [System.Environment]::GetFolderPath([System.Environment+SpecialFolder]::ProgramFilesX86)
$makeAppxPath = Join-Path $programFilesX86 "Windows Kits\10\bin\$WindowsSdkVersion\x64\MakeAppx.exe"
$makePriPath = Join-Path $programFilesX86 "Windows Kits\10\bin\$WindowsSdkVersion\x64\MakePri.exe"
Assert-Condition (Test-Path -LiteralPath $makeAppxPath -PathType Leaf) "MakeAppx.exe from Windows SDK $WindowsSdkVersion was not found"
Assert-Condition (Test-Path -LiteralPath $makePriPath -PathType Leaf) "MakePri.exe from Windows SDK $WindowsSdkVersion was not found"

$outputPath = [System.IO.Path]::GetFullPath($OutputDirectory)
[System.IO.Directory]::CreateDirectory($outputPath) | Out-Null
$artifactBaseName = "VniDrop_" + $Version + "_x64"
$msixPath = Join-Path $outputPath "$artifactBaseName.msix"
$uploadPath = Join-Path $outputPath "$artifactBaseName.msixupload"
$directInstallerPath = Join-Path $outputPath "$artifactBaseName.exe"
$buildInfoPath = Join-Path $outputPath "$artifactBaseName.build-info.json"
$checksumsPath = Join-Path $outputPath "SHA256SUMS"
@($msixPath, $uploadPath, $directInstallerPath, $buildInfoPath, $checksumsPath) |
	Where-Object { Test-Path -LiteralPath $_ } |
	ForEach-Object { Remove-Item -LiteralPath $_ -Force }
Copy-Item -LiteralPath $directInstallerSourcePath -Destination $directInstallerPath

$stageRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("vnidrop-msix-" + [System.Guid]::NewGuid().ToString("N"))
$packageRoot = Join-Path $stageRoot "package"
$unpackedRoot = Join-Path $stageRoot "unpacked"
$visualRoot = Join-Path $stageRoot "visuals"
$priConfigPath = Join-Path $stageRoot "priconfig.xml"
[System.IO.Directory]::CreateDirectory($packageRoot) | Out-Null

try {
	Get-ChildItem -LiteralPath $appImagePath -Force | Copy-Item -Destination $packageRoot -Recurse -Force
	Get-ChildItem -LiteralPath $packageRoot -Recurse -File -Filter '*.pdb' | Remove-Item -Force
	Copy-Item -LiteralPath (Join-Path $PSScriptRoot "Assets") -Destination $packageRoot -Recurse -Force

	$manifestTemplate = Get-Content -LiteralPath (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw
	Assert-Condition (([regex]::Matches($manifestTemplate, "__VERSION__")).Count -eq 1) "AppxManifest.xml must contain exactly one __VERSION__ placeholder"
	$manifestText = $manifestTemplate.Replace("__VERSION__", $packageVersion)
	[xml]$nativeManifest = $manifestText
	$runtimeExtensions = $nativeManifest.CreateElement('Extensions', $nativeManifest.DocumentElement.NamespaceURI)
	$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
	$registrationInputs = @(Get-Content -LiteralPath (Join-Path $repoRoot 'build/windows/package-registration-inputs.txt') | Where-Object { $_.Trim() })
	Assert-Condition ($registrationInputs.Count -gt 0) 'Publish the native app before packaging its Windows App SDK registrations'
	foreach ($inputFile in $registrationInputs) {
		[xml]$fragment = Get-Content -LiteralPath $inputFile -Raw
		foreach ($extension in $fragment.SelectNodes('/*/*[local-name()="Extensions"]/*')) {
			[void]$runtimeExtensions.AppendChild($nativeManifest.ImportNode($extension, $true))
		}
	}
	# Packaged processes resolve WinRT classes through package registration, not the EXE's reg-free manifest.
	[void]$nativeManifest.DocumentElement.AppendChild($runtimeExtensions)
	$nativeManifest.Save((Join-Path $packageRoot 'AppxManifest.xml'))
	[IO.Directory]::CreateDirectory($visualRoot) | Out-Null
	Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Assets') -Destination $visualRoot -Recurse
	Copy-Item -LiteralPath (Join-Path $packageRoot 'VniDrop.pri') -Destination $visualRoot
	Copy-Item -LiteralPath (Join-Path $packageRoot 'AppxManifest.xml') -Destination $visualRoot

	Invoke-Checked -FilePath $makePriPath -Arguments @(
		"createconfig",
		"/cf", $priConfigPath,
		"/dq", "en-US",
		"/o"
	)
	Invoke-Checked -FilePath $makePriPath -Arguments @(
		"new",
		# VniDrop.pri already merges the app and framework resources. Import it once, without re-indexing framework files.
		"/pr", $visualRoot,
		"/cf", $priConfigPath,
		"/mn", (Join-Path $packageRoot "AppxManifest.xml"),
		"/of", (Join-Path $packageRoot "resources.pri"),
		"/o"
	)
	Assert-Condition ((Get-Item -LiteralPath (Join-Path $packageRoot "resources.pri")).Length -gt 0) "MakePri created an empty resources.pri"

	Invoke-Checked -FilePath $makeAppxPath -Arguments @(
		"pack",
		"/v",
		"/h", "SHA256",
		"/d", $packageRoot,
		"/p", $msixPath,
		"/o"
	)
	Invoke-Checked -FilePath $makeAppxPath -Arguments @(
		"unpack",
		"/v",
		"/p", $msixPath,
		"/d", $unpackedRoot,
		"/o"
	)

	[xml] $manifest = Get-Content -LiteralPath (Join-Path $unpackedRoot "AppxManifest.xml") -Raw
	$namespaces = [System.Xml.XmlNamespaceManager]::new($manifest.NameTable)
	$namespaces.AddNamespace("f", "http://schemas.microsoft.com/appx/manifest/foundation/windows10")
	$namespaces.AddNamespace("uap", "http://schemas.microsoft.com/appx/manifest/uap/windows10")
	$namespaces.AddNamespace("uap10", "http://schemas.microsoft.com/appx/manifest/uap/windows10/10")
	$namespaces.AddNamespace("rescap", "http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities")
	$identity = $manifest.SelectSingleNode("/f:Package/f:Identity", $namespaces)
	Assert-Condition ($null -ne $identity) "The packed manifest has no Identity"
	Assert-Condition ($identity.GetAttribute("Name") -eq "SudosyLabs.Vnidrop") "The packed package identity name is incorrect"
	Assert-Condition ($identity.GetAttribute("Publisher") -eq "CN=6456DC8E-2C31-44BD-AACC-2E6813C833CB") "The packed publisher identity is incorrect"
	Assert-Condition ($identity.GetAttribute("Version") -eq $packageVersion) "The packed package version is incorrect"
	Assert-Condition ($identity.GetAttribute("ProcessorArchitecture") -eq "x64") "The packed package architecture is not x64"
	Assert-Condition ($manifest.SelectSingleNode("/f:Package/f:Properties/f:DisplayName", $namespaces).InnerText -eq "Vnidrop") "The packed display name does not match the reserved Store name"
	Assert-Condition ($manifest.SelectSingleNode("/f:Package/f:Properties/f:PublisherDisplayName", $namespaces).InnerText -eq "Sudosy Labs") "The packed publisher display name is incorrect"
	$expectedLanguages = @("en-US", "de-DE", "es-ES", "fr-FR", "it-IT", "nl-NL", "pl-PL", "pt-PT", "ru-RU")
	$actualLanguages = @($manifest.SelectNodes("/f:Package/f:Resources/f:Resource", $namespaces) | ForEach-Object { $_.GetAttribute("Language") })
	Assert-Condition (($actualLanguages -join ";") -eq ($expectedLanguages -join ";")) "The packed package language list is incorrect: $($actualLanguages -join ', ')"
	$targetFamily = $manifest.SelectSingleNode("/f:Package/f:Dependencies/f:TargetDeviceFamily", $namespaces)
	Assert-Condition ($null -ne $targetFamily) "The packed manifest has no target device family"
	Assert-Condition ($targetFamily.GetAttribute("Name") -eq "Windows.Desktop") "The packed package does not target Windows.Desktop"
	Assert-Condition ($null -ne $manifest.SelectSingleNode("/f:Package/f:Capabilities/rescap:Capability[@Name='runFullTrust']", $namespaces)) "The packed package does not declare runFullTrust"

	$application = $manifest.SelectSingleNode("/f:Package/f:Applications/f:Application", $namespaces)
	Assert-Condition ($null -ne $application) "The packed manifest has no Application"
	Assert-Condition ($application.GetAttribute("Id") -eq "VniDrop") "The packed application ID is incorrect"
	$executable = $application.GetAttribute("Executable")
	Assert-Condition ($executable -eq "VniDrop.exe") "The packed manifest executable is incorrect"
	Assert-Condition ($application.GetAttribute("RuntimeBehavior", "http://schemas.microsoft.com/appx/manifest/uap/windows10/10") -eq "packagedClassicApp") "The packed runtime behavior is incorrect"
	Assert-Condition ($application.GetAttribute("TrustLevel", "http://schemas.microsoft.com/appx/manifest/uap/windows10/10") -eq "mediumIL") "The packed trust level is incorrect"
	Assert-Condition ($manifest.SelectSingleNode("/f:Package/f:Applications/f:Application/f:Extensions/uap:Extension/uap:FileTypeAssociation/uap:SupportedFileTypes/uap:FileType[text()='.vnd']", $namespaces) -ne $null) "The packed package is missing the .vnd file association"
	Assert-Condition ($null -ne $manifest.SelectSingleNode("/f:Package/f:Extensions/f:Extension/f:InProcessServer/f:ActivatableClass[@ActivatableClassId='Microsoft.UI.Xaml.Application']", $namespaces)) 'The package is missing WinUI runtime registration'
	Assert-Condition (Test-Path -LiteralPath (Join-Path $unpackedRoot $executable) -PathType Leaf) "The packed executable is missing"
	Assert-Condition (Test-Path -LiteralPath (Join-Path $unpackedRoot "resources.pri") -PathType Leaf) "The packed resource index is missing"
	Assert-NativeAppImage $unpackedRoot $Version
	foreach ($source in Get-ChildItem -LiteralPath $appImagePath -Recurse -File | Where-Object Extension -ne '.pdb') {
		$relative = $source.FullName.Substring($appImagePath.Length + 1)
		$packed = Join-Path $unpackedRoot $relative
		Assert-Condition ((Get-FileHash -LiteralPath $source.FullName).Hash -eq (Get-FileHash -LiteralPath $packed).Hash) "MSIX changed native asset $relative"
	}
}
finally {
	if (Test-Path -LiteralPath $stageRoot) {
		$resolvedStage = (Resolve-Path -LiteralPath $stageRoot).Path
		$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
		Assert-Condition ($resolvedStage.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($resolvedStage) -match '^vnidrop-msix-[a-f0-9]{32}$') "Unsafe MSIX staging cleanup path"
		Remove-Item -LiteralPath $stageRoot -Recurse -Force
	}
}

$temporaryZip = Join-Path $outputPath "$artifactBaseName.zip"
if (Test-Path -LiteralPath $temporaryZip) {
	Remove-Item -LiteralPath $temporaryZip -Force
}
Compress-Archive -LiteralPath $msixPath -DestinationPath $temporaryZip -CompressionLevel Optimal
Move-Item -LiteralPath $temporaryZip -Destination $uploadPath

$dotnet = Get-PackagingDotnet
$sourceCommit = [System.Environment]::GetEnvironmentVariable("GITHUB_SHA")
if ([string]::IsNullOrWhiteSpace($sourceCommit)) {
	$sourceCommit = "local"
}
$sourceRef = [System.Environment]::GetEnvironmentVariable("GITHUB_REF")
if ([string]::IsNullOrWhiteSpace($sourceRef)) {
	$sourceRef = "local"
}

$buildInfo = [ordered] @{
	appVersion = $Version
	packageVersion = $packageVersion
	architecture = "x64"
	identityName = "SudosyLabs.Vnidrop"
	publisher = "CN=6456DC8E-2C31-44BD-AACC-2E6813C833CB"
	storeId = "9NJ5Q0FG7TGL"
	sourceCommit = $sourceCommit
	sourceRef = $sourceRef
	runnerImage = [System.Environment]::GetEnvironmentVariable("ImageOS")
	runnerImageVersion = [System.Environment]::GetEnvironmentVariable("ImageVersion")
	appHost = "WinUI 3"
	dotnetVersion = ((& $dotnet --version) | Out-String).Trim()
	rustVersion = ((& rustc --version) | Out-String).Trim()
	cargoVersion = ((& cargo --version) | Out-String).Trim()
	wixVersion = "4.0.6"
	windowsSdkVersion = $WindowsSdkVersion
	makeAppxVersion = (Get-Item -LiteralPath $makeAppxPath).VersionInfo.FileVersion
	makePriVersion = (Get-Item -LiteralPath $makePriPath).VersionInfo.FileVersion
	unsignedForMicrosoftStore = $true
	directInstaller = [ordered] @{
		artifact = [System.IO.Path]::GetFileName($directInstallerPath)
		unsigned = $true
		smartScreenWarningExpected = $true
	}
	builtAtUtc = [System.DateTimeOffset]::UtcNow.ToString("O")
}
[System.IO.File]::WriteAllText(
	$buildInfoPath,
	($buildInfo | ConvertTo-Json -Depth 4),
	[System.Text.UTF8Encoding]::new($false)
)

$checksumTargets = @($msixPath, $uploadPath, $directInstallerPath, $buildInfoPath)
[string[]] $checksumLines = $checksumTargets | ForEach-Object {
	$hash = Get-FileHash -LiteralPath $_ -Algorithm SHA256
	$hash.Hash.ToLowerInvariant() + "  " + [System.IO.Path]::GetFileName($_)
}
[System.IO.File]::WriteAllLines($checksumsPath, $checksumLines, [System.Text.Encoding]::ASCII)

Write-Host "Created Windows release artifacts:"
Write-Host "  $msixPath"
Write-Host "  $uploadPath"
Write-Host "  $directInstallerPath (unsigned; SmartScreen warning expected)"
Write-Host "  $checksumsPath"
