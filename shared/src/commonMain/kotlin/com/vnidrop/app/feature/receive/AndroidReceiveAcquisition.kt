package com.vnidrop.app.feature.receive

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.FolderAccessStatus
import com.vnidrop.app.ui.components.ProgressRow
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiText
import com.vnidrop.app.ui.feedback.VniDropSnackbarHost
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.FullscreenDialog
import com.vnidrop.app.ui.state.formatBytes
import com.vnidrop.app.ui.state.progressForTransfer
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AndroidReceiveAcquisition(
	core: CoreState,
	state: ReceiveState,
	actions: ReceiveInvitationActions,
	onDismiss: () -> Unit,
	onNameChanged: (String) -> Unit,
	onResult: (ReceiveMethod, Result<String>) -> Unit,
	onReceive: () -> Unit,
	onCancelReceive: () -> Unit,
	messages: UiMessageController?,
) {
	FullscreenDialog(onDismiss) {
		Scaffold(
			modifier = Modifier.fillMaxSize().imePadding().testTag("receive-acquisition"),
			snackbarHost = { messages?.let { VniDropSnackbarHost(it) } },
			topBar = {
				TopAppBar(
					title = { Text(stringResource(if (state.ticket.isBlank()) Res.string.receive_title else Res.string.receive_review_title), maxLines = 1, overflow = TextOverflow.Ellipsis) },
					navigationIcon = { IconButton(onClick = onDismiss, enabled = !state.isReceiving && !state.isInspecting) { PlatformIcon(AppIcon.Close, stringResource(Res.string.button_close)) } },
				)
			},
			bottomBar = {
				if (state.inspection != null) Surface {
					Button(
						onClick = if (state.isReceiving) onCancelReceive else onReceive,
						enabled = state.isReceiving || state.canReceive(core.isInitialized),
						modifier = Modifier.navigationBarsPadding().padding(16.dp).fillMaxWidth(),
						contentPadding = PaddingValues(16.dp),
					) { Text(stringResource(if (state.isReceiving) Res.string.button_cancel_receive else Res.string.button_receive)) }
				}
			},
		) { padding ->
			LazyColumn(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding), contentPadding = PaddingValues(bottom = 24.dp)) {
				if (state.ticket.isBlank()) {
					item { Text(stringResource(Res.string.receive_choose_method_body), Modifier.padding(24.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
					item { AcquisitionMethod(AppIcon.Scan, stringResource(Res.string.receive_method_scan), stringResource(Res.string.receive_method_scan_description), actions.qrAvailability) { actions.scanQrCode { onResult(ReceiveMethod.QrCode, it) } } }
					item { AcquisitionMethod(AppIcon.File, stringResource(Res.string.receive_method_file), stringResource(Res.string.receive_method_file_description), actions.fileAvailability) { actions.pickInvitation { onResult(ReceiveMethod.InvitationFile, it) } } }
				} else {
					if (state.isInspecting) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
					state.inspection?.metadata?.let { metadata ->
						item {
							Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
								Text(metadata.transferName, style = MaterialTheme.typography.headlineSmall)
								val count = metadata.fileCount.coerceAtMost(Int.MAX_VALUE.toULong()).toInt()
								Text("${pluralStringResource(Res.plurals.transfer_file_count, count, count)} · ${formatBytes(metadata.totalSize)}", color = MaterialTheme.colorScheme.onSurfaceVariant)
							}
						}
						item { OutlinedTextField(state.receiverName, onNameChanged, modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp).fillMaxWidth(), label = { Text(stringResource(Res.string.field_receiver_name)) }, enabled = !state.isReceiving, singleLine = true) }
						item { ListItem(headlineContent = { Text(stringResource(Res.string.storage_title)) }, supportingContent = { Text(state.receiveFolder?.displayName ?: stringResource(Res.string.value_unavailable), color = if (state.folderAccessStatus == FolderAccessStatus.Writable) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error) }, leadingContent = { PlatformIcon(AppIcon.Folder, null) }) }
						if (state.isReceiving) item {
							val progress = state.activeReceiveTransferId?.let { progressForTransfer(core.events, it) }
							Column(Modifier.padding(16.dp)) { ProgressRow(progress?.label ?: Res.string.progress_receiving, progress?.progress, detail = progress?.detail) }
						}
					}
					state.lastReceiveError?.let { error -> item { Text(when (error) { is UiText.Dynamic -> error.value; is UiText.Resource -> stringResource(error.resource, *error.formatArgs.toTypedArray()) }, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) } }
				}
			}
		}
	}
}

@Composable
private fun AcquisitionMethod(icon: AppIcon, title: String, description: String, availability: ReceiveMethodAvailability, onClick: () -> Unit) {
	if (availability == ReceiveMethodAvailability.Hidden) return
	ListItem(
		modifier = Modifier.clickable(enabled = availability == ReceiveMethodAvailability.Available, onClick = onClick),
		headlineContent = { Text(title) },
		supportingContent = { Text(if (availability == ReceiveMethodAvailability.Available) description else stringResource(Res.string.value_unavailable)) },
		leadingContent = { PlatformIcon(icon, null) },
	)
}
