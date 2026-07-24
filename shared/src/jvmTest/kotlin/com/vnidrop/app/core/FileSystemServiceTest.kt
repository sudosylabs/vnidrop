package com.vnidrop.app.core

import java.nio.file.Files
import kotlin.io.path.createDirectories
import kotlin.io.path.createTempDirectory
import kotlin.test.Test
import kotlin.test.assertEquals

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
}
