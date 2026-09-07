package com.vnidrop.app.ui.platform

import androidx.compose.runtime.Composable

@Composable
internal expect fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit)

@Composable
internal expect fun FullscreenDialog(onDismissRequest: () -> Unit, content: @Composable () -> Unit)
