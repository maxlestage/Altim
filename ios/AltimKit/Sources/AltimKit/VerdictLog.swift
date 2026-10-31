import Foundation

/// « Bilan des verdicts »: every change of Altim's verdict seen on this iPhone, with the price of that moment, and
/// what the price did until the next change (or until now). The live track record of the advice, not a replay: only
/// what the app really said, on this iPhone, is counted. Stored on the iPhone only.
public struct VerdictLogEntry: Codable, Sendable, Equatable, Identifiable {
    public var symbol: String
    public var kind: Kind
    public var name: String
    /// "buy" (trend or pull-back: buy / keep), "mixed" (keep without adding) or "out" (stay out).
    public var side: String
    public var label: String
    public var price: Double
    /// ms.
    public var at: Double
    public var id: String { "\(kind.rawValue):\(symbol):\(at)" }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }
}

public enum VerdictLog {
    public static let maxEntries = 300
    /// Label of an exit decided by the trend bot (its real results are shown apart in the summary).
    public static let botExitLabel = "SORTIR (BOT TENDANCE)"

    static func side(_ v: TrendEngine.Verdict) -> (String, String)? {
        switch v.effectiveState {
        case .trend, .pullback: return ("buy", "ACHETER / CONSERVER")
        case .mixed: return ("mixed", "CONSERVER SANS RENFORCER")
        case .down:
            // A way out decided by the trend bot inside an up trend: kept apart to follow what the bot's exits are worth.
            return v.state == .trend || v.state == .pullback ? ("out", botExitLabel) : ("out", "RESTER À L'ÉCART")
        case .unknown: return nil
        }
    }

    /// The log with this verdict added when it differs from the asset's last entry (a pull-back inside the trend is
    /// the same advice). Newest last, `maxEntries` at most.
    public static func record(_ log: [VerdictLogEntry], _ v: TrendEngine.Verdict, asset: Asset, now: Double) -> [VerdictLogEntry] {
        guard let (side, label) = side(v), let price = v.price, price > 0, price.isFinite else { return log }
        if let last = log.last(where: { $0.symbol == asset.symbol && $0.kind == asset.kind }), last.side == side { return log }
        let e = VerdictLogEntry(symbol: asset.symbol, kind: asset.kind, name: asset.name, side: side, label: label, price: price, at: now)
        return Array((log + [e]).suffix(maxEntries))
    }

    public struct Outcome: Sendable, Identifiable {
        public var entry: VerdictLogEntry
        /// Price at the next verdict of the asset, or now; nil when unknown.
        public var endPrice: Double?
        public var ended: Bool
        /// In %.
        public var change: Double? { endPrice.map { ($0 / entry.price - 1) * 100 } }
        public var id: String { entry.id }
    }

    /// Each verdict and the price move until the next one (closed) or until now (`prices` by "kind:SYMBOL").
    public static func outcomes(_ log: [VerdictLogEntry], prices: [String: Double]) -> [Outcome] {
        log.enumerated().map { i, e in
            let next = log[(i + 1)...].first { $0.symbol == e.symbol && $0.kind == e.kind }
            return Outcome(entry: e, endPrice: next?.price ?? prices["\(e.kind.rawValue):\(e.symbol)"], ended: next != nil)
        }
    }

    public struct Summary: Sendable, Equatable {
        public var buyCount: Int
        /// Mean move while the advice was « acheter / conserver », in %.
        public var buyMean: Double?
        public var outCount: Int
        /// Mean move while the advice was « rester à l'écart » (negative: a fall avoided), in %.
        public var outMean: Double?
        /// Exits decided by the trend bot, and the mean move after them (negative: a fall avoided).
        public var botExitCount: Int = 0
        public var botExitMean: Double? = nil

        public var text: String {
            var parts: [String] = []
            if let b = buyMean { parts.append("Après « acheter / conserver » : \(Self.signed(b)) en moyenne (\(buyCount) cas).") }
            if let o = outMean {
                parts.append("Après « rester à l'écart » : \(Self.signed(o)) en moyenne (\(outCount) cas)\(o < 0 ? ", une baisse évitée" : o > 0 ? ", une hausse manquée" : "").")
            }
            if let e = botExitMean {
                parts.append("Dont sorties décidées par le bot Tendance : \(Self.signed(e)) en moyenne ensuite (\(botExitCount) cas)\(e < 0 ? ", une baisse évitée" : e > 0 ? ", une hausse manquée" : "").")
            }
            return parts.isEmpty ? "Pas encore de verdict suivi : le bilan se remplit à chaque changement de verdict de vos actifs." : parts.joined(separator: " ")
        }

        static func signed(_ v: Double) -> String { "\(v >= 0 ? "+" : "−")\(JSFormat.fr(abs(v), min: 1, max: 1)) %" }
    }

    public static func summary(_ outcomes: [Outcome]) -> Summary {
        func mean(_ side: String) -> (Int, Double?) {
            let v = outcomes.filter { $0.entry.side == side }.compactMap(\.change)
            return (v.count, v.isEmpty ? nil : v.reduce(0, +) / Double(v.count))
        }
        let b = mean("buy"), o = mean("out")
        let bot = outcomes.filter { $0.entry.label == botExitLabel }.compactMap(\.change)
        return Summary(buyCount: b.0, buyMean: b.1, outCount: o.0, outMean: o.1,
                       botExitCount: bot.count, botExitMean: bot.isEmpty ? nil : bot.reduce(0, +) / Double(bot.count))
    }
}
