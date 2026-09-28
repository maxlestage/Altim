import Foundation

/// Private data of the app (watch list, holdings, alert state) in a file of the app's container, excluded from the
/// iCloud / computer backups, encrypted by iOS while the iPhone is locked after a restart (readable after the first
/// unlock, which the background alert check needs). Replaces UserDefaults, which is backed up.
enum LocalStore {
    private static var directory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Altim", isDirectory: true)
        if !FileManager.default.fileExists(atPath: base.path) {
            try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
            var url = base
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try? url.setResourceValues(values)
        }
        return base
    }

    static func load<T: Decodable>(_ type: T.Type, _ name: String) -> T? {
        guard let data = try? Data(contentsOf: directory.appendingPathComponent("\(name).json")) else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    static func save<T: Encodable>(_ value: T, _ name: String) {
        guard let data = try? JSONEncoder().encode(value) else { return }
        try? data.write(to: directory.appendingPathComponent("\(name).json"), options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    /// Raw content of a saved file (to tell an unreadable file from a missing one).
    static func data(_ name: String) -> Data? {
        try? Data(contentsOf: directory.appendingPathComponent("\(name).json"))
    }

    static func write(_ data: Data, _ name: String) {
        try? data.write(to: directory.appendingPathComponent("\(name).json"), options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    static func remove(_ name: String) {
        try? FileManager.default.removeItem(at: directory.appendingPathComponent("\(name).json"))
    }
}
