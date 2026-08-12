package com.vnidrop.app.feature.send

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.ui.components.Field
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.QuietButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.state.formatBytes
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_change_files
import vnidrop.shared.generated.resources.button_choose_files
import vnidrop.shared.generated.resources.button_choose_folder
import vnidrop.shared.generated.resources.button_clear
import vnidrop.shared.generated.resources.button_remove_file
import vnidrop.shared.generated.resources.button_share_file
import vnidrop.shared.generated.resources.button_sharing_file
import vnidrop.shared.generated.resources.field_sender_name
import vnidrop.shared.generated.resources.field_transfer_name
import vnidrop.shared.generated.resources.send_access_anyone
import vnidrop.shared.generated.resources.send_access_anyone_description
import vnidrop.shared.generated.resources.send_access_anyone_warning
import vnidrop.shared.generated.resources.send_access_approval
import vnidrop.shared.generated.resources.send_access_approval_description
import vnidrop.shared.generated.resources.send_access_title
import vnidrop.shared.generated.resources.send_choose_file_body
import vnidrop.shared.generated.resources.send_choose_file_title
import vnidrop.shared.generated.resources.send_file_size_unknown
import vnidrop.shared.generated.resources.send_folder_label
import vnidrop.shared.generated.resources.send_review_title
import vnidrop.shared.generated.resources.send_selected_files_count
import vnidrop.shared.generated.resources.saved_devices_send_action

@Composable
internal fun TransferComposer(
	coreInitialized: Boolean,
	state: TransferDraftState,
	windowClass: WindowClass,
	onSelectFile: () -> Unit,
	onSelectFolder: () -> Unit,
	onClearFile: () -> Unit,
	onRemoveFile: (DraftSourceId) -> Unit,
	onTransferNameChanged: (String) -> Unit,
	onSenderNameChanged: (String) -> Unit,
	onAccessPolicyChanged: (ShareAccessPolicy) -> Unit,
	onSubmit: () -> Unit,
) {
	Column(
		modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 12.dp),
		verticalArrangement = Arrangement.spacedBy(16.dp),
	) {
		if (state.sources.isEmpty()) {
			ChooseFileStep(onSelectFile, onSelectFolder)
		} else {
			ReviewFileStep(
				state = state,
				windowClass = windowClass,
				onSelectFile = onSelectFile,
				onSelectFolder = onSelectFolder,
				onClearFile = onClearFile,
				onRemoveFile = onRemoveFile,
				onTransferNameChanged = onTransferNameChanged,
				onSenderNameChanged = onSenderNameChanged,
				onAccessPolicyChanged = onAccessPolicyChanged,
				onSubmit = onSubmit,
				coreInitialized = coreInitialized,
			)
		}
	}
}

@Composable
private fun ChooseFileStep(onSelectFile: () -> Unit, onSelectFolder: () -> Unit) {
	Text(stringResource(Res.string.send_choose_file_title), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
	Text(
		stringResource(Res.string.send_choose_file_body),
		color = LocalVniDropColors.current.foregroundLighter,
		style = MaterialTheme.typography.bodyMedium,
	)
	Surface(shape = RoundedCornerShape(16.dp), color = LocalVniDropColors.current.backgroundSurface200) {
		Column(
			modifier = Modifier.fillMaxWidth().padding(24.dp),
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(14.dp),
		) {
			PlatformIcon(AppIcon.File, contentDescription = null, tint = LocalVniDropColors.current.brandLink, modifier = Modifier.size(32.dp))
			PrimaryButton(stringResource(Res.string.button_choose_files), onClick = onSelectFile)
			QuietButton(stringResource(Res.string.button_choose_folder), onClick = onSelectFolder)
		}
	}
}

@Composable
private fun ReviewFileStep(
	state: TransferDraftState,
	windowClass: WindowClass,
	onSelectFile: () -> Unit,
	onSelectFolder: () -> Unit,
	onClearFile: () -> Unit,
	onRemoveFile: (DraftSourceId) -> Unit,
	onTransferNameChanged: (String) -> Unit,
	onSenderNameChanged: (String) -> Unit,
	onAccessPolicyChanged: (ShareAccessPolicy) -> Unit,
	onSubmit: () -> Unit,
	coreInitialized: Boolean,
) {
	Text(stringResource(Res.string.send_review_title), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
	if (state.sources.size > 1) {
		Text(
			stringResource(Res.string.send_selected_files_count, state.sources.size),
			color = LocalVniDropColors.current.foregroundLighter,
			style = MaterialTheme.typography.bodyMedium,
		)
	}
	state.sources.forEach { file ->
		SelectedFileCard(
			file = file,
			canRemove = state.sources.size > 1 && !state.isSubmitting,
			onRemove = { onRemoveFile(file.id) },
		)
	}
	Field(state.transferName, onTransferNameChanged, stringResource(Res.string.field_transfer_name), enabled = !state.isSubmitting)
	when (val destination = state.destination) {
		TransferDraftDestination.Invitation -> {
			Field(state.senderName, onSenderNameChanged, stringResource(Res.string.field_sender_name), enabled = !state.isSubmitting)
			Text(stringResource(Res.string.send_access_title), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
			PolicyOption(
				icon = AppIcon.Shield,
				title = stringResource(Res.string.send_access_approval),
				description = stringResource(Res.string.send_access_approval_description),
				selected = state.accessPolicy == ShareAccessPolicy.RequireApproval,
				onClick = { onAccessPolicyChanged(ShareAccessPolicy.RequireApproval) },
			)
			PolicyOption(
				icon = AppIcon.Globe,
				title = stringResource(Res.string.send_access_anyone),
				description = stringResource(Res.string.send_access_anyone_description),
				selected = state.accessPolicy == ShareAccessPolicy.AnyoneWithTransfer,
				onClick = { onAccessPolicyChanged(ShareAccessPolicy.AnyoneWithTransfer) },
			)
			if (state.accessPolicy == ShareAccessPolicy.AnyoneWithTransfer) {
				Text(
					stringResource(Res.string.send_access_anyone_warning),
					color = LocalVniDropColors.current.destructiveDefault,
					style = MaterialTheme.typography.bodySmall,
				)
			}
		}
		is TransferDraftDestination.Targeted -> Text(
			destination.receiver.displayName,
			style = MaterialTheme.typography.titleMedium,
			fontWeight = FontWeight.SemiBold,
		)
		null -> Unit
	}
	Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
		SubmitButton(state, coreInitialized, onSubmit, Modifier.fillMaxWidth())
		Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
			SourceButton(
				text = stringResource(Res.string.button_change_files),
				icon = AppIcon.File,
				onClick = onSelectFile,
				modifier = Modifier.weight(1f),
				enabled = !state.isSubmitting,
			)
			SourceButton(
				text = stringResource(Res.string.button_choose_folder),
				icon = AppIcon.Folder,
				onClick = onSelectFolder,
				modifier = Modifier.weight(1f),
				enabled = !state.isSubmitting,
			)
			if (windowClass != WindowClass.Phone) {
				SourceButton(
					text = stringResource(Res.string.button_clear),
					icon = AppIcon.Close,
					onClick = onClearFile,
					modifier = Modifier.weight(1f),
					enabled = !state.isSubmitting,
				)
			}
		}
	}
}

@Composable
private fun SourceButton(
	text: String,
	icon: AppIcon,
	onClick: () -> Unit,
	modifier: Modifier = Modifier,
	enabled: Boolean,
) {
	OutlinedButton(onClick = onClick, modifier = modifier, enabled = enabled) {
		PlatformIcon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
		Spacer(Modifier.width(8.dp))
		Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

@Composable
private fun SubmitButton(state: TransferDraftState, coreInitialized: Boolean, onSubmit: () -> Unit, modifier: Modifier = Modifier) {
	val targeted = state.destination is TransferDraftDestination.Targeted
	PrimaryButton(
		when {
			state.isSubmitting -> stringResource(Res.string.button_sharing_file)
			targeted -> stringResource(Res.string.saved_devices_send_action)
			else -> stringResource(Res.string.button_share_file)
		},
		onClick = onSubmit,
		modifier = modifier,
		enabled = state.canSubmit(coreInitialized),
	)
}

@Composable
private fun SelectedFileCard(
	file: TransferDraftSource,
	canRemove: Boolean,
	onRemove: () -> Unit,
) {
	Surface(shape = RoundedCornerShape(14.dp), color = LocalVniDropColors.current.backgroundSurface200) {
		Row(modifier = Modifier.fillMaxWidth().padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
			Box(Modifier.size(44.dp).background(LocalVniDropColors.current.backgroundSurface300, RoundedCornerShape(11.dp))) {
				FileArtwork(file.thumbnailBytes, Modifier.fillMaxSize())
			}
			Spacer(Modifier.width(12.dp))
			Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
				Text(file.displayName, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(
					when {
						file.isDirectory -> stringResource(Res.string.send_folder_label)
						file.sizeBytes != null -> formatBytes(file.sizeBytes)
						else -> stringResource(Res.string.send_file_size_unknown)
					},
					color = LocalVniDropColors.current.foregroundLighter,
					style = MaterialTheme.typography.bodySmall,
				)
			}
			if (canRemove) {
				IconButton(onClick = onRemove) {
					PlatformIcon(AppIcon.Delete, stringResource(Res.string.button_remove_file), tint = LocalVniDropColors.current.destructiveDefault)
				}
			}
		}
	}
}

@Composable
private fun PolicyOption(
	icon: AppIcon,
	title: String,
	description: String,
	selected: Boolean,
	onClick: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val shape = RoundedCornerShape(14.dp)
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.clip(shape)
			.background(if (selected) colors.backgroundSelection else colors.backgroundSurface200)
			.selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
			.padding(14.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		PlatformIcon(icon, contentDescription = null, tint = if (selected) colors.brandLink else colors.foregroundLight, modifier = Modifier.size(22.dp))
		Spacer(Modifier.width(12.dp))
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
			Text(title, fontWeight = FontWeight.SemiBold)
			Text(description, color = colors.foregroundLighter, style = MaterialTheme.typography.bodySmall)
		}
		RadioButton(selected = selected, onClick = null)
	}
}
