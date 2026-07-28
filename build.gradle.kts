import java.util.Properties

plugins {
	// this is necessary to avoid the plugins to be loaded multiple times
	// in each subproject's classloader
	alias(libs.plugins.androidApplication) apply false
	alias(libs.plugins.androidLibrary) apply false
	alias(libs.plugins.androidMultiplatformLibrary) apply false
	alias(libs.plugins.composeMultiplatform) apply false
	alias(libs.plugins.composeCompiler) apply false
	alias(libs.plugins.gobleyCargo) apply false
	alias(libs.plugins.gobleyUniffi) apply false
	alias(libs.plugins.kotlinAtomicfu) apply false
	alias(libs.plugins.kotlinAndroid) apply false
	alias(libs.plugins.kotlinJvm) apply false
	alias(libs.plugins.kotlinMultiplatform) apply false
}

val versionFile = layout.projectDirectory.file("version.properties")
val versionProperties = Properties().apply {
	versionFile.asFile.inputStream().use(::load)
}

fun requiredVersionProperty(name: String): String =
	versionProperties.getProperty(name)?.takeIf { it.isNotBlank() }
		?: error("Missing $name in ${versionFile.asFile}")

fun canonicalInteger(name: String, value: String, range: LongRange): Long {
	require(value.matches(Regex("0|[1-9][0-9]*"))) {
		"$name must be a canonical non-negative integer"
	}
	val number = value.toLongOrNull()
	require(number != null && number in range) {
		"$name must be between ${range.first} and ${range.last}"
	}
	return number
}

val productVersion = requiredVersionProperty("PRODUCT_VERSION")
val productVersionMatch = Regex("(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)")
	.matchEntire(productVersion)
	?: error("PRODUCT_VERSION must use canonical MAJOR.MINOR.PATCH integers")
val productVersionParts = productVersionMatch.groupValues.drop(1).map(String::toLong)
require(productVersionParts[0] <= 65534 && productVersionParts.drop(1).all { it <= 65535 }) {
	"PRODUCT_VERSION components exceed the supported store ranges"
}

val releaseChannel = requiredVersionProperty("RELEASE_CHANNEL")
require(releaseChannel.matches(Regex("[a-z][a-z0-9-]*"))) {
	"RELEASE_CHANNEL contains unsupported characters"
}
val androidVersionCode = canonicalInteger(
	"ANDROID_VERSION_CODE",
	requiredVersionProperty("ANDROID_VERSION_CODE"),
	1L..2_100_000_000L,
).toInt()
val appleBuildNumber = requiredVersionProperty("APPLE_BUILD_NUMBER")
require(appleBuildNumber.matches(Regex("[1-9][0-9]*(\\.[0-9]+){0,2}"))) {
	"APPLE_BUILD_NUMBER must contain one to three period-separated non-negative integers and start above zero"
}
val windowsVersionEpoch = canonicalInteger(
	"WINDOWS_VERSION_EPOCH",
	requiredVersionProperty("WINDOWS_VERSION_EPOCH"),
	1L..65535L,
)
val windowsMajor = productVersionParts[0] + windowsVersionEpoch
require(windowsMajor <= 65535) {
	"Derived Windows package major exceeds 65535"
}
val windowsPackageVersion =
	"$windowsMajor.${productVersionParts[1]}.${productVersionParts[2]}.0"

extra["vnidrop.productVersion"] = productVersion
extra["vnidrop.releaseChannel"] = releaseChannel
extra["vnidrop.androidVersionCode"] = androidVersionCode
extra["vnidrop.appleBuildNumber"] = appleBuildNumber
extra["vnidrop.windowsPackageVersion"] = windowsPackageVersion

tasks.register("verifyVersion") {
	group = "verification"
	description = "Validates the canonical cross-platform application version."
	inputs.file(versionFile)
	inputs.property("productVersion", productVersion)
	inputs.property("releaseChannel", releaseChannel)
	inputs.property("androidVersionCode", androidVersionCode)
	inputs.property("appleBuildNumber", appleBuildNumber)
	inputs.property("windowsPackageVersion", windowsPackageVersion)
}
