import SFSafeSymbols
import SwiftUI

/// Picks a remembered device to send an existing transfer to.
///
/// Offered next to the QR code as another way to deliver the same invitation,
/// not as a second share of the same files.
struct DevicePickerSheet: View {
	@ObservedObject var model: ContactsModel
	let transferId: UInt64
	@Environment(\.dismiss) private var dismiss

	/// Only devices holding a live grant: the rest cannot be reached until they
	/// are paired again, so offering them here would fail on tap.
	private var reachable: [DeviceContact] {
		model.state.contacts.filter(\.canSend)
	}

	var body: some View {
		NavigationStack {
			Group {
				if reachable.isEmpty {
					ContentUnavailableView {
						Label(
							String(localized: L10n.Contacts.pickDeviceTitle),
							systemSymbol: .macbookAndIphone
						)
					} description: {
						Text(String(localized: L10n.Contacts.pickDeviceEmpty))
					}
				} else {
					List(reachable) { contact in
						Button {
							Task {
								await model.offerTransfer(transferId: transferId, to: contact)
								dismiss()
							}
						} label: {
							HStack {
								VStack(alignment: .leading, spacing: 2) {
									Text(contact.displayName)
									Text(contact.shortFingerprint)
										.font(.caption.monospaced())
										.foregroundStyle(.secondary)
								}
								Spacer()
								if model.state.busyEndpoints.contains(contact.endpointId) {
									ProgressView().controlSize(.small)
								}
							}
						}
						.disabled(!model.state.busyEndpoints.isEmpty)
					}
				}
			}
			.navigationTitle(Text(String(localized: L10n.Contacts.pickDeviceTitle)))
			#if os(iOS)
			.navigationBarTitleDisplayMode(.inline)
			#endif
			.toolbar {
				ToolbarItem(placement: .cancellationAction) {
					Button(String(localized: L10n.Button.cancel)) { dismiss() }
				}
			}
		}
		.task { await model.refresh() }
		#if os(macOS)
		.frame(minWidth: 380, minHeight: 320)
		#endif
	}
}
