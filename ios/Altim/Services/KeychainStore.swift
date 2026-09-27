import Foundation
import Security

/// Encrypted storage in the iOS Keychain, never synced to iCloud nor included in backups ("ThisDeviceOnly").
/// - The password is readable only while the iPhone is unlocked, and only if it has a passcode: removing the
///   passcode erases it.
/// - The session cookie stays readable while the iPhone is locked (after the first unlock since start-up):
///   the background check of the buy alerts needs it.
enum KeychainStore {
    enum Key: String, CaseIterable {
        case credentials, session

        var accessibility: CFString {
            self == .credentials ? kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly : kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        }
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
        attributes[kSecAttrAccessible as String] = key.accessibility
        let status = SecItemAdd(attributes as CFDictionary, nil)
        if status != errSecSuccess, key == .credentials {
            // No passcode on this iPhone: the "passcode set" class is refused; kept while unlocked only.
            attributes[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            SecItemAdd(attributes as CFDictionary, nil)
        }
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
