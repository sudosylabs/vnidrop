package com.vnidrop.app.ui.components

import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.theme.LocalVniDropColors

@Composable
fun PrimaryButton(
	text: String,
	onClick: () -> Unit,
	modifier: Modifier = Modifier,
	enabled: Boolean = true,
	leadingIcon: @Composable (() -> Unit)? = null,
) {
	val desktop = LocalUiPlatform.current.isDesktop
	Button(
		onClick = onClick,
		enabled = enabled,
		modifier = modifier.heightIn(min = if (desktop) 36.dp else 44.dp),
		shape = RoundedCornerShape(if (desktop) 6.dp else 8.dp),
		colors = ButtonDefaults.buttonColors(containerColor = LocalVniDropColors.current.brandButton, contentColor = Color.White),
	) {
		leadingIcon?.let {
			it()
			Spacer(Modifier.width(8.dp))
		}
		Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

@Composable
fun SecondaryButton(
	text: String,
	onClick: () -> Unit,
	modifier: Modifier = Modifier,
	enabled: Boolean = true,
	leadingIcon: @Composable (() -> Unit)? = null,
) {
	val desktop = LocalUiPlatform.current.isDesktop
	OutlinedButton(
		onClick = onClick,
		enabled = enabled,
		modifier = modifier.heightIn(min = if (desktop) 36.dp else 44.dp),
		shape = RoundedCornerShape(if (desktop) 6.dp else 8.dp),
	) {
		leadingIcon?.let {
			it()
			Spacer(Modifier.width(8.dp))
		}
		Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

@Composable
fun QuietButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
	val desktop = LocalUiPlatform.current.isDesktop
	TextButton(onClick = onClick, enabled = enabled, modifier = modifier.heightIn(min = if (desktop) 32.dp else 40.dp)) {
		Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

@Composable
fun DestructiveQuietButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
	val desktop = LocalUiPlatform.current.isDesktop
	TextButton(
		onClick = onClick,
		enabled = enabled,
		modifier = modifier.heightIn(min = if (desktop) 32.dp else 40.dp),
		colors = ButtonDefaults.textButtonColors(contentColor = LocalVniDropColors.current.destructiveDefault),
	) {
		Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

@Composable
fun DestructiveButton(
	text: String,
	onClick: () -> Unit,
	modifier: Modifier = Modifier,
	enabled: Boolean = true,
	leadingIcon: @Composable (() -> Unit)? = null,
) {
	val desktop = LocalUiPlatform.current.isDesktop
	Button(
		onClick = onClick,
		enabled = enabled,
		modifier = modifier.heightIn(min = if (desktop) 36.dp else 44.dp),
		shape = RoundedCornerShape(if (desktop) 6.dp else 8.dp),
		colors = ButtonDefaults.buttonColors(
			containerColor = LocalVniDropColors.current.destructiveDefault,
			contentColor = Color.White,
		),
	) {
		leadingIcon?.let {
			it()
			Spacer(Modifier.width(8.dp))
		}
		Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}
