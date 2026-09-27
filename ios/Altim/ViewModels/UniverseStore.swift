import Foundation
import Observation
import AltimCore

/// Full catalogue (every crypto, every US-listed stock / ETF), cached on the iPhone for 12 h.
/// If the download fails, the last saved list is used.
@MainActor
@Observable
final class UniverseStore {
    private(set) var lists: [AssetClass: [UniverseEntry]] = [:]
    private(set) var loading: Set<AssetClass> = []
    private(set) var errors: [AssetClass: String] = [:]

    private static let maxAge: TimeInterval = 12 * 3600

    private static func cacheURL(_ kind: AssetClass) -> URL? {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first?
            .appendingPathComponent("altim-universe-\(kind.rawValue).json")
    }

    func ensure(_ kind: AssetClass, transport: HTTPTransport) async {
        if lists[kind] != nil || loading.contains(kind) { return }
        var stale: [UniverseEntry]?
        if let url = Self.cacheURL(kind), let data = try? Data(contentsOf: url),
           let saved = try? JSONDecoder().decode([UniverseEntry].self, from: data) {
            let age = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate).map { Date().timeIntervalSince($0) } ?? .infinity
            if age < Self.maxAge { lists[kind] = saved; return }
            stale = saved
            lists[kind] = saved // shown right away, refreshed below
        }
        loading.insert(kind)
        defer { loading.remove(kind) }
        do {
            let fresh = try await AssetUniverse.load(kind, transport: transport)
            lists[kind] = fresh
            errors[kind] = nil
            if let url = Self.cacheURL(kind), let data = try? JSONEncoder().encode(fresh) {
                try? data.write(to: url, options: .atomic)
            }
        } catch {
            if stale == nil { errors[kind] = "Catalogue indisponible : \(error.localizedDescription)" }
        }
    }
}
