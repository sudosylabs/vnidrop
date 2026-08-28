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
	testImplementation(libs.compose.uiTest)
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
			targetFormats(TargetFormat.Deb, TargetFormat.Rpm, TargetFormat.Exe)
			packageName = "VniDrop"
			packageVersion = appVersion
			description = "Send files directly across your devices"
			vendor = "Sudosy Labs"
			licenseFile.set(project.file("../LICENSE"))
			windows {
				iconFile.set(project.file("../assets/windows/app-icon.ico"))
				perUserInstall = true
				shortcut = true
				menu = true
				upgradeUuid = "E08E256E-2F07-479E-8AA9-4898D424F6C5"
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
