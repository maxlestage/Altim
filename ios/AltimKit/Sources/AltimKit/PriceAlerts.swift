import Foundation

/// "Préviens-moi si BTC passe sous 80 000 $": a price threshold chosen by the user, checked with the buy alerts.
/// One shot: once reached it is marked triggered (the user can re-arm it).
public struct PriceTarget: Codable, Sendable, Identifiable, Hashable {
    public var id: UUID
    public var asset: Asset
    /// true: notify when the price rises to or above `price`; false: when it falls to or below.
    public var above: Bool
    public var price: Double
    public var created: Date
    public var triggered: Date?
    /// Move alert: notify when the price moves by at least this many % (up or down) from `price`, the reference.
    public var move: Double?

    public init(id: UUID = UUID(), asset: Asset, above: Bool, price: Double, created: Date = Date(), triggered: Date? = nil, move: Double? = nil) {
        self.id = id
        self.asset = asset
        self.above = above
        self.price = price
        self.created = created
        self.triggered = triggered
        self.move = move
    }

    public var label: String {
        if let move { return "Variation de ±\(Format.plain(move, digits: 1)) % (depuis \(Format.price(price)))" }
        return "\(above ? "Au-dessus de" : "En dessous de") \(Format.price(price))"
    }

    /// Reached at this price?
    public func isReached(by current: Double) -> Bool {
        if let move { return price > 0 && abs(current / price - 1) * 100 >= move }
        return above ? current >= price : current <= price
    }

    /// Re-armed: a move alert starts again from the current price.
    public func rearmed(at current: Double?) -> PriceTarget {
        var t = self
        t.triggered = nil
        if move != nil, let current, current > 0 { t.price = current }
        return t
    }

    /// Armed targets reached at the given prices (key "kind:SYMBOL"): returns the updated list and those just reached.
    public static func evaluate(_ targets: [PriceTarget], prices: [String: Double], now: Date = Date()) -> (targets: [PriceTarget], fired: [(PriceTarget, Double)]) {
        var fired: [(PriceTarget, Double)] = []
        let updated = targets.map { t -> PriceTarget in
            guard t.triggered == nil, let p = prices[t.asset.id], p.isFinite, t.isReached(by: p) else { return t }
            var done = t
            done.triggered = now
            fired.append((done, p))
            return done
        }
        return (updated, fired)
    }
}

/// One notification received, kept to measure what it gave afterwards (honest follow-up of the alerts).
public struct JournalEntry: Codable, Sendable, Identifiable, Hashable {
    public enum Source: String, Codable, Sendable { case buy, strongBuy, target }
    public var id: UUID
    public var asset: Asset
    public var source: Source
    public var title: String
    /// Price when the alert was sent.
    public var price: Double
    public var date: Date

    public init(id: UUID = UUID(), asset: Asset, source: Source, title: String, price: Double, date: Date = Date()) {
        self.id = id
        self.asset = asset
        self.source = source
        self.title = title
        self.price = price
        self.date = date
    }

    /// Change since the alert, in %.
    public func change(at current: Double?) -> Double? {
        guard let current, current.isFinite, price > 0 else { return nil }
        return (current / price - 1) * 100
    }
}

public enum AlertJournal {
    /// Most recent first, 200 entries at most.
    public static let limit = 200

    public static func add(_ entries: [JournalEntry], to journal: [JournalEntry]) -> [JournalEntry] {
        Array((entries + journal).sorted { $0.date > $1.date }.prefix(limit))
    }

    /// Summary of the buy alerts (price targets are the user's own thresholds, not Altim's advice): how many went up
    /// since, average change. Without fees nor exit rule: an indication, not a backtest.
    public struct Summary: Sendable, Equatable {
        public var count: Int
        public var up: Int
        public var average: Double
        public var upShare: Double { count > 0 ? Double(up) / Double(count) * 100 : 0 }
    }

    public static func summary(_ journal: [JournalEntry], prices: [String: Double], now: Date = Date(), minAge: TimeInterval = 3600) -> Summary? {
        // An alert of a few minutes says nothing yet.
        let changes = journal
            .filter { $0.source != .target && now.timeIntervalSince($0.date) >= minAge }
            .compactMap { $0.change(at: prices[$0.asset.id]) }
        guard !changes.isEmpty else { return nil }
        return Summary(count: changes.count, up: changes.filter { $0 > 0 }.count, average: changes.reduce(0, +) / Double(changes.count))
    }
}
