import SFSafeSymbols
import SwiftUI

/// Consent prompts for device history, presented as sheets like the receiver
/// approval modal.
///
/// Both are dismissable by answering only. An incoming offer in particular must
/// not be acceptable by accident, and a swipe-away would leave the sender
/// waiting on a decision that never comes.
struct ContactPromptHost: View {
	/// Driven by the host so a prompt is never presented while another sheet is
	/// still animating out — macOS silently drops the second one.
	@Binding var isPresented: Bool
	let state: ContactsState
	let onPairingResponse: (String, Bool) -> Void
	let onOfferResponse: (String, Bool) -> Void

	var body: some View {
		Color.clear
			.sheet(isPresented: $isPresented) {
				// An incoming transfer is the more urgent of the two, and a
				// pairing offer keeps until the consent window lapses.
				if let offer = state.currentOffer {
					OfferSheet(
						offer: offer,
						busy: state.busyOfferIds.contains(offer.offerId),
						onRespond: onOfferResponse
					)
					.interactiveDismissDisabled(true)
					.modifier(ContactPromptDetents())
				} else if let pairing = state.currentPairing {
					PairingSheet(
						pairing: pairing,
						busy: state.busyEndpoints.contains(pairing.endpointId),
						onRespond: onPairingResponse
					)
					.interactiveDismissDisabled(true)
					.modifier(ContactPromptDetents())
				}
			}
	}
}

private struct ContactPromptDetents: ViewModifier {
	func body(content: Content) -> some View {
		#if os(iOS)
		content.presentationDetents([.medium])
		#else
		content.frame(minWidth: 420, minHeight: 300)
		#endif
	}
}

/// "A remembered device wants to send you files."
private struct OfferSheet: View {
	let offer: IncomingOfferModel
	let busy: Bool
	let onRespond: (String, Bool) -> Void

	var body: some View {
		VStack(spacing: 16) {
			Image(systemSymbol: .trayAndArrowDownFill)
				.font(.system(size: 44))
				.foregroundStyle(.tint)
				.padding(.top, 12)
			Text(String(localized: L10n.Offer.title))
				.font(.title2).fontWeight(.semibold)
			Text(L10n.Offer.body(device: offer.resolvedSenderName, transferName: offer.transferName))
				.multilineTextAlignment(.center)
			Text(L10n.Transfer.fileCount(count: Int(offer.fileCount)))
				.font(.caption)
				.foregroundStyle(.secondary)
			Spacer(minLength: 0)
			HStack(spacing: 12) {
				Button(role: .cancel) {
					onRespond(offer.offerId, false)
				} label: {
					Text(String(localized: L10n.Offer.decline)).frame(maxWidth: .infinity)
				}
				Button {
					onRespond(offer.offerId, true)
				} label: {
					Text(String(localized: L10n.Offer.accept)).frame(maxWidth: .infinity)
				}
				.buttonStyle(.borderedProminent)
			}
			.disabled(busy)
		}
		.padding(20)
	}
}

/// "This device offered to let you reach it. Remember it?"
private struct PairingSheet: View {
	let pairing: PendingPairingModel
	let busy: Bool
	let onRespond: (String, Bool) -> Void

	var body: some View {
		VStack(spacing: 16) {
			Image(systemSymbol: .laptopcomputerAndIphone)
				.font(.system(size: 44))
				.foregroundStyle(.tint)
				.padding(.top, 12)
			Text(String(localized: L10n.Pairing.requestTitle))
				.font(.title2).fontWeight(.semibold)
			Text(L10n.Pairing.requestBody(device: pairing.resolvedName))
				.multilineTextAlignment(.center)
			// Names are peer-supplied; the endpoint id is what actually identifies
			// the device.
			Text(L10n.Approval.endpointId(deviceId: pairing.endpointId))
				.font(.caption)
				.foregroundStyle(.secondary)
				.multilineTextAlignment(.center)
			Spacer(minLength: 0)
			HStack(spacing: 12) {
				Button(role: .cancel) {
					onRespond(pairing.endpointId, false)
				} label: {
					Text(String(localized: L10n.Pairing.decline)).frame(maxWidth: .infinity)
				}
				Button {
					onRespond(pairing.endpointId, true)
				} label: {
					Text(String(localized: L10n.Pairing.accept)).frame(maxWidth: .infinity)
				}
				.buttonStyle(.borderedProminent)
			}
			.disabled(busy)
		}
		.padding(20)
	}
}
