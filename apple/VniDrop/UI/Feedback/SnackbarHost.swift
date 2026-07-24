import SwiftUI
import SFSafeSymbols

/// Bottom toast host driven by `UiMessageController`, ported from
/// `ui/feedback/VniDropSnackbarHost.kt`. Tone drives the accent color; errors get
/// a longer display duration.
struct SnackbarHost: View {
	@ObservedObject var controller: UiMessageController
	@State private var dismissTask: Task<Void, Never>?

	var body: some View {
		VStack {
			Spacer()
			if let message = controller.current {
				content(for: message)
					.frame(maxWidth: 520)
					.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
					.overlay(RoundedRectangle(cornerRadius: 14).stroke(.gray.opacity(0.25), lineWidth: 0.5))
					.shadow(color: .black.opacity(0.15), radius: 8, y: 2)
					.padding(.horizontal, 16)
					.padding(.bottom, 8)
					.transition(.move(edge: .bottom).combined(with: .opacity))
					.id(message.id)
					.onAppear { scheduleDismiss(for: message) }
			}
		}
		.animation(.easeInOut(duration: 0.2), value: controller.current?.id)
	}

	@ViewBuilder
	private func content(for message: UiMessage) -> some View {
		let accent: Color = {
			switch message.tone {
			case .info: return VniDropColors.brandPurple
			case .success: return .green
			case .warning: return .orange
			case .error: return .red
			}
		}()
		HStack(alignment: .center, spacing: 8) {
			Circle().fill(accent).frame(width: 8, height: 8)
			Text(message.text.resolved())
				.font(.subheadline)
				.frame(maxWidth: .infinity, alignment: .leading)
				.padding(.vertical, 10)
			if let actionLabel = message.actionLabel {
				Button(action: {
					message.onAction?()
					dismiss()
				}) {
					Text(actionLabel.resolved()).fontWeight(.semibold)
				}
				.buttonStyle(.borderless)
			}
			Button(action: dismiss) {
				Image(systemSymbol: .xmark)
					.font(.footnote.weight(.semibold))
					.foregroundStyle(.secondary)
					.frame(width: 36, height: 36)
			}
			.buttonStyle(.plain)
		}
		.padding(.leading, 16)
		.padding(.trailing, 4)
	}

	private func scheduleDismiss(for message: UiMessage) {
		dismissTask?.cancel()
		let seconds: UInt64 = message.tone == .error ? 6 : 4
		dismissTask = Task {
			try? await Task.sleep(nanoseconds: seconds * 1_000_000_000)
			if !Task.isCancelled { controller.advance() }
		}
	}

	private func dismiss() {
		dismissTask?.cancel()
		controller.advance()
	}
}
