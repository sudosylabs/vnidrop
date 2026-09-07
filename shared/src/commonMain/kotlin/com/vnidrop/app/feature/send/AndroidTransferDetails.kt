package com.vnidrop.app.feature.send

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.ui.components.DestructiveQuietButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.displayNameForStatus
import com.vnidrop.app.ui.state.formatBytes
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AndroidTransferDetails(
	transfer: Transfer,
	pendingReceivers: Int,
	completedReceivers: Int,
	onBack: () -> Unit,
	onActivity: () -> Unit,
	onReceivers: () -> Unit,
	onShare: () -> Unit,
	onStopSharing: () -> Unit,
	onDelete: () -> Unit,
) {
	Scaffold(
		topBar = {
			TopAppBar(
				title = { Text(stringResource(Res.string.send_transfer_details_title), maxLines = 1, overflow = TextOverflow.Ellipsis) },
				navigationIcon = {
					IconButton(onClick = onBack) { PlatformIcon(AppIcon.ArrowBack, stringResource(Res.string.button_back)) }
				},
				actions = {
					if (transfer.status in setOf(TransferStatus.Importing, TransferStatus.Sharing)) {
						IconButton(onClick = onShare) { PlatformIcon(AppIcon.Share, stringResource(Res.string.transfer_share_title)) }
					}
				},
			)
		},
	) { insets ->
		LazyColumn(
			modifier = Modifier.fillMaxSize().padding(insets).consumeWindowInsets(insets),
			contentPadding = PaddingValues(bottom = 24.dp),
		) {
			item {
				Column(Modifier.padding(horizontal = 24.dp, vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
					Text(transfer.transferName ?: stringResource(Res.string.send_new_transfer_title), style = MaterialTheme.typography.headlineSmall)
					val fileCount = transfer.fileCount.coerceAtMost(Int.MAX_VALUE.toULong()).toInt()
					Text("${pluralStringResource(Res.plurals.transfer_file_count, fileCount, fileCount)} · ${formatBytes(transfer.totalSize)}", color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			item {
				ListItem(
					headlineContent = { Text(stringResource(Res.string.metadata_status)) },
					supportingContent = { Text(displayNameForStatus(transfer.status)) },
				)
				ListItem(
					headlineContent = { Text(stringResource(Res.string.send_access_title)) },
					supportingContent = { Text(accessPolicyLabel(transfer.accessPolicy)) },
				)
				HorizontalDivider(Modifier.padding(horizontal = 16.dp))
			}
			item {
				ListItem(
					modifier = Modifier.clickable(onClick = onActivity),
					headlineContent = { Text(stringResource(Res.string.transfer_activity_title)) },
					supportingContent = { Text(stringResource(Res.string.transfer_activity_description)) },
					trailingContent = { PlatformIcon(AppIcon.ChevronRight, null) },
				)
				ListItem(
					modifier = Modifier.clickable(onClick = onReceivers),
					headlineContent = { Text(stringResource(Res.string.transfer_receivers_title)) },
					supportingContent = { Text(receiversDescription(pendingReceivers, completedReceivers)) },
					trailingContent = { PlatformIcon(AppIcon.ChevronRight, null) },
				)
			}
			item {
				Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
					if (transfer.status == TransferStatus.Sharing) SecondaryButton(
						stringResource(Res.string.send_stop_sharing), onStopSharing,
						modifier = Modifier.fillMaxWidth().testTag("transfer-stop-sharing-action"),
					)
					DestructiveQuietButton(
						stringResource(Res.string.button_delete_transfer), onDelete,
						modifier = Modifier.fillMaxWidth().testTag("transfer-delete-action"),
					)
				}
			}
		}
	}
}
