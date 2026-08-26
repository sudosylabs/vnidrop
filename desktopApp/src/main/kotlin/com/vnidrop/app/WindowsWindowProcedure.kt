package com.vnidrop.app

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.awt.ComposeWindow
import androidx.compose.ui.geometry.Rect
import com.sun.jna.CallbackReference
import com.sun.jna.Native
import com.sun.jna.Platform
import com.sun.jna.Pointer
import com.sun.jna.Structure
import com.sun.jna.platform.win32.BaseTSD.LONG_PTR
import com.sun.jna.platform.win32.BaseTSD.ULONG_PTRByReference
import com.sun.jna.platform.win32.User32
import com.sun.jna.platform.win32.WinDef.HWND
import com.sun.jna.platform.win32.WinDef.LPARAM
import com.sun.jna.platform.win32.WinDef.LRESULT
import com.sun.jna.platform.win32.WinDef.POINT
import com.sun.jna.platform.win32.WinDef.RECT
import com.sun.jna.platform.win32.WinDef.UINT
import com.sun.jna.platform.win32.WinDef.WPARAM
import com.sun.jna.platform.win32.WinNT.HRESULT
import com.sun.jna.platform.win32.WinUser
import com.sun.jna.platform.win32.WinUser.WindowProc
import com.sun.jna.ptr.IntByReference
import com.sun.jna.win32.StdCallLibrary
import com.sun.jna.win32.W32APIOptions
import org.jetbrains.skiko.GraphicsApi
import org.jetbrains.skiko.SkiaLayer
import java.awt.AlphaComposite
import java.awt.BorderLayout
import java.awt.Color as AwtColor
import java.awt.Component
import java.awt.Container
import java.awt.EventQueue
import java.awt.Graphics
import java.awt.Graphics2D
import java.awt.Window
import java.util.concurrent.ConcurrentHashMap
import javax.swing.JComponent
import javax.swing.JPanel
import kotlin.math.roundToInt

internal const val WindowsTitleBarHeightDip = 48

internal data class WindowsFrameInsets(
	val left: Int = 0,
	val top: Int = 0,
	val right: Int = 0,
	val bottom: Int = 0,
)

internal fun windowsContentInsets(
	isMaximized: Boolean,
	horizontalResizeBorder: Int,
	verticalResizeBorder: Int,
): WindowsFrameInsets = if (isMaximized) {
	WindowsFrameInsets(
		left = horizontalResizeBorder,
		top = verticalResizeBorder,
		right = horizontalResizeBorder,
		bottom = verticalResizeBorder,
	)
} else {
	WindowsFrameInsets()
}

internal data class WindowsHitTestGeometry(
	val width: Int,
	val height: Int,
	val horizontalResizeBorder: Int,
	val verticalResizeBorder: Int,
	val isMaximized: Boolean,
	val caption: Rect,
	val minimizeButton: Rect = Rect.Zero,
	val maximizeButton: Rect = Rect.Zero,
	val closeButton: Rect = Rect.Zero,
)

internal enum class WindowsCaptionButton {
	Minimize,
	Maximize,
	Close,
}

internal data class WindowsScreenPoint(val x: Int, val y: Int)

internal data class WindowsCaptionButtonBounds(
	val minimize: Rect,
	val maximize: Rect,
	val close: Rect,
)

internal fun windowsScreenPoint(packedCoordinates: Long): WindowsScreenPoint = WindowsScreenPoint(
	x = (packedCoordinates and 0xffff).toShort().toInt(),
	y = ((packedCoordinates shr 16) and 0xffff).toShort().toInt(),
)

internal fun splitWindowsCaptionButtonBounds(bounds: Rect): WindowsCaptionButtonBounds {
	val buttonWidth = bounds.width / 3f
	return WindowsCaptionButtonBounds(
		minimize = Rect(bounds.left, bounds.top, bounds.left + buttonWidth, bounds.bottom),
		maximize = Rect(bounds.left + buttonWidth, bounds.top, bounds.right - buttonWidth, bounds.bottom),
		close = Rect(bounds.right - buttonWidth, bounds.top, bounds.right, bounds.bottom),
	)
}

internal fun windowsTitleBarHeightPixels(dpi: Int): Int =
	(WindowsTitleBarHeightDip * dpi / DefaultDpi.toFloat()).roundToInt().coerceAtLeast(1)

internal object WindowsHitTestResult {
	const val Client = 1
	const val Caption = 2
	const val MinimizeButton = 8
	const val MaximizeButton = 9
	const val Left = 10
	const val Right = 11
	const val Top = 12
	const val TopLeft = 13
	const val TopRight = 14
	const val Bottom = 15
	const val BottomLeft = 16
	const val BottomRight = 17
	const val CloseButton = 20
	const val Transparent = -1
}

internal fun windowsHitTest(
	x: Int,
	y: Int,
	geometry: WindowsHitTestGeometry,
): Int {
	if (!geometry.isMaximized) {
		val left = x < geometry.horizontalResizeBorder
		val right = x >= geometry.width - geometry.horizontalResizeBorder
		val top = y < geometry.verticalResizeBorder
		val bottom = y >= geometry.height - geometry.verticalResizeBorder
		when {
			top && left -> return WindowsHitTestResult.TopLeft
			top && right -> return WindowsHitTestResult.TopRight
			bottom && left -> return WindowsHitTestResult.BottomLeft
			bottom && right -> return WindowsHitTestResult.BottomRight
			left -> return WindowsHitTestResult.Left
			right -> return WindowsHitTestResult.Right
			top -> return WindowsHitTestResult.Top
			bottom -> return WindowsHitTestResult.Bottom
		}
	}

	return when {
		geometry.closeButton.contains(x, y) -> WindowsHitTestResult.CloseButton
		geometry.maximizeButton.contains(x, y) -> WindowsHitTestResult.MaximizeButton
		geometry.minimizeButton.contains(x, y) -> WindowsHitTestResult.MinimizeButton
		geometry.caption.contains(x, y) -> WindowsHitTestResult.Caption
		else -> WindowsHitTestResult.Client
	}
}

internal class WindowsNativeWindowController private constructor(
	private val window: ComposeWindow,
	private val skiaLayer: SkiaLayer,
	private val windowHandle: HWND,
	private val user32: WindowsUser32,
	private val dwm: WindowsDwmApi?,
) : AutoCloseable {
	private val outerProcedure = OuterWindowProcedure(this)
	private val contentProcedure = ContentWindowProcedure(this)
	@Volatile
	private var outerSubclassInstalled = false

	@Volatile
	private var contentSubclassInstalled = false

	@Volatile
	private var previousOuterProcedure: LONG_PTR? = null

	@Volatile
	private var previousContentProcedure: LONG_PTR? = null
	private var contentHandle: HWND? = null
	private var originalContentPane: Container? = null
	private var backdropContentPane: WindowsAlphaClearingContentPane? = null
	private var originalContentChildren = emptyList<WindowsContentPaneChild>()
	private var transparentSurfaceConfigured = false

	@Volatile
	private var systemFrameExtended = false

	@Volatile
	private var systemCaptionButtonBoundsAvailable = false

	@Volatile
	private var closed = false

	@Volatile
	private var nativePressedCaptionButton: WindowsCaptionButton? = null
	private var trackingNonClientMouse = false
	private var darkTheme: Boolean? = null

	@Volatile
	private var metrics = WindowsWindowMetrics()

	@Volatile
	private var regions = WindowsHitRegions()

	var frameInsets by mutableStateOf(WindowsFrameInsets())
		private set

	var usesNativeBackdrop by mutableStateOf(false)
		private set

	var usesCustomChrome by mutableStateOf(true)
		private set

	var usesSystemCaptionButtonHandling by mutableStateOf(false)
		private set

	var hoveredCaptionButton by mutableStateOf<WindowsCaptionButton?>(null)
		private set

	var pressedCaptionButton by mutableStateOf<WindowsCaptionButton?>(null)
		private set

	var isWindowActive by mutableStateOf(true)
		private set

	fun updateCaptionBounds(bounds: Rect) {
		regions = regions.copy(caption = bounds)
	}

	fun updateMinimizeButtonBounds(bounds: Rect) {
		regions = regions.copy(minimizeButton = bounds)
	}

	fun updateMaximizeButtonBounds(bounds: Rect) {
		regions = regions.copy(maximizeButton = bounds)
	}

	fun updateCloseButtonBounds(bounds: Rect) {
		regions = regions.copy(closeButton = bounds)
	}

	fun minimize() {
		user32.ShowWindow(windowHandle, WinUser.SW_MINIMIZE)
	}

	fun toggleMaximize() {
		val command = if (isMaximized()) WinUser.SW_RESTORE else WinUser.SW_MAXIMIZE
		user32.ShowWindow(windowHandle, command)
	}

	fun postCloseRequest() {
		user32.PostMessage(windowHandle, WindowsMessage.Close, WPARAM(0), LPARAM(0))
	}

	fun setDarkTheme(isDarkTheme: Boolean) {
		if (darkTheme == isDarkTheme || closed) return
		darkTheme = isDarkTheme
		val value = if (isDarkTheme) 1 else 0
		val currentAttributeApplied = dwm?.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.UseImmersiveDarkMode,
			value,
		) == true
		if (!currentAttributeApplied) {
			dwm?.setIntAttribute(windowHandle, DwmWindowAttribute.UseImmersiveDarkModeBefore20H1, value)
		}
	}

	private fun attach() {
		val outerPrevious = user32.installWindowProcedure(windowHandle, outerProcedure)
		outerPrevious.requireInstalled("top-level")
		previousOuterProcedure = outerPrevious
		outerSubclassInstalled = true
		activeControllers[windowHandle.value()] = this
		try {
			installContentWindowProcedure()
			refreshMetrics()
			applyCustomFrameBorder()
			requestFrameRecalculation()
			EventQueue.invokeLater {
				if (!closed) usesNativeBackdrop = enableNativeBackdrop()
			}
		} catch (error: Throwable) {
			close()
			throw error
		}
	}

	private fun LONG_PTR.requireInstalled(target: String) {
		require(toLong() != 0L) { "Could not subclass the $target window" }
	}

	private fun installContentWindowProcedure() {
		check(!contentSubclassInstalled)
		val finalContentHandle = nativeContentHandle(skiaLayer)
		val contentPrevious = user32.installWindowProcedure(finalContentHandle, contentProcedure)
		contentPrevious.requireInstalled("Compose content")
		contentHandle = finalContentHandle
		previousContentProcedure = contentPrevious
		contentSubclassInstalled = true
	}

	private fun detachContentWindowProcedure(): Boolean {
		if (!contentSubclassInstalled) return true
		val currentContentHandle = contentHandle ?: return false
		val previousContent = previousContentProcedure ?: return false
		val restoreResult = runCatching {
			user32.restoreWindowProcedureIfOwned(currentContentHandle, contentProcedure, previousContent)
		}.getOrDefault(WindowProcedureRestoreResult.StillInstalled)
		if (restoreResult == WindowProcedureRestoreResult.StillInstalled) return false
		contentSubclassInstalled = false
		previousContentProcedure = null
		contentHandle = null
		return true
	}

	private fun enableNativeBackdrop(): Boolean {
		if (!window.renderApi.supportsWindowsTransparentBackground()) return false
		val frameExtended = dwm?.extendFrame(windowHandle, extendedFrameMargins()) == true
		val backdropApplied = frameExtended && dwm.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.SystemBackdropType,
			DwmSystemBackdropType.MainWindow,
		) == true
		if (!backdropApplied) {
			resetDwmBackdrop()
			return false
		}
		applySystemFrameBorder()
		systemFrameExtended = true
		dwm.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.RedirectionBitmapAlpha,
			DwmAttributeValue.Enabled,
		)
		if (!detachContentWindowProcedure()) {
			resetDwmBackdrop()
			return false
		}

		return runCatching {
			skiaLayer.transparency = true
			installBackdropContentPane()
			transparentSurfaceConfigured = true
			installContentWindowProcedure()
			refreshSystemCaptionButtonBounds()
			publishUsesSystemCaptionButtonHandling(systemCaptionButtonBoundsAvailable)
			true
		}.getOrElse {
			runCatching { restoreContentPane() }
			runCatching { skiaLayer.transparency = false }
			transparentSurfaceConfigured = false
			resetDwmBackdrop()
			val contentHookRecovered = runCatching {
				if (!closed && !contentSubclassInstalled) installContentWindowProcedure()
			}.isSuccess
			if (!contentHookRecovered) transitionToDecoratedFallback()
			false
		}
	}

	private fun reapplyNativeBackdrop() {
		if (!transparentSurfaceConfigured) return
		val frameExtended = dwm?.extendFrame(windowHandle, extendedFrameMargins()) == true
		val backdropApplied = frameExtended && dwm.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.SystemBackdropType,
			DwmSystemBackdropType.MainWindow,
		) == true
		if (backdropApplied) {
			applySystemFrameBorder()
			systemFrameExtended = true
			dwm.setIntAttribute(
				windowHandle,
				DwmWindowAttribute.RedirectionBitmapAlpha,
				DwmAttributeValue.Enabled,
			)
			refreshSystemCaptionButtonBounds()
			publishUsesSystemCaptionButtonHandling(systemCaptionButtonBoundsAvailable)
		} else {
			resetDwmBackdrop()
		}
		usesNativeBackdrop = backdropApplied
	}

	private fun disableNativeBackdrop() {
		resetDwmBackdrop()
		runCatching { restoreContentPane() }
		runCatching { skiaLayer.transparency = false }
		transparentSurfaceConfigured = false
		usesNativeBackdrop = false
	}

	private fun resetDwmBackdrop() {
		systemFrameExtended = false
		systemCaptionButtonBoundsAvailable = false
		publishUsesSystemCaptionButtonHandling(false)
		dwm?.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.RedirectionBitmapAlpha,
			DwmAttributeValue.Disabled,
		)
		dwm?.setIntAttribute(windowHandle, DwmWindowAttribute.SystemBackdropType, DwmSystemBackdropType.Auto)
		dwm?.extendFrame(windowHandle, WindowsWindowMargins())
		applyCustomFrameBorder()
	}

	private fun applyCustomFrameBorder() {
		dwm?.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.BorderColor,
			DwmColor.None,
		)
	}

	private fun applySystemFrameBorder() {
		dwm?.setIntAttribute(
			windowHandle,
			DwmWindowAttribute.BorderColor,
			DwmColor.Default,
		)
	}

	private fun installBackdropContentPane() {
		if (backdropContentPane != null) return
		val oldContentPane = window.contentPane
		val oldBorderLayout = oldContentPane.layout as? BorderLayout
		val children = oldContentPane.components.map { component ->
			WindowsContentPaneChild(component, oldBorderLayout?.getConstraints(component))
		}
		val newContentPane = WindowsAlphaClearingContentPane().apply {
			name = "${window.name}.contentPane"
			layout = object : BorderLayout() {
				override fun addLayoutComponent(component: Component, constraints: Any?) {
					super.addLayoutComponent(component, constraints ?: CENTER)
				}
			}
			background = TransparentAwtColor
		}

		originalContentPane = oldContentPane
		originalContentChildren = children
		backdropContentPane = newContentPane
		children.forEach { newContentPane.add(it.component) }
		window.contentPane = newContentPane
		window.validate()
	}

	private fun restoreContentPane() {
		val oldContentPane = originalContentPane ?: return
		val currentContentPane = backdropContentPane ?: return
		currentContentPane.components.toList().forEach { component ->
			val original = originalContentChildren.firstOrNull { it.component === component }
			if (original?.constraint != null) {
				oldContentPane.add(component, original.constraint)
			} else {
				oldContentPane.add(component)
			}
		}
		window.contentPane = oldContentPane
		window.validate()
		originalContentPane = null
		originalContentChildren = emptyList()
		backdropContentPane = null
	}

	private fun handleOuterMessage(
		hWnd: HWND,
		message: Int,
		wParam: WPARAM,
		lParam: LPARAM,
	): LRESULT {
		if (closed && message != WindowsMessage.NonClientDestroy) {
			return callPreviousOuter(hWnd, message, wParam, lParam)
		}
		return when (message) {
			WindowsMessage.NonClientDestroy -> {
				val result = callPreviousOuter(hWnd, message, wParam, lParam)
				runCatching {
					closed = true
					outerSubclassInstalled = false
					previousOuterProcedure = null
					releaseControllerIfDetached()
				}
				result
			}
			WindowsMessage.NonClientCalculateSize -> {
				runCatching { refreshMetrics() }
				if (wParam.toLong() != 0L) LRESULT(0) else callPreviousOuter(hWnd, message, wParam, lParam)
			}
			WindowsMessage.Activate -> {
				runCatching {
					publishWindowActive(wParam.toInt() and 0xffff != WindowsActivation.Inactive)
				}
				callPreviousOuter(hWnd, message, wParam, lParam)
			}
			WindowsMessage.NonClientHitTest ->
				(if (usesSystemCaptionButtonHandling) dwm?.defaultWindowProcedure(hWnd, message, wParam, lParam) else null)
					?: runCatching {
						pointInWindow(lParam)?.let { point -> LRESULT(hitTest(point.x, point.y).toLong()) }
					}.getOrNull()
					?: callPreviousOuter(hWnd, message, wParam, lParam)
			WindowsMessage.NonClientMouseMove -> {
				runCatching {
					trackNonClientMouseLeave()
					publishCaptionInteraction(
						hovered = wParam.toInt().captionButton(),
						pressed = nativePressedCaptionButton,
					)
				}
				callWindowsCaptionProcedure(hWnd, message, wParam, lParam)
			}
			WindowsMessage.NonClientLeftButtonDown -> {
				val button = wParam.toInt().captionButton()
				if (button == null) {
					callPreviousOuter(hWnd, message, wParam, lParam)
				} else if (usesSystemCaptionButtonHandling) {
					callWindowsCaptionProcedure(hWnd, message, wParam, lParam)
				} else {
					beginCaptionButtonPress(button)
					LRESULT(0)
				}
			}
			WindowsMessage.NonClientLeftButtonUp -> {
				if (nativePressedCaptionButton != null) {
					finishCaptionButtonPress()
				} else {
					callWindowsCaptionProcedure(hWnd, message, wParam, lParam)
				}
			}
			WindowsMessage.NonClientLeftButtonDoubleClick -> when {
				wParam.toInt().captionButton() == null -> callPreviousOuter(hWnd, message, wParam, lParam)
				usesSystemCaptionButtonHandling -> callWindowsCaptionProcedure(hWnd, message, wParam, lParam)
				else -> LRESULT(0)
			}
			WindowsMessage.NonClientMouseLeave -> {
				trackingNonClientMouse = false
				runCatching {
					publishCaptionInteraction(hovered = null, pressed = nativePressedCaptionButton)
				}
				dwm?.defaultWindowProcedure(hWnd, message, wParam, lParam)
					?: user32.DefWindowProc(hWnd, message, wParam, lParam)
			}
			WindowsMessage.MouseMove -> if (nativePressedCaptionButton != null) {
				val hovered = captionButtonUnderCursor()
				publishCaptionInteraction(hovered = hovered, pressed = nativePressedCaptionButton)
				LRESULT(0)
			} else {
				callPreviousOuter(hWnd, message, wParam, lParam)
			}
			WindowsMessage.LeftButtonUp -> if (nativePressedCaptionButton != null) {
				finishCaptionButtonPress()
			} else {
				callPreviousOuter(hWnd, message, wParam, lParam)
			}
			WindowsMessage.CancelMode -> {
				cancelCaptionButtonPress(releaseCapture = true)
				callPreviousOuter(hWnd, message, wParam, lParam)
			}
			WindowsMessage.CaptureChanged -> {
				cancelCaptionButtonPress(releaseCapture = false)
				callPreviousOuter(hWnd, message, wParam, lParam)
			}
			WindowsMessage.Size,
			WindowsMessage.DpiChanged,
			-> {
				val result = callPreviousOuter(hWnd, message, wParam, lParam)
				runCatching { refreshMetrics() }
				if (message == WindowsMessage.DpiChanged) runCatching { reapplyNativeBackdrop() }
				result
			}
			WindowsMessage.DwmCompositionChanged -> {
				val result = callPreviousOuter(hWnd, message, wParam, lParam)
				runCatching { reapplyNativeBackdrop() }
				result
			}
			else -> callPreviousOuter(hWnd, message, wParam, lParam)
		}
	}

	private fun handleContentMessage(
		hWnd: HWND,
		message: Int,
		wParam: WPARAM,
		lParam: LPARAM,
	): LRESULT {
		if (closed && message != WindowsMessage.NonClientDestroy) {
			return callPreviousContent(hWnd, message, wParam, lParam)
		}
		return when (message) {
			WindowsMessage.NonClientDestroy -> {
				val result = callPreviousContent(hWnd, message, wParam, lParam)
				runCatching {
					contentSubclassInstalled = false
					previousContentProcedure = null
					contentHandle = null
					releaseControllerIfDetached()
				}
				result
			}
			WindowsMessage.NonClientHitTest -> runCatching {
				pointInWindow(lParam)?.let { point ->
					val result = hitTest(point.x, point.y)
					if (result == WindowsHitTestResult.Client) {
						LRESULT(WindowsHitTestResult.Client.toLong())
					} else {
						LRESULT(WindowsHitTestResult.Transparent.toLong())
					}
				}
			}.getOrNull() ?: callPreviousContent(hWnd, message, wParam, lParam)
			else -> callPreviousContent(hWnd, message, wParam, lParam)
		}
	}

	private fun hitTest(x: Int, y: Int): Int {
		refreshMetrics()
		val currentMetrics = metrics
		val currentRegions = regions
		return windowsHitTest(
			x = x,
			y = y,
			geometry = WindowsHitTestGeometry(
				width = currentMetrics.width,
				height = currentMetrics.height,
				horizontalResizeBorder = currentMetrics.horizontalResizeBorder,
				verticalResizeBorder = currentMetrics.verticalResizeBorder,
				isMaximized = currentMetrics.isMaximized,
				caption = currentRegions.caption,
				minimizeButton = currentRegions.minimizeButton,
				maximizeButton = currentRegions.maximizeButton,
				closeButton = currentRegions.closeButton,
			),
		)
	}

	private fun refreshMetrics() {
		if (closed) return
		val clientRect = RECT()
		if (!user32.GetClientRect(windowHandle, clientRect)) return
		clientRect.read()
		val dpi = runCatching { user32.GetDpiForWindow(windowHandle) }.getOrDefault(UINT(DefaultDpi))
		val frameX = metricForDpi(WinUser.SM_CXFRAME, dpi)
		val frameY = metricForDpi(WinUser.SM_CYFRAME, dpi)
		val paddedBorder = metricForDpi(WinUser.SM_CXPADDEDBORDER, dpi)
		val maximized = isMaximized()
		val nextMetrics = WindowsWindowMetrics(
			width = clientRect.right - clientRect.left,
			height = clientRect.bottom - clientRect.top,
			horizontalResizeBorder = frameX + paddedBorder,
			verticalResizeBorder = frameY + paddedBorder,
			isMaximized = maximized,
		)
		metrics = nextMetrics
		refreshSystemCaptionButtonBounds()
		val nextInsets = windowsContentInsets(
			isMaximized = maximized,
			horizontalResizeBorder = nextMetrics.horizontalResizeBorder,
			verticalResizeBorder = nextMetrics.verticalResizeBorder,
		)
		publishFrameInsets(nextInsets)
	}

	private fun refreshSystemCaptionButtonBounds() {
		val nativeBounds = nativeCaptionButtonBounds() ?: return
		systemCaptionButtonBoundsAvailable = true
		regions = regions.copy(
			minimizeButton = nativeBounds.minimize,
			maximizeButton = nativeBounds.maximize,
			closeButton = nativeBounds.close,
		)
		if (systemFrameExtended) publishUsesSystemCaptionButtonHandling(true)
	}

	private fun extendedFrameMargins(): WindowsWindowMargins {
		val dpi = runCatching { user32.GetDpiForWindow(windowHandle).toInt() }
			.getOrDefault(DefaultDpi.toInt())
		return WindowsWindowMargins(top = windowsTitleBarHeightPixels(dpi))
	}

	private fun nativeCaptionButtonBounds(): WindowsCaptionButtonBounds? {
		val nativeRect = dwm?.getRectAttribute(
			windowHandle,
			DwmWindowAttribute.CaptionButtonBounds,
		) ?: return null
		if (nativeRect.right <= nativeRect.left || nativeRect.bottom <= nativeRect.top) return null

		val windowRect = RECT()
		if (!user32.GetWindowRect(windowHandle, windowRect)) return null
		windowRect.read()
		val clientOrigin = POINT(0, 0)
		if (!user32.ClientToScreen(windowHandle, clientOrigin)) return null
		clientOrigin.read()
		val clientBounds = Rect(
			left = (windowRect.left + nativeRect.left - clientOrigin.x).toFloat(),
			top = (windowRect.top + nativeRect.top - clientOrigin.y).toFloat(),
			right = (windowRect.left + nativeRect.right - clientOrigin.x).toFloat(),
			bottom = (windowRect.top + nativeRect.bottom - clientOrigin.y).toFloat(),
		)
		return splitWindowsCaptionButtonBounds(clientBounds)
	}

	private fun metricForDpi(metric: Int, dpi: UINT): Int = runCatching {
		user32.GetSystemMetricsForDpi(metric, dpi)
	}.getOrElse {
		user32.GetSystemMetrics(metric)
	}

	private fun isMaximized(): Boolean {
		val placement = WinUser.WINDOWPLACEMENT()
		return user32.GetWindowPlacement(windowHandle, placement).booleanValue() &&
			placement.showCmd == WinUser.SW_SHOWMAXIMIZED
	}

	private fun publishFrameInsets(nextInsets: WindowsFrameInsets) {
		if (nextInsets == frameInsets) return
		if (EventQueue.isDispatchThread()) {
			frameInsets = nextInsets
		} else {
			EventQueue.invokeLater {
				if (!closed) frameInsets = nextInsets
			}
		}
	}

	private fun publishUsesSystemCaptionButtonHandling(available: Boolean) {
		if (available == usesSystemCaptionButtonHandling) return
		if (EventQueue.isDispatchThread()) {
			usesSystemCaptionButtonHandling = available
		} else {
			EventQueue.invokeLater {
				if (!closed) usesSystemCaptionButtonHandling = available
			}
		}
	}

	private fun publishCaptionInteraction(
		hovered: WindowsCaptionButton?,
		pressed: WindowsCaptionButton?,
	) {
		if (hovered == hoveredCaptionButton && pressed == pressedCaptionButton) return
		val update = {
			if (!closed) {
				hoveredCaptionButton = hovered
				pressedCaptionButton = pressed
			}
		}
		if (EventQueue.isDispatchThread()) update() else EventQueue.invokeLater { update() }
	}

	private fun publishWindowActive(active: Boolean) {
		if (active == isWindowActive) return
		if (EventQueue.isDispatchThread()) {
			isWindowActive = active
		} else {
			EventQueue.invokeLater {
				if (!closed) isWindowActive = active
			}
		}
	}

	private fun pointInWindow(lParam: LPARAM): POINT? = pointInClient(windowHandle, lParam)

	private fun pointInClient(handle: HWND, lParam: LPARAM): POINT? {
		val screenPoint = windowsScreenPoint(lParam.toLong())
		val point = POINT(screenPoint.x, screenPoint.y)
		return if (user32.ScreenToClient(handle, point)) {
			point.read()
			point
		} else {
			null
		}
	}

	private fun callPreviousOuter(hWnd: HWND, message: Int, wParam: WPARAM, lParam: LPARAM): LRESULT {
		val previous = previousOuterProcedure
		return if (previous != null) {
			user32.CallWindowProc(previous.toPointer(), hWnd, message, wParam, lParam)
		} else {
			user32.DefWindowProc(hWnd, message, wParam, lParam)
		}
	}

	private fun callPreviousContent(hWnd: HWND, message: Int, wParam: WPARAM, lParam: LPARAM): LRESULT {
		val previous = previousContentProcedure
		return if (previous != null) {
			user32.CallWindowProc(previous.toPointer(), hWnd, message, wParam, lParam)
		} else {
			user32.DefWindowProc(hWnd, message, wParam, lParam)
		}
	}

	private fun callWindowsCaptionProcedure(
		hWnd: HWND,
		message: Int,
		wParam: WPARAM,
		lParam: LPARAM,
	): LRESULT = if (wParam.toInt().captionButton() != null) {
		dwm?.defaultWindowProcedure(hWnd, message, wParam, lParam)
			?: user32.DefWindowProc(hWnd, message, wParam, lParam)
	} else {
		callPreviousOuter(hWnd, message, wParam, lParam)
	}

	private fun trackNonClientMouseLeave() {
		if (trackingNonClientMouse) return
		val event = WindowsTrackMouseEvent(
			flags = TrackMouseEventFlag.Leave or TrackMouseEventFlag.NonClient,
			window = windowHandle,
		)
		event.write()
		trackingNonClientMouse = user32.TrackMouseEvent(event)
	}

	private fun beginCaptionButtonPress(button: WindowsCaptionButton) {
		nativePressedCaptionButton = button
		user32.SetCapture(windowHandle)
		publishCaptionInteraction(hovered = button, pressed = button)
	}

	private fun finishCaptionButtonPress(): LRESULT {
		val pressed = nativePressedCaptionButton
		val released = captionButtonUnderCursor()
		nativePressedCaptionButton = null
		user32.ReleaseCapture()
		publishCaptionInteraction(hovered = released, pressed = null)
		if (shouldActivateWindowsCaptionButton(pressed, released)) {
			checkNotNull(pressed)
			when (pressed) {
				WindowsCaptionButton.Minimize -> minimize()
				WindowsCaptionButton.Maximize -> toggleMaximize()
				WindowsCaptionButton.Close -> postCloseRequest()
			}
		}
		return LRESULT(0)
	}

	private fun cancelCaptionButtonPress(releaseCapture: Boolean) {
		val hadPressedButton = nativePressedCaptionButton != null
		nativePressedCaptionButton = null
		if (hadPressedButton && releaseCapture && user32.GetCapture()?.value() == windowHandle.value()) {
			user32.ReleaseCapture()
		}
		publishCaptionInteraction(hovered = null, pressed = null)
	}

	private fun captionButtonUnderCursor(): WindowsCaptionButton? {
		val point = POINT()
		if (!user32.GetCursorPos(point) || !user32.ScreenToClient(windowHandle, point)) return null
		point.read()
		return hitTest(point.x, point.y).captionButton()
	}

	private fun requestFrameRecalculation() {
		user32.SetWindowPos(
			windowHandle,
			null,
			0,
			0,
			0,
			0,
			SetWindowPositionFlag.NoMove or
				SetWindowPositionFlag.NoSize or
				SetWindowPositionFlag.NoZOrder or
				SetWindowPositionFlag.NoActivate or
				SetWindowPositionFlag.FrameChanged,
		)
	}

	private fun transitionToDecoratedFallback() {
		if (!EventQueue.isDispatchThread()) {
			EventQueue.invokeLater { transitionToDecoratedFallback() }
			return
		}
		closed = true
		detachContentWindowProcedure()
		restoreOuterWindowProcedure()
		runCatching { resetDwmBackdrop() }
		runCatching { applySystemFrameBorder() }
		runCatching { restoreContentPane() }
		runCatching { skiaLayer.transparency = false }
		transparentSurfaceConfigured = false
		usesNativeBackdrop = false
		usesSystemCaptionButtonHandling = false
		usesCustomChrome = false
		if (window.isDisplayable) runCatching { requestFrameRecalculation() }
		releaseControllerIfDetached()
	}

	private fun restoreOuterWindowProcedure() {
		if (!outerSubclassInstalled) return
		val previousOuter = previousOuterProcedure ?: return
		val restoreResult = runCatching {
			user32.restoreWindowProcedureIfOwned(windowHandle, outerProcedure, previousOuter)
		}.getOrDefault(WindowProcedureRestoreResult.StillInstalled)
		if (restoreResult != WindowProcedureRestoreResult.StillInstalled) {
			outerSubclassInstalled = false
			previousOuterProcedure = null
		}
	}

	override fun close() {
		if (!EventQueue.isDispatchThread()) {
			EventQueue.invokeLater { close() }
			return
		}
		if (closed && !outerSubclassInstalled && !contentSubclassInstalled) return
		closed = true
		detachContentWindowProcedure()
		restoreOuterWindowProcedure()
		runCatching { disableNativeBackdrop() }
		if (window.isDisplayable) {
			runCatching { requestFrameRecalculation() }
		}
		releaseControllerIfDetached()
	}

	private fun releaseControllerIfDetached() {
		if (!outerSubclassInstalled && !contentSubclassInstalled) {
			activeControllers.remove(windowHandle.value(), this)
		}
	}

	private class OuterWindowProcedure(
		private val owner: WindowsNativeWindowController,
	) : WindowProc {
		override fun callback(hWnd: HWND, message: Int, wParam: WPARAM, lParam: LPARAM): LRESULT =
			owner.handleOuterMessage(hWnd, message, wParam, lParam)
	}

	private class ContentWindowProcedure(
		private val owner: WindowsNativeWindowController,
	) : WindowProc {
		override fun callback(hWnd: HWND, message: Int, wParam: WPARAM, lParam: LPARAM): LRESULT =
			owner.handleContentMessage(hWnd, message, wParam, lParam)
	}

	companion object {
		fun install(window: Window): WindowsNativeWindowController? {
			if (!Platform.isWindows()) return null
			return runCatching {
				check(EventQueue.isDispatchThread()) { "Windows chrome must be installed on the AWT event thread" }
				val composeWindow = requireNotNull(window as? ComposeWindow) {
					"Windows window is ${window.javaClass.name}, not ComposeWindow"
				}
				val skiaLayer = requireNotNull(composeWindow.findComponent<SkiaLayer>()) {
					"Compose did not create a Skia layer"
				}
				val nativeWindow = composeWindow.windowHandle
				require(nativeWindow != 0L) { "Compose did not create a native window" }
				WindowsNativeWindowController(
					window = composeWindow,
					skiaLayer = skiaLayer,
					windowHandle = HWND(Pointer(nativeWindow)),
					user32 = WindowsUser32.instance,
					dwm = WindowsDwmApi.load(),
				).also { it.attach() }
			}.getOrNull()
		}

		private val activeControllers = ConcurrentHashMap<Long, WindowsNativeWindowController>()

		private fun nativeContentHandle(skiaLayer: SkiaLayer): HWND {
			val nativeContent = Native.getComponentPointer(skiaLayer.canvas)
			require(Pointer.nativeValue(nativeContent) != 0L) { "Compose did not create a native content window" }
			return HWND(nativeContent)
		}
	}
}

private data class WindowsWindowMetrics(
	val width: Int = 0,
	val height: Int = 0,
	val horizontalResizeBorder: Int = 0,
	val verticalResizeBorder: Int = 0,
	val isMaximized: Boolean = false,
)

private data class WindowsHitRegions(
	val caption: Rect = Rect.Zero,
	val minimizeButton: Rect = Rect.Zero,
	val maximizeButton: Rect = Rect.Zero,
	val closeButton: Rect = Rect.Zero,
)

private data class WindowsContentPaneChild(
	val component: Component,
	val constraint: Any?,
)

private class WindowsAlphaClearingContentPane : JPanel() {
	override fun paint(graphics: Graphics) {
		if (background.alpha != 255) {
			val copy = graphics.create()
			try {
				if (copy is Graphics2D) {
					copy.color = background
					copy.composite = AlphaComposite.getInstance(AlphaComposite.SRC)
					copy.fillRect(0, 0, width, height)
				}
			} finally {
				copy.dispose()
			}
		}
		super.paint(graphics)
	}
}

@Structure.FieldOrder("size", "flags", "window", "hoverTime")
private class WindowsTrackMouseEvent(
	@JvmField var flags: Int = 0,
	@JvmField var window: HWND? = null,
) : Structure() {
	@JvmField var size: Int = size()
	@JvmField var hoverTime: Int = 0
}

private fun Rect.contains(x: Int, y: Int): Boolean =
	x.toFloat() >= left && x.toFloat() < right && y.toFloat() >= top && y.toFloat() < bottom

private fun Int.captionButton(): WindowsCaptionButton? = when (this) {
	WindowsHitTestResult.MinimizeButton -> WindowsCaptionButton.Minimize
	WindowsHitTestResult.MaximizeButton -> WindowsCaptionButton.Maximize
	WindowsHitTestResult.CloseButton -> WindowsCaptionButton.Close
	else -> null
}

private fun HWND.value(): Long = Pointer.nativeValue(pointer)

private fun <T : JComponent> Container.findComponent(type: Class<T>): T? {
	components.forEach { component ->
		if (type.isInstance(component)) return type.cast(component)
		if (component is Container) {
			component.findComponent(type)?.let { return it }
		}
	}
	return null
}

private inline fun <reified T : JComponent> Container.findComponent(): T? = findComponent(T::class.java)

@Suppress("FunctionName")
private interface WindowsUser32 : User32 {
	fun SetWindowLong(hWnd: HWND, nIndex: Int, procedure: WindowProc): Int
	fun SetWindowLongPtr(hWnd: HWND, nIndex: Int, procedure: WindowProc): LONG_PTR
	fun SetCapture(hWnd: HWND): HWND?
	fun GetCapture(): HWND?
	fun ReleaseCapture(): Boolean
	fun TrackMouseEvent(eventTrack: WindowsTrackMouseEvent): Boolean
	fun GetSystemMetricsForDpi(nIndex: Int, dpi: UINT): Int
	fun GetDpiForWindow(hWnd: HWND): UINT
	fun ScreenToClient(hWnd: HWND, point: POINT): Boolean
	fun ClientToScreen(hWnd: HWND, point: POINT): Boolean

	companion object {
		val instance: WindowsUser32 by lazy {
			Native.load("user32", WindowsUser32::class.java, W32APIOptions.DEFAULT_OPTIONS)
		}
	}
}

private enum class WindowProcedureRestoreResult {
	Restored,
	AlreadyDetached,
	StillInstalled,
}

private fun WindowsUser32.installWindowProcedure(handle: HWND, procedure: WindowProc): LONG_PTR {
	Native.setLastError(0)
	val previous = if (Platform.is64Bit()) {
		SetWindowLongPtr(handle, WinUser.GWL_WNDPROC, procedure)
	} else {
		LONG_PTR(SetWindowLong(handle, WinUser.GWL_WNDPROC, procedure).toLong())
	}
	check(previous.toLong() != 0L || Native.getLastError() == 0) {
		"SetWindowLongPtr failed with error ${Native.getLastError()}"
	}
	return previous
}

private fun WindowsUser32.restoreWindowProcedureIfOwned(
	handle: HWND,
	procedure: WindowProc,
	previous: LONG_PTR,
): WindowProcedureRestoreResult {
	if (!IsWindow(handle)) return WindowProcedureRestoreResult.AlreadyDetached
	val current = getWindowProcedure(handle)
	val callback = Pointer.nativeValue(CallbackReference.getFunctionPointer(procedure))
	if (current.toLong() == previous.toLong()) return WindowProcedureRestoreResult.AlreadyDetached
	if (current.toLong() != callback) return WindowProcedureRestoreResult.StillInstalled

	Native.setLastError(0)
	val replaced = if (Platform.is64Bit()) {
		LONG_PTR(Pointer.nativeValue(SetWindowLongPtr(handle, WinUser.GWL_WNDPROC, previous.toPointer())))
	} else {
		LONG_PTR(SetWindowLong(handle, WinUser.GWL_WNDPROC, previous.toInt()).toLong())
	}
	if (replaced.toLong() == 0L && Native.getLastError() != 0) {
		return WindowProcedureRestoreResult.StillInstalled
	}
	return if (getWindowProcedure(handle).toLong() == previous.toLong()) {
		WindowProcedureRestoreResult.Restored
	} else {
		WindowProcedureRestoreResult.StillInstalled
	}
}

private fun WindowsUser32.getWindowProcedure(handle: HWND): LONG_PTR =
	if (Platform.is64Bit()) GetWindowLongPtr(handle, WinUser.GWL_WNDPROC)
	else LONG_PTR(GetWindowLong(handle, WinUser.GWL_WNDPROC).toLong())

@Structure.FieldOrder("left", "right", "top", "bottom")
internal class WindowsWindowMargins(
	@JvmField var left: Int = 0,
	@JvmField var right: Int = 0,
	@JvmField var top: Int = 0,
	@JvmField var bottom: Int = 0,
) : Structure()

@Suppress("FunctionName")
private interface WindowsDwmApi : StdCallLibrary {
	fun DwmDefWindowProc(
		hWnd: HWND,
		message: Int,
		wParam: WPARAM,
		lParam: LPARAM,
		result: ULONG_PTRByReference,
	): Boolean

	fun DwmExtendFrameIntoClientArea(hWnd: HWND, margins: WindowsWindowMargins): HRESULT
	fun DwmGetWindowAttribute(hWnd: HWND, attribute: Int, value: RECT, valueSize: Int): HRESULT
	fun DwmSetWindowAttribute(hWnd: HWND, attribute: Int, value: IntByReference, valueSize: Int): HRESULT

	fun extendFrame(handle: HWND, margins: WindowsWindowMargins): Boolean =
		DwmExtendFrameIntoClientArea(handle, margins).succeeded()

	fun setIntAttribute(handle: HWND, attribute: Int, value: Int): Boolean =
		DwmSetWindowAttribute(handle, attribute, IntByReference(value), Int.SIZE_BYTES).succeeded()

	fun getRectAttribute(handle: HWND, attribute: Int): RECT? {
		val value = RECT()
		if (!DwmGetWindowAttribute(handle, attribute, value, value.size()).succeeded()) return null
		value.read()
		return value
	}

	fun defaultWindowProcedure(hWnd: HWND, message: Int, wParam: WPARAM, lParam: LPARAM): LRESULT? {
		val result = ULONG_PTRByReference()
		return if (DwmDefWindowProc(hWnd, message, wParam, lParam, result)) {
			LRESULT(result.value.toLong())
		} else {
			null
		}
	}

	companion object {
		fun load(): WindowsDwmApi? = runCatching {
			Native.load("dwmapi", WindowsDwmApi::class.java, W32APIOptions.DEFAULT_OPTIONS)
		}.getOrNull()
	}
}

private fun HRESULT.succeeded(): Boolean = toInt() >= 0

internal fun GraphicsApi.supportsWindowsTransparentBackground(): Boolean = when (this) {
	GraphicsApi.UNKNOWN,
	GraphicsApi.SOFTWARE_FAST,
	GraphicsApi.SOFTWARE_COMPAT,
	-> false
	else -> true
}

internal fun shouldActivateWindowsCaptionButton(
	pressed: WindowsCaptionButton?,
	released: WindowsCaptionButton?,
): Boolean = pressed != null && pressed == released

private object WindowsMessage {
	const val Size = 0x0005
	const val Activate = 0x0006
	const val Close = 0x0010
	const val CancelMode = 0x001f
	const val NonClientDestroy = 0x0082
	const val NonClientCalculateSize = 0x0083
	const val NonClientHitTest = 0x0084
	const val NonClientMouseMove = 0x00a0
	const val NonClientLeftButtonDown = 0x00a1
	const val NonClientLeftButtonUp = 0x00a2
	const val NonClientLeftButtonDoubleClick = 0x00a3
	const val MouseMove = 0x0200
	const val LeftButtonUp = 0x0202
	const val CaptureChanged = 0x0215
	const val NonClientMouseLeave = 0x02a2
	const val DwmCompositionChanged = 0x031e
	const val DpiChanged = 0x02e0
}

private object WindowsActivation {
	const val Inactive = 0
}

private object SetWindowPositionFlag {
	const val NoSize = 0x0001
	const val NoMove = 0x0002
	const val NoZOrder = 0x0004
	const val NoActivate = 0x0010
	const val FrameChanged = 0x0020
}

private object DwmWindowAttribute {
	const val CaptionButtonBounds = 5
	const val UseImmersiveDarkModeBefore20H1 = 19
	const val UseImmersiveDarkMode = 20
	const val BorderColor = 34
	const val SystemBackdropType = 38
	const val RedirectionBitmapAlpha = 39
}

private object DwmColor {
	const val Default = -1
	const val None = -2
}

private object TrackMouseEventFlag {
	const val Leave = 0x00000002
	const val NonClient = 0x00000010
}

private object DwmAttributeValue {
	const val Disabled = 0
	const val Enabled = 1
}

private object DwmSystemBackdropType {
	const val Auto = 0
	const val MainWindow = 2
}

private val TransparentAwtColor = AwtColor(0, 0, 0, 0)
private const val DefaultDpi = 96L
