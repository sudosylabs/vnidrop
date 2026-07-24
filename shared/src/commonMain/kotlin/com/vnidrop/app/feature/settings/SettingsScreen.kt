package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.ui.theme.ThemeMode

@Composable
fun SettingsScreen(
	state: SettingsState,
	windowClass: WindowClass,
	onSectionSelected: (SettingsSection) -> Unit,
	onUsernameChanged: (String) -> Unit,
	onThemeModeChanged: (ThemeMode) -> Unit,
	onChooseFolder: () -> Unit,
	onResetFolder: () -> Unit,
	onNotificationsChanged: (Boolean) -> Unit,
	onOpenNotificationSettings: () -> Unit,
	onDiagnosticsChanged: (Boolean) -> Unit,
	onBugWhatChanged: (String) -> Unit,
	onBugExpectedChanged: (String) -> Unit,
	onBugStepsChanged: (String) -> Unit,
	onBugContactChanged: (String) -> Unit,
	onBugIncludeLogsChanged: (Boolean) -> Unit,
	onSubmitBugReport: () -> Unit,
	onDeleteAllTransfers: () -> Unit = {},
	onClearTransferCache: () -> Unit = {},
	onRelayModeChanged: (RelayMode) -> Unit = {},
	onRelayUrlChanged: (Int, String) -> Unit = { _, _ -> },
	onAddRelayUrl: () -> Unit = {},
	onRemoveRelayUrl: (Int) -> Unit = {},
	onApplyRelaySettings: () -> Unit = {},
) {
	if (windowClass == WindowClass.Desktop) {
		Row(
			modifier = Modifier.fillMaxWidth(),
			horizontalArrangement = Arrangement.spacedBy(24.dp),
		) {
			Column(Modifier.widthIn(min = 280.dp, max = 340.dp)) {
				SettingsOverview(state, onSectionSelected, largeTitle = false)
			}
			Column(Modifier.weight(1f)) {
				SettingsSectionContent(
					state = state,
					windowClass = windowClass,
					section = state.selectedSection.takeUnless { it == SettingsSection.Overview } ?: SettingsSection.Preferences,
					onBack = {},
					showBack = false,
					onSectionSelected = onSectionSelected,
					onUsernameChanged = onUsernameChanged,
					onThemeModeChanged = onThemeModeChanged,
					onChooseFolder = onChooseFolder,
					onResetFolder = onResetFolder,
					onNotificationsChanged = onNotificationsChanged,
					onOpenNotificationSettings = onOpenNotificationSettings,
					onDiagnosticsChanged = onDiagnosticsChanged,
					onBugWhatChanged = onBugWhatChanged,
					onBugExpectedChanged = onBugExpectedChanged,
					onBugStepsChanged = onBugStepsChanged,
					onBugContactChanged = onBugContactChanged,
					onBugIncludeLogsChanged = onBugIncludeLogsChanged,
					onSubmitBugReport = onSubmitBugReport,
					onDeleteAllTransfers = onDeleteAllTransfers,
					onClearTransferCache = onClearTransferCache,
					onRelayModeChanged = onRelayModeChanged,
					onRelayUrlChanged = onRelayUrlChanged,
					onAddRelayUrl = onAddRelayUrl,
					onRemoveRelayUrl = onRemoveRelayUrl,
					onApplyRelaySettings = onApplyRelaySettings,
				)
			}
		}
	} else {
		when (state.selectedSection) {
			SettingsSection.Overview -> SettingsOverview(state, onSectionSelected, largeTitle = true)
			else -> SettingsSectionContent(
				state = state,
				windowClass = windowClass,
				section = state.selectedSection,
				onBack = {
					onSectionSelected(
						if (state.selectedSection == SettingsSection.BugReport) {
							SettingsSection.About
						} else {
							SettingsSection.Overview
						},
					)
				},
				showBack = true,
				onSectionSelected = onSectionSelected,
				onUsernameChanged = onUsernameChanged,
				onThemeModeChanged = onThemeModeChanged,
				onChooseFolder = onChooseFolder,
				onResetFolder = onResetFolder,
				onNotificationsChanged = onNotificationsChanged,
				onOpenNotificationSettings = onOpenNotificationSettings,
				onDiagnosticsChanged = onDiagnosticsChanged,
				onBugWhatChanged = onBugWhatChanged,
				onBugExpectedChanged = onBugExpectedChanged,
				onBugStepsChanged = onBugStepsChanged,
				onBugContactChanged = onBugContactChanged,
				onBugIncludeLogsChanged = onBugIncludeLogsChanged,
				onSubmitBugReport = onSubmitBugReport,
				onDeleteAllTransfers = onDeleteAllTransfers,
				onClearTransferCache = onClearTransferCache,
				onRelayModeChanged = onRelayModeChanged,
				onRelayUrlChanged = onRelayUrlChanged,
				onAddRelayUrl = onAddRelayUrl,
				onRemoveRelayUrl = onRemoveRelayUrl,
				onApplyRelaySettings = onApplyRelaySettings,
			)
		}
	}
}

@Composable
private fun SettingsSectionContent(
	state: SettingsState,
	windowClass: WindowClass,
	section: SettingsSection,
	onBack: () -> Unit,
	showBack: Boolean,
	onSectionSelected: (SettingsSection) -> Unit,
	onUsernameChanged: (String) -> Unit,
	onThemeModeChanged: (ThemeMode) -> Unit,
	onChooseFolder: () -> Unit,
	onResetFolder: () -> Unit,
	onNotificationsChanged: (Boolean) -> Unit,
	onOpenNotificationSettings: () -> Unit,
	onDiagnosticsChanged: (Boolean) -> Unit,
	onBugWhatChanged: (String) -> Unit,
	onBugExpectedChanged: (String) -> Unit,
	onBugStepsChanged: (String) -> Unit,
	onBugContactChanged: (String) -> Unit,
	onBugIncludeLogsChanged: (Boolean) -> Unit,
	onSubmitBugReport: () -> Unit,
	onDeleteAllTransfers: () -> Unit,
	onClearTransferCache: () -> Unit,
	onRelayModeChanged: (RelayMode) -> Unit,
	onRelayUrlChanged: (Int, String) -> Unit,
	onAddRelayUrl: () -> Unit,
	onRemoveRelayUrl: (Int) -> Unit,
	onApplyRelaySettings: () -> Unit,
) {
	when (section) {
		SettingsSection.Overview -> Unit
		SettingsSection.Preferences -> PreferencesSettings(state, onUsernameChanged, onChooseFolder, onResetFolder, onBack, showBack)
		SettingsSection.Appearance -> AppearanceSettings(state.themeMode, onThemeModeChanged, onBack, showBack)
		SettingsSection.Network -> NetworkSettings(
			state = state,
			onModeChanged = onRelayModeChanged,
			onUrlChanged = onRelayUrlChanged,
			onAddUrl = onAddRelayUrl,
			onRemoveUrl = onRemoveRelayUrl,
			onApply = onApplyRelaySettings,
			onBack = onBack,
			showBack = showBack,
		)
		SettingsSection.Notifications -> NotificationSettings(state, onNotificationsChanged, onOpenNotificationSettings, onBack, showBack)
		SettingsSection.Storage -> StorageSettings(
			state,
			windowClass,
			onDeleteAllTransfers,
			onClearTransferCache,
			onBack,
			showBack,
		)
		SettingsSection.About -> AboutSettings(
			state = state,
			onDiagnosticsChanged = onDiagnosticsChanged,
			onReportBug = { onSectionSelected(SettingsSection.BugReport) },
			onBack = onBack,
			showBack = showBack,
		)
		SettingsSection.BugReport -> BugReportSettings(
			state = state,
			onWhatChanged = onBugWhatChanged,
			onExpectedChanged = onBugExpectedChanged,
			onStepsChanged = onBugStepsChanged,
			onContactChanged = onBugContactChanged,
			onIncludeLogsChanged = onBugIncludeLogsChanged,
			onSubmit = onSubmitBugReport,
			onBack = onBack,
			showBack = showBack,
		)
	}
}
