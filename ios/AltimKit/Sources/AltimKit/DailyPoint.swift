import Foundation

/// « Point du jour » computed on the device (the server's daily brief is gone): the state of each market for the market
/// filter, what the watchlist's verdicts say, the next Fed decision. Facts the app already computed, nothing predicted.
public enum DailyPoint {
    public static func lines(macro: MacroInfo?, rows: [RadarRow], earnings: [EarningsEvent] = [],
                             now: Double = Date().timeIntervalSince1970 * 1000) -> [String] {
        var out: [String] = []
        if let up = macro?.marketUp {
            for (key, name) in [("crypto", "Marché crypto (Bitcoin)"), ("stock", "Marché actions (S&P 500)")] {
                guard let u = up[key] else { continue }
                out.append(u ? "\(name) en tendance haussière : le filtre de marché laisse acheter."
                             : "\(name) hors tendance haussière : le filtre de marché suspend les achats et conseille de sortir.")
            }
        }
        let actions = rows.compactMap { $0.signal?.action }
        if !actions.isEmpty {
            let buy = actions.filter { $0 == .buy || $0 == .strongBuy }.count
            let strong = actions.filter { $0 == .strongBuy }.count
            let wait = actions.filter { $0 == .hold }.count
            let out_ = actions.filter { $0 == .sell || $0 == .strongSell }.count
            var parts: [String] = []
            if buy > 0 { parts.append("\(buy) à l'achat" + (strong > 0 ? " (dont \(strong) sur repli)" : "")) }
            if wait > 0 { parts.append("\(wait) en attente") }
            if out_ > 0 { parts.append("\(out_) à l'écart ou à la sortie") }
            out.append("Vos \(actions.count) actifs suivis : " + parts.joined(separator: ", ") + ".")
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
