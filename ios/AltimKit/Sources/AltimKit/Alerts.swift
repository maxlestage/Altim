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

/// Decides which alerts deserve a notification: only when an asset becomes buyable, or when the reason changes
/// (a zone is reached after a signal…). An asset that stops being buyable is forgotten, so its next buy
/// opportunity notifies again. Persisted between background checks.
public struct AlertTracker: Codable, Sendable, Equatable {
    public private(set) var notified: [String: String] = [:]

    public init() {}

    public mutating func newAlerts(_ items: [BuyAlert], onlyStrong: Bool) -> [BuyAlert] {
        var out: [BuyAlert] = []
        for it in items {
            let wanted = it.buy && (!onlyStrong || it.strong)
            if wanted {
                if notified[it.id] != it.key {
                    out.append(it)
                    notified[it.id] = it.key
                }
            } else if !it.buy {
                notified[it.id] = nil
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
