import SwiftUI

// MARK: - StatusPill

enum PillTone { case neutral, success, warning, destructive, brand }

struct StatusPill: View {
	let label: String
	var tone: PillTone = .neutral

	private var color: Color {
		switch tone {
		case .neutral: return .secondary
		case .success, .brand: return VniDropColors.brandPurple
		case .warning: return .orange
		case .destructive: return .red
		}
	}

	var body: some View {
		HStack(spacing: 5) {
			Circle().fill(color).frame(width: 6, height: 6)
			Text(label).font(.caption).fontWeight(.medium).foregroundStyle(color).lineLimit(1)
		}
		.padding(.horizontal, 9).padding(.vertical, 4)
		.background(color.opacity(0.14), in: Capsule())
	}
}

// MARK: - ProgressRow

struct ProgressRow: View {
	let labelKey: String.LocalizationValue
	let progress: Double?
	var detail: String? = nil
	/// Pre-resolved label; when set it overrides `labelKey`.
	var labelText: String? = nil

	var body: some View {
		VStack(alignment: .leading, spacing: 4) {
			HStack {
				label.font(.subheadline).lineLimit(1)
				Spacer()
				if let progress {
					Text(verbatim: "\(Int(progress * 100))%").font(.caption).foregroundStyle(.secondary)
				}
			}
			if let detail {
				Text(detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
			}
			if let progress {
				ProgressView(value: progress)
			} else {
				ProgressView().progressViewStyle(.linear)
			}
		}
		.frame(maxWidth: .infinity)
	}

	@ViewBuilder
	private var label: some View {
		if let labelText {
			Text(labelText)
		} else {
			Text(String(localized: labelKey))
		}
	}
}

// MARK: - Field

/// Labeled text field using native styling. Renders cleanly both inside a `Form`
/// row and standalone (e.g. inside a sheet).
struct Field: View {
	let label: String
	@Binding var value: String
	var minLines: Int = 1
	var enabled: Bool = true

	var body: some View {
		VStack(alignment: .leading, spacing: 6) {
			Text(label).font(.subheadline).foregroundStyle(.secondary)
			Group {
				if minLines > 1 {
					TextField(label, text: $value, axis: .vertical)
						.lineLimit(minLines, reservesSpace: true)
				} else {
					TextField(label, text: $value)
				}
			}
			.textFieldStyle(.roundedBorder)
			.disabled(!enabled)
		}
	}
}

