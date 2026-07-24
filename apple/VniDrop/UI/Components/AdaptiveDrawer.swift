import SwiftUI

/// Presents modal content in a native sheet. On phones it uses medium/large
/// detents with a grabber; on wider layouts the sheet is form-sized. Content is
/// wrapped in a `NavigationStack` so it gets a native title bar + Close button.
struct AdaptiveDrawer<DrawerContent: View>: ViewModifier {
	@Binding var isPresented: Bool
	let windowClass: WindowClass
	let onDismiss: () -> Void
	@ViewBuilder let drawerContent: () -> DrawerContent

	func body(content: Content) -> some View {
		content.sheet(
			isPresented: Binding(get: { isPresented }, set: { if !$0 { onDismiss() } })
		) {
			SheetChrome(onClose: onDismiss) { drawerContent() }
				.modifier(PhoneDetents(enabled: windowClass == .phone))
		}
	}
}

private struct PhoneDetents: ViewModifier {
	let enabled: Bool
	func body(content: Content) -> some View {
		if enabled {
			content
				.presentationDetents([.medium, .large])
				.presentationDragIndicator(.visible)
		} else {
			content.frame(minWidth: 460, minHeight: 480)
		}
	}
}

private struct SheetChrome<Content: View>: View {
	let onClose: () -> Void
	@ViewBuilder let content: () -> Content

	var body: some View {
		NavigationStack {
			ScrollView { content().padding(.top, 4) }
				.toolbar {
					ToolbarItem(placement: .cancellationAction) {
						Button(String(localized: L10n.Button.close), action: onClose)
					}
				}
				#if os(iOS)
				.navigationBarTitleDisplayMode(.inline)
				#endif
		}
	}
}

extension View {
	func adaptiveDrawer<DrawerContent: View>(
		isPresented: Binding<Bool>,
		windowClass: WindowClass,
		onDismiss: @escaping () -> Void,
		@ViewBuilder content: @escaping () -> DrawerContent
	) -> some View {
		modifier(AdaptiveDrawer(
			isPresented: isPresented, windowClass: windowClass,
			onDismiss: onDismiss, drawerContent: content
		))
	}
}
