package com.vnidrop.app.feature.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.vnidrop.app.showsExperimentalSavedDevices
import com.vnidrop.app.core.rememberReceiveFolderPicker
import com.vnidrop.app.core.rememberShareFilePicker
import com.vnidrop.app.feature.saveddevices.SavedDevicesEffect
import com.vnidrop.app.feature.saveddevices.SavedDevicesViewModel
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.state.WindowClass

@Composable
fun SettingsRoute(
	viewModel: SettingsViewModel,
	savedDevicesViewModel: SavedDevicesViewModel,
	windowClass: WindowClass,
) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val savedDevicesState by savedDevicesViewModel.state.collectAsStateWithLifecycle()
	val showExperimental = showsExperimentalSavedDevices(LocalUiPlatform.current)
	val folderPicker = rememberReceiveFolderPicker(viewModel::onReceiveFolderPicked, viewModel::onReceiveFolderPickFailed)
	val sharePicker = rememberShareFilePicker(
		savedDevicesViewModel::onFilesPicked,
		savedDevicesViewModel::onFilePickFailed,
	)
	LaunchedEffect(viewModel) {
		viewModel.effectFlow.collect { effect ->
			when (effect) {
				SettingsEffect.OpenReceiveFolderPicker -> folderPicker.pickFolder()
			}
		}
	}
	LaunchedEffect(savedDevicesViewModel) {
		savedDevicesViewModel.effectFlow.collect { effect ->
			when (effect) {
				SavedDevicesEffect.OpenFilePicker -> sharePicker.pickFiles()
			}
		}
	}
	SettingsScreen(
		state = state,
		windowClass = windowClass,
		onSectionSelected = viewModel::selectSection,
		onUsernameChanged = viewModel::setUsername,
		onThemeModeChanged = viewModel::setThemeMode,
		onRelayModeChanged = viewModel::setRelayMode,
		onRelayUrlChanged = viewModel::setRelayUrl,
		onAddRelayUrl = viewModel::addRelayUrl,
		onRemoveRelayUrl = viewModel::removeRelayUrl,
		onApplyRelaySettings = viewModel::applyRelaySettings,
		onChooseFolder = viewModel::chooseReceiveFolder,
		onResetFolder = viewModel::resetReceiveFolder,
		onNotificationsChanged = viewModel::setNotificationsEnabled,
		onExperimentalSavedDevicesChanged = viewModel::setExperimentalSavedDevicesEnabled,
		showExperimental = showExperimental,
		savedDevicesState = savedDevicesState,
		onRememberEligibleDevice = savedDevicesViewModel::rememberEligible,
		onDeclineEligibleDevice = savedDevicesViewModel::declineEligible,
		onAcceptIncomingPairing = savedDevicesViewModel::acceptIncoming,
		onDeclineIncomingPairing = savedDevicesViewModel::declineIncoming,
		onSendToSavedDevice = savedDevicesViewModel::startSend,
		onOpenSavedDeviceLabel = savedDevicesViewModel::openLabelEditor,
		onForgetSavedDevice = savedDevicesViewModel::forget,
		onBlockSavedDevice = savedDevicesViewModel::block,
		onSavedDeviceLabelDraftChanged = savedDevicesViewModel::setLabelDraft,
		onSaveSavedDeviceLabel = savedDevicesViewModel::saveLabel,
		onClearSavedDeviceLabel = savedDevicesViewModel::clearLabel,
		onDismissSavedDeviceLabel = savedDevicesViewModel::dismissLabelEditor,
		onOpenNotificationSettings = viewModel::openNotificationSettings,
		onBugWhatChanged = viewModel::setBugWhatHappened,
		onBugExpectedChanged = viewModel::setBugExpected,
		onBugStepsChanged = viewModel::setBugSteps,
		onBugContactChanged = viewModel::setBugContact,
		onBugIncludeLogsChanged = viewModel::setBugIncludeLogs,
		onSubmitBugReport = viewModel::submitBugReport,
		onDeleteAllTransfers = viewModel::deleteAllTransfers,
		onClearTransferCache = viewModel::clearTransferCache,
		onFreeUpSpace = viewModel::freeUpSpace,
		onRefreshStorage = viewModel::loadStorageUsage,
	)
}
