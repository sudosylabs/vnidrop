import SwiftUI

/// Semantic text styles follow the system's Dynamic Type setting.
enum VniType {
	static let titleLarge = Font.title2.weight(.semibold)
	static let bodyLarge = Font.callout
	static let bodySmall = Font.caption
	static let labelSmall = Font.caption2.weight(.medium)
}
