package com.vnidrop.app

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.window.WindowPlacement
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.jetbrains.skiko.GraphicsApi

class DesktopWindowChromeTest {
	@Test
	fun maximizeControlTogglesBetweenFloatingAndMaximized() {
		assertEquals(WindowPlacement.Maximized, toggledWindowPlacement(WindowPlacement.Floating))
		assertEquals(WindowPlacement.Floating, toggledWindowPlacement(WindowPlacement.Maximized))
	}

	@Test
	fun closeControlUsesDestructiveStateWhenHoveredOrPressed() {
		assertEquals(
			LinuxWindowControlVisualState.DestructiveHovered,
			linuxWindowControlVisualState(isClose = true, isHovered = true, isPressed = false),
		)
		assertEquals(
			LinuxWindowControlVisualState.DestructivePressed,
			linuxWindowControlVisualState(isClose = true, isHovered = false, isPressed = true),
		)
	}

	@Test
	fun standardControlsKeepNeutralInteractionState() {
		assertEquals(
			LinuxWindowControlVisualState.NeutralHovered,
			linuxWindowControlVisualState(isClose = false, isHovered = true, isPressed = false),
		)
		assertEquals(
			LinuxWindowControlVisualState.NeutralPressed,
			linuxWindowControlVisualState(isClose = false, isHovered = false, isPressed = true),
		)
		assertEquals(
			LinuxWindowControlVisualState.Default,
			linuxWindowControlVisualState(isClose = false, isHovered = false, isPressed = false),
		)
	}

	@Test
	fun inactiveWindowDimsControlsUntilTheyAreInteractedWith() {
		assertEquals(
			LinuxWindowControlVisualState.Inactive,
			linuxWindowControlVisualState(
				isClose = false,
				isHovered = false,
				isPressed = false,
				isWindowFocused = false,
			),
		)
		assertEquals(
			LinuxWindowControlVisualState.NeutralHovered,
			linuxWindowControlVisualState(
				isClose = false,
				isHovered = true,
				isPressed = false,
				isWindowFocused = false,
			),
		)
	}

	@Test
	fun windowsHitTestingPreservesNativeResizeAndCaptionBehavior() {
		val geometry = windowsGeometry()

		assertEquals(WindowsHitTestResult.TopLeft, windowsHitTest(2, 2, geometry))
		assertEquals(WindowsHitTestResult.Right, windowsHitTest(1_198, 300, geometry))
		assertEquals(WindowsHitTestResult.Bottom, windowsHitTest(600, 798, geometry))
		assertEquals(WindowsHitTestResult.Caption, windowsHitTest(600, 24, geometry))
		assertEquals(WindowsHitTestResult.Client, windowsHitTest(600, 200, geometry))
	}

	@Test
	fun windowsCaptionButtonsReturnNativeHitCodesForSnapLayouts() {
		val geometry = windowsGeometry()

		assertEquals(WindowsHitTestResult.MinimizeButton, windowsHitTest(1_075, 20, geometry))
		assertEquals(WindowsHitTestResult.MaximizeButton, windowsHitTest(1_120, 20, geometry))
		assertEquals(WindowsHitTestResult.CloseButton, windowsHitTest(1_175, 20, geometry))
	}

	@Test
	fun nativeCaptionStripIsSplitIntoThreeAdjacentButtons() {
		val bounds = splitWindowsCaptionButtonBounds(Rect(646f, 0f, 793f, 30f))

		assertEquals(Rect(646f, 0f, 695f, 30f), bounds.minimize)
		assertEquals(Rect(695f, 0f, 744f, 30f), bounds.maximize)
		assertEquals(Rect(744f, 0f, 793f, 30f), bounds.close)
	}

	@Test
	fun extendedDwmTitleBarMarginScalesWithWindowDpi() {
		assertEquals(48, windowsTitleBarHeightPixels(96))
		assertEquals(60, windowsTitleBarHeightPixels(120))
		assertEquals(72, windowsTitleBarHeightPixels(144))
	}

	@Test
	fun maximizedWindowsDoNotExposeResizeBorders() {
		val geometry = windowsGeometry().copy(isMaximized = true)

		assertEquals(WindowsHitTestResult.Caption, windowsHitTest(2, 2, geometry))
	}

	@Test
	fun restoredWindowsPaintThroughTheResizeBorderWhileMaximizedWindowsStayInset() {
		assertEquals(
			WindowsFrameInsets(),
			windowsContentInsets(
				isMaximized = false,
				horizontalResizeBorder = 8,
				verticalResizeBorder = 8,
			),
		)
		assertEquals(
			WindowsFrameInsets(left = 8, top = 9, right = 8, bottom = 9),
			windowsContentInsets(
				isMaximized = true,
				horizontalResizeBorder = 8,
				verticalResizeBorder = 9,
			),
		)
	}

	@Test
	fun nativeMouseCoordinatesPreserveNegativeMonitorPositions() {
		val x = -320
		val y = -48
		val packed = ((y and 0xffff).toLong() shl 16) or (x and 0xffff).toLong()

		assertEquals(WindowsScreenPoint(x, y), windowsScreenPoint(packed))
	}

	@Test
	fun nativeBackdropRequiresAGpuRendererWithTransparentSwapChainSupport() {
		assertTrue(GraphicsApi.DIRECT3D.supportsWindowsTransparentBackground())
		assertTrue(GraphicsApi.OPENGL.supportsWindowsTransparentBackground())
		assertFalse(GraphicsApi.UNKNOWN.supportsWindowsTransparentBackground())
		assertFalse(GraphicsApi.SOFTWARE_FAST.supportsWindowsTransparentBackground())
		assertFalse(GraphicsApi.SOFTWARE_COMPAT.supportsWindowsTransparentBackground())
	}

	@Test
	fun captionActionRequiresReleaseOnTheOriginallyPressedButton() {
		assertTrue(
			shouldActivateWindowsCaptionButton(
				WindowsCaptionButton.Maximize,
				WindowsCaptionButton.Maximize,
			),
		)
		assertFalse(
			shouldActivateWindowsCaptionButton(
				WindowsCaptionButton.Close,
				WindowsCaptionButton.Maximize,
			),
		)
		assertFalse(shouldActivateWindowsCaptionButton(WindowsCaptionButton.Close, null))
	}

	private fun windowsGeometry() = WindowsHitTestGeometry(
		width = 1_200,
		height = 800,
		horizontalResizeBorder = 8,
		verticalResizeBorder = 8,
		isMaximized = false,
		caption = Rect(0f, 0f, 1_200f, 48f),
		minimizeButton = Rect(1_062f, 8f, 1_108f, 40f),
		maximizeButton = Rect(1_108f, 8f, 1_154f, 40f),
		closeButton = Rect(1_154f, 8f, 1_200f, 40f),
	)
}
