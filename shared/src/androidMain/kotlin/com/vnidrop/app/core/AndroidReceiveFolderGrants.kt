package com.vnidrop.app.core

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import com.vnidrop.app.logging.AppLogger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.vnidrop.VnidropException

private const val ReadPermission = Intent.FLAG_GRANT_READ_URI_PERMISSION
private const val WritePermission = Intent.FLAG_GRANT_WRITE_URI_PERMISSION

internal class AndroidReceiveFolderGrants(
	private val persisted: () -> Map<Uri, Int>,
	private val take: (Uri, Int) -> Unit,
	private val release: (Uri, Int) -> Unit,
) {
	constructor(resolver: ContentResolver) : this(
		persisted = {
			resolver.persistedUriPermissions.filter { DocumentsContract.isTreeUri(it.uri) }.associate { permission ->
				val modes = (if (permission.isReadPermission) ReadPermission else 0) or
					(if (permission.isWritePermission) WritePermission else 0)
				permission.uri to modes
			}
		},
		take = resolver::takePersistableUriPermission,
		release = resolver::releasePersistableUriPermission,
	)

	private val lock = Mutex()
	private val active = mutableMapOf<Uri, Int>()
	private var configured: Uri? = null
	// A failed preferences read cannot authorize reclaiming persisted grants.
	private var hasConfiguration = false

	suspend fun reconcile(folder: Uri?) = withContext(Dispatchers.IO) {
		lock.withLock {
			configured = folder
			hasConfiguration = true
			releaseUnused()
		}
	}

	suspend fun update(folder: Uri?, commit: suspend () -> Unit) = withContext(NonCancellable + Dispatchers.IO) {
		lock.withLock {
			val requested = ReadPermission or WritePermission
			val added = if (folder == null) 0 else requested and (persisted()[folder] ?: 0).inv()
			if (folder != null && added != 0) {
				try {
					take(folder, requested)
				} catch (_: SecurityException) {
					throw VnidropException.FilesystemPermission("Could not persist access to the selected folder")
				}
			}
			try {
				commit()
			} catch (error: Throwable) {
				if (folder != null && added != 0) releaseGrant(folder, added)
				throw error
			}
			configured = folder
			hasConfiguration = true
			releaseUnused()
		}
	}

	suspend fun <T> withAccess(folder: Uri, operation: suspend () -> T): T {
		lock.withLock { active[folder] = (active[folder] ?: 0) + 1 }
		try {
			return operation()
		} finally {
			withContext(NonCancellable + Dispatchers.IO) {
				lock.withLock {
					val remaining = active.getValue(folder) - 1
					if (remaining == 0) active.remove(folder) else active[folder] = remaining
					if (hasConfiguration) releaseUnused()
				}
			}
		}
	}

	private fun releaseUnused() {
		for ((folder, modes) in persisted()) {
			if (folder != configured && folder !in active) releaseGrant(folder, modes)
		}
	}

	private fun releaseGrant(folder: Uri, modes: Int) {
		try {
			release(folder, modes)
		} catch (_: SecurityException) {
			AppLogger.warn("storage", "Could not release an unused document tree grant")
		}
	}
}
