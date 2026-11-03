import Foundation

/// « Point du jour » computed on the device (the server's daily brief is gone): the state of each market for the market
/// filter, what the watchlist's verdicts say, the next Fed decision. Facts the app already computed, nothing predicted.
public enum DailyPoint {
    /// `markets`: the market filter's states read on their own (`MarketBoard.read`), used when the macro report is
    /// missing (its VIX / S&P sources down) so that the market lines do not vanish with it.
    public static func lines(macro: MacroInfo?, rows: [RadarRow], earnings: [EarningsEvent] = [], markets: [Kind: Bool?]? = nil,
                             now: Double = Date().timeIntervalSince1970 * 1000) -> [String] {
        var out: [String] = []
        if let up = macro?.marketUp {
            for kind in MarketBoard.kinds { out.append(MarketBoard.text(kind, up[kind.rawValue])) }
        } else if let markets, markets.values.contains(where: { $0 != nil }) {
            for kind in MarketBoard.kinds { out.append(MarketBoard.text(kind, markets[kind] ?? nil)) }
        }
        let signals = rows.compactMap(\.signal)
        let actions = signals.map(\.action)
        if !actions.isEmpty {
            let buy = actions.filter { $0 == .buy || $0 == .strongBuy }.count
            let strong = actions.filter { $0 == .strongBuy }.count
            let held = signals.filter { $0.action == .hold && $0.suspended == true }.count
            let wait = actions.filter { $0 == .hold }.count - held
            let out_ = actions.filter { $0 == .sell || $0 == .strongSell }.count
            var parts: [String] = []
            if buy > 0 { parts.append("\(buy) à l'achat" + (strong > 0 ? " (dont \(strong) sur repli)" : "")) }
            if wait > 0 { parts.append("\(wait) en attente") }
            if held > 0 { parts.append("\(held) suspendu\(held > 1 ? "s" : "") faute de données à jour ou fiables") }
            if out_ > 0 { parts.append("\(out_) à l'écart ou à la sortie") }
            out.append((actions.count == 1 ? "Votre actif suivi : " : "Vos \(actions.count) actifs suivis : ") + parts.joined(separator: ", ") + ".")
        }
        let missing = rows.filter { $0.signal == nil }.count
        if missing > 0 {
            out.append(missing == 1 ? "1 actif suivi sans verdict : ses données n'ont pas pu être lues."
                                    : "\(missing) actifs suivis sans verdict : leurs données n'ont pas pu être lues.")
        }
        let date = Date(timeIntervalSince1970: now / 1000)
        let soon = earnings.filter { (0...7).contains($0.daysAway(now: date)) }.sorted { $0.time < $1.time }
        if !soon.isEmpty {
            out.append("Résultats trimestriels dans les 7 jours : " + soon.prefix(5).map { "\($0.symbol) (\($0.dayText))" }.joined(separator: ", ")
                       + (soon.count > 5 ? " et \(soon.count - 5) autres" : "") + ".")
        }
        if let fed = macro?.nextFedText(now: now) { out.append("Prochaine décision de taux de la Fed : \(fed).") }
        return out
    }
}
