#if os(macOS)
import Foundation
import AppKit

/// Builds the macOS dependency graph, mirroring `rememberIosAppDependencies`.
@MainActor
func makeAppDependencies(externalInvitations: ExternalInvitationController) -> AppDependencies {
	let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
	let host = Host.current().localizedName ?? "Mac"
	let env = PlatformEnvironment(
		name: "macOS " + ProcessInfo.processInfo.operatingSystemVersionString,
		appVersion: version,
		defaultCoreDataDir: applicationDataDirectory(),
		defaultUsername: host
	)
	return AppDependencies(
		environment: env,
		deviceInfoProvider: MacDeviceInfoProvider(),
		fileSystemService: MacFileSystemService(),
		notificationService: LocalNotificationService(),
		externalInvitations: externalInvitations
	)
}

private func applicationDataDirectory() -> String {
	let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
	return base.appendingPathComponent("VniDrop").path
}

private struct MacDeviceInfoProvider: DeviceInfoProvider {
	func load() async -> DeviceInfo {
		DeviceInfo(
			deviceName: Host.current().localizedName,
			deviceModel: modelIdentifier(),
			operatingSystem: "macOS " + ProcessInfo.processInfo.operatingSystemVersionString,
			network: nil,
			batteryLevel: nil
		)
	}

	private func modelIdentifier() -> String? {
		var size = 0
		sysctlbyname("hw.model", nil, &size, nil, 0)
		guard size > 0 else { return nil }
		var model = [UInt8](repeating: 0, count: size)
		sysctlbyname("hw.model", &model, &size, nil, 0)
		// sysctl reports a NUL-terminated C string; drop the terminator(s).
		return String(decoding: model.prefix(while: { $0 != 0 }), as: UTF8.self)
	}
}
#endif
