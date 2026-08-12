package com.vnidrop.app.core

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import io.github.vinceglb.filekit.FileKit
import io.github.vinceglb.filekit.dialogs.FileKitDialogSettings
import io.github.vinceglb.filekit.dialogs.FileKitMode
import io.github.vinceglb.filekit.dialogs.openDirectoryPicker
import io.github.vinceglb.filekit.dialogs.openFilePicker
import java.awt.EventQueue
import java.awt.FileDialog
import java.awt.Frame
import java.awt.KeyboardFocusManager
import java.awt.image.BufferedImage
import java.io.ByteArrayOutputStream
import java.io.File
import javax.imageio.ImageIO
import javax.swing.JFileChooser
import javax.swing.filechooser.FileSystemView
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
actual fun rememberShareFilePicker(
	onFilesPicked: (List<PickedShareFile>) -> Unit,
	onError: (String) -> Unit,
): ShareFilePicker {
	val scope = rememberCoroutineScope()
	return remember(onFilesPicked, onError, scope) {
		object : ShareFilePicker {
			override fun pickFiles() {
				when (jvmFilePickerBackend(System.getProperty("os.name"))) {
					JvmFilePickerBackend.XdgPortal -> scope.launch {
						try {
							val selected = withContext(Dispatchers.IO) { pickShareFilesWithPortal() }
							onFilesPicked(selected)
						} catch (error: CancellationException) {
							throw error
						} catch (error: Throwable) {
							onError(error.message ?: error.toString())
						}
					}
					JvmFilePickerBackend.WindowsNative -> {
						val owner = activeFrame()
						scope.launch {
							try {
								val selected = withContext(Dispatchers.IO) { pickWindowsFiles(owner) }
								onFilesPicked(selected)
							} catch (error: CancellationException) {
								throw error
							} catch (error: Throwable) {
								onError(error.message ?: error.toString())
							}
						}
					}
					JvmFilePickerBackend.AwtSwing -> openPicker(onError) {
						val selected = pickShareFiles()
						onFilesPicked(selected)
					}
				}
			}

			override fun pickFolder() {
				when (jvmFilePickerBackend(System.getProperty("os.name"))) {
					JvmFilePickerBackend.XdgPortal -> scope.launch {
						try {
							val selected = withContext(Dispatchers.IO) {
								pickDirectoryWithPortal("Select folder to share")?.toPickedShareFile(isDirectory = true)
							}
							onFilesPicked(selected?.let(::listOf).orEmpty())
						} catch (error: CancellationException) {
							throw error
						} catch (error: Throwable) {
							onError(error.message ?: error.toString())
						}
					}
					JvmFilePickerBackend.WindowsNative -> {
						val owner = activeFrame()
						scope.launch {
							try {
								val selected = withContext(Dispatchers.IO) {
									pickWindowsFolder("Select folder to share", owner)
								}
								onFilesPicked(selected?.let { listOf(it.toPickedShareFile(isDirectory = true)) }.orEmpty())
							} catch (error: CancellationException) {
								throw error
							} catch (error: Throwable) {
								onError(error.message ?: error.toString())
							}
						}
					}
					JvmFilePickerBackend.AwtSwing -> openPicker(onError) {
						val selected = pickDirectory(title = "Select folder to share")
						onFilesPicked(selected?.let { listOf(it.toPickedShareFile(isDirectory = true)) }.orEmpty())
					}
				}
			}
		}
	}
}

@Composable
actual fun rememberReceiveFolderPicker(
	onFolderPicked: (ReceiveFolder) -> Unit,
	onError: (String) -> Unit,
): ReceiveFolderPicker {
	val scope = rememberCoroutineScope()
	return remember(onFolderPicked, onError, scope) {
		object : ReceiveFolderPicker {
			override fun pickFolder() {
				when (jvmFilePickerBackend(System.getProperty("os.name"))) {
					JvmFilePickerBackend.XdgPortal -> scope.launch {
						try {
							val selected = withContext(Dispatchers.IO) {
								pickDirectoryWithPortal("Select receive folder")
							} ?: return@launch
							onFolderPicked(selected.toReceiveFolder())
						} catch (error: CancellationException) {
							throw error
						} catch (error: Throwable) {
							onError(error.message ?: error.toString())
						}
					}
					JvmFilePickerBackend.WindowsNative -> {
						val owner = activeFrame()
						scope.launch {
							try {
								val selected = withContext(Dispatchers.IO) {
									pickWindowsFolder("Select receive folder", owner)
								} ?: return@launch
								onFolderPicked(selected.toReceiveFolder())
							} catch (error: CancellationException) {
								throw error
							} catch (error: Throwable) {
								onError(error.message ?: error.toString())
							}
						}
					}
					JvmFilePickerBackend.AwtSwing -> openPicker(onError) {
						val selected = pickDirectory(title = "Select receive folder") ?: return@openPicker
						onFolderPicked(selected.toReceiveFolder())
					}
				}
			}
		}
	}
}

internal enum class JvmFilePickerBackend {
	XdgPortal,
	WindowsNative,
	AwtSwing,
}

internal fun jvmFilePickerBackend(osName: String?): JvmFilePickerBackend =
	when {
		osName.orEmpty().startsWith("Linux", ignoreCase = true) -> JvmFilePickerBackend.XdgPortal
		osName.orEmpty().startsWith("Windows", ignoreCase = true) -> JvmFilePickerBackend.WindowsNative
		else -> JvmFilePickerBackend.AwtSwing
	}

private fun openPicker(
	onError: (String) -> Unit,
	block: () -> Unit,
) {
	EventQueue.invokeLater {
		try {
			block()
		} catch (error: Throwable) {
			onError(error.message ?: error.toString())
		}
	}
}

private fun nativeFileDialog(title: String): FileDialog =
	FileDialog(activeFrame(), title, FileDialog.LOAD)

private fun activeFrame(): Frame? {
	val activeWindow = KeyboardFocusManager.getCurrentKeyboardFocusManager().activeWindow
	return activeWindow as? Frame
		?: Frame.getFrames().firstOrNull { it.isActive || it.isFocused }
		?: Frame.getFrames().firstOrNull { it.isVisible }
}

private fun pickShareFiles(): List<PickedShareFile> {
	val dialog = nativeFileDialog("Select files to share").apply {
		isMultipleMode = true
	}
	return try {
		dialog.isVisible = true
		val directory = dialog.directory ?: return emptyList()
		val names = dialog.files?.map { it.name }.orEmpty().ifEmpty {
			dialog.file?.let { listOf(it) }.orEmpty()
		}
		names.map { name -> File(directory, name).toPickedShareFile(isDirectory = false) }
	} finally {
		dialog.dispose()
	}
}

private suspend fun pickShareFilesWithPortal(): List<PickedShareFile> =
	FileKit.openFilePicker(
		mode = FileKitMode.Multiple(),
		dialogSettings = FileKitDialogSettings(title = "Select files to share", parentWindow = activeFrame()),
	).orEmpty().map { it.file.toPickedShareFile(isDirectory = false) }

private suspend fun pickDirectoryWithPortal(title: String): File? =
	FileKit.openDirectoryPicker(
		dialogSettings = FileKitDialogSettings(title = title, parentWindow = activeFrame()),
	)?.file

internal fun File.toPickedShareFile(isDirectory: Boolean): PickedShareFile =
	PickedShareFile(
		value = absolutePath,
		displayName = name.ifBlank { absolutePath },
		sizeBytes = if (isDirectory) null else length().takeIf { it >= 0L }?.toULong(),
		thumbnailBytes = systemIconPng(),
		isDirectory = isDirectory,
	)

private fun File.toReceiveFolder(): ReceiveFolder =
	ReceiveFolder(
		kind = ReceiveFolderKind.FileSystemPath,
		value = absolutePath,
		displayName = name.ifBlank { absolutePath },
	)

private fun File.systemIconPng(): ByteArray? = runCatching {
	val icon = FileSystemView.getFileSystemView().getSystemIcon(this, 128, 128)
	val image = BufferedImage(icon.iconWidth, icon.iconHeight, BufferedImage.TYPE_INT_ARGB)
	val graphics = image.createGraphics()
	try {
		icon.paintIcon(null, graphics, 0, 0)
	} finally {
		graphics.dispose()
	}
	ByteArrayOutputStream().use { output ->
		ImageIO.write(image, "png", output)
		output.toByteArray()
	}
}.getOrNull()

private fun pickDirectory(title: String): File? {
	val chooser = JFileChooser().apply {
		dialogTitle = title
		fileSelectionMode = JFileChooser.DIRECTORIES_ONLY
		isAcceptAllFileFilterUsed = false
	}
	return if (chooser.showOpenDialog(null) == JFileChooser.APPROVE_OPTION) chooser.selectedFile else null
}
