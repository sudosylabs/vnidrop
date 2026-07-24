package com.vnidrop.app.ui.feedback

import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarData
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.snackbar_dismiss

@Composable
fun VniDropSnackbarHost(
	controller: UiMessageController,
	modifier: Modifier = Modifier,
) {
	val hostState = remember { SnackbarHostState() }
	var tone by remember { mutableStateOf(UiMessageTone.Info) }
	LaunchedEffect(controller) {
		controller.messages.collect { message ->
			tone = message.tone
			val result = hostState.showSnackbar(
				message = message.text.resolve(),
				actionLabel = message.actionLabel?.resolve(),
				withDismissAction = true,
				duration = if (message.tone == UiMessageTone.Error) SnackbarDuration.Long else SnackbarDuration.Short,
			)
			if (result == SnackbarResult.ActionPerformed) message.onAction?.invoke()
		}
	}
	LaunchedEffect(controller, hostState) {
		controller.dismissals.collect {
			hostState.currentSnackbarData?.dismiss()
		}
	}

	SnackbarHost(hostState = hostState, modifier = modifier) { data ->
		val colors = LocalVniDropColors.current
		val accent = when (tone) {
			UiMessageTone.Info -> colors.brandLink
			UiMessageTone.Success -> colors.brandDefault
			UiMessageTone.Warning -> colors.warningDefault
			UiMessageTone.Error -> colors.destructiveDefault
		}
		Surface(
			modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp).widthIn(max = 520.dp).fillMaxWidth(),
			shape = RoundedCornerShape(10.dp),
			color = colors.backgroundSurface200,
			contentColor = colors.foregroundDefault,
			shadowElevation = 6.dp,
		) {
			SnackbarContent(data, accent)
		}
	}
}

@Composable
private fun SnackbarContent(data: SnackbarData, actionColor: Color) {
	BoxWithConstraints {
		val actionLabel = data.visuals.actionLabel
		if (actionLabel != null && maxWidth < 420.dp) {
			Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 6.dp, top = 8.dp, bottom = 6.dp)) {
				MessageAndDismiss(data)
				TextButton(onClick = data::performAction, modifier = Modifier.align(Alignment.End)) {
					Text(actionLabel, color = actionColor)
				}
			}
		} else {
			Row(
				modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
				verticalAlignment = Alignment.CenterVertically,
			) {
				SnackbarMessage(data.visuals.message, Modifier.weight(1f))
				actionLabel?.let { label ->
					TextButton(onClick = data::performAction) { Text(label, color = actionColor) }
				}
				DismissButton(data::dismiss)
			}
		}
	}
}

@Composable
private fun MessageAndDismiss(data: SnackbarData) {
	Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
		SnackbarMessage(data.visuals.message, Modifier.weight(1f))
		DismissButton(data::dismiss)
	}
}

@Composable
private fun SnackbarMessage(message: String, modifier: Modifier = Modifier) {
	Text(
		text = message,
		modifier = modifier.padding(vertical = 6.dp),
		style = MaterialTheme.typography.bodyMedium,
	)
}

@Composable
private fun DismissButton(onClick: () -> Unit) {
	IconButton(onClick = onClick, modifier = Modifier.size(40.dp)) {
		PlatformIcon(
			icon = AppIcon.Close,
			contentDescription = stringResource(Res.string.snackbar_dismiss),
			tint = LocalVniDropColors.current.foregroundLighter,
			modifier = Modifier.size(18.dp),
		)
	}
}

private suspend fun UiText.resolve(): String = when (this) {
	is UiText.Dynamic -> value
	is UiText.Resource -> getString(resource, *formatArgs.toTypedArray())
}
