package com.vnidrop.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.theme.LocalVniDropColors

@Composable
fun Field(
	value: String,
	onValueChange: (String) -> Unit,
	label: String,
	modifier: Modifier = Modifier,
	minLines: Int = 1,
	enabled: Boolean = true,
) {
	val desktop = LocalUiPlatform.current.isDesktop
	Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
		if (desktop) {
			Text(label, color = LocalVniDropColors.current.foregroundLight, style = MaterialTheme.typography.bodySmall)
		}
		OutlinedTextField(
			value = value,
			onValueChange = onValueChange,
			label = if (desktop) {
				null
			} else {
				{ Text(label) }
			},
			modifier = Modifier.fillMaxWidth(),
			minLines = minLines,
			enabled = enabled,
			shape = if (desktop) RoundedCornerShape(5.dp) else OutlinedTextFieldDefaults.shape,
		)
	}
}
