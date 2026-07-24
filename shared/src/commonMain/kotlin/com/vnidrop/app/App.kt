package com.vnidrop.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.vnidrop.app.feature.app.AppViewModel
import com.vnidrop.app.feature.app.AppGraphViewModel
import com.vnidrop.app.feature.approvals.ApprovalModalHost
import com.vnidrop.app.feature.receive.ReceiveRoute
import com.vnidrop.app.feature.receive.ReceiveFloatingAction
import com.vnidrop.app.feature.receive.ReceiveViewModel
import com.vnidrop.app.feature.receive.ReceiveMethod
import com.vnidrop.app.feature.send.SendRoute
import com.vnidrop.app.feature.send.SendFloatingAction
import com.vnidrop.app.feature.send.SendViewModel
import com.vnidrop.app.feature.settings.SettingsRoute
import com.vnidrop.app.feature.settings.SettingsViewModel
import com.vnidrop.app.platform.PlatformSystemAppearance
import com.vnidrop.app.ui.feedback.VniDropSnackbarHost
import com.vnidrop.app.ui.navigation.AppDestination
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.platform.contentWindowClassFor
import com.vnidrop.app.ui.platform.usesMobilePresentation
import com.vnidrop.app.ui.shell.AppShell
import com.vnidrop.app.ui.shell.ScreenScrollContainer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import com.vnidrop.app.ui.theme.VniDropTheme
import com.vnidrop.app.ui.theme.rememberResolvedDarkTheme
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.app_starting

@Composable
fun App(
	dependencies: AppDependencies,
	windowChromeTopInset: Dp = 0.dp,
	windowContentTopStartRadius: Dp = 0.dp,
	useNativeWindowBackdrop: Boolean = false,
	onResolvedDarkThemeChanged: (Boolean) -> Unit = {},
	windowChrome: (@Composable () -> Unit)? = null,
) {
	val graphHolder = viewModel { AppGraphViewModel(dependencies) }
	val graph = graphHolder.graph

	val appViewModel = viewModel {
		AppViewModel(
			dependencies.environment,
			graph.coreRepository,
			graph.preferencesRepository,
			graph.messages,
			graph.diagnostics,
		)
	}
	val sendViewModel = viewModel {
		SendViewModel(
			graph.coreRepository,
			dependencies.fileSystemService,
			graph.preferencesRepository,
			graph.filePreviewRepository,
			graph.messages,
		)
	}
	val receiveViewModel = viewModel {
		ReceiveViewModel(graph.coreRepository, dependencies.fileSystemService, graph.preferencesRepository, graph.messages)
	}
	val settingsViewModel = viewModel {
		SettingsViewModel(
			dependencies.environment,
			dependencies.deviceInfoProvider,
			dependencies.fileSystemService,
			graph.coreRepository,
			graph.preferencesRepository,
			dependencies.localNotificationService,
			graph.messages,
			graph.diagnostics.bugReports,
			graph.diagnostics,
		)
	}
	val appState by appViewModel.state.collectAsStateWithLifecycle()
	val sendState by sendViewModel.state.collectAsStateWithLifecycle()
	val sendCoreState by sendViewModel.coreState.collectAsStateWithLifecycle()
	val receiveState by receiveViewModel.state.collectAsStateWithLifecycle()
	val receiveCoreState by receiveViewModel.coreState.collectAsStateWithLifecycle()
	val approvalState by graph.approvalCoordinator.state.collectAsStateWithLifecycle()
	val lifecycleOwner = LocalLifecycleOwner.current
	LaunchedEffect(dependencies.externalInvitations, appViewModel, receiveViewModel) {
		dependencies.externalInvitations.invitations.collect { invitation ->
			appViewModel.selectDestination(AppDestination.Receive)
			if (invitation.isSuccess) {
				// Cold-open can race app startup. Wait for core before inspecting so
				// the ticket is not dropped as "not initialized", but do not block
				// forever if initialization failed.
				val ready = withTimeoutOrNull(30_000) {
					receiveViewModel.coreState.filter { it.isInitialized }.first()
				}
				if (ready == null) {
					receiveViewModel.onInvitationResult(
						ReceiveMethod.InvitationFile,
						Result.failure(IllegalStateException("VniDrop is still starting up")),
					)
					return@collect
				}
				// Avoid clobbering an in-flight inspection or receive.
				receiveViewModel.state.filter { state ->
					!state.isInspecting && !state.isReceiving && state.ticket.isBlank()
				}.first()
			}
			receiveViewModel.onInvitationResult(ReceiveMethod.InvitationFile, invitation)
		}
	}
	DisposableEffect(lifecycleOwner, graph, settingsViewModel) {
		val observer = LifecycleEventObserver { _, event ->
			when (event) {
				Lifecycle.Event.ON_START -> {
					graph.visibility.setForeground(true)
					settingsViewModel.refreshNotificationPermission()
				}
				Lifecycle.Event.ON_STOP -> graph.visibility.setForeground(false)
				else -> Unit
			}
		}
		lifecycleOwner.lifecycle.addObserver(observer)
		onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
	}

	val darkTheme = rememberResolvedDarkTheme(appState.themeMode)
	LaunchedEffect(darkTheme, onResolvedDarkThemeChanged) {
		onResolvedDarkThemeChanged(darkTheme)
	}
	PlatformSystemAppearance(darkTheme)
	CompositionLocalProvider(LocalUiPlatform provides dependencies.environment.uiPlatform) {
		VniDropTheme(isDarkTheme = darkTheme) {
			Box(
				modifier = Modifier
					.fillMaxSize()
					.background(
						if (useNativeWindowBackdrop) Color.Transparent
						else LocalVniDropColors.current.backgroundSurface200,
					),
			) {
				BoxWithConstraints(
					modifier = Modifier
						.fillMaxSize()
						.padding(top = windowChromeTopInset),
				) {
					val windowClass = contentWindowClassFor(dependencies.environment.uiPlatform, maxWidth.value)
					val usesFloatingActions = usesMobilePresentation(dependencies.environment.uiPlatform, windowClass)
					val showSendAction = appState.destination == AppDestination.Send &&
						usesFloatingActions &&
						sendState.selectedTransferId?.let { selectedId ->
							sendCoreState.transfers.any { it.transferId == selectedId }
						} != true &&
						sendCoreState.transfers.any { it.direction == TransferDirection.Send }
					val showReceiveAction = appState.destination == AppDestination.Receive &&
						usesFloatingActions &&
						!receiveState.isAcquisitionOpen &&
						receiveCoreState.transfers.any { it.direction == TransferDirection.Receive }
					AppShell(
						modifier = Modifier.fillMaxSize(),
						selectedDestination = appState.destination,
						windowClass = windowClass,
						uiPlatform = dependencies.environment.uiPlatform,
						mainContentTopStartRadius = windowContentTopStartRadius,
						useNativeWindowBackdrop = useNativeWindowBackdrop,
						onDestinationSelected = appViewModel::selectDestination,
						overlay = {
							VniDropSnackbarHost(graph.messages, Modifier.align(Alignment.BottomCenter))
						},
						floatingAction = if (showSendAction) {
							{
								SendFloatingAction(
									onClick = sendViewModel::openComposer,
									modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
								)
							}
						} else if (showReceiveAction) {
							{
								ReceiveFloatingAction(
									onClick = receiveViewModel::openAcquisition,
									modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
								)
							}
						} else {
							null
						},
					) {
						when (appState.destination) {
							AppDestination.Send -> SendRoute(sendViewModel, windowClass)
							AppDestination.Receive -> ReceiveRoute(receiveViewModel, windowClass)
							AppDestination.Settings -> ScreenScrollContainer { SettingsRoute(settingsViewModel, windowClass) }
						}
					}
					ApprovalModalHost(
						state = approvalState,
						onAccept = graph.approvalCoordinator::accept,
						onRefuse = graph.approvalCoordinator::refuse,
					)
				}
				windowChrome?.invoke()
				val startingLabel = stringResource(Res.string.app_starting)
				AnimatedVisibility(
					visible = !sendCoreState.isInitialized,
					enter = fadeIn(),
					exit = fadeOut(),
				) {
					Box(
						modifier = Modifier
							.fillMaxSize()
							.background(LocalVniDropColors.current.backgroundSurface100)
							.semantics { contentDescription = startingLabel },
						contentAlignment = Alignment.Center,
					) {
						Column(
							horizontalAlignment = Alignment.CenterHorizontally,
							verticalArrangement = Arrangement.spacedBy(16.dp),
						) {
							CircularProgressIndicator()
							Text(
								startingLabel,
								style = MaterialTheme.typography.titleMedium,
								color = LocalVniDropColors.current.foregroundLighter,
							)
						}
					}
				}
			}
		}
	}
}
