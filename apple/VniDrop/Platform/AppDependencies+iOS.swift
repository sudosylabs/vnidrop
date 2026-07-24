#if os(iOS)
import Foundation
import UIKit

/// Builds the iOS dependency graph, ported from `rememberIosAppDependencies`.
@MainActor
func makeAppDependencies(externalInvitations: ExternalInvitationController) -> AppDependencies {
	let device = UIDevice.current
	let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.1.0"
	let env = PlatformEnvironment(
		name: "\(device.systemName) \(device.systemVersion)",
		appVersion: version,
		defaultCoreDataDir: applicationDataDirectory(),
		defaultUsername: device.name.isEmpty ? "Receiver" : device.name
	)
	return AppDependencies(
		environment: env,
		deviceInfoProvider: IosDeviceInfoProvider(),
		fileSystemService: IosFileSystemService(),
		notificationService: LocalNotificationService(),
		externalInvitations: externalInvitations
	)
}

private func applicationDataDirectory() -> String {
	let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
	return base.appendingPathComponent("VniDrop").path
}

private struct IosDeviceInfoProvider: DeviceInfoProvider {
	@MainActor
	func load() async -> DeviceInfo {
		let device = UIDevice.current
		let battery: String? = {
			let wasMonitoring = device.isBatteryMonitoringEnabled
			device.isBatteryMonitoringEnabled = true
			defer { device.isBatteryMonitoringEnabled = wasMonitoring }
			let level = device.batteryLevel
			return level >= 0 ? L10n.Battery.levelValue(level: "\(Int(level * 100))") : nil
		}()
		return DeviceInfo(
			deviceName: device.name,
			deviceModel: device.model,
			operatingSystem: "\(device.systemName) \(device.systemVersion)",
			network: nil,
			batteryLevel: battery
		)
	}
}
#endif
