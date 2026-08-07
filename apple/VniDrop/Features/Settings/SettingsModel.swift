import Foundation
import Combine

/// Settings sections, ported from `feature/settings/SettingsViewModel.kt`.
enum SettingsSection: Hashable {
	case overview
	case preferences
	case appearance
	case notifications
	case network
	case contacts
	/// One device's detail. Part of this enum because the Settings stack has a
	/// typed path: a link carrying any other value type cannot push onto it.
	case contactDetail(endpointId: String)
	case storage
	case about
	case bugReport

	var titleKey: String.LocalizationValue {
		switch self {
		case .overview: return L10n.Settings.title
		case .preferences: return L10n.Preferences.title
		case .appearance: return L10n.Appearance.title
		case .notifications: return L10n.Notifications.title
		case .network: return L10n.Settings.networkTitle
		case .contacts, .contactDetail: return L10n.Contacts.title
		case .storage: return L10n.Storage.title
		case .about: return L10n.About.title
		case .bugReport: return L10n.About.bugReport
		}
	}
}

/// On-disk usage breakdown for the Storage screen.
struct StorageBreakdown: Equatable {
	var receivedFiles: UInt64 = 0
	var transferCache: UInt64 = 0
	var appData: UInt64 = 0
	var temporary: UInt64 = 0
	var total: UInt64 { receivedFiles + transferCache + appData + temporary }
}

struct SettingsState: Equatable {
	var selectedSection: SettingsSection = .overview
	var username = ""
	var receiveFolder: ReceiveFolder?
	var folderAccessStatus: FolderAccessStatus = .unavailable
	var isValidatingFolder = false
	var supportsCustomReceiveFolders = true
	var themeMode: ThemeMode = .system
	var notificationPermission: NotificationPermission = .notDetermined
	var relayMode: RelayPreferenceMode = .automatic
	var relayURLs: [String] = []
	var relayValidationError: RelayConfigurationValidationError?
	var relayConfigurationIsDirty = false
	var isApplyingRelayConfiguration = false
	var hasActiveNetworkWork = false
	var endpointId: String?
	var relayApplyErrorKey: String.LocalizationValue?
	var deviceInfo: DeviceInfo?
	var appVersion = ""
	var isLoadingDeviceInfo = false
	var bugWhatHappened = ""
	var bugExpected = ""
	var bugSteps = ""
	var bugContact = ""
	var bugIncludeLogs = true
	var isSubmittingBugReport = false
	var bugLogPreviewBytes = 0
	var storage: StorageBreakdown?
	var isCalculatingStorage = false
	var storageLoadFailed = false
	var isDeletingTransfers = false
	var isCleaningStorage = false

	static func == (lhs: SettingsState, rhs: SettingsState) -> Bool {
		lhs.selectedSection == rhs.selectedSection && lhs.username == rhs.username
			&& lhs.receiveFolder == rhs.receiveFolder && lhs.folderAccessStatus == rhs.folderAccessStatus
			&& lhs.isValidatingFolder == rhs.isValidatingFolder
			&& lhs.supportsCustomReceiveFolders == rhs.supportsCustomReceiveFolders
			&& lhs.themeMode == rhs.themeMode
			&& lhs.notificationPermission == rhs.notificationPermission
			&& lhs.appVersion == rhs.appVersion
			&& lhs.relayMode == rhs.relayMode && lhs.relayURLs == rhs.relayURLs
			&& lhs.relayValidationError == rhs.relayValidationError
			&& lhs.relayConfigurationIsDirty == rhs.relayConfigurationIsDirty
			&& lhs.isApplyingRelayConfiguration == rhs.isApplyingRelayConfiguration
			&& lhs.hasActiveNetworkWork == rhs.hasActiveNetworkWork
			&& lhs.endpointId == rhs.endpointId
			&& lhs.relayApplyErrorKey == rhs.relayApplyErrorKey
			&& lhs.isLoadingDeviceInfo == rhs.isLoadingDeviceInfo
			&& lhs.bugWhatHappened == rhs.bugWhatHappened && lhs.bugExpected == rhs.bugExpected
			&& lhs.bugSteps == rhs.bugSteps && lhs.bugContact == rhs.bugContact
			&& lhs.bugIncludeLogs == rhs.bugIncludeLogs && lhs.isSubmittingBugReport == rhs.isSubmittingBugReport
			&& lhs.bugLogPreviewBytes == rhs.bugLogPreviewBytes
			&& lhs.storage == rhs.storage && lhs.isCalculatingStorage == rhs.isCalculatingStorage
			&& lhs.storageLoadFailed == rhs.storageLoadFailed
			&& lhs.isDeletingTransfers == rhs.isDeletingTransfers
			&& lhs.isCleaningStorage == rhs.isCleaningStorage
			&& lhs.deviceInfo?.operatingSystem == rhs.deviceInfo?.operatingSystem
	}
}

@MainActor
final class SettingsModel: ObservableObject {
	@Published private(set) var state: SettingsState
	/// Set by the view to request the receive-folder picker (macOS).
	@Published var pendingReceiveFolderPick = false

	private let environment: PlatformEnvironment
	private let deviceInfoProvider: DeviceInfoProvider
	private let fileSystemService: FileSystemService
	private let repository: CoreGateway
	private let preferences: AppPreferencesRepository
	private let notifications: LocalNotificationService
	private let messages: UiMessageController
	private let bugReports: BugReportService

	private var usernamePersistTask: Task<Void, Never>?
	private var hasLocalUsernameDraft = false
	private var hasRelayConfigurationDraft = false
	private var cancellables = Set<AnyCancellable>()

	init(
		environment: PlatformEnvironment,
		deviceInfoProvider: DeviceInfoProvider,
		fileSystemService: FileSystemService,
		repository: CoreGateway,
		preferences: AppPreferencesRepository,
		notifications: LocalNotificationService,
		messages: UiMessageController,
		bugReports: BugReportService
	) {
		self.environment = environment
		self.deviceInfoProvider = deviceInfoProvider
		self.fileSystemService = fileSystemService
		self.repository = repository
		self.preferences = preferences
		self.notifications = notifications
		self.messages = messages
		self.bugReports = bugReports
		self.state = SettingsState(
			supportsCustomReceiveFolders: fileSystemService.supportsCustomReceiveFolders,
			appVersion: environment.appVersion
		)

		preferences.$preferences
			.sink { [weak self] prefs in
				guard let self else { return }
				let previousFolder = self.state.receiveFolder
				let folder = self.fileSystemService.effectiveReceiveFolder(prefs.receiveFolder)
				self.state.username = self.hasLocalUsernameDraft ? self.state.username : prefs.username
				self.state.receiveFolder = folder
				self.state.themeMode = prefs.themeMode
				if !self.hasRelayConfigurationDraft {
					self.state.relayMode = prefs.relayConfiguration.mode
					self.state.relayURLs = prefs.relayConfiguration.relayURLs
					self.state.relayConfigurationIsDirty = false
				}
				if folder != previousFolder { Task { await self.validateFolder(folder) } }
			}
			.store(in: &cancellables)

		repository.statePublisher
			.sink { [weak self] coreState in
				guard let self else { return }
				let hasActiveWork = (coreState.status?.activeTransfers ?? 0) > 0
					|| (coreState.status?.activeShares ?? 0) > 0
					|| coreState.transfers.contains(where: { $0.status.isActiveTransfer })
				self.state.hasActiveNetworkWork = hasActiveWork
				self.state.endpointId = coreState.status?.endpointId
				if !hasActiveWork && self.state.relayApplyErrorKey == L10n.Relay.applyActiveTransfers {
					self.state.relayApplyErrorKey = nil
				}
			}
			.store(in: &cancellables)

		refreshNotificationPermission()
		loadDeviceInfo()
	}

	/// Surfaces "nothing waiting" from the contacts screen, which has no
	/// message controller of its own.
	func reportNothingWaiting() {
		messages.tryShow(UiMessage(text: .resource(L10n.Contacts.checkNone), tone: .info))
	}

	func selectSection(_ section: SettingsSection) {
		state.selectedSection = section
		if section == .about || section == .bugReport {
			loadDeviceInfo()
			if section == .bugReport { refreshBugLogPreview() }
		}
	}

	func setUsername(_ value: String) {
		hasLocalUsernameDraft = true
		state.username = value
		usernamePersistTask?.cancel()
		usernamePersistTask = Task {
			try? await Task.sleep(nanoseconds: 350_000_000)
			if Task.isCancelled { return }
			preferences.setUsername(value)
		}
	}

	func setThemeMode(_ mode: ThemeMode) { preferences.setThemeMode(mode) }

	func chooseReceiveFolder() {
		if !fileSystemService.supportsCustomReceiveFolders { return }
		pendingReceiveFolderPick = true
	}

	func onReceiveFolderPicked(_ folder: ReceiveFolder) { preferences.setReceiveFolder(folder) }
	func onReceiveFolderPickFailed(_ reason: String) { messages.error(InvitationError.raw(reason)) }
	func resetReceiveFolder() { preferences.resetReceiveFolder() }

	/// Whether the current receive folder is the platform default (so the reset
	/// action can be hidden when it would be a no-op). Compared by location, not
	/// display name, which can differ once resolved.
	var isUsingDefaultReceiveFolder: Bool {
		guard let folder = state.receiveFolder else { return true }
		let fallback = fileSystemService.defaultReceiveFolder()
		return folder.kind == fallback.kind && folder.value == fallback.value
	}

	/// Ask the OS for notification permission. This is the only time the app can
	/// grant it; disabling or fine-tuning afterwards happens in the Settings app.
	func requestNotifications() {
		Task {
			let permission = await notifications.requestPermission()
			state.notificationPermission = permission
			if permission == .unsupported {
				messages.show(UiMessage(text: .resource(L10n.Notifications.unsupported), tone: .warning))
			}
		}
	}

	// MARK: - Network

	func setRelayMode(_ mode: RelayPreferenceMode) {
		hasRelayConfigurationDraft = true
		state.relayMode = mode
		if mode.usesCustomRelayURLs && state.relayURLs.isEmpty { state.relayURLs = [""] }
		updateRelayConfigurationDraft()
	}

	func setRelayURL(_ value: String, at index: Int) {
		guard state.relayURLs.indices.contains(index) else { return }
		hasRelayConfigurationDraft = true
		state.relayURLs[index] = value
		updateRelayConfigurationDraft()
	}

	func addRelayURL() {
		guard state.relayURLs.count < RelayConfigurationValidator.maximumRelayCount else { return }
		hasRelayConfigurationDraft = true
		state.relayURLs.append("")
		updateRelayConfigurationDraft()
	}

	func removeRelayURL(at index: Int) {
		guard state.relayURLs.indices.contains(index) else { return }
		hasRelayConfigurationDraft = true
		state.relayURLs.remove(at: index)
		if state.relayURLs.isEmpty { state.relayURLs = [""] }
		updateRelayConfigurationDraft()
	}

	func applyRelayConfiguration() {
		guard !state.isApplyingRelayConfiguration, state.relayConfigurationIsDirty else { return }

		let configuration: RelayConfiguration
		do {
			configuration = try RelayConfigurationValidator.validate(
				mode: state.relayMode,
				relayURLs: state.relayURLs,
				retainedRelayURLs: preferences.preferences.relayConfiguration.relayURLs
			)
		} catch let error as RelayConfigurationValidationError {
			state.relayValidationError = error
			state.relayApplyErrorKey = nil
			return
		} catch {
			return
		}

		let coreState = repository.state
		let hasActiveWork = (coreState.status?.activeTransfers ?? 0) > 0
			|| (coreState.status?.activeShares ?? 0) > 0
			|| coreState.transfers.contains(where: { $0.status.isActiveTransfer })
		guard !hasActiveWork else {
			state.hasActiveNetworkWork = true
			state.relayApplyErrorKey = L10n.Relay.applyActiveTransfers
			messages.show(UiMessage(text: .resource(L10n.Relay.applyActiveTransfers), tone: .warning))
			return
		}

		let previousConfiguration = preferences.preferences.relayConfiguration
		state.relayValidationError = nil
		state.relayApplyErrorKey = nil
		state.isApplyingRelayConfiguration = true
		Task {
			let applyResult = await repository.initialize(
				appDataDir: environment.defaultCoreDataDir,
				networkConfiguration: configuration
			)
			switch applyResult {
			case .success:
				hasRelayConfigurationDraft = false
				preferences.setRelayConfiguration(configuration)
				state.isApplyingRelayConfiguration = false
				state.relayConfigurationIsDirty = false
				messages.show(UiMessage(text: .resource(L10n.Relay.settingsApplied), tone: .success))
			case .failure(let error):
				if let lifecycleError = error as? CoreNetworkLifecycleError {
					state.isApplyingRelayConfiguration = false
					switch lifecycleError {
					case .activeNetworkWork:
						state.hasActiveNetworkWork = true
						state.relayApplyErrorKey = L10n.Relay.applyActiveTransfers
					case .transitionInProgress:
						state.relayApplyErrorKey = L10n.Relay.applyFailed
					}
					return
				}
				let rollbackResult = await repository.initialize(
					appDataDir: environment.defaultCoreDataDir,
					networkConfiguration: previousConfiguration
				)
				state.isApplyingRelayConfiguration = false
				if case .success = rollbackResult {
					state.relayApplyErrorKey = L10n.Relay.applyFailed
					messages.show(UiMessage(text: .resource(L10n.Relay.applyFailed), tone: .error))
				} else {
					state.relayApplyErrorKey = L10n.Relay.restoreFailed
					messages.show(UiMessage(text: .resource(L10n.Relay.restoreFailed), tone: .error))
				}
			}
		}
	}

	private func updateRelayConfigurationDraft() {
		state.relayValidationError = nil
		state.relayApplyErrorKey = nil
		let saved = preferences.preferences.relayConfiguration
		let draftURLs = state.relayMode.usesCustomRelayURLs ? state.relayURLs : saved.relayURLs
		state.relayConfigurationIsDirty = saved != RelayConfiguration(mode: state.relayMode, relayURLs: draftURLs)
		hasRelayConfigurationDraft = state.relayConfigurationIsDirty
	}

	func setBugWhatHappened(_ value: String) { state.bugWhatHappened = value }
	func setBugExpected(_ value: String) { state.bugExpected = value }
	func setBugSteps(_ value: String) { state.bugSteps = value }
	func setBugContact(_ value: String) { state.bugContact = value }
	func setBugIncludeLogs(_ value: Bool) { state.bugIncludeLogs = value }

	func submitBugReport(onSuccess: @escaping () -> Void = {}) {
		if state.isSubmittingBugReport { return }
		Task {
			let snapshot = state
			let what = snapshot.bugWhatHappened.trimmingCharacters(in: .whitespacesAndNewlines)
			let expected = snapshot.bugExpected.trimmingCharacters(in: .whitespacesAndNewlines)
			if what.isEmpty {
				messages.show(UiMessage(text: .resource(L10n.Bug.reportMissingWhat), tone: .warning))
				return
			}
			if expected.isEmpty {
				messages.show(UiMessage(text: .resource(L10n.Bug.reportMissingExpected), tone: .warning))
				return
			}
			state.isSubmittingBugReport = true
			let result = await bugReports.submit(
				BugReportDraft(
					whatHappened: what, expected: expected, steps: snapshot.bugSteps,
					contact: snapshot.bugContact, includeLogs: snapshot.bugIncludeLogs
				),
				deviceInfo: snapshot.deviceInfo
			)
			switch result {
			case .success:
				state.isSubmittingBugReport = false
				state.bugWhatHappened = ""
				state.bugExpected = ""
				state.bugSteps = ""
				state.bugContact = ""
				state.bugIncludeLogs = true
				messages.show(UiMessage(text: .resource(L10n.Bug.reportSubmitted), tone: .success))
				onSuccess()
			case .failure:
				state.isSubmittingBugReport = false
				messages.show(UiMessage(text: .resource(L10n.Bug.reportSubmitFailed), tone: .error))
			}
		}
	}

	func openNotificationSettings() {
		Task {
			if case .failure = await notifications.openSettings() {
				messages.show(UiMessage(text: .resource(L10n.Notifications.settingsOpenFailed), tone: .error))
			}
		}
	}

	/// Re-read the OS permission (called on appear and when returning to the
	/// foreground, e.g. after a trip to Settings) so the toggle stays in sync.
	func refreshNotificationPermission() {
		Task {
			state.notificationPermission = await notifications.refreshPermission()
		}
	}

	// MARK: - Storage

	/// Recomputes the on-disk usage breakdown off the main actor. Safe to call
	/// before the core is ready: it keeps the spinner up and waits for the core to
	/// finish initializing (it starts asynchronously at launch) rather than bailing.
	func loadStorageUsage() {
		if state.isCalculatingStorage { return }
		state.isCalculatingStorage = true
		state.storageLoadFailed = false
		let tempDir = NSTemporaryDirectory()
		Task {
			// The core initializes asynchronously at launch; poll briefly so opening
			// Storage early doesn't leave the summary stuck.
			var attempts = 0
			while !repository.state.isInitialized && attempts < 100 {
				try? await Task.sleep(nanoseconds: 100_000_000)
				attempts += 1
			}
			let coreResult = await repository.storageUsage()
			let artifactsResult = await repository.receivedArtifacts()
			guard case .success(let core) = coreResult,
				case .success(let artifacts) = artifactsResult else {
				state.isCalculatingStorage = false
				state.storageLoadFailed = true
				return
			}
			let diskSizes = await Task.detached {
				let received = artifacts.reduce(UInt64(0)) { total, artifact in
					total + SettingsModel.fileSize(artifact.locator)
				}
				return (received, SettingsModel.directorySize(tempDir))
			}.value
			let breakdown = StorageBreakdown(
				receivedFiles: diskSizes.0,
				transferCache: core.blobStoreBytes,
				appData: core.appDataBytes,
				temporary: diskSizes.1
			)
			state.storage = breakdown
			state.isCalculatingStorage = false
		}
	}

	/// Deletes every send/receive transfer via the core, freeing the imported
	/// shared-file content and clearing history. Node identity and received files
	/// are left untouched.
	func deleteAllTransfers() {
		if state.isDeletingTransfers { return }
		state.isDeletingTransfers = true
		Task {
			var failures = 0
			for id in repository.state.transfers.map(\.transferId) {
				if case .failure = await repository.delete(transferId: id) { failures += 1 }
			}
			_ = await repository.refresh()
			state.isDeletingTransfers = false
			if failures == 0 {
				loadStorageUsage()
				messages.show(UiMessage(text: .resource(L10n.Storage.transfersDeleted), tone: .success))
			} else {
				messages.error(InvitationError.deleteRecordsFailed)
			}
		}
	}

	/// Reclaims disk space the core's transfer deletion doesn't touch: the app's
	/// temporary directory (leftover picker/staging copies) and any stray `.Trash`
	/// folders that accumulate inside app-owned directories. Never touches received
	/// files, the core database, or user-chosen receive folders.
	func freeUpSpace() {
		if state.isCleaningStorage { return }
		// Purging staging while a transfer is mid-flight could break it.
		let hasActive = repository.state.transfers.contains {
			$0.status == .sharing || $0.status == .importing || $0.status == .receiving
		}
		if hasActive {
			messages.tryShow(UiMessage(text: .resource(L10n.Storage.cleanupBusy), tone: .warning))
			return
		}
		state.isCleaningStorage = true
		let tempDir = NSTemporaryDirectory()
		let dataDir = environment.defaultCoreDataDir
		// Only clean the receive folder's trash when it is app-owned (iOS fixed
		// Documents), never a user-chosen macOS folder like ~/Downloads.
		let receiveTrashRoot = fileSystemService.supportsCustomReceiveFolders ? nil : state.receiveFolder?.value
		Task {
			let freed = await Task.detached {
				SettingsModel.reclaimJunk(tempDir: tempDir, dataDir: dataDir, receiveTrashRoot: receiveTrashRoot)
			}.value
			state.isCleaningStorage = false
			loadStorageUsage()
			messages.show(UiMessage(
				text: .dynamic(L10n.Storage.cleanupFreed(size: formatBytes(freed))),
				tone: .success
			))
		}
	}

	/// Deletes temp-directory contents and `.Trash` folders under the given roots,
	/// returning the number of bytes reclaimed. Runs off the main actor.
	nonisolated static func reclaimJunk(tempDir: String, dataDir: String, receiveTrashRoot: String?) -> UInt64 {
		let fm = FileManager.default
		var freed: UInt64 = 0
		// Empty the temporary directory.
		if let entries = try? fm.contentsOfDirectory(atPath: tempDir) {
			for name in entries {
				let path = (tempDir as NSString).appendingPathComponent(name)
				freed += itemSize(path)
				try? fm.removeItem(atPath: path)
			}
		}
		// Remove stray `.Trash` folders inside app-owned directories.
		for root in [dataDir, receiveTrashRoot].compactMap({ $0 }) {
			for trash in trashDirectories(under: root) {
				freed += directorySize(trash)
				try? fm.removeItem(atPath: trash)
			}
		}
		return freed
	}

	/// Paths of every directory named `.Trash` under `root` (not descending into them).
	private nonisolated static func trashDirectories(under root: String) -> [String] {
		let url = URL(fileURLWithPath: root, isDirectory: true)
		guard let enumerator = FileManager.default.enumerator(
			at: url, includingPropertiesForKeys: [.isDirectoryKey]
		) else { return [] }
		var result: [String] = []
		for case let fileURL as URL in enumerator where fileURL.lastPathComponent == ".Trash" {
			result.append(fileURL.path)
			enumerator.skipDescendants()
		}
		return result
	}

	/// Allocated size of a file or directory (0 if missing).
	private nonisolated static func itemSize(_ path: String) -> UInt64 {
		var isDirectory: ObjCBool = false
		guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) else { return 0 }
		return isDirectory.boolValue ? directorySize(path) : fileSize(path)
	}

	nonisolated static func fileSize(_ path: String) -> UInt64 {
		let values = try? URL(fileURLWithPath: path).resourceValues(
			forKeys: [.isRegularFileKey, .totalFileAllocatedSizeKey, .fileSizeKey]
		)
		guard values?.isRegularFile == true else { return 0 }
		return UInt64(values?.totalFileAllocatedSize ?? values?.fileSize ?? 0)
	}

	/// Recursive size of every regular file under `path` (0 if missing).
	nonisolated static func directorySize(_ path: String) -> UInt64 {
		let url = URL(fileURLWithPath: path)
		guard let enumerator = FileManager.default.enumerator(
			at: url, includingPropertiesForKeys: [.isRegularFileKey, .totalFileAllocatedSizeKey, .fileSizeKey]
		) else { return 0 }
		var total: UInt64 = 0
		for case let fileURL as URL in enumerator {
			let values = try? fileURL.resourceValues(forKeys: [.isRegularFileKey, .totalFileAllocatedSizeKey, .fileSizeKey])
			guard values?.isRegularFile == true else { continue }
			total += UInt64(values?.totalFileAllocatedSize ?? values?.fileSize ?? 0)
		}
		return total
	}

	private func loadDeviceInfo() {
		if state.isLoadingDeviceInfo { return }
		state.isLoadingDeviceInfo = true
		Task {
			let info = await deviceInfoProvider.load()
			state.deviceInfo = info
			state.isLoadingDeviceInfo = false
		}
	}

	private func refreshBugLogPreview() {
		Task {
			let bytes = await bugReports.previewLogBytes()
			state.bugLogPreviewBytes = bytes
		}
	}

	private func validateFolder(_ folder: ReceiveFolder) async {
		state.isValidatingFolder = true
		let status = await fileSystemService.validateReceiveFolder(folder)
		state.folderAccessStatus = status
		state.isValidatingFolder = false
	}
}
