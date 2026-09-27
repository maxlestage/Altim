import Foundation
import Security

/// Stockage chiffré des clés API dans le trousseau iOS (jamais dans UserDefaults ni iCloud).
enum KeychainStore {
    enum Key: String, CaseIterable {
        case alpacaKey, alpacaSecret, twelveDataKey, polygonKey, finnhubKey
    }

    private static let service = "com.altim.app.credentials"

    static func set(_ value: String, for key: Key) {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key.rawValue,
        ]
        SecItemDelete(query as CFDictionary)
        guard !trimmed.isEmpty else { return }
        var attributes = query
        attributes[kSecValueData as String] = Data(trimmed.utf8)
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        SecItemAdd(attributes as CFDictionary, nil)
    }

    static func get(_ key: Key) -> String {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key.rawValue,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data else { return "" }
        return String(decoding: data, as: UTF8.self)
    }

    static func has(_ key: Key) -> Bool { !get(key).isEmpty }
}
