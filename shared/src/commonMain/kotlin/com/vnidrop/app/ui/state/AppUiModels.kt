package com.vnidrop.app.ui.state

import androidx.compose.runtime.Composable
import com.vnidrop.app.core.CoreEventModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferStatus
import kotlin.math.roundToInt
import org.jetbrains.compose.resources.PluralStringResource
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.progress_cancelled
import vnidrop.shared.generated.resources.progress_completed
import vnidrop.shared.generated.resources.progress_connected
import vnidrop.shared.generated.resources.progress_connecting
import vnidrop.shared.generated.resources.progress_downloading
import vnidrop.shared.generated.resources.progress_failed
import vnidrop.shared.generated.resources.progress_getting_ready
import vnidrop.shared.generated.resources.progress_interrupted
import vnidrop.shared.generated.resources.progress_preparing
import vnidrop.shared.generated.resources.progress_ready
import vnidrop.shared.generated.resources.progress_requesting_access
import vnidrop.shared.generated.resources.progress_saving
import vnidrop.shared.generated.resources.progress_sending
import vnidrop.shared.generated.resources.progress_share_ready
import vnidrop.shared.generated.resources.progress_working
import vnidrop.shared.generated.resources.status_available
import vnidrop.shared.generated.resources.status_cancelled
import vnidrop.shared.generated.resources.status_completed
import vnidrop.shared.generated.resources.status_failed
import vnidrop.shared.generated.resources.status_preparing
import vnidrop.shared.generated.resources.status_receiving
import vnidrop.shared.generated.resources.status_stopped
import vnidrop.shared.generated.resources.transfer_file_count

enum class WindowClass {
	Phone,
	Tablet,
	Desktop,
}

fun windowClassFor(widthDp: Float): WindowClass =
	when {
		widthDp >= 920f -> WindowClass.Desktop
		widthDp >= 600f -> WindowClass.Tablet
		else -> WindowClass.Phone
	}

fun useBottomNavigation(windowClass: WindowClass): Boolean =
	windowClass == WindowClass.Phone

data class TransferProgress(
	val transferId: ULong?,
	val phase: String,
	val kind: String,
	val label: StringResource,
	val progress: Float?,
	val detail: String? = null,
)

fun statusLabelResource(status: TransferStatus): StringResource =
	when (status) {
		TransferStatus.Importing -> Res.string.status_preparing
		TransferStatus.Sharing -> Res.string.status_available
		TransferStatus.Receiving -> Res.string.status_receiving
		TransferStatus.Done -> Res.string.status_completed
		TransferStatus.Cancelled -> Res.string.status_cancelled
		TransferStatus.Stopped -> Res.string.status_stopped
		TransferStatus.Failed -> Res.string.status_failed
	}

@Composable
fun displayNameForStatus(status: TransferStatus): String =
	stringResource(statusLabelResource(status))

fun Transfer.isActiveTransfer(): Boolean =
	status in activeTransferStatuses

fun Transfer.canCancelTransfer(): Boolean =
	status in setOf(TransferStatus.Importing, TransferStatus.Sharing, TransferStatus.Receiving)

/**
 * Latest progress snapshot for a transfer, derived from core events.
 *
 * Events are assumed newest-first (as stored by [com.vnidrop.app.core.CoreRepository]).
 */
fun progressForTransfer(events: List<CoreEventModel>, transferId: ULong): TransferProgress? {
	val relevant = events.filter { event ->
		event.transferId == transferId && event.phase in progressPhases && event.kind in progressKinds
	}
	val latest = relevant.firstOrNull() ?: return null
	if (latest.endsVisibleProgress()) return null
	val sizeHint = findKnownSize(events, transferId)
	return TransferProgress(
		transferId = transferId,
		phase = latest.phase,
		kind = latest.kind,
		label = humanProgressLabel(latest),
		progress = parseProgress(latest.dataJson, sizeHint),
		detail = progressDetail(latest),
	)
}

/**
 * Live byte progress for one receiver on an outgoing share.
 *
 * Core emits `transfer` phase events per provider connection with
 * `endpoint_id` (and `connection_id`). Multiple blob requests for the same
 * receiver are aggregated so multi-file collections show a single bar.
 *
 * Falls back to mapping `connection_id` → endpoint via provider
 * `client-connected` events when older events lack `endpoint_id`.
 */
fun progressForReceiver(
	events: List<CoreEventModel>,
	transferId: ULong,
	remoteEndpointId: String,
	totalSizeHint: ULong? = null,
): TransferProgress? {
	if (remoteEndpointId.isBlank()) return null
	val connectionIds = connectionIdsForEndpoint(events, remoteEndpointId)
	val transferEvents = events.filter { event ->
		event.transferId == transferId &&
			event.direction == "send" &&
			event.phase == "transfer" &&
			event.kind in setOf("started", "progress", "completed", "aborted") &&
			eventBelongsToReceiver(event, remoteEndpointId, connectionIds)
	}
	if (transferEvents.isEmpty()) return null

	val latest = transferEvents.first()
	if (latest.kind == "aborted") {
		return TransferProgress(
			transferId = transferId,
			phase = "transfer",
			kind = "aborted",
			label = Res.string.progress_interrupted,
			progress = null,
			detail = null,
		)
	}
	val progress = aggregateReceiverProgress(transferEvents, totalSizeHint)
	if (latest.kind == "completed" && (progress == null || progress >= 0.999f)) {
		return TransferProgress(
			transferId = transferId,
			phase = "transfer",
			kind = "completed",
			label = Res.string.progress_completed,
			progress = 1f,
			detail = null,
		)
	}

	return TransferProgress(
		transferId = transferId,
		phase = "transfer",
		kind = latest.kind,
		label = Res.string.progress_sending,
		progress = progress,
		detail = progressDetail(latest),
	)
}

/**
 * Best send-side progress for a transfer (any receiver), used on catalog cards
 * while status is Sharing.
 */
fun activeSendProgress(
	events: List<CoreEventModel>,
	transferId: ULong,
	activeReceiverEndpointIds: Set<String>,
	totalSizeHint: ULong? = null,
): TransferProgress? {
	if (activeReceiverEndpointIds.isEmpty()) return null
	val endpointIds = events
		.asSequence()
		.filter { it.transferId == transferId && it.direction == "send" && it.phase == "transfer" }
		.mapNotNull { findString(it.dataJson, "endpoint_id") }
		.filter { it in activeReceiverEndpointIds }
		.distinct()
		.toList()
	if (endpointIds.isEmpty()) return null
	return endpointIds
		.mapNotNull { progressForReceiver(events, transferId, it, totalSizeHint) }
		.firstOrNull { it.kind == "progress" || it.kind == "started" }
}

fun summarizeProgress(events: List<CoreEventModel>): List<TransferProgress> =
	events
		.mapNotNull { it.transferId }
		.distinct()
		.take(6)
		.mapNotNull { progressForTransfer(events, it) }

/** File-count plural resource for transfer subtitles (resolve with [pluralStringResource]). */
fun transferFileCountResource(): PluralStringResource = Res.plurals.transfer_file_count

fun formatBytes(size: ULong): String {
	val value = size.toDouble()
	val units = listOf("B", "KB", "MB", "GB", "TB")
	var scaled = value
	var unitIndex = 0
	while (scaled >= 1024.0 && unitIndex < units.lastIndex) {
		scaled /= 1024.0
		unitIndex += 1
	}
	return if (unitIndex == 0) {
		"${size} ${units[unitIndex]}"
	} else {
		"${(scaled * 10).roundToInt() / 10.0} ${units[unitIndex]}"
	}
}

private val progressPhases = setOf(
	"import", "ticket", "access", "transfer", "download", "export",
	"lifecycle", "network", "handshake", "error",
)

private val progressKinds = setOf(
	"started", "copy-progress", "copy-done", "outboard-progress", "done",
	"created", "progress", "completed", "aborted", "failed",
	"connecting", "connected", "found-collection",
	"cancelled", "share-stopped",
)

private fun CoreEventModel.endsVisibleProgress(): Boolean =
	kind in terminalProgressKinds ||
		(phase == "ticket" && kind == "created")

private val terminalProgressKinds = setOf(
	"done", "completed", "aborted", "failed", "cancelled", "share-stopped",
)

private val activeTransferStatuses = setOf(
	TransferStatus.Importing,
	TransferStatus.Sharing,
	TransferStatus.Receiving,
)

private fun humanProgressLabel(event: CoreEventModel): StringResource = when {
	event.phase == "import" && event.kind == "copy-progress" -> Res.string.progress_preparing
	event.phase == "import" && event.kind == "outboard-progress" -> Res.string.progress_preparing
	event.phase == "import" && event.kind == "started" -> Res.string.progress_preparing
	event.phase == "import" && event.kind == "done" -> Res.string.progress_ready
	event.phase == "ticket" && event.kind == "created" -> Res.string.progress_share_ready
	event.phase == "network" && event.kind == "connecting" -> Res.string.progress_connecting
	event.phase == "network" && event.kind == "connected" -> Res.string.progress_connected
	event.phase == "handshake" -> Res.string.progress_requesting_access
	event.phase == "download" && event.kind == "found-collection" -> Res.string.progress_getting_ready
	event.phase == "download" && event.kind == "progress" -> Res.string.progress_downloading
	event.phase == "export" && event.kind == "progress" -> Res.string.progress_saving
	event.phase == "transfer" && event.kind == "progress" -> Res.string.progress_sending
	event.phase == "transfer" && event.kind == "started" -> Res.string.progress_connected
	event.phase == "transfer" && event.kind == "completed" -> Res.string.progress_completed
	event.phase == "lifecycle" && event.kind == "done" -> Res.string.progress_completed
	event.phase == "lifecycle" && event.kind == "cancelled" -> Res.string.progress_cancelled
	event.kind == "failed" -> Res.string.progress_failed
	// Never dump raw phase/kind tokens into the UI.
	else -> Res.string.progress_working
}

private fun progressDetail(event: CoreEventModel): String? {
	val fileName = findString(event.dataJson, "file_name")
	val current = findNumber(event.dataJson, "current_file_index")?.toLong()
	val totalFiles = findNumber(event.dataJson, "total_files")?.toLong()
	return when {
		fileName != null && current != null && totalFiles != null && totalFiles > 0 ->
			"$fileName (${current + 1}/$totalFiles)"
		fileName != null -> fileName
		else -> null
	}
}

internal fun parseProgress(json: String, sizeHint: Double? = null): Float? {
	val transferred = findNumber(json, "exported")
		?: findNumber(json, "downloaded")
		?: findNumber(json, "offset")
		?: findNumber(json, "end_offset")
		?: findNumber(json, "transferred")
		?: findNumber(json, "written")
	val total = findNumber(json, "file_size")
		?: findNumber(json, "total_size")
		?: findNumber(json, "size")
		?: findNumber(json, "total")
		?: sizeHint
	if (transferred == null || total == null || total <= 0.0) return null
	return (transferred / total).toFloat().coerceIn(0f, 1f)
}

private fun findKnownSize(events: List<CoreEventModel>, transferId: ULong): Double? {
	for (event in events) {
		if (event.transferId != transferId) continue
		findNumber(event.dataJson, "size")?.takeIf { it > 0 }?.let { return it }
		findNumber(event.dataJson, "total_size")?.takeIf { it > 0 }?.let { return it }
		findNumber(event.dataJson, "file_size")?.takeIf { it > 0 }?.let { return it }
	}
	return null
}

/**
 * Aggregate multi-blob provider progress for one receiver connection.
 *
 * For each `request_id`, take the latest known size/offset/completion, then
 * sum transferred / sum sizes. Prefer the transfer's total size when the
 * collection size is known and larger than the sum of observed blob sizes.
 */
private fun aggregateReceiverProgress(
	events: List<CoreEventModel>,
	totalSizeHint: ULong?,
): Float? {
	// Events are newest-first; walk oldest→newest so later states win.
	val chronological = events.asReversed()
	data class BlobState(var size: Double? = null, var offset: Double = 0.0, var completed: Boolean = false, var aborted: Boolean = false)
	val byRequest = linkedMapOf<String, BlobState>()
	var connectionScopedOffset: Double? = null
	var connectionScopedSize: Double? = null

	for (event in chronological) {
		val requestKey = findNumber(event.dataJson, "request_id")?.toLong()?.toString()
			?: findString(event.dataJson, "request_id")
		val size = findNumber(event.dataJson, "size")
		val endOffset = findNumber(event.dataJson, "end_offset")
			?: findNumber(event.dataJson, "offset")
			?: findNumber(event.dataJson, "transferred")

		if (requestKey != null) {
			val state = byRequest.getOrPut(requestKey) { BlobState() }
			if (size != null && size > 0) state.size = size
			when (event.kind) {
				"progress", "started" -> {
					if (endOffset != null) state.offset = maxOf(state.offset, endOffset)
					state.aborted = false
				}
				"completed" -> {
					state.completed = true
					state.size?.let { state.offset = it }
				}
				"aborted" -> state.aborted = true
			}
		} else {
			if (size != null && size > 0) connectionScopedSize = size
			if (endOffset != null) connectionScopedOffset = endOffset
		}
	}

	if (byRequest.isNotEmpty()) {
		val active = byRequest.values.filterNot { it.aborted }
		if (active.isEmpty()) return null
		val transferred = active.sumOf { state ->
			when {
				state.completed -> state.size ?: state.offset
				else -> state.offset
			}
		}
		val observedSize = active.mapNotNull { it.size }.sum()
		val total = totalSizeHint?.toDouble()?.takeIf { it > 0 }
			?: observedSize.takeIf { it > 0 }
		if (total == null || total <= 0.0) return null
		return (transferred / total).toFloat().coerceIn(0f, 1f)
	}

	val total = totalSizeHint?.toDouble()?.takeIf { it > 0 }
		?: connectionScopedSize?.takeIf { it > 0 }
	val transferred = connectionScopedOffset
	if (transferred == null || total == null || total <= 0.0) return null
	return (transferred / total).toFloat().coerceIn(0f, 1f)
}

private fun connectionIdsForEndpoint(events: List<CoreEventModel>, remoteEndpointId: String): Set<String> {
	val ids = mutableSetOf<String>()
	for (event in events) {
		val endpoint = findString(event.dataJson, "endpoint_id") ?: continue
		if (endpoint != remoteEndpointId) continue
		findNumber(event.dataJson, "connection_id")?.toLong()?.toString()?.let(ids::add)
		findString(event.dataJson, "connection_id")?.let(ids::add)
	}
	return ids
}

private fun eventBelongsToReceiver(
	event: CoreEventModel,
	remoteEndpointId: String,
	connectionIds: Set<String>,
): Boolean {
	val endpoint = findString(event.dataJson, "endpoint_id")
	if (endpoint != null) return endpoint == remoteEndpointId
	val connectionId = findNumber(event.dataJson, "connection_id")?.toLong()?.toString()
		?: findString(event.dataJson, "connection_id")
		?: return false
	return connectionId in connectionIds
}

private fun findNumber(json: String, key: String): Double? {
	val marker = "\"$key\":"
	val start = json.indexOf(marker)
	if (start < 0) return null
	val valueStart = start + marker.length
	val raw = json.substring(valueStart).trimStart()
	// Skip JSON null so endpoint_id:null does not parse as a number key neighbor.
	if (raw.startsWith("null")) return null
	val valueEnd = json.indexOfAny(charArrayOf(',', '}', ']'), valueStart).takeIf { it >= 0 } ?: json.length
	return json.substring(valueStart, valueEnd).trim().trim('"').toDoubleOrNull()
}

private fun findString(json: String, key: String): String? {
	val marker = "\"$key\":"
	val start = json.indexOf(marker)
	if (start < 0) return null
	val after = json.substring(start + marker.length).trimStart()
	if (after.startsWith("null")) return null
	if (!after.startsWith('"')) return null
	val end = after.indexOf('"', 1)
	if (end <= 1) return null
	return after.substring(1, end)
}
