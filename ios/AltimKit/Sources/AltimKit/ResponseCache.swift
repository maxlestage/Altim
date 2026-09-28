import Foundation

/// Last good answers of the server, kept on the phone: when the network or the server is down, the app shows them
/// with their time instead of an error ("hors ligne : données de 10:32").
public protocol ResponseCache: Sendable {
    func load(_ key: String) -> (data: Data, date: Date)?
    func save(_ key: String, _ data: Data)
}

/// One file per request in a directory (the app gives a protected one), at most `limit` files, the oldest dropped.
public final class FileResponseCache: ResponseCache, @unchecked Sendable {
    private let directory: URL
    private let limit: Int
    private let lock = NSLock()
    private var writes = 0

    public init(directory: URL, limit: Int = 300) {
        self.directory = directory
        self.limit = limit
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    /// FNV-1a 64 bits of the request (path and query): a stable file name without any personal data in it.
    static func fileName(_ key: String) -> String {
        var h: UInt64 = 0xcbf29ce484222325
        for b in key.utf8 {
            h ^= UInt64(b)
            h = h &* 0x100000001b3
        }
        return String(h, radix: 16) + ".json"
    }

    public func load(_ key: String) -> (data: Data, date: Date)? {
        let url = directory.appendingPathComponent(Self.fileName(key))
        guard let data = try? Data(contentsOf: url),
              let date = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date else { return nil }
        return (data, date)
    }

    public func save(_ key: String, _ data: Data) {
        let url = directory.appendingPathComponent(Self.fileName(key))
        #if os(iOS) || os(watchOS)
        try? data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try? data.write(to: url, options: .atomic)
        #endif
        lock.lock()
        writes += 1
        let prune = writes % 25 == 0
        lock.unlock()
        if prune { self.prune() }
    }

    /// Keeps the `limit` most recent files.
    func prune() {
        let fm = FileManager.default
        guard let files = try? fm.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.contentModificationDateKey]), files.count > limit else { return }
        let dated = files.map { ($0, (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast) }
        for (url, _) in dated.sorted(by: { $0.1 > $1.1 }).dropFirst(limit) { try? fm.removeItem(at: url) }
    }

    public func clear() {
        try? FileManager.default.removeItem(at: directory)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }
}
