package com.vnidrop.app.feature.receive

import java.io.ByteArrayOutputStream
import java.io.InputStream

/**
 * Reads one byte past the invitation limit so oversize files are rejected by [decodeInvitationBytes].
 * `InputStream.readNBytes` would do this but needs API 33; minSdk is 24.
 */
fun InputStream.readInvitationBytes(): ByteArray {
	val limit = MaxVniDropInvitationBytes + 1
	val output = ByteArrayOutputStream()
	val buffer = ByteArray(8 * 1024)
	while (output.size() < limit) {
		val read = read(buffer, 0, minOf(buffer.size, limit - output.size()))
		if (read < 0) break
		output.write(buffer, 0, read)
	}
	return output.toByteArray()
}
