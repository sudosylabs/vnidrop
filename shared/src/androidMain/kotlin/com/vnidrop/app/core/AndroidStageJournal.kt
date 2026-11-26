package com.vnidrop.app.core

import android.content.Context
import android.content.SharedPreferences
import android.net.Uri
import android.provider.DocumentsContract
import java.io.FileNotFoundException
import java.util.UUID
import org.json.JSONArray

internal class AndroidStageJournal(
	private val preferences: SharedPreferences,
	private val owner: String = processOwner,
) {
	companion object {
		private val processOwner = UUID.randomUUID().toString()
		fun open(context: Context) = AndroidStageJournal(context.getSharedPreferences("receive_stages", Context.MODE_PRIVATE))
	}

	fun remember(uri: Uri, name: String) {
		val record = JSONArray().put(owner).put(name).toString()
		check(preferences.edit().putString(uri.toString(), record).commit()) { "Could not journal receive stage" }
	}

	fun forget(uri: Uri) {
		preferences.edit().remove(uri.toString()).apply()
	}

	fun recover(removeStage: (Uri, String) -> Boolean) {
		for ((key, value) in preferences.all) {
			val record = JSONArray(value as String)
			if (record.getString(0) == owner) continue
			val uri = Uri.parse(key)
			if (removeStage(uri, record.getString(1))) forget(uri)
		}
	}
}

internal fun Context.recoverReceiveStages() {
	val column = DocumentsContract.Document.COLUMN_DISPLAY_NAME
	AndroidStageJournal.open(this).recover { uri, name ->
		try {
			// A provider may retain the URI after publishing under the final name.
			contentResolver.query(uri, arrayOf(column), null, null, null)?.use { cursor ->
				when {
					cursor.extras.getBoolean(DocumentsContract.EXTRA_LOADING) -> false
					cursor.extras.getString(DocumentsContract.EXTRA_ERROR) != null -> false
					!cursor.moveToFirst() -> true
					cursor.isNull(0) -> false
					cursor.getString(0) != name -> true
					else -> DocumentsContract.deleteDocument(contentResolver, uri)
				}
			} ?: false
		} catch (_: FileNotFoundException) {
			true
		} catch (_: Exception) {
			false
		}
	}
}
