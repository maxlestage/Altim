import Foundation

/// A line of the portfolio, stored on the phone only (never sent to the server).
public struct Holding: Codable, Sendable, Identifiable, Hashable {
    public var id: UUID
    public var asset: Asset
    public var quantity: Double
    /// Average purchase price in dollars (optional: without it, no gain / loss).
    public var averagePrice: Double?

    public init(id: UUID = UUID(), asset: Asset, quantity: Double, averagePrice: Double?) {
        self.id = id
        self.asset = asset
        self.quantity = quantity
        self.averagePrice = averagePrice
    }
}

public struct PortfolioLine: Sendable, Identifiable {
    public var holding: Holding
    public var price: Double?
    public var value: Double?
    public var cost: Double?
    public var gain: Double? { value.flatMap { v in cost.map { v - $0 } } }
    public var gainPercent: Double? { gain.flatMap { g in cost.flatMap { $0 > 0 ? g / $0 * 100 : nil } } }
    /// Share of the portfolio value (0–100).
    public var weight: Double?
    public var id: UUID { holding.id }
}

public struct Portfolio: Sendable {
    public var lines: [PortfolioLine]
    public var total: Double
    public var cost: Double?
    public var gain: Double? { cost.map { total - $0 } }
    public var gainPercent: Double? { cost.flatMap { $0 > 0 ? (total - $0) / $0 * 100 : nil } }
    /// Share in cryptos (0–100).
    public var cryptoShare: Double
    /// Largest line (0–100): above 40 %, the portfolio depends on one asset.
    public var largest: Double
    public var warnings: [String]

    /// Values the holdings at the given prices (key "kind:SYMBOL"). Lines without a price are left out of the total.
    public init(holdings: [Holding], prices: [String: Double]) {
        var lines = holdings.map { h -> PortfolioLine in
            let p = prices[h.asset.id]
            return PortfolioLine(holding: h, price: p, value: p.map { $0 * h.quantity }, cost: h.averagePrice.map { $0 * h.quantity }, weight: nil)
        }
        total = lines.compactMap(\.value).reduce(0, +)
        let t = total
        for i in lines.indices { lines[i].weight = lines[i].value.map { t > 0 ? $0 / t * 100 : 0 } }
        lines.sort { ($0.value ?? -1) > ($1.value ?? -1) }
        self.lines = lines
        let priced = lines.filter { $0.value != nil }
        cost = !priced.isEmpty && priced.allSatisfy({ $0.cost != nil }) ? priced.compactMap(\.cost).reduce(0, +) : nil
        cryptoShare = t > 0 ? priced.filter { $0.holding.asset.kind == .crypto }.compactMap(\.value).reduce(0, +) / t * 100 : 0
        largest = lines.compactMap(\.weight).max() ?? 0
        var w: [String] = []
        if priced.count >= 2, largest > 40, let top = lines.first {
            w.append("\(top.holding.asset.symbol) pèse \(Format.plain(largest, digits: 0)) % du portefeuille : une seule ligne décide de presque tout.")
        }
        if t > 0, cryptoShare > 50 {
            w.append("Plus de la moitié en cryptos (\(Format.plain(cryptoShare, digits: 0)) %) : des baisses de 50 % ou plus arrivent régulièrement.")
        }
        let missing = lines.filter { $0.price == nil }.map(\.holding.asset.symbol)
        if !missing.isEmpty { w.append("Prix indisponible pour \(missing.joined(separator: ", ")) : ligne hors du total.") }
        warnings = w
    }
}
