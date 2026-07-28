[CmdletBinding()]
param(
	[Parameter(Mandatory)]
	[string] $AppImage,

	[Parameter(Mandatory)]
	[string] $OutputDirectory,

	[string] $WindowsSdkVersion = "10.0.26100.0"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

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

function Read-ZipEntry {
	param(
		[string] $ArchivePath,
		[string] $EntryPath
	)

	$archive = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
	try {
		$entry = $archive.GetEntry($EntryPath)
		if ($null -eq $entry) {
			throw "$ArchivePath does not contain $EntryPath"
		}
		$reader = [System.IO.StreamReader]::new($entry.Open())
		try {
			return $reader.ReadToEnd()
		}
		finally {
			$reader.Dispose()
		}
	}
	finally {
		$archive.Dispose()
	}
}

function Get-ZipEntryLength {
	param(
		[string] $ArchivePath,
		[string] $EntryPath
	)

	$archive = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
	try {
		$entry = $archive.GetEntry($EntryPath)
		if ($null -eq $entry) {
			throw "$ArchivePath does not contain $EntryPath"
		}
		return $entry.Length
	}
	finally {
		$archive.Dispose()
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
Assert-Condition (Test-Path -LiteralPath (Join-Path $appImagePath "VniDrop.exe") -PathType Leaf) "The app image does not contain VniDrop.exe"
Assert-Condition (Test-Path -LiteralPath (Join-Path $appImagePath "runtime\bin\server\jvm.dll") -PathType Leaf) "The app image does not contain its bundled JVM"

Add-Type -AssemblyName System.IO.Compression.FileSystem
$appFiles = @(Get-ChildItem -LiteralPath $appImagePath -Recurse -File)
$debugRustJars = @($appFiles | Where-Object { $_.Name -match "^shared-win32-x86-64-debug-.+\.jar$" })
Assert-Condition ($debugRustJars.Count -eq 0) "The app image contains a debug Rust runtime JAR"
$releaseRustJars = @($appFiles | Where-Object { $_.Name -match "^shared-win32-x86-64-(?!debug-).+\.jar$" })
Assert-Condition ($releaseRustJars.Count -eq 1) "Expected exactly one release Rust runtime JAR"
$nativeDllLength = Get-ZipEntryLength -ArchivePath $releaseRustJars[0].FullName -EntryPath "win32-x86-64/vnidrop.dll"
Assert-Condition ($nativeDllLength -gt 0) "The release Rust runtime JAR contains an empty vnidrop.dll"

$sharedJars = @($appFiles | Where-Object { $_.Name -match "^shared-jvm-.+\.jar$" })
Assert-Condition ($sharedJars.Count -eq 1) "Expected exactly one shared JVM JAR"
$sharedManifest = Read-ZipEntry -ArchivePath $sharedJars[0].FullName -EntryPath "META-INF/MANIFEST.MF"
Assert-Condition ($sharedManifest -match "(?m)^Implementation-Version: $([regex]::Escape($Version))\r?$") "The packaged app version does not match $Version"

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
$buildInfoPath = Join-Path $outputPath "$artifactBaseName.build-info.json"
$checksumsPath = Join-Path $outputPath "SHA256SUMS"
@($msixPath, $uploadPath, $buildInfoPath, $checksumsPath) |
	Where-Object { Test-Path -LiteralPath $_ } |
	ForEach-Object { Remove-Item -LiteralPath $_ -Force }

$stageRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("vnidrop-msix-" + [System.Guid]::NewGuid().ToString("N"))
$packageRoot = Join-Path $stageRoot "package"
$unpackedRoot = Join-Path $stageRoot "unpacked"
$priConfigPath = Join-Path $stageRoot "priconfig.xml"
[System.IO.Directory]::CreateDirectory($packageRoot) | Out-Null

try {
	Get-ChildItem -LiteralPath $appImagePath -Force | Copy-Item -Destination $packageRoot -Recurse -Force
	Copy-Item -LiteralPath (Join-Path $PSScriptRoot "Assets") -Destination $packageRoot -Recurse -Force

	$manifestTemplate = Get-Content -LiteralPath (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw
	Assert-Condition (([regex]::Matches($manifestTemplate, "__VERSION__")).Count -eq 1) "AppxManifest.xml must contain exactly one __VERSION__ placeholder"
	$manifestText = $manifestTemplate.Replace("__VERSION__", $packageVersion)
	[System.IO.File]::WriteAllText(
		(Join-Path $packageRoot "AppxManifest.xml"),
		$manifestText,
		[System.Text.UTF8Encoding]::new($false)
	)

	Invoke-Checked -FilePath $makePriPath -Arguments @(
		"createconfig",
		"/cf", $priConfigPath,
		"/dq", "en-US",
		"/o"
	)
	Invoke-Checked -FilePath $makePriPath -Arguments @(
		"new",
		"/pr", $packageRoot,
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
	Assert-Condition (Test-Path -LiteralPath (Join-Path $unpackedRoot $executable) -PathType Leaf) "The packed executable is missing"
	Assert-Condition (Test-Path -LiteralPath (Join-Path $unpackedRoot "resources.pri") -PathType Leaf) "The packed resource index is missing"
}
finally {
	if (Test-Path -LiteralPath $stageRoot) {
		Remove-Item -LiteralPath $stageRoot -Recurse -Force
	}
}

$temporaryZip = Join-Path $outputPath "$artifactBaseName.zip"
if (Test-Path -LiteralPath $temporaryZip) {
	Remove-Item -LiteralPath $temporaryZip -Force
}
Compress-Archive -LiteralPath $msixPath -DestinationPath $temporaryZip -CompressionLevel Optimal
Move-Item -LiteralPath $temporaryZip -Destination $uploadPath

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..\..")).Path
$wrapperProperties = Get-Content -LiteralPath (Join-Path $repoRoot "gradle\wrapper\gradle-wrapper.properties")
$gradleDistribution = $wrapperProperties | Where-Object { $_.StartsWith("distributionUrl=") } | Select-Object -First 1
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
	javaVersion = ((& java --version | Select-Object -First 1) | Out-String).Trim()
	rustVersion = ((& rustc --version) | Out-String).Trim()
	cargoVersion = ((& cargo --version) | Out-String).Trim()
	gradleDistribution = $gradleDistribution
	windowsSdkVersion = $WindowsSdkVersion
	makeAppxVersion = (Get-Item -LiteralPath $makeAppxPath).VersionInfo.FileVersion
	makePriVersion = (Get-Item -LiteralPath $makePriPath).VersionInfo.FileVersion
	unsignedForMicrosoftStore = $true
	builtAtUtc = [System.DateTimeOffset]::UtcNow.ToString("O")
}
[System.IO.File]::WriteAllText(
	$buildInfoPath,
	($buildInfo | ConvertTo-Json -Depth 4),
	[System.Text.UTF8Encoding]::new($false)
)

$checksumTargets = @($msixPath, $uploadPath, $buildInfoPath)
[string[]] $checksumLines = $checksumTargets | ForEach-Object {
	$hash = Get-FileHash -LiteralPath $_ -Algorithm SHA256
	$hash.Hash.ToLowerInvariant() + "  " + [System.IO.Path]::GetFileName($_)
}
[System.IO.File]::WriteAllLines($checksumsPath, $checksumLines, [System.Text.Encoding]::ASCII)

Write-Host "Created unsigned Microsoft Store artifacts:"
Write-Host "  $msixPath"
Write-Host "  $uploadPath"
Write-Host "  $checksumsPath"
