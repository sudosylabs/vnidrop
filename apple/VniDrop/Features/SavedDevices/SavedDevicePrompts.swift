import SwiftUI
import SFSafeSymbols

/// Consent prompts for the saved-device domain: the pairing question and the
/// targeted-offer approval. Both are blocking decisions, so each is a native
/// alert rather than an inline banner that could be scrolled past.
///
/// These live outside Experimental Settings — saving a device and approving a
/// targeted transfer are top-level product decisions.

extension View {
	/// Hosts both saved-device consent prompts. `suppressed` is set while the
	/// transfer-approval modal is up, so the user is never answering two blocking
	/// decisions at once — the approval belongs to a transfer this device is
	/// sending, these belong to a device asking to reach it.
	func savedDevicePrompts(model: SavedDevicesModel, suppressed: Bool) -> some View {
		modifier(PairingPromptHost(model: model, suppressed: suppressed))
			.modifier(TargetedOfferPromptHost(model: model, suppressed: suppressed))
	}
}

private struct PairingPromptHost: ViewModifier {
	@ObservedObject var model: SavedDevicesModel
	let suppressed: Bool

	private var prompt: PairingPrompt? {
		suppressed ? nil : model.state.pairingPrompt.prompt
	}

	func body(content: Content) -> some View {
		content.alert(
			Text(String(localized: title)),
			isPresented: Binding(
				get: { prompt != nil },
				// A swipe/escape dismissal is not an answer: suppress locally
				// without consuming the core's single-use eligibility. The
				// `suppressed` guard matters — hiding the alert for the approval
				// modal also fires this setter, and must not count as a dismissal.
				set: { if !$0, !suppressed { model.dismissPairingPrompt() } }
			),
			presenting: prompt
		) { prompt in
			Button(String(localized: acceptLabel(prompt)), action: model.acceptPairingPrompt)
			Button(String(localized: L10n.Saved.devicesDeclineAction), role: .destructive, action: model.declinePairingPrompt)
			Button(String(localized: L10n.Button.cancel), role: .cancel, action: model.dismissPairingPrompt)
		} message: { prompt in
			messageText(prompt)
		}
	}

	private var title: String.LocalizationValue {
		switch prompt {
		case .incomingRequest: return L10n.Pairing.requestTitle
		case .eligibility, nil: return L10n.Pairing.allowTitle
		}
	}

	/// The incoming-request copy names the asking device; the eligibility copy is
	/// a fixed explanation of what remembering allows.
	@ViewBuilder
	private func messageText(_ prompt: PairingPrompt) -> some View {
		switch prompt {
		case .incomingRequest:
			Text(L10n.Pairing.requestBody(device: deviceName(prompt)))
		case .eligibility:
			Text(String(localized: L10n.Pairing.allowBody))
		}
	}

	private func acceptLabel(_ prompt: PairingPrompt) -> String.LocalizationValue {
		switch prompt {
		case .incomingRequest: return L10n.Pairing.accept
		case .eligibility: return L10n.Saved.devicesRememberAction
		}
	}

	/// Falls back to a neutral placeholder: an unsaved peer's name is an untrusted
	/// hint, and there may be none at all.
	private func deviceName(_ prompt: PairingPrompt) -> String {
		prompt.remoteDisplayName ?? String(localized: L10n.Saved.devicesUnnamed)
	}
}

private struct TargetedOfferPromptHost: ViewModifier {
	@ObservedObject var model: SavedDevicesModel
	let suppressed: Bool

	private var offer: PendingTargetedOfferModel? {
		suppressed ? nil : model.state.targetedOffers.current
	}

	func body(content: Content) -> some View {
		content.alert(
			// Not the invitation-approval copy: there the remote device asks to
			// *receive* from us, here it is offering to *send* to us.
			Text(String(localized: L10n.Targeted.offerTitle)),
			isPresented: Binding(
				get: { offer != nil },
				// Dismissal declines: an unanswered offer would otherwise hold a
				// slot in the core's bounded per-sender queue. Never while
				// `suppressed`, though — being hidden behind the approval modal
				// must not silently decline the sender.
				set: { if !$0, !suppressed, let offer { model.declineTargetedOffer(offer.transferId) } }
			),
			presenting: offer
		) { offer in
			Button(String(localized: L10n.Button.approve)) {
				model.acceptTargetedOffer(offer.transferId)
			}
			Button(String(localized: L10n.Button.refuse), role: .destructive) {
				model.declineTargetedOffer(offer.transferId)
			}
		} message: { offer in
			Text(L10n.Targeted.offerBody(
				device: senderName,
				transferName: offer.transferName
			))
		}
	}

	/// Only a *saved* sender has a name we can vouch for; anything else stays
	/// generic rather than rendering a peer-supplied string as verified.
	private var senderName: String {
		model.state.targetedOffers.currentSenderDisplayName
			?? String(localized: L10n.Approval.nearbyDevice)
	}
}
