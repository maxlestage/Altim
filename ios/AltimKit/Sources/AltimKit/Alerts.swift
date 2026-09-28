import Foundation

/// "Can I buy now?" for one asset (/api/alerts): the same rule for the iPhone, the Apple Watch and Android.
public struct BuyAlert: Codable, Sendable, Identifiable, Hashable {
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var price: Double?
    public var asOf: Double?
    public var buy: Bool
    public var strong: Bool
    public var reasons: [String]
    public var blockers: [String]
    public var cautions: [String]
    /// Stable identifier of the situation ("signal+zone:medium"): a new notification only when it changes.
    public var key: String
    public var title: String
    public var body: String
    public var id: String { "\(kind.rawValue):\(symbol)" }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }

    enum CodingKeys: String, CodingKey { case symbol, kind, name, price, asOf, buy, strong, reasons, blockers, cautions, key, title, body }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        symbol = try c.decode(String.self, forKey: .symbol)
        kind = try c.decode(Kind.self, forKey: .kind)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? symbol
        price = try c.decodeIfPresent(Double.self, forKey: .price)
        asOf = try c.decodeIfPresent(Double.self, forKey: .asOf)
        // An asset whose data is unavailable comes back with only an error: never buyable.
        buy = try c.decodeIfPresent(Bool.self, forKey: .buy) ?? false
        strong = try c.decodeIfPresent(Bool.self, forKey: .strong) ?? false
        reasons = try c.decodeIfPresent([String].self, forKey: .reasons) ?? []
        blockers = try c.decodeIfPresent([String].self, forKey: .blockers) ?? []
        cautions = try c.decodeIfPresent([String].self, forKey: .cautions) ?? []
        key = try c.decodeIfPresent(String.self, forKey: .key) ?? ""
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? symbol
        body = try c.decodeIfPresent(String.self, forKey: .body) ?? "Données indisponibles."
    }

    public init(symbol: String, kind: Kind, name: String, price: Double?, buy: Bool, strong: Bool, key: String, title: String, body: String) {
        self.symbol = symbol
        self.kind = kind
        self.name = name
        self.price = price
        self.buy = buy
        self.strong = strong
        self.key = key
        self.title = title
        self.body = body
        reasons = []
        blockers = []
        cautions = []
    }
}

/// Decides which alerts deserve a notification: when an asset becomes buyable, or when a new reason appears (a zone
/// is reached after a signal…). A price that hovers at the edge of a zone makes the verdict flicker: an asset is
/// forgotten only after `cooldown` without being buyable, and a reason already notified never notifies again meanwhile.
/// Persisted between background checks.
public struct AlertTracker: Codable, Sendable, Equatable {
    /// Asset → every reason already notified ("signal+zone:medium").
    public private(set) var notified: [String: String] = [:]
    /// Asset → since when it is no longer buyable.
    public private(set) var lost: [String: Date] = [:]
    public static let cooldown: TimeInterval = 6 * 3600

    public init() {}

    private static func parts(_ key: String) -> Set<String> { Set(key.split(separator: "+").map(String.init)) }

    public mutating func newAlerts(_ items: [BuyAlert], onlyStrong: Bool, now: Date = Date()) -> [BuyAlert] {
        var out: [BuyAlert] = []
        for it in items {
            let wanted = it.buy && (!onlyStrong || it.strong)
            if wanted {
                lost[it.id] = nil
                let before = notified[it.id].map(Self.parts)
                let reasons = Self.parts(it.key)
                if before == nil || !reasons.isSubset(of: before!) { out.append(it) }
                notified[it.id] = (before ?? []).union(reasons).sorted().joined(separator: "+")
            } else if !it.buy, notified[it.id] != nil {
                let since = lost[it.id] ?? now
                lost[it.id] = since
                if now.timeIntervalSince(since) >= Self.cooldown {
                    notified[it.id] = nil
                    lost[it.id] = nil
                }
            }
        }
        return out
    }
}

extension AltimClient {
    public func alerts(_ assets: [Asset]) async throws -> [BuyAlert] {
        guard !assets.isEmpty else { return [] }
        let chunks = stride(from: 0, to: assets.count, by: 20).map { Array(assets[$0..<min($0 + 20, assets.count)]) }
        var out: [BuyAlert] = []
        for chunk in chunks { out += try await getAlerts(chunk) }
        return out
    }
}
