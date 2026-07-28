[CmdletBinding()]
param(
	[ValidateSet("Product", "Channel", "AndroidCode", "AppleBuild", "WindowsPackage", "Json", "Verify")]
	[string] $Field = "Verify",

	[switch] $VerifyTag,

	[string] $VersionFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($VersionFile)) {
	$VersionFile = Join-Path $PSScriptRoot "..\..\version.properties"
}
$VersionFile = (Resolve-Path -LiteralPath $VersionFile).Path

function Read-VersionProperty {
	param([string] $Name)

	$prefix = "$Name="
	$matches = @(Get-Content -LiteralPath $VersionFile | Where-Object { $_.StartsWith($prefix) })
	if ($matches.Count -ne 1) {
		throw "Expected exactly one $Name entry in $VersionFile"
	}
	return $matches[0].Substring($prefix.Length)
}

function Convert-CanonicalInteger {
	param(
		[string] $Name,
		[string] $Value,
		[long] $Minimum,
		[long] $Maximum
	)

	if ($Value -notmatch "^(0|[1-9][0-9]*)$") {
		throw "$Name must be a canonical non-negative integer"
	}
	$number = 0L
	if (-not [long]::TryParse($Value, [ref] $number) -or $number -lt $Minimum -or $number -gt $Maximum) {
		throw "$Name must be between $Minimum and $Maximum"
	}
	return $number
}

$productVersion = Read-VersionProperty "PRODUCT_VERSION"
$releaseChannel = Read-VersionProperty "RELEASE_CHANNEL"
$androidVersionCodeText = Read-VersionProperty "ANDROID_VERSION_CODE"
$appleBuildNumber = Read-VersionProperty "APPLE_BUILD_NUMBER"
$windowsVersionEpochText = Read-VersionProperty "WINDOWS_VERSION_EPOCH"

if ($productVersion -notmatch "^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$") {
	throw "PRODUCT_VERSION must use canonical MAJOR.MINOR.PATCH integers"
}
$productParts = $productVersion.Split(".")
$productMajor = Convert-CanonicalInteger "PRODUCT_VERSION major" $productParts[0] 0 65534
$null = Convert-CanonicalInteger "PRODUCT_VERSION minor" $productParts[1] 0 65535
$null = Convert-CanonicalInteger "PRODUCT_VERSION patch" $productParts[2] 0 65535
if ($releaseChannel -notmatch "^[a-z][a-z0-9-]*$") {
	throw "RELEASE_CHANNEL contains unsupported characters"
}
$androidVersionCode = Convert-CanonicalInteger "ANDROID_VERSION_CODE" $androidVersionCodeText 1 2100000000
if ($appleBuildNumber -notmatch "^[1-9][0-9]*(\.[0-9]+){0,2}$") {
	throw "APPLE_BUILD_NUMBER must contain one to three period-separated non-negative integers and start above zero"
}
$windowsVersionEpoch = Convert-CanonicalInteger "WINDOWS_VERSION_EPOCH" $windowsVersionEpochText 1 65535
$windowsMajor = $productMajor + $windowsVersionEpoch
if ($windowsMajor -gt 65535) {
	throw "Derived Windows package major exceeds 65535"
}
$windowsPackageVersion = "$windowsMajor.$($productParts[1]).$($productParts[2]).0"

if ($VerifyTag -and $env:GITHUB_REF_TYPE -eq "tag" -and $env:GITHUB_REF_NAME -ne "v$productVersion") {
	throw "Release tag must be v$productVersion, got $($env:GITHUB_REF_NAME)"
}

$versionInfo = [ordered] @{
	productVersion = $productVersion
	releaseChannel = $releaseChannel
	androidVersionCode = $androidVersionCode
	appleBuildNumber = $appleBuildNumber
	windowsPackageVersion = $windowsPackageVersion
}

switch ($Field) {
	"Product" { $productVersion }
	"Channel" { $releaseChannel }
	"AndroidCode" { $androidVersionCode }
	"AppleBuild" { $appleBuildNumber }
	"WindowsPackage" { $windowsPackageVersion }
	"Json" { $versionInfo | ConvertTo-Json -Compress }
	"Verify" {
		"VniDrop $productVersion ($releaseChannel), Android $androidVersionCode, Apple $appleBuildNumber, MSIX $windowsPackageVersion"
	}
}
