package com.vnidrop.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.notifications_enable_action
import vnidrop.shared.generated.resources.notifications_request_prompt

@Composable
fun NotificationEnableCallout(
	onEnable: () -> Unit,
	modifier: Modifier = Modifier,
) {
	val colors = LocalVniDropColors.current
	Surface(
		modifier = modifier.fillMaxWidth(),
		shape = RoundedCornerShape(12.dp),
		color = colors.backgroundSelection,
	) {
		Column(
			modifier = Modifier.padding(start = 14.dp, end = 8.dp, top = 12.dp, bottom = 4.dp),
		) {
			Text(
				text = stringResource(Res.string.notifications_request_prompt),
				style = MaterialTheme.typography.bodySmall,
				color = colors.foregroundLight,
			)
			TextButton(onClick = onEnable, modifier = Modifier.align(Alignment.End)) {
				Text(
					text = stringResource(Res.string.notifications_enable_action),
					color = colors.brandLink,
				)
			}
		}
	}
}
