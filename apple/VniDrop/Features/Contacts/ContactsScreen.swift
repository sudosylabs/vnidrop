import SFSafeSymbols
import SwiftUI

/// Device history: the remembered devices, their detail, and the block list.
///
/// Pushed from Settings rather than owning a tab — it is a management surface,
/// not part of the send/receive flow.
struct ContactsScreen: View {
	@ObservedObject var model: ContactsModel
	/// Reports an empty result, which a silent refresh cannot convey.
	let onNothingWaiting: () -> Void

	var body: some View {
		Form {
			Section {
				Text(String(localized: L10n.Contacts.subtitle))
					.font(.footnote)
					.foregroundStyle(.secondary)
			}

			if model.state.contacts.isEmpty {
				Section {
					ContactsEmptyState()
				}
			} else {
				Section(String(localized: L10n.Contacts.title)) {
					ForEach(model.state.contacts) { contact in
						NavigationLink(value: SettingsSection.contactDetail(endpointId: contact.endpointId)) {
							ContactRow(contact: contact)
						}
					}
				}
			}

			if !model.state.heldOffers.isEmpty {
				Section(String(localized: L10n.Contacts.waitingTitle)) {
					ForEach(model.state.heldOffers) { offer in
						VStack(alignment: .leading, spacing: 2) {
							Text(offer.transferName)
							Text(String(offer.endpointId.prefix(16)))
								.font(.caption.monospaced())
								.foregroundStyle(.secondary)
								.lineLimit(1)
								.truncationMode(.middle)
						}
					}
					Text(String(localized: L10n.Contacts.waitingHint))
						.font(.footnote)
						.foregroundStyle(.secondary)
				}
			}

			if !model.state.blocked.isEmpty {
				Section(String(localized: L10n.Contacts.blockedTitle)) {
					ForEach(model.state.blocked, id: \.self) { endpointId in
						BlockedRow(endpointId: endpointId) {
							Task { await model.unblock(endpointId: endpointId) }
						}
					}
					Text(String(localized: L10n.Contacts.unblockHint))
						.font(.footnote)
						.foregroundStyle(.secondary)
				}
			}

			CollectOffersSection(model: model, onNothingWaiting: onNothingWaiting)

			GrantLifetimeSection(model: model)

			if !model.state.contacts.isEmpty {
				Section {
					ForgetAllButton { Task { await model.forgetAll() } }
				}
			}
		}
		.formStyle(.grouped)
		.navigationTitle(Text(String(localized: L10n.Contacts.title)))
		.task { await model.refresh() }
	}
}

private struct ContactsEmptyState: View {
	var body: some View {
		VStack(spacing: 8) {
			Image(systemSymbol: .macbookAndIphone)
				.font(.system(size: 32))
				.foregroundStyle(.tint)
			Text(String(localized: L10n.Contacts.emptyTitle))
				.font(.headline)
			Text(String(localized: L10n.Contacts.emptyBody))
				.font(.footnote)
				.foregroundStyle(.secondary)
				.multilineTextAlignment(.center)
		}
		.frame(maxWidth: .infinity)
		.padding(.vertical, 12)
	}
}

private struct ContactRow: View {
	let contact: DeviceContact

	var body: some View {
		VStack(alignment: .leading, spacing: 2) {
			Text(contact.displayName)
			if contact.canSend {
				if let lastTransferAt = contact.lastTransferAt {
					Text(L10n.Contacts.lastTransfer(date: Self.format(lastTransferAt)))
						.font(.caption)
						.foregroundStyle(.secondary)
				}
			} else {
				// Reachability is derived from holding a live grant, so this is
				// the honest signal that sending will not work.
				Label(
					String(localized: L10n.Contacts.unreachable),
					systemSymbol: .exclamationmarkTriangleFill
				)
				.font(.caption)
				.foregroundStyle(.orange)
			}
		}
	}

	private static func format(_ millis: Int64) -> String {
		let date = Date(timeIntervalSince1970: TimeInterval(millis) / 1_000)
		return date.formatted(.relative(presentation: .named))
	}
}

private struct BlockedRow: View {
	let endpointId: String
	let onUnblock: () -> Void

	var body: some View {
		HStack {
			Text(String(endpointId.prefix(16)))
				.font(.callout.monospaced())
				.lineLimit(1)
				.truncationMode(.middle)
			Spacer()
			Button(String(localized: L10n.Contacts.unblock), action: onUnblock)
				.buttonStyle(.borderless)
		}
	}
}

private struct CollectOffersSection: View {
	@ObservedObject var model: ContactsModel
	let onNothingWaiting: () -> Void

	var body: some View {
		Section {
			Toggle(
				String(localized: L10n.Contacts.checkOnOpen),
				isOn: Binding(
					get: { model.state.checkForOffersOnOpen },
					set: { model.setCheckForOffersOnOpen($0) }
				)
			)
			Button {
				Task {
					let collected = await model.collectWaitingOffers()
					if collected == 0 { onNothingWaiting() }
				}
			} label: {
				HStack {
					Text(String(localized: L10n.Contacts.checkNow))
					if model.state.isCheckingForOffers {
						Spacer()
						ProgressView().controlSize(.small)
					}
				}
			}
			.disabled(model.state.isCheckingForOffers)
		} footer: {
			// The privacy cost is the point of the setting, so it is stated
			// where the switch is, not buried elsewhere.
			Text(String(localized: L10n.Contacts.checkOnOpenHint))
		}
	}
}

private struct GrantLifetimeSection: View {
	@ObservedObject var model: ContactsModel

	var body: some View {
		Section {
			Picker(
				String(localized: L10n.Contacts.grantLifetimeTitle),
				selection: Binding(
					get: { model.state.grantLifetime },
					set: { model.setGrantLifetime($0) }
				)
			) {
				ForEach(GrantLifetimeOption.allCases) { option in
					Text(Self.label(option)).tag(option)
				}
			}
			Text(String(localized: L10n.Contacts.grantLifetimeHint))
				.font(.footnote)
				.foregroundStyle(.secondary)
		}
	}

	private static func label(_ option: GrantLifetimeOption) -> String {
		guard let days = option.days else {
			return String(localized: L10n.Contacts.grantLifetimeNever)
		}
		return L10n.Contacts.grantLifetimeDays(count: days)
	}
}

private struct ForgetAllButton: View {
	let onConfirm: () -> Void
	@State private var isConfirming = false

	var body: some View {
		Button(role: .destructive) {
			isConfirming = true
		} label: {
			Text(String(localized: L10n.Contacts.forgetAll))
		}
		.confirmationDialog(
			String(localized: L10n.Contacts.forgetAll),
			isPresented: $isConfirming,
			titleVisibility: .visible
		) {
			Button(String(localized: L10n.Contacts.forgetAll), role: .destructive, action: onConfirm)
		} message: {
			Text(String(localized: L10n.Contacts.forgetBody))
		}
	}
}

/// Detail for one remembered device: rename, send, forget, block.
struct ContactDetailScreen: View {
	@ObservedObject var model: ContactsModel
	let endpointId: String

	@State private var label = ""
	@State private var isConfirmingForget = false
	@State private var isConfirmingBlock = false

	private var contact: DeviceContact? {
		model.state.contacts.first { $0.endpointId == endpointId }
	}

	var body: some View {
		Form {
			if let contact {
				Section {
					TextField(
						String(localized: L10n.Contacts.nameField),
						text: $label,
						prompt: Text(contact.displayName)
					)
					.onSubmit { commitLabel() }
					Text(String(localized: L10n.Contacts.nameHint))
						.font(.footnote)
						.foregroundStyle(.secondary)
				}

				Section {
					// The endpoint id is the only real identity: two devices can
					// claim the same name, but not the same key. Shown in full
					// and selectable so it can actually be compared.
					Text(L10n.Approval.endpointId(deviceId: contact.endpointId))
						.font(.caption.monospaced())
						.foregroundStyle(.secondary)
						.textSelection(.enabled)
				}

				if contact.canSend {
					Section {
						Button {
							model.chooseFilesToSend(to: endpointId)
						} label: {
							Label(
								String(localized: L10n.Contacts.sendTo),
								systemSymbol: .paperplane
							)
						}
						.disabled(model.state.busyEndpoints.contains(endpointId))
					}
				} else {
					Section {
						Label(
							String(localized: L10n.Contacts.unreachableBody),
							systemSymbol: .exclamationmarkTriangleFill
						)
						.font(.footnote)
					}
				}

				Section {
					Button(role: .destructive) {
						isConfirmingForget = true
					} label: {
						Text(String(localized: L10n.Contacts.forget))
					}
					Button(role: .destructive) {
						isConfirmingBlock = true
					} label: {
						Text(String(localized: L10n.Contacts.block))
					}
				}
				.disabled(model.state.busyEndpoints.contains(endpointId))
			}
		}
		.formStyle(.grouped)
		.navigationTitle(Text(contact?.displayName ?? ""))
		.contactSendPickers(model: model)
		.onAppear { label = contact?.localLabel ?? "" }
		.onDisappear { commitLabel() }
		.confirmationDialog(
			String(localized: L10n.Contacts.forget),
			isPresented: $isConfirmingForget,
			titleVisibility: .visible
		) {
			Button(String(localized: L10n.Contacts.forget), role: .destructive) {
				Task { await model.forget(endpointId: endpointId) }
			}
		} message: {
			Text(String(localized: L10n.Contacts.forgetBody))
		}
		.confirmationDialog(
			String(localized: L10n.Contacts.block),
			isPresented: $isConfirmingBlock,
			titleVisibility: .visible
		) {
			Button(String(localized: L10n.Contacts.block), role: .destructive) {
				Task { await model.block(endpointId: endpointId) }
			}
		} message: {
			Text(String(localized: L10n.Contacts.unblockHint))
		}
	}

	private func commitLabel() {
		guard label != (contact?.localLabel ?? "") else { return }
		Task { await model.setLabel(endpointId: endpointId, label: label) }
	}
}
