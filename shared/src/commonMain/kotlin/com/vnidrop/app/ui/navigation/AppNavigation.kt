package com.vnidrop.app.ui.navigation

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.DesktopNavigationWidthDp
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource

enum class NavigationStyle {
	AndroidBottomBar,
	AndroidRail,
	DesktopSidebar,
}

fun navigationStyleFor(uiPlatform: UiPlatform, windowClass: WindowClass): NavigationStyle = when {
	uiPlatform.isDesktop -> NavigationStyle.DesktopSidebar
	windowClass == WindowClass.Phone -> NavigationStyle.AndroidBottomBar
	else -> NavigationStyle.AndroidRail
}

@Composable
fun AppSidebarNavigation(
	selected: AppDestination,
	style: NavigationStyle,
	onDestinationSelected: (AppDestination) -> Unit,
	modifier: Modifier = Modifier,
	useNativeWindowBackdrop: Boolean = false,
) {
	when (style) {
		NavigationStyle.AndroidRail -> AndroidNavigationRail(selected, onDestinationSelected, modifier)
		NavigationStyle.DesktopSidebar -> DesktopSidebarNavigation(
			selected = selected,
			onDestinationSelected = onDestinationSelected,
			modifier = modifier,
			useNativeWindowBackdrop = useNativeWindowBackdrop,
		)
		NavigationStyle.AndroidBottomBar -> error("Bottom navigation is rendered by the phone shell")
	}
}

@Composable
private fun AndroidNavigationRail(
	selected: AppDestination,
	onDestinationSelected: (AppDestination) -> Unit,
	modifier: Modifier = Modifier,
) {
	val colors = LocalVniDropColors.current
	NavigationRail(
		modifier = modifier.fillMaxHeight(),
		containerColor = colors.backgroundSurface200,
	) {
		Spacer(Modifier.height(8.dp))
		primaryNavigationItems.forEach { item ->
			val label = stringResource(item.label)
			NavigationRailItem(
				selected = item.destination == selected,
				onClick = { onDestinationSelected(item.destination) },
				icon = { PlatformIcon(item.icon, contentDescription = label) },
				label = {
					Text(
						label,
						color = colors.foregroundLight,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
					)
				},
				colors = NavigationRailItemDefaults.colors(
					selectedIconColor = colors.brandLink,
					selectedTextColor = colors.brandLink,
					indicatorColor = colors.backgroundSelection,
					unselectedIconColor = colors.foregroundLight,
					unselectedTextColor = colors.foregroundLight,
				),
			)
		}
	}
}

@Composable
private fun DesktopSidebarNavigation(
	selected: AppDestination,
	onDestinationSelected: (AppDestination) -> Unit,
	modifier: Modifier = Modifier,
	useNativeWindowBackdrop: Boolean = false,
) {
	val colors = LocalVniDropColors.current
	Column(
		modifier = modifier
			.width(DesktopNavigationWidthDp.dp)
			.fillMaxHeight()
			.background(if (useNativeWindowBackdrop) Color.Transparent else colors.backgroundSurface200)
			.padding(horizontal = 12.dp, vertical = 14.dp),
		verticalArrangement = Arrangement.spacedBy(4.dp),
	) {
		Text(
			text = "VniDrop",
			modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
			style = MaterialTheme.typography.titleMedium,
			fontWeight = FontWeight.SemiBold,
		)
		Spacer(Modifier.height(4.dp))
		primaryNavigationItems.forEach { item ->
			DesktopNavigationItem(
				item = item,
				selected = item.destination == selected,
				onClick = { onDestinationSelected(item.destination) },
			)
		}
	}
}

@Composable
private fun DesktopNavigationItem(
	item: NavigationItem,
	selected: Boolean,
	onClick: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val foreground = if (selected) colors.foregroundDefault else colors.foregroundLight
	val label = stringResource(item.label)
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.clip(RoundedCornerShape(8.dp))
			.background(if (selected) colors.backgroundSelection else Color.Transparent)
			.selectable(selected = selected, onClick = onClick)
			.padding(horizontal = 12.dp, vertical = 10.dp),
		verticalAlignment = Alignment.CenterVertically,
		horizontalArrangement = Arrangement.spacedBy(12.dp),
	) {
		PlatformIcon(item.icon, contentDescription = label, tint = if (selected) colors.brandLink else foreground, modifier = Modifier.size(20.dp))
		Text(
			text = label,
			color = foreground,
			style = MaterialTheme.typography.bodyMedium,
			fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
			maxLines = 1,
			overflow = TextOverflow.Ellipsis,
		)
	}
}

@Composable
fun AppBottomNavigation(
	selected: AppDestination,
	onDestinationSelected: (AppDestination) -> Unit,
	modifier: Modifier = Modifier,
) {
	val colors = LocalVniDropColors.current
	NavigationBar(
		modifier = modifier.fillMaxWidth(),
		containerColor = colors.backgroundSurface200,
	) {
		primaryNavigationItems.forEach { item ->
			val label = stringResource(item.label)
			val isSelected = item.destination == selected
			NavigationBarItem(
				selected = isSelected,
				onClick = { onDestinationSelected(item.destination) },
				icon = {
					Box(
						modifier = Modifier
							.width(64.dp)
							.height(32.dp)
							.testTag("bottom-nav-indicator-${item.destination.name}")
							.then(
								if (isSelected) {
									Modifier.background(
										brush = Brush.horizontalGradient(listOf(colors.brand200, colors.brand300)),
										shape = RoundedCornerShape(16.dp),
									)
								} else {
									Modifier
								},
							),
						contentAlignment = Alignment.Center,
					) {
						PlatformIcon(item.icon, contentDescription = label, modifier = Modifier.size(24.dp))
					}
				},
				label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
				colors = NavigationBarItemDefaults.colors(
					selectedIconColor = colors.brandLink,
					selectedTextColor = colors.foregroundLight,
					indicatorColor = Color.Transparent,
					unselectedIconColor = colors.foregroundLight,
					unselectedTextColor = colors.foregroundLight,
				),
			)
		}
	}
}
