import SFSafeSymbols
import SwiftUI

/// App root, ported from `App.kt`. Owns the object graph and feature models, wires
/// the adaptive shell, floating actions, snackbar host, and approval modal.
struct RootView: View {
	@StateObject private var graph: AppGraph
	@StateObject private var appModel: AppModel
	@StateObject private var sendModel: SendModel
	@StateObject private var receiveModel: ReceiveModel
	@StateObject private var settingsModel: SettingsModel
	@StateObject private var savedDevicesModel: SavedDevicesModel
	/// Held so it stays alive for the app's lifetime; it has no view of its own.
	@StateObject private var savedDeviceNotifications: SavedDeviceNotificationCoordinator

	@Environment(\.scenePhase) private var scenePhase

	init(dependencies: AppDependencies) {
		let graph = AppGraph(dependencies: dependencies)
		_graph = StateObject(wrappedValue: graph)
		_appModel = StateObject(wrappedValue: AppModel(
			environment: dependencies.environment,
			repository: graph.coreRepository,
			preferences: graph.preferencesRepository,
			messages: graph.messages
		))
		_sendModel = StateObject(wrappedValue: SendModel(
			repository: graph.coreRepository,
			fileSystemService: dependencies.fileSystemService,
			preferences: graph.preferencesRepository,
			filePreviewRepository: graph.filePreviewRepository,
			messages: graph.messages
		))
		_receiveModel = StateObject(wrappedValue: ReceiveModel(
			repository: graph.coreRepository,
			fileSystemService: dependencies.fileSystemService,
			preferences: graph.preferencesRepository,
			messages: graph.messages
		))
		_settingsModel = StateObject(wrappedValue: SettingsModel(
			environment: dependencies.environment,
			deviceInfoProvider: dependencies.deviceInfoProvider,
			fileSystemService: dependencies.fileSystemService,
			repository: graph.coreRepository,
			preferences: graph.preferencesRepository,
			notifications: dependencies.notificationService,
			messages: graph.messages,
			bugReports: NoopBugReportService()
		))
		let savedDevices = SavedDevicesModel(
			repository: graph.coreRepository,
			fileSystemService: dependencies.fileSystemService,
			preferences: graph.preferencesRepository,
			messages: graph.messages
		)
		_savedDevicesModel = StateObject(wrappedValue: savedDevices)
		// Reads the model's snapshot rather than the core directly, so it observes
		// exactly the state the UI is showing.
		_savedDeviceNotifications = StateObject(wrappedValue: SavedDeviceNotificationCoordinator(
			model: savedDevices,
			notifications: dependencies.notificationService,
			visibility: graph.visibility,
			messages: graph.messages
		))
	}

	var body: some View {
		GeometryReader { proxy in
			let windowClass = windowClassFor(width: proxy.size.width)
			let isDark = resolveDarkTheme(appModel.themeMode, systemDark: systemDark)
			ZStack {
				navigation(windowClass: windowClass)
					// Hosted at the root so a pairing request or targeted offer is
					// answerable from any tab, and suppressed while a transfer approval
					// is up so two blocking decisions never stack.
					.savedDevicePrompts(
						model: savedDevicesModel,
						suppressed: graph.approvalCoordinator.state.current != nil
					)
				// Observe the coordinator/messages from the *persisted* `graph`
				// StateObject. Deriving them in `init` bound the view to a throwaway
				// AppGraph rebuilt on every re-init, whose coordinator never receives
				// core events — so the approval modal never appeared.
				ApprovalLayer(
					approvals: graph.approvalCoordinator,
					sendModel: sendModel
				)
				// Top-most so the toast is never covered by the approval overlay's
				// full-bleed clear layer. Observes the live `graph.messages` directly.
				SnackbarHost(controller: graph.messages)
			}
			.overlay {
				// A small, unobtrusive indicator while the core finishes its async
				// startup — otherwise the lists look empty and the app feels stalled.
				if !sendModel.coreState.isInitialized {
					CoreStartingOverlay(
						error: appModel.startupError,
						detail: appModel.startupErrorDetail,
						onRetry: appModel.retryStartup,
						recovery: appModel.startupRecovery,
						isResettingIdentity: appModel.isResettingIdentity,
						onResetIdentity: {
							Task { await appModel.resetUnrecoverableIdentity() }
						}
					)
				}
			}
			.animation(.easeInOut(duration: 0.25), value: sendModel.coreState.isInitialized)
			.vniDropTheme(isDark: isDark)
			.preferredColorScheme(appModel.themeMode.preferredColorScheme)
			.environment(\.vniColors, isDark ? .dark : .light)
		}
		.platformPickers(settingsModel: settingsModel)
		.task { await consumeExternalInvitations() }
		// Launch pass; `.onChange(of: scenePhase)` doesn't fire for the initial value.
		.task { settingsModel.purgeUnreachableTrash() }
		.onChange(of: scenePhase) { _, phase in
			switch phase {
			case .active:
				graph.visibility.setForeground(true)
				graph.backgroundActivity.didBecomeForeground()
				settingsModel.refreshNotificationPermission()
				// Files deleted from the app's Documents while it was away land in a
				// trash the user can't reach on iOS; take them out on the way back in.
				settingsModel.purgeUnreachableTrash()
				// Reconcile against the durable snapshot: while the window was
				// unfocused/occluded (common on macOS) live events may not have
				// rendered, leaving progress/status stale.
				Task { _ = await graph.coreRepository.refresh() }
			case .background:
				graph.visibility.setForeground(false)
				// Hold the process open for iOS's grace window so an active
				// transfer can finish and notify before suspension.
				graph.backgroundActivity.didEnterBackground()
			case .inactive:
				graph.visibility.setForeground(false)
			@unknown default:
				break
			}
		}
		#if os(macOS)
		// macOS keeps `scenePhase == .active` even when the app loses focus, so
		// drive foreground/background off NSApplication's active state instead —
		// otherwise notifications (only posted when unfocused) never fire.
		.onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in
			graph.visibility.setForeground(false)
		}
		.onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
			graph.visibility.setForeground(true)
			settingsModel.refreshNotificationPermission()
			Task { _ = await graph.coreRepository.refresh() }
		}
		#endif
	}

	/// iOS uses a bottom tab bar; macOS uses a native source-list sidebar so each
	/// screen's toolbar lives in the detail column instead of the shared title bar.
	@ViewBuilder
	private func navigation(windowClass: WindowClass) -> some View {
		#if os(macOS)
		NavigationSplitView {
			List(AppDestination.allCases, selection: sidebarBinding) { destination in
				Label(String(localized: destination.labelKey), systemSymbol: destination.systemSymbol)
					.tag(destination)
			}
			.navigationSplitViewColumnWidth(min: 180, ideal: 200, max: 260)
		} detail: {
			screen(for: appModel.destination, windowClass: windowClass)
		}
		#else
		TabView(selection: destinationBinding) {
			ForEach(AppDestination.allCases) { destination in
				screen(for: destination, windowClass: windowClass)
					.tabItem {
						Label(String(localized: destination.labelKey), systemSymbol: destination.systemSymbol)
					}
					.tag(destination)
			}
		}
		#endif
	}

	private var sidebarBinding: Binding<AppDestination?> {
		Binding(
			get: { appModel.destination },
			set: { newValue in
				if let value = newValue {
					Task { @MainActor in appModel.selectDestination(value) }
				}
			}
		)
	}

	private var destinationBinding: Binding<AppDestination> {
		// Defer the write out of the current view-update cycle: TabView reconciles
		// its selection synchronously during body evaluation on macOS, and mutating
		// the published `destination` there triggers a "publishing within view
		// updates" warning.
		Binding(get: { appModel.destination }, set: { newValue in
			Task { @MainActor in appModel.selectDestination(newValue) }
		})
	}

	@ViewBuilder
	private func screen(for destination: AppDestination, windowClass: WindowClass) -> some View {
		switch destination {
		case .send: SendScreen(model: sendModel, windowClass: windowClass)
		case .receive: ReceiveScreen(model: receiveModel, windowClass: windowClass)
		case .savedDevices:
			SavedDevicesScreen(model: savedDevicesModel, windowClass: windowClass)
		case .settings:
			SettingsScreen(model: settingsModel, windowClass: windowClass)
		}
	}

	private var systemDark: Bool {
		#if os(iOS)
		return UITraitCollection.current.userInterfaceStyle == .dark
		#else
		return NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
		#endif
	}

	private func consumeExternalInvitations() async {
		for await invitation in graph.dependencies.externalInvitations.invitations {
			appModel.selectDestination(.receive)
			switch invitation {
			case .success(let raw):
				receiveModel.onInvitationResult(.invitationFile, .success(raw))
			case .failure(let error):
				receiveModel.onInvitationResult(.invitationFile, .failure(error))
			}
		}
	}
}

/// Hosts the approval modal, observing the coordinator passed in from the persisted
/// `AppGraph`. Kept as a child view so the `@ObservedObject` subscription is
/// established here (in `body`) against the live instance, rather than in
/// `RootView.init` against a throwaway graph.
private struct ApprovalLayer: View {
	@ObservedObject var approvals: ApprovalCoordinator
	let sendModel: SendModel

	/// Drives the approval sheet; toggled from the pending-approval `onChange` so the
	/// presentation can be deferred until the Share/QR sheet has dismissed on macOS.
	@State private var showApproval = false

	/// macOS-only: an approval arrived while a share/QR sheet was still up. We close
	/// that sheet and present the approval once its dismissal completes (see
	/// `sendModel.shareSheetsDismissed`), since macOS drops a sheet shown mid-dismissal.
	@State private var approvalAwaitingSheetDismiss = false

	var body: some View {
		ApprovalModalHost(
			isPresented: $showApproval,
			state: approvals.state,
			onAccept: approvals.accept,
			onRefuse: approvals.refuse
		)
		// A pending approval is a blocking modal. Close any open share/QR sheet first
		// (the detail-view panel *or* the list-level share sheet), then present the
		// approval sheet: the approval is presented from the app root and neither
		// platform reliably stacks it over a sheet owned by the Send screen.
		.onChange(of: approvals.state.current?.id) { _, id in
			guard id != nil else {
				showApproval = false
				approvalAwaitingSheetDismiss = false
				return
			}
			let wasShowingSheet = sendModel.state.detailPanel != nil
				|| sendModel.state.shareTargetId != nil
			sendModel.dismissShareSheets()
			#if os(macOS)
			// macOS silently drops a sheet presented while another is still dismissing,
			// so wait for that sheet's real dismissal completion before presenting.
			if wasShowingSheet {
				approvalAwaitingSheetDismiss = true
			} else {
				showApproval = true
			}
			#else
			_ = wasShowingSheet
			showApproval = true
			#endif
		}
		#if os(macOS)
		.onReceive(sendModel.shareSheetsDismissed) { _ in
			guard approvalAwaitingSheetDismiss else { return }
			approvalAwaitingSheetDismiss = false
			if approvals.state.current != nil { showApproval = true }
		}
		#endif
	}
}

/// A full-window cover shown while the core is starting — or, when startup fails,
/// the reason and a retry. This overlay sits above the snackbar host, so a failure
/// reported only through a toast would be invisible behind it and the app would
/// look like it was loading forever.
private struct CoreStartingOverlay: View {
	let error: UiText?
	/// Debug builds only; nil in Release.
	let detail: String?
	let onRetry: () -> Void
	let recovery: AppStartupRecovery?
	let isResettingIdentity: Bool
	let onResetIdentity: () -> Void
	@State private var confirmsIdentityReset = false

	var body: some View {
		ZStack {
			backgroundColor.ignoresSafeArea()
			// A repairable identity comes first: it is the one failure the user can
			// actually act on, and its own copy explains the consequences.
			if recovery == .identityUnrecoverable {
				VStack(spacing: 18) {
					Image(systemSymbol: .exclamationmarkTriangleFill)
						.font(.system(size: 44))
						.foregroundStyle(.orange)
					Text(String(localized: L10n.App.identityResetTitle))
						.font(.title2.bold())
					Text(String(localized: L10n.App.identityResetMessage))
						.multilineTextAlignment(.center)
						.foregroundStyle(.secondary)
						.frame(maxWidth: 420)
					Button(role: .destructive) {
						confirmsIdentityReset = true
					} label: {
						if isResettingIdentity {
							ProgressView()
						} else {
							Text(String(localized: L10n.App.identityResetAction))
						}
					}
					.buttonStyle(.borderedProminent)
					.disabled(isResettingIdentity)
				}
				.padding(32)
			} else if let error {
				VStack(spacing: 16) {
					Image(systemSymbol: .exclamationmarkTriangleFill)
						.font(.system(size: 34))
						.foregroundStyle(.orange)
					// Not "Starting…": startup has stopped, and saying otherwise
					// while showing an error contradicts itself.
					Text(String(localized: L10n.Error.initialization))
						.font(.headline)
						.multilineTextAlignment(.center)
					Text(error.resolved())
						.font(.subheadline)
						.foregroundStyle(.secondary)
						.multilineTextAlignment(.center)
						.textSelection(.enabled)
						.frame(maxWidth: 420)
					if let detail {
						ScrollView {
							Text(detail)
								.font(.caption.monospaced())
								.foregroundStyle(.secondary)
								.textSelection(.enabled)
								.multilineTextAlignment(.leading)
								.frame(maxWidth: .infinity, alignment: .leading)
								.padding(10)
						}
						.frame(maxWidth: 420, maxHeight: 180)
						.background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
					}
					Button(String(localized: L10n.Button.retry), action: onRetry)
						.buttonStyle(.borderedProminent)
						.controlSize(.large)
				}
				.padding(32)
			} else {
				VStack(spacing: 16) {
					ProgressView().controlSize(.large)
					Text(String(localized: L10n.App.starting))
						.font(.headline)
						.foregroundStyle(.secondary)
				}
			}
		}
		.transition(.opacity)
		.alert(
			String(localized: L10n.App.identityResetTitle),
			isPresented: $confirmsIdentityReset
		) {
			Button(String(localized: L10n.Button.cancel), role: .cancel) {}
			Button(String(localized: L10n.App.identityResetAction), role: .destructive) {
				onResetIdentity()
			}
		} message: {
			Text(String(localized: L10n.App.identityResetConfirmation))
		}
		.accessibilityElement(children: .combine)
		.accessibilityLabel(Text(accessibilityLabel))
	}

	/// Mirrors the three visual states, so VoiceOver never announces "Starting…"
	/// over a screen that has actually stopped and is asking for a decision.
	private var accessibilityLabel: String {
		if recovery == .identityUnrecoverable {
			return String(localized: L10n.App.identityResetTitle)
		}
		return error?.resolved() ?? String(localized: L10n.App.starting)
	}

	private var backgroundColor: Color {
		#if os(iOS)
		Color(uiColor: .systemBackground)
		#else
		Color(nsColor: .windowBackgroundColor)
		#endif
	}
}

#if os(iOS)
import UIKit
#else
import AppKit
#endif
