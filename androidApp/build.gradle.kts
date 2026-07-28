import org.gradle.api.DefaultTask
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.SetProperty
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.util.zip.ZipFile

abstract class VerifyVnidropLibrariesTask : DefaultTask() {
	@get:InputFile
	@get:PathSensitive(PathSensitivity.RELATIVE)
	abstract val apk: RegularFileProperty

	@get:Input
	abstract val requiredLibraries: SetProperty<String>

	@TaskAction
	fun verify() {
		ZipFile(apk.get().asFile).use { archive ->
			val missing = requiredLibraries.get().filter { path ->
				archive.getEntry(path)?.size?.takeIf { it > 0L } == null
			}
			check(missing.isEmpty()) {
				"Debug APK has missing or empty VniDrop libraries: ${missing.joinToString()}"
			}
		}
	}
}

plugins {
	alias(libs.plugins.androidApplication)
	alias(libs.plugins.kotlinAndroid)
	alias(libs.plugins.composeMultiplatform)
	alias(libs.plugins.composeCompiler)
}

val appVersion = rootProject.extra["vnidrop.productVersion"] as String
val androidVersionCode = rootProject.extra["vnidrop.androidVersionCode"] as Int

kotlin {
	compilerOptions {
		jvmTarget = JvmTarget.JVM_11
	}
}
dependencies {
	implementation(projects.shared)

	implementation(libs.androidx.activity.compose)

	implementation(libs.compose.uiToolingPreview)
	debugImplementation(libs.compose.uiTooling)
}

android {
	namespace = "com.vnidrop.app"
	compileSdk = libs.versions.android.compileSdk.get().toInt()

	defaultConfig {
		applicationId = "com.vnidrop.app"
		minSdk = libs.versions.android.minSdk.get().toInt()
		targetSdk = libs.versions.android.targetSdk.get().toInt()
		versionCode = androidVersionCode
		versionName = appVersion
	}
	packaging {
		resources {
			excludes += "/META-INF/{AL2.0,LGPL2.1}"
		}
		jniLibs {
			pickFirsts += setOf(
				"lib/arm64-v8a/libvnidrop.so",
				"lib/x86_64/libvnidrop.so",
			)
		}
	}
	buildTypes {
		getByName("release") {
			isMinifyEnabled = false
		}
	}
	compileOptions {
		sourceCompatibility = JavaVersion.VERSION_11
		targetCompatibility = JavaVersion.VERSION_11
	}
	sourceSets {
		getByName("debug") {
			jniLibs.srcDir(project(":shared").layout.buildDirectory.dir("intermediates/rust/aarch64-linux-android/debug"))
			jniLibs.srcDir(project(":shared").layout.buildDirectory.dir("intermediates/rust/x86_64-linux-android/debug"))
		}
	}
}

tasks.configureEach {
	if (name == "mergeDebugJniLibFolders" || name == "mergeDebugNativeLibs") {
		dependsOn(
			":shared:copyAndroidAndroidArm64Debug",
			":shared:copyAndroidAndroidX64Debug",
		)
	}
}

val verifyDebugVnidropLibraries = tasks.register<VerifyVnidropLibrariesTask>("verifyDebugVnidropLibraries") {
	group = "verification"
	description = "Verifies that the debug APK packages VniDrop for every supported Android ABI."
	dependsOn("assembleDebug")
	apk.set(layout.buildDirectory.file("outputs/apk/debug/androidApp-debug.apk"))
	requiredLibraries.set(
		setOf(
			"lib/arm64-v8a/libvnidrop.so",
			"lib/x86_64/libvnidrop.so",
		),
	)
}

tasks.named("check") {
	dependsOn(verifyDebugVnidropLibraries)
}
