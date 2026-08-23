package com.vnidrop.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors

@Composable
internal fun FeatureEmptyState(
	icon: AppIcon,
	title: String,
	description: String,
	modifier: Modifier = Modifier,
	iconTestTag: String? = null,
	action: (@Composable () -> Unit)? = null,
) {
	val colors = LocalVniDropColors.current
	Column(
		modifier = modifier.padding(horizontal = 24.dp),
		horizontalAlignment = Alignment.CenterHorizontally,
		verticalArrangement = Arrangement.spacedBy(10.dp),
	) {
		PlatformIcon(
			icon = icon,
			contentDescription = null,
			tint = colors.foregroundLighter,
			modifier = Modifier
				.size(36.dp)
				.then(if (iconTestTag == null) Modifier else Modifier.testTag(iconTestTag)),
		)
		Text(
			text = title,
			style = MaterialTheme.typography.titleMedium,
			fontWeight = FontWeight.SemiBold,
			textAlign = TextAlign.Center,
		)
		Text(
			text = description,
			modifier = Modifier.widthIn(max = 480.dp),
			style = MaterialTheme.typography.bodyMedium,
			color = colors.foregroundLight,
			textAlign = TextAlign.Center,
		)
		action?.let {
			androidx.compose.foundation.layout.Spacer(Modifier.size(2.dp))
			it()
		}
	}
}
