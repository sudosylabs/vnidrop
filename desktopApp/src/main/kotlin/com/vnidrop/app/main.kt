package com.vnidrop.app

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.window.WindowDraggableArea
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.awt.awtEventOrNull
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathBuilder
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.onPointerEvent
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.WindowPlacement
import androidx.compose.ui.window.WindowScope
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import com.vnidrop.app.feature.receive.ExternalInvitationController
import com.vnidrop.app.feature.receive.MaxVniDropInvitationBytes
import com.vnidrop.app.feature.receive.VniDropInvitationExtension
import com.vnidrop.app.feature.receive.decodeInvitationBytes
import com.vnidrop.app.platform.DesktopAppearanceBridge
import com.vnidrop.app.ui.theme.LocalVniDropColors
import io.github.vinceglb.filekit.FileKit
import java.awt.Desktop
import java.awt.event.WindowEvent
import java.awt.event.WindowFocusListener
import java.io.File

fun main(args: Array<String>) {
	FileKit.init(appId = "vnidrop")
	val externalInvitations = ExternalInvitationController()
	val linux = DesktopAppearanceBridge.isLinux()
	val windows = DesktopAppearanceBridge.isWindows()
	configureInvitationOpenHandler(externalInvitations)
	args.asSequence()
		.map(::File)
		.filter { it.extension.equals(VniDropInvitationExtension, ignoreCase = true) }
		.forEach { externalInvitations.openFile(it) }
	application {
		val windowState = rememberWindowState()
		Window(
			onCloseRequest = ::exitApplication,
			state = windowState,
			title = "VniDrop",
			// Compose keeps edge resizers active for this client-decorated Linux window.
			undecorated = linux,
		) {
			var windowFocused by remember { mutableStateOf(window.isFocused) }
			DisposableEffect(window) {
				val listener = object : WindowFocusListener {
					override fun windowGainedFocus(event: WindowEvent?) {
						windowFocused = true
					}

					override fun windowLostFocus(event: WindowEvent?) {
						windowFocused = false
					}
				}
				window.addWindowFocusListener(listener)
				onDispose { window.removeWindowFocusListener(listener) }
			}
			val dependencies = rememberJvmAppDependencies(externalInvitations)
			if (windows) {
				WindowsWindowFrame(windowState) { chrome ->
					App(
						dependencies = dependencies,
						windowChromeTopInset = chrome.topInset,
						windowContentTopStartRadius = chrome.contentTopStartRadius,
						useNativeWindowBackdrop = chrome.useNativeBackdrop,
						onResolvedDarkThemeChanged = chrome.onDarkThemeChanged,
						windowChrome = chrome.chrome,
						windowFocused = windowFocused,
					)
				}
			} else {
				App(
					dependencies = dependencies,
					windowChromeTopInset = if (linux) LinuxTitleBarHeight else 0.dp,
					windowContentTopStartRadius = if (linux) DesktopContentCornerRadius else 0.dp,
					windowChrome = if (linux) {
						{
							LinuxTitleBar(
								isMaximized = windowState.placement == WindowPlacement.Maximized,
								isWindowFocused = windowFocused,
								onMinimize = { windowState.isMinimized = true },
								onToggleMaximize = {
									windowState.placement = toggledWindowPlacement(windowState.placement)
								},
								onClose = ::exitApplication,
							)
						}
					} else {
						null
					},
					windowFocused = windowFocused,
				)
			}
		}
	}
}

private fun configureInvitationOpenHandler(controller: ExternalInvitationController) {
	if (!Desktop.isDesktopSupported()) return
	val desktop = Desktop.getDesktop()
	if (!desktop.isSupported(Desktop.Action.APP_OPEN_FILE)) return
	desktop.setOpenFileHandler { event -> event.files.forEach(controller::openFile) }
}

private fun ExternalInvitationController.openFile(file: File) {
	val result = runCatching {
		require(file.extension.equals(VniDropInvitationExtension, ignoreCase = true)) { "This is not a VniDrop invitation" }
		val bytes = file.inputStream().use { it.readNBytes(MaxVniDropInvitationBytes + 1) }
		decodeInvitationBytes(bytes)
	}
	result.fold(::openInvitation) { error ->
		reportOpenFailure(error.message ?: "The invitation could not be opened")
	}
}

private val LinuxTitleBarHeight = 48.dp
private val LinuxWindowControlHitTargetSize = 34.dp
private val LinuxWindowControlVisualSize = 28.dp
private val LinuxWindowControlsWidth = 112.dp
internal val DesktopContentCornerRadius = 20.dp

@Composable
@OptIn(ExperimentalComposeUiApi::class)
private fun WindowScope.LinuxTitleBar(
	isMaximized: Boolean,
	isWindowFocused: Boolean,
	onMinimize: () -> Unit,
	onToggleMaximize: () -> Unit,
	onClose: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	Box(
		modifier = Modifier
			.fillMaxWidth()
			.height(LinuxTitleBarHeight)
			.background(colors.backgroundSurface200),
	) {
		BasicText(
			text = "VniDrop",
			modifier = Modifier.align(Alignment.Center),
			style = TextStyle(
				color = if (isWindowFocused) colors.foregroundDefault else colors.foregroundLighter,
				fontSize = 13.sp,
				fontWeight = FontWeight.SemiBold,
			),
		)
		WindowDraggableArea(
			modifier = Modifier
				.fillMaxSize()
				.padding(end = LinuxWindowControlsWidth)
				.onPointerEvent(PointerEventType.Press) { event ->
					if (event.awtEventOrNull?.clickCount == 2) onToggleMaximize()
				},
		) {
			Box(Modifier.fillMaxSize())
		}
		Row(
			modifier = Modifier
				.align(Alignment.CenterEnd)
				.padding(end = 10.dp),
		) {
			LinuxWindowControlButton(
				icon = LinuxMinimizeIcon,
				contentDescription = "Minimize window",
				isWindowFocused = isWindowFocused,
				onClick = onMinimize,
			)
			LinuxWindowControlButton(
				icon = if (isMaximized) LinuxRestoreIcon else LinuxMaximizeIcon,
				contentDescription = if (isMaximized) "Restore window" else "Maximize window",
				isWindowFocused = isWindowFocused,
				onClick = onToggleMaximize,
			)
			LinuxWindowControlButton(
				icon = LinuxCloseIcon,
				contentDescription = "Close window",
				isClose = true,
				isWindowFocused = isWindowFocused,
				onClick = onClose,
			)
		}
	}
}

@Composable
private fun LinuxWindowControlButton(
	icon: ImageVector,
	contentDescription: String,
	isClose: Boolean = false,
	isWindowFocused: Boolean,
	onClick: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val interactionSource = remember { MutableInteractionSource() }
	val hovered by interactionSource.collectIsHoveredAsState()
	val pressed by interactionSource.collectIsPressedAsState()
	val visualState = linuxWindowControlVisualState(isClose, hovered, pressed, isWindowFocused)
	val background = when (visualState) {
		LinuxWindowControlVisualState.Inactive,
		LinuxWindowControlVisualState.Default -> Color.Transparent
		LinuxWindowControlVisualState.NeutralHovered -> colors.backgroundOverlayHover
		LinuxWindowControlVisualState.NeutralPressed -> colors.backgroundSurface400
		LinuxWindowControlVisualState.DestructiveHovered -> colors.destructiveDefault
		LinuxWindowControlVisualState.DestructivePressed -> colors.destructive600
	}
	Box(
		modifier = Modifier
			.size(LinuxWindowControlHitTargetSize)
			.hoverable(interactionSource)
			.clickable(
				interactionSource = interactionSource,
				indication = null,
				role = Role.Button,
				onClick = onClick,
			),
		contentAlignment = Alignment.Center,
	) {
		Box(
			modifier = Modifier
				.size(LinuxWindowControlVisualSize)
				.background(background, CircleShape),
			contentAlignment = Alignment.Center,
		) {
			Image(
				painter = rememberVectorPainter(icon),
				contentDescription = contentDescription,
				colorFilter = ColorFilter.tint(
					when (visualState) {
						LinuxWindowControlVisualState.DestructiveHovered,
						LinuxWindowControlVisualState.DestructivePressed -> Color.White
						LinuxWindowControlVisualState.Inactive -> colors.foregroundLighter
						else -> colors.foregroundLight
					},
				),
				modifier = Modifier.size(16.dp),
			)
		}
	}
}

internal enum class LinuxWindowControlVisualState {
	Inactive,
	Default,
	NeutralHovered,
	NeutralPressed,
	DestructiveHovered,
	DestructivePressed,
}

internal fun linuxWindowControlVisualState(
	isClose: Boolean,
	isHovered: Boolean,
	isPressed: Boolean,
	isWindowFocused: Boolean = true,
): LinuxWindowControlVisualState = when {
	isClose && isPressed -> LinuxWindowControlVisualState.DestructivePressed
	isClose && isHovered -> LinuxWindowControlVisualState.DestructiveHovered
	isPressed -> LinuxWindowControlVisualState.NeutralPressed
	isHovered -> LinuxWindowControlVisualState.NeutralHovered
	!isWindowFocused -> LinuxWindowControlVisualState.Inactive
	else -> LinuxWindowControlVisualState.Default
}

internal fun toggledWindowPlacement(current: WindowPlacement): WindowPlacement =
	if (current == WindowPlacement.Maximized) WindowPlacement.Floating else WindowPlacement.Maximized

private val LinuxMinimizeIcon = windowControlIcon("Minimize") {
	moveTo(6f, 12f)
	lineTo(18f, 12f)
}

private val LinuxMaximizeIcon = windowControlIcon("Maximize") {
	moveTo(6.5f, 6.5f)
	lineTo(17.5f, 6.5f)
	lineTo(17.5f, 17.5f)
	lineTo(6.5f, 17.5f)
	close()
}

private val LinuxRestoreIcon = windowControlIcon("Restore") {
	moveTo(8.5f, 8.5f)
	lineTo(18f, 8.5f)
	lineTo(18f, 18f)
	lineTo(8.5f, 18f)
	close()
	moveTo(6f, 15.5f)
	lineTo(6f, 6f)
	lineTo(15.5f, 6f)
}

private val LinuxCloseIcon = windowControlIcon("Close") {
	moveTo(7f, 7f)
	lineTo(17f, 17f)
	moveTo(17f, 7f)
	lineTo(7f, 17f)
}

private fun windowControlIcon(name: String, block: PathBuilder.() -> Unit): ImageVector =
	ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f).apply {
		path(
			fill = SolidColor(Color.Transparent),
			stroke = SolidColor(Color.Black),
			strokeLineWidth = 1.8f,
			strokeLineCap = StrokeCap.Round,
			strokeLineJoin = StrokeJoin.Round,
			pathFillType = PathFillType.NonZero,
			pathBuilder = block,
		)
	}.build()
