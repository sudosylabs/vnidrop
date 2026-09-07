package com.vnidrop.app.feature.receive

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import android.util.Size
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.VniDropThemeTokens
import com.vnidrop.app.ui.platform.FullscreenDialog
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.*
import zxingcpp.BarcodeReader

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun QrScanner(activity: ComponentActivity, onResult: (Result<String>) -> Unit, onDismiss: () -> Unit) {
	var granted by remember { mutableStateOf(ContextCompat.checkSelfPermission(activity, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) }
	var requested by rememberSaveable { mutableStateOf(false) }
	val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it }
	val owner = LocalLifecycleOwner.current
	DisposableEffect(owner) {
		val observer = LifecycleEventObserver { _, event ->
			if (event == Lifecycle.Event.ON_RESUME) granted = ContextCompat.checkSelfPermission(activity, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
		}
		owner.lifecycle.addObserver(observer)
		onDispose { owner.lifecycle.removeObserver(observer) }
	}
	LaunchedEffect(Unit) {
		if (!granted && !requested) { requested = true; permission.launch(Manifest.permission.CAMERA) }
	}
	FullscreenDialog(onDismiss) {
		Scaffold(
			containerColor = MaterialTheme.colorScheme.surface,
			modifier = Modifier.fillMaxSize().testTag("qr-scanner"),
			topBar = { TopAppBar(title = { Text(stringResource(Res.string.receive_method_scan)) }, navigationIcon = { IconButton(onClick = onDismiss) { PlatformIcon(AppIcon.Close, stringResource(Res.string.button_close)) } }) },
		) { padding ->
			Box(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
				if (granted) CameraScanner(activity, onResult)
				else Column(Modifier.align(Alignment.Center).padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
					PlatformIcon(AppIcon.Scan, null, modifier = Modifier.size(48.dp))
					Text(stringResource(Res.string.error_camera))
					val canRequest = activity.shouldShowRequestPermissionRationale(Manifest.permission.CAMERA)
					Button(onClick = {
						if (canRequest) permission.launch(Manifest.permission.CAMERA)
						else activity.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}")))
					}) { Text(stringResource(if (canRequest) Res.string.scanner_allow_camera else Res.string.scanner_open_settings)) }
				}
			}
		}
	}
}

@Composable
private fun CameraScanner(activity: ComponentActivity, onResult: (Result<String>) -> Unit) {
	val lifecycleOwner = LocalLifecycleOwner.current
	val currentResult = rememberUpdatedState(onResult)
	val previewView = remember(activity) { PreviewView(activity).apply { implementationMode = PreviewView.ImplementationMode.COMPATIBLE } }
	var camera by remember { mutableStateOf<Camera?>(null) }
	var unavailable by remember { mutableStateOf(false) }
	var attempt by remember { mutableIntStateOf(0) }
	var torch by remember { mutableStateOf(false) }
	DisposableEffect(lifecycleOwner, previewView, attempt) {
		val mainExecutor = ContextCompat.getMainExecutor(activity)
		val executor = Executors.newSingleThreadExecutor()
		val disposed = AtomicBoolean(false)
		torch = false
		camera = null
		val session = QrScanSession { currentResult.value(it) }
		val preview = Preview.Builder().build().also { it.setSurfaceProvider(previewView.surfaceProvider) }
		val analysis = ImageAnalysis.Builder()
			.setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
			.setResolutionSelector(ResolutionSelector.Builder().setResolutionStrategy(ResolutionStrategy(Size(1280, 720), ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER)).build())
			.build()
		val reader = BarcodeReader(BarcodeReader.Options(formats = setOf(BarcodeReader.Format.QR_CODE), tryRotate = true, tryInvert = true, tryHarder = true))
		analysis.setAnalyzer(executor) { frame ->
			try {
				if (!disposed.get()) {
					val value = reader.read(frame).firstOrNull { !it.text.isNullOrBlank() }?.text
					if (value != null) mainExecutor.execute { session.complete(Result.success(value)) }
				}
			} catch (_: Exception) {
				// A malformed frame must not end the camera session; the next frame can decode.
			} finally { frame.close() }
		}
		var provider: ProcessCameraProvider? = null
		val future = ProcessCameraProvider.getInstance(activity)
		future.addListener({
			if (!disposed.get()) {
				runCatching {
					val ready = future.get().also { provider = it }
					val selector = if (ready.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA)) CameraSelector.DEFAULT_BACK_CAMERA else CameraSelector.DEFAULT_FRONT_CAMERA
					camera = ready.bindToLifecycle(lifecycleOwner, selector, preview, analysis)
				}.onFailure { unavailable = true }
			}
		}, mainExecutor)
		onDispose {
			// Close delivery before unbinding: decoded frames may already be queued on main.
			session.close()
			disposed.set(true)
			analysis.clearAnalyzer()
			provider?.unbind(preview, analysis)
			executor.shutdown()
		}
	}
	Box(Modifier.fillMaxSize()) {
		AndroidView(factory = { previewView }, modifier = Modifier.fillMaxSize())
		if (unavailable) Surface(Modifier.align(Alignment.Center).padding(24.dp), shape = MaterialTheme.shapes.large) {
			Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
				Text(stringResource(Res.string.scanner_unavailable))
				Button(onClick = { unavailable = false; attempt++ }) { Text(stringResource(Res.string.button_retry)) }
			}
		} else {
			Box(Modifier.align(Alignment.Center).widthIn(max = 280.dp).fillMaxWidth(0.7f).aspectRatio(1f).border(2.dp, VniDropThemeTokens.cameraOverlay, RoundedCornerShape(24.dp)))
			Surface(Modifier.align(Alignment.BottomCenter).fillMaxWidth(), color = MaterialTheme.colorScheme.surface) {
				Column(Modifier.padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
					Text(stringResource(Res.string.receive_method_scan_description), style = MaterialTheme.typography.bodyMedium)
					if (camera?.cameraInfo?.hasFlashUnit() == true) FilledTonalButton(onClick = { torch = !torch; camera?.cameraControl?.enableTorch(torch) }) {
						Text(stringResource(if (torch) Res.string.scanner_flash_off else Res.string.scanner_flash_on))
					}
				}
			}
		}
	}
}
