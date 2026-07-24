package com.vnidrop.app.core

import java.nio.file.Files
import kotlin.io.path.createDirectories
import kotlin.io.path.createTempDirectory
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class FileSystemServiceTest {
	@Test
	fun desktopTemporaryUsageCountsOnlyVnidropPartFiles() {
		val root = createTempDirectory("vnidrop-temporary-usage")
		try {
			val nested = root.resolve("nested").createDirectories()
			Files.write(nested.resolve(".photo.jpg.vnidrop-test.part"), ByteArray(7))
			Files.write(nested.resolve("photo.jpg"), ByteArray(13))
			Files.write(nested.resolve(".unrelated.part"), ByteArray(17))

			assertEquals(
				7UL,
				desktopTemporaryUsage(
					ReceiveFolder(ReceiveFolderKind.FileSystemPath, root.toString(), "Test"),
				),
			)
		} finally {
			root.toFile().deleteRecursively()
		}
	}

	@Test
	fun desktopReclaimTemporaryStorageKeepsUserFiles() {
		val root = createTempDirectory("vnidrop-storage-cleanup")
		try {
			val receive = root.resolve("receive").createDirectories()
			val appData = root.resolve("app-data").createDirectories()
			val partial = receive.resolve(".photo.jpg.vnidrop-test.part")
			val received = receive.resolve("photo.jpg")
			val trash = appData.resolve("nested/.Trash").createDirectories()
			Files.write(partial, ByteArray(7))
			Files.write(received, ByteArray(13))
			Files.write(trash.resolve("stale.bin"), ByteArray(11))

			assertEquals(
				18UL,
				desktopReclaimTemporaryStorage(
					appData.toString(),
					ReceiveFolder(ReceiveFolderKind.FileSystemPath, receive.toString(), "Test"),
				),
			)
			assertFalse(Files.exists(partial))
			assertFalse(Files.exists(trash))
			assertTrue(Files.exists(received))
		} finally {
			root.toFile().deleteRecursively()
		}
	}
}
