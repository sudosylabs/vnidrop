package com.vnidrop.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.platform.usesMobilePresentation
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_close

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AdaptiveDrawer(
	windowClass: WindowClass,
	onDismissRequest: () -> Unit,
	dialogMaxWidth: Dp = 560.dp,
	content: @Composable () -> Unit,
) {
	val uiPlatform = LocalUiPlatform.current
	if (usesMobilePresentation(uiPlatform, windowClass)) {
		ModalBottomSheet(
			onDismissRequest = onDismissRequest,
			sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
			containerColor = LocalVniDropColors.current.backgroundDialog,
		) {
			Box(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 12.dp)) { content() }
		}
	} else {
		Dialog(
			onDismissRequest = onDismissRequest,
			properties = DialogProperties(usePlatformDefaultWidth = false),
		) {
			Surface(
				modifier = Modifier.widthIn(max = dialogMaxWidth).fillMaxWidth().testTag("adaptive-dialog-surface"),
				shape = RoundedCornerShape(if (uiPlatform.isDesktop) 10.dp else 24.dp),
				color = LocalVniDropColors.current.backgroundDialog,
				shadowElevation = 12.dp,
			) { ClosableModalContent(onDismissRequest, content = content) }
		}
	}
}

@Composable
private fun ClosableModalContent(
	onClose: () -> Unit,
	modifier: Modifier = Modifier,
	content: @Composable () -> Unit,
) {
	Box(modifier) {
		content()
		IconButton(
			onClick = onClose,
			modifier = Modifier.align(androidx.compose.ui.Alignment.TopEnd).padding(8.dp).size(40.dp),
		) {
			PlatformIcon(AppIcon.Close, stringResource(Res.string.button_close), tint = LocalVniDropColors.current.foregroundLight)
		}
	}
}
