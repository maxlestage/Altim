import Foundation
import Security

/// Encrypted storage in the iOS Keychain: identifiers and session cookie. Readable only while the iPhone
/// is unlocked, never synced to iCloud nor included in backups ("ThisDeviceOnly").
enum KeychainStore {
    enum Key: String, CaseIterable {
        case credentials, session
    }

    private static let service = "com.maxlestage.altim.access"

    private static func base(_ key: Key) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key.rawValue,
        ]
    }

    static func set(_ data: Data?, for key: Key) {
        SecItemDelete(base(key) as CFDictionary)
        guard let data, !data.isEmpty else { return }
        var attributes = base(key)
        attributes[kSecValueData as String] = data
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        SecItemAdd(attributes as CFDictionary, nil)
    }

    static func get(_ key: Key) -> Data? {
        var query = base(key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess else { return nil }
        return item as? Data
    }

    static func setString(_ value: String?, for key: Key) { set(value.map { Data($0.utf8) }, for: key) }
    static func string(_ key: Key) -> String? { get(key).map { String(decoding: $0, as: UTF8.self) } }

    static func clear() { Key.allCases.forEach { set(nil, for: $0) } }
}
