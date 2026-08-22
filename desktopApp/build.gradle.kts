import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
	alias(libs.plugins.kotlinJvm)
	alias(libs.plugins.composeMultiplatform)
	alias(libs.plugins.composeCompiler)
}

val appVersion = rootProject.extra["vnidrop.productVersion"] as String

dependencies {
	implementation(projects.shared)

	implementation(compose.desktop.currentOs)
	implementation(libs.filekit.dialogs)
	implementation(libs.jna.platform)
	implementation(libs.kotlinx.coroutinesSwing)

	implementation(libs.compose.uiToolingPreview)
	testImplementation(libs.kotlin.testJunit)
}

sourceSets {
	main {
		resources {
			srcDir(rootProject.file("assets/desktop"))
			include("app-icon.png")
		}
	}
}

compose.desktop {
	application {
		mainClass = "com.vnidrop.app.MainKt"
		buildTypes.release.proguard.isEnabled.set(false)

		nativeDistributions {
			targetFormats(TargetFormat.Deb, TargetFormat.Rpm)
			packageName = "VniDrop"
			packageVersion = appVersion
			description = "Send files directly across your devices"
			vendor = "Sudosy Labs"
			licenseFile.set(project.file("../LICENSE"))
			windows {
				iconFile.set(project.file("../assets/windows/app-icon.ico"))
			}
			linux {
				packageName = "vnidrop"
				iconFile.set(project.file("../assets/linux/app-icon.png"))
				modules("jdk.security.auth", "jdk.unsupported")
				debMaintainer = "support@sudosy.fr"
				appRelease = "1"
				rpmLicenseType = "Apache-2.0"
			}
			fileAssociation(
				mimeType = "application/vnd.vnidrop.transfer",
				extension = "vnd",
				description = "VniDrop Invitation",
			)
		}
	}
}
