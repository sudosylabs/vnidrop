package com.vnidrop.app.feature.send

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.formatBytes
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AndroidTransferComposer(
	coreInitialized: Boolean,
	state: TransferDraftState,
	onSelectFile: () -> Unit,
	onSelectFolder: () -> Unit,
	onRemoveFile: (DraftSourceId) -> Unit,
	onTransferNameChanged: (String) -> Unit,
	onSenderNameChanged: (String) -> Unit,
	onAccessPolicyChanged: (ShareAccessPolicy) -> Unit,
	onSubmit: () -> Unit,
	onDismiss: () -> Unit,
	snackbarHost: @Composable () -> Unit,
) {
	val editable = !state.isSubmitting && !state.isPicking
	Scaffold(
		modifier = Modifier.fillMaxSize().imePadding().testTag("android-transfer-composer"),
		topBar = {
			TopAppBar(
				title = { Text(stringResource(if (state.sources.isEmpty()) Res.string.send_choose_file_title else Res.string.send_review_title), maxLines = 1, overflow = TextOverflow.Ellipsis) },
				navigationIcon = {
					IconButton(onClick = onDismiss, enabled = !state.isSubmitting) { PlatformIcon(AppIcon.Close, stringResource(Res.string.button_close)) }
				},
			)
		},
		snackbarHost = snackbarHost,
		bottomBar = {
			if (state.sources.isNotEmpty()) Surface(color = MaterialTheme.colorScheme.surface) {
				Button(
					onClick = onSubmit,
					enabled = state.canSubmit(coreInitialized),
					modifier = Modifier.navigationBarsPadding().padding(horizontal = 24.dp, vertical = 12.dp).fillMaxWidth().testTag("submit-transfer"),
					contentPadding = PaddingValues(16.dp),
				) {
					Text(stringResource(when {
						state.isSubmitting -> Res.string.button_sharing_file
						state.destination is TransferDraftDestination.Targeted -> Res.string.saved_devices_send_action
						else -> Res.string.button_share_file
					}))
				}
			}
		},
	) { insets ->
		LazyColumn(
			modifier = Modifier.fillMaxSize().padding(insets).consumeWindowInsets(insets),
			contentPadding = PaddingValues(bottom = 16.dp),
		) {
			if (state.isPicking || state.isSubmitting) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
			if (state.sources.isEmpty()) {
				item {
					Text(stringResource(Res.string.send_choose_file_body), Modifier.padding(horizontal = 24.dp, vertical = 16.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			} else {
				items(state.sources, key = { it.id.value }) { file ->
					ListItem(
						headlineContent = { Text(file.displayName, maxLines = 2, overflow = TextOverflow.Ellipsis) },
						supportingContent = { Text(when {
							file.isDirectory -> stringResource(Res.string.send_folder_label)
							file.sizeBytes != null -> formatBytes(file.sizeBytes)
							else -> stringResource(Res.string.send_file_size_unknown)
						}) },
						leadingContent = { PlatformIcon(if (file.isDirectory) AppIcon.Folder else AppIcon.File, null) },
						trailingContent = {
							IconButton(onClick = { onRemoveFile(file.id) }, enabled = editable) { PlatformIcon(AppIcon.Close, stringResource(Res.string.button_remove_file)) }
						},
					)
				}
			}
			item {
				ListItem(
					modifier = Modifier.clickable(enabled = editable, onClick = onSelectFile),
					headlineContent = { Text(stringResource(if (state.sources.isEmpty()) Res.string.button_choose_files else Res.string.button_change_files)) },
					leadingContent = { PlatformIcon(AppIcon.File, null, tint = MaterialTheme.colorScheme.primary) },
				)
				ListItem(
					modifier = Modifier.clickable(enabled = editable, onClick = onSelectFolder),
					headlineContent = { Text(stringResource(Res.string.button_choose_folder)) },
					leadingContent = { PlatformIcon(AppIcon.Folder, null, tint = MaterialTheme.colorScheme.primary) },
				)
			}
			if (state.sources.isNotEmpty()) {
				item {
					HorizontalDivider(Modifier.padding(horizontal = 16.dp))
					Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
						OutlinedTextField(
							value = state.transferName, onValueChange = onTransferNameChanged,
							label = { Text(stringResource(Res.string.field_transfer_name)) },
							modifier = Modifier.fillMaxWidth(), enabled = editable, singleLine = true,
						)
						when (val destination = state.destination) {
							TransferDraftDestination.Invitation -> OutlinedTextField(
								value = state.senderName, onValueChange = onSenderNameChanged,
								label = { Text(stringResource(Res.string.field_sender_name)) },
								modifier = Modifier.fillMaxWidth(), enabled = editable, singleLine = true,
							)
							is TransferDraftDestination.Targeted -> Text(destination.receiver.displayName, style = MaterialTheme.typography.titleMedium)
							null -> Unit
						}
					}
				}
				if (state.destination == TransferDraftDestination.Invitation) item {
					Text(stringResource(Res.string.send_access_title), Modifier.padding(horizontal = 24.dp, vertical = 8.dp), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
					Column(Modifier.selectableGroup()) {
						listOf(ShareAccessPolicy.RequireApproval, ShareAccessPolicy.AnyoneWithTransfer).forEach { policy ->
							val approval = policy == ShareAccessPolicy.RequireApproval
							ListItem(
								modifier = Modifier.selectable(selected = state.accessPolicy == policy, enabled = editable, role = Role.RadioButton, onClick = { onAccessPolicyChanged(policy) }),
								headlineContent = { Text(stringResource(if (approval) Res.string.send_access_approval else Res.string.send_access_anyone)) },
								supportingContent = { Text(stringResource(if (approval) Res.string.send_access_approval_description else Res.string.send_access_anyone_description)) },
								leadingContent = { RadioButton(selected = state.accessPolicy == policy, onClick = null, enabled = editable) },
							)
						}
					}
					if (state.accessPolicy == ShareAccessPolicy.AnyoneWithTransfer) Text(
						stringResource(Res.string.send_access_anyone_warning), Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
						color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium,
					)
				}
			}
		}
	}
}
