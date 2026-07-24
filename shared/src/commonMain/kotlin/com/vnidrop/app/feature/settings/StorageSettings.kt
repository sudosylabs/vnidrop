package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.components.AdaptiveDrawer
import com.vnidrop.app.ui.components.DestructiveButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.state.formatBytes
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.storage_app_data
import vnidrop.shared.generated.resources.storage_calculating
import vnidrop.shared.generated.resources.storage_clear_transfer_cache
import vnidrop.shared.generated.resources.storage_clear_transfer_cache_description
import vnidrop.shared.generated.resources.storage_clearing_transfer_cache
import vnidrop.shared.generated.resources.storage_delete_transfers
import vnidrop.shared.generated.resources.storage_delete_transfers_description
import vnidrop.shared.generated.resources.storage_deleting
import vnidrop.shared.generated.resources.storage_total
import vnidrop.shared.generated.resources.storage_received_files
import vnidrop.shared.generated.resources.storage_temporary
import vnidrop.shared.generated.resources.storage_title
import vnidrop.shared.generated.resources.storage_transfer_data
import vnidrop.shared.generated.resources.storage_footer

@Composable
internal fun StorageSettings(
	state: SettingsState,
	windowClass: WindowClass,
	onDeleteAllTransfers: () -> Unit,
	onClearTransferCache: () -> Unit,
	onBack: () -> Unit,
	showBack: Boolean,
) {
	var showDeleteConfirmation by rememberSaveable { mutableStateOf(false) }
	var showClearCacheConfirmation by rememberSaveable { mutableStateOf(false) }
	Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
		SettingsTopBar(stringResource(Res.string.storage_title), onBack, showBack)
		val storage = state.storage
		if (storage == null || state.isCalculatingStorage) {
			SettingsGroup {
				StorageRow(
					title = stringResource(Res.string.storage_calculating),
					trailing = { CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) },
				)
			}
		} else {
			SettingsGroup {
				StorageRow(stringResource(Res.string.storage_received_files), storage.receivedBytes)
				SettingsDivider(startPadding = 16.dp)
				StorageRow(stringResource(Res.string.storage_transfer_data), storage.transferCacheBytes)
				SettingsDivider(startPadding = 16.dp)
				StorageRow(stringResource(Res.string.storage_app_data), storage.appDataBytes)
				SettingsDivider(startPadding = 16.dp)
				StorageRow(stringResource(Res.string.storage_temporary), storage.temporaryBytes)
				SettingsDivider(startPadding = 16.dp)
				StorageRow(
					title = stringResource(Res.string.storage_total),
					bytes = storage.deviceImpactBytes,
					emphasized = true,
				)
			}
		}
		SecondaryButton(
			text = stringResource(
				if (state.isClearingTransferCache) {
					Res.string.storage_clearing_transfer_cache
				} else {
					Res.string.storage_clear_transfer_cache
				},
			),
			onClick = { showClearCacheConfirmation = true },
			enabled = !state.isDeletingTransfers &&
				!state.isClearingTransferCache &&
				!state.isCalculatingStorage &&
				!state.hasActiveNetworkWork,
		)
		Button(
			onClick = { showDeleteConfirmation = true },
			enabled = !state.isDeletingTransfers &&
				!state.isClearingTransferCache &&
				!state.isCalculatingStorage,
			colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
		) {
			Text(stringResource(if (state.isDeletingTransfers) Res.string.storage_deleting else Res.string.storage_delete_transfers))
		}
		Text(
			stringResource(Res.string.storage_footer),
			style = MaterialTheme.typography.bodySmall,
			color = LocalVniDropColors.current.foregroundLighter,
		)
	}
	if (showClearCacheConfirmation) {
		AdaptiveDrawer(
			windowClass = windowClass,
			onDismissRequest = { showClearCacheConfirmation = false },
		) {
			ClearTransferCachePanel(
				onCancel = { showClearCacheConfirmation = false },
				onConfirm = {
					showClearCacheConfirmation = false
					onClearTransferCache()
				},
			)
		}
	}
	if (showDeleteConfirmation) {
		AdaptiveDrawer(
			windowClass = windowClass,
			onDismissRequest = { showDeleteConfirmation = false },
		) {
			DeleteAllTransfersPanel(
				onCancel = { showDeleteConfirmation = false },
				onConfirm = {
					showDeleteConfirmation = false
					onDeleteAllTransfers()
				},
			)
		}
	}
}

@Composable
private fun ClearTransferCachePanel(
	onCancel: () -> Unit,
	onConfirm: () -> Unit,
) {
	Column(
		Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp),
		verticalArrangement = Arrangement.spacedBy(14.dp),
	) {
		Text(
			stringResource(Res.string.storage_clear_transfer_cache),
			style = MaterialTheme.typography.titleLarge,
			fontWeight = FontWeight.Bold,
		)
		Text(
			stringResource(Res.string.storage_clear_transfer_cache_description),
			color = LocalVniDropColors.current.foregroundLighter,
			style = MaterialTheme.typography.bodyMedium,
		)
		Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End)) {
			SecondaryButton(stringResource(Res.string.button_cancel), onClick = onCancel)
			DestructiveButton(
				stringResource(Res.string.storage_clear_transfer_cache),
				onClick = onConfirm,
				modifier = Modifier.testTag("confirm-clear-transfer-cache"),
			)
		}
	}
}

@Composable
private fun DeleteAllTransfersPanel(
	onCancel: () -> Unit,
	onConfirm: () -> Unit,
) {
	Column(
		Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp),
		verticalArrangement = Arrangement.spacedBy(14.dp),
	) {
		Text(
			stringResource(Res.string.storage_delete_transfers),
			style = MaterialTheme.typography.titleLarge,
			fontWeight = FontWeight.Bold,
		)
		Text(
			stringResource(Res.string.storage_delete_transfers_description),
			color = LocalVniDropColors.current.foregroundLighter,
			style = MaterialTheme.typography.bodyMedium,
		)
		Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End)) {
			SecondaryButton(stringResource(Res.string.button_cancel), onClick = onCancel)
			DestructiveButton(
				stringResource(Res.string.storage_delete_transfers),
				onClick = onConfirm,
				modifier = Modifier.testTag("confirm-delete-all-transfers"),
			)
		}
	}
}

@Composable
private fun StorageRow(
	title: String,
	bytes: ULong? = null,
	emphasized: Boolean = false,
	trailing: @Composable (() -> Unit)? = null,
) {
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.heightIn(min = 52.dp)
			.padding(horizontal = 16.dp, vertical = 10.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		Text(
			title,
			modifier = Modifier.weight(1f),
			style = MaterialTheme.typography.bodyLarge,
			fontWeight = if (emphasized) FontWeight.SemiBold else FontWeight.Normal,
		)
		if (bytes != null) {
			Text(
				formatBytes(bytes),
				color = LocalVniDropColors.current.foregroundLighter,
				style = MaterialTheme.typography.bodyMedium,
				fontWeight = if (emphasized) FontWeight.SemiBold else FontWeight.Normal,
			)
		}
		trailing?.invoke()
	}
}
