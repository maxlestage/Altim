import Foundation

/// The spoken answers (plain words: Siri reads them aloud). A wait forced by the data (last close too old, sources too
/// unreliable) is said as such, and an asset whose data could not be read is counted, never left out in silence.
public enum SiriText {
    public static func radar(_ rows: [RadarRow]) -> String {
        let ok = rows.filter { $0.signal != nil }
        guard !ok.isEmpty else { return "Les marchés ne répondent pas pour l'instant. Réessayez dans un moment." }
        let buys = ok.filter { $0.signal?.action == .buy || $0.signal?.action == .strongBuy }.map { $0.name ?? $0.symbol }
        let sells = ok.filter { $0.signal?.action == .sell || $0.signal?.action == .strongSell }.map { $0.name ?? $0.symbol }
        let held = ok.filter { $0.signal?.action == .hold && $0.signal?.suspended == true }.map { $0.name ?? $0.symbol }
        var parts: [String] = []
        parts.append(buys.isEmpty ? "Aucun signal d'achat sur votre radar." : "Signal d'achat sur \(list(buys)).")
        if !sells.isEmpty { parts.append("Signal de sortie sur \(list(sells)).") }
        let wait = ok.count - buys.count - sells.count - held.count
        if wait > 0 { parts.append("\(wait) actif\(wait > 1 ? "s" : "") à attendre.") }
        if !held.isEmpty { parts.append("Verdict suspendu faute de données à jour ou fiables pour \(list(held)).") }
        let missing = rows.count - ok.count
        if missing > 0 { parts.append("\(missing) actif\(missing > 1 ? "s" : "") sans données pour l'instant.") }
        if let best = ok.max(by: { ($0.change ?? -.infinity) < ($1.change ?? -.infinity) }), let c = best.change {
            parts.append("Plus forte hausse : \(best.name ?? best.symbol), \(Format.percent(c, digits: 1)) sur 24 heures.")
        }
        parts.append("Altim ne passe aucun ordre.")
        return parts.joined(separator: " ")
    }

    public static func asset(_ a: Asset, _ row: RadarRow?) -> String {
        guard let row, let price = row.price else { return "Le cours de \(a.name) est indisponible pour l'instant." }
        var text = "\(a.name) vaut \(Format.price(price))"
        if let c = row.change { text += ", \(Format.percent(c, digits: 1)) sur 24 heures" }
        text += "."
        if let s = row.signal {
            text += s.suspended == true ? " Verdict Altim suspendu : données en retard ou peu fiables."
                                        : " Verdict Altim : \(s.action.label.lowercased())."
        }
        return text
    }

    static func list(_ names: [String]) -> String {
        names.count <= 1 ? names.joined() : names.dropLast().joined(separator: ", ") + " et " + names.last!
    }
}
