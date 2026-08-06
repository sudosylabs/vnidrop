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
	}

	var body: some View {
		GeometryReader { proxy in
			let windowClass = windowClassFor(width: proxy.size.width)
			let isDark = resolveDarkTheme(appModel.themeMode, systemDark: systemDark)
			ZStack {
				navigation(windowClass: windowClass)
				// Observe the coordinator/messages from the *persisted* `graph`
				// StateObject. Deriving them in `init` bound the view to a throwaway
				// AppGraph rebuilt on every re-init, whose coordinator never receives
				// core events — so the approval modal never appeared.
				ApprovalLayer(
					approvals: graph.approvalCoordinator,
					sendModel: sendModel
				)
				ContactPromptLayer(
					contacts: graph.contactsModel,
					receiveModel: receiveModel,
					approvals: graph.approvalCoordinator
				)
				// Top-most so the toast is never covered by the approval overlay's
				// full-bleed clear layer. Observes the live `graph.messages` directly.
				SnackbarHost(controller: graph.messages)
			}
			.overlay {
				// A small, unobtrusive indicator while the core finishes its async
				// startup — otherwise the lists look empty and the app feels stalled.
				if !sendModel.coreState.isInitialized {
					CoreStartingOverlay()
				}
			}
			.animation(.easeInOut(duration: 0.25), value: sendModel.coreState.isInitialized)
			.vniDropTheme(isDark: isDark)
			.preferredColorScheme(appModel.themeMode.preferredColorScheme)
			.environment(\.vniColors, isDark ? .dark : .light)
		}
		.platformPickers(settingsModel: settingsModel)
		.task { await consumeExternalInvitations() }
		.onChange(of: scenePhase) { _, phase in
			switch phase {
			case .active:
				graph.visibility.setForeground(true)
				graph.backgroundActivity.didBecomeForeground()
				settingsModel.refreshNotificationPermission()
				// Reconcile against the durable snapshot: while the window was
				// unfocused/occluded (common on macOS) live events may not have
				// rendered, leaving progress/status stale.
				Task { _ = await graph.coreRepository.refresh() }
				// Opt-in and foreground-only: collecting transfers held for this
				// device also tells every contact that the app was opened.
				Task { await graph.contactsModel.checkForOffersOnForeground() }
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
		case .settings:
			SettingsScreen(model: settingsModel, contacts: graph.contactsModel, windowClass: windowClass)
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

/// A full-window cover with a centered spinner shown while the core is starting.
private struct CoreStartingOverlay: View {
	var body: some View {
		ZStack {
			backgroundColor.ignoresSafeArea()
			VStack(spacing: 16) {
				ProgressView().controlSize(.large)
				Text(String(localized: L10n.App.starting))
					.font(.headline)
					.foregroundStyle(.secondary)
			}
		}
		.transition(.opacity)
		.accessibilityElement(children: .combine)
		.accessibilityLabel(Text(String(localized: L10n.App.starting)))
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

/// Hosts the device-history consent prompts, alongside `ApprovalLayer`.
///
/// Separate from the approval layer because the two never compete: an approval
/// belongs to a transfer this device is sending, and these belong to a device
/// asking to reach it. Both are suppressed while the other is up so the user is
/// never answering two modals at once.
private struct ContactPromptLayer: View {
	@ObservedObject var contacts: ContactsModel
	let receiveModel: ReceiveModel
	@ObservedObject var approvals: ApprovalCoordinator

	@State private var showPrompt = false

	var body: some View {
		ContactPromptHost(
			isPresented: $showPrompt,
			state: contacts.state,
			onPairingResponse: { endpointId, accepted in
				Task { await contacts.respondToPairing(endpointId: endpointId, accepted: accepted) }
			},
			onOfferResponse: { offerId, accepted in
				Task {
					// The ticket is released only on acceptance; the receive then
					// runs through the ordinary path so the platform picks the
					// destination.
					if let ticket = await contacts.respondToOffer(offerId: offerId, accepted: accepted) {
						receiveModel.receiveOffered(ticket: ticket)
					}
				}
			},
			onSuggestionResponse: { suggestion, accepted in
				if accepted {
					Task { await contacts.acceptSuggestion(suggestion) }
				} else {
					contacts.declineSuggestion(suggestion)
				}
			}
		)
		.onChange(of: promptKey) { _, key in
			showPrompt = key != nil
		}
	}

	/// One identity for "is there something to answer", so an offer replacing a
	/// pairing prompt re-presents rather than silently swapping content.
	private var promptKey: String? {
		guard approvals.state.current == nil else { return nil }
		if let offer = contacts.state.currentOffer { return "offer-\(offer.offerId)" }
		if let pairing = contacts.state.currentPairing { return "pairing-\(pairing.endpointId)" }
		if let suggestion = contacts.state.currentSuggestion { return "suggest-\(suggestion.endpointId)" }
		return nil
	}
}
