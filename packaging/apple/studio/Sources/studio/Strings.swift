import Foundation

// Mirrors strings.json. Captions live only here (not app strings).
struct Caption: Decodable {
    let title: String
    let subtitle: String
    // Optional in-composition banner labels (stay-private): CHIFFREMENT / PROTECTION.
    let encryption: String?
    let protection: String?
}

struct LocaleStrings: Decodable {
    let folder: String
    private let screens: [String: Caption]

    subscript(_ screen: String) -> Caption? { screens[screen] }

    private enum CodingKeys: String, CodingKey { case folder = "_folder" }

    init(from decoder: Decoder) throws {
        // `_folder` is a reserved key; every other key is a screen id -> Caption.
        let dyn = try decoder.container(keyedBy: DynamicKey.self)
        var acc: [String: Caption] = [:]
        var folderName = ""
        for key in dyn.allKeys {
            if key.stringValue == "_folder" {
                folderName = try dyn.decode(String.self, forKey: key)
            } else if let cap = try? dyn.decode(Caption.self, forKey: key) {
                acc[key.stringValue] = cap
            }
        }
        self.folder = folderName
        self.screens = acc
    }

    private struct DynamicKey: CodingKey {
        var stringValue: String
        var intValue: Int? { nil }
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
    }
}

struct Strings: Decodable {
    let screens: [String]
    let locales: [String: LocaleStrings]

    static func load(_ url: URL) throws -> Strings {
        try JSONDecoder().decode(Strings.self, from: Data(contentsOf: url))
    }
}
