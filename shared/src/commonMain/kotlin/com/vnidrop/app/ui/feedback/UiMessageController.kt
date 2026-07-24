package com.vnidrop.app.ui.feedback

import com.vnidrop.app.logging.AppLogger
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.receiveAsFlow
import org.jetbrains.compose.resources.StringResource

sealed interface UiText {
	data class Resource(val resource: StringResource, val formatArgs: List<Any> = emptyList()) : UiText
	data class Dynamic(val value: String) : UiText
}

enum class UiMessageTone {
	Info,
	Success,
	Warning,
	Error,
}

data class UiMessage(
	val text: UiText,
	val tone: UiMessageTone = UiMessageTone.Info,
	val actionLabel: UiText? = null,
	val onAction: (() -> Unit)? = null,
)

class UiMessageController {
	private val channel = Channel<UiMessage>(Channel.BUFFERED)
	val messages: Flow<UiMessage> = channel.receiveAsFlow()
	private val _dismissals = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
	val dismissals: SharedFlow<Unit> = _dismissals.asSharedFlow()

	suspend fun show(message: UiMessage) {
		channel.send(message)
	}

	fun tryShow(message: UiMessage): Boolean = channel.trySend(message).isSuccess

	fun dismissCurrent() {
		_dismissals.tryEmit(Unit)
	}

	/**
	 * Surfaces a user-facing error snackbar. Logs the full technical error.
	 * User cancellations are logged and suppressed.
	 */
	fun error(error: Throwable) {
		if (error.isUserCancellation()) {
			AppLogger.info(
				"ui",
				"suppressed user cancellation",
				mapOf("detail" to error.technicalDetail().ifBlank { error::class.simpleName.orEmpty() }),
			)
			return
		}
		AppLogger.error("ui", "user-facing error", error)
		tryShow(
			UiMessage(
				text = error.toUiText(),
				tone = UiMessageTone.Error,
			),
		)
	}

	fun error(text: UiText) {
		tryShow(UiMessage(text = text, tone = UiMessageTone.Error))
	}
}
