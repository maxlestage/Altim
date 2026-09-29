import Foundation

/// Texts of the risk cards of Mes avoirs, as on the web (web/src/webapp/RiskCards.tsx and MyHoldings.tsx).
public enum RiskText {
    /// A dollar amount in the display currency, without sign: "250 €".
    public static func usd(_ v: Double) -> String { Money.money(abs(v), min: 0, max: 0, sep: " ") }
    public static func pc(_ v: Double) -> String { "\(JSFormat.fr(abs(v), max: 1)) %" }

    /// "Perte ≈ −250 €, soit 8,3 % du patrimoine." (with what the cash cushions when there is some).
    public static func stressLine(_ r: StressResult, cash: Double) -> String {
        var s = "\(r.loss >= 0 ? "Perte" : "Gain") ≈ \(r.loss > 0 ? "−" : "+")\(usd(r.loss)), soit \(pc(r.lossPercent)) du patrimoine"
        if cash > 0 && r.loss > 0 {
            s += " (\(pc(r.investedLossPercent)) de vos placements : les liquidités amortissent \(pc(r.investedLossPercent - r.lossPercent)))"
        }
        return s + "."
    }

    /// "Ligne la plus touchée : SOL (−35,8 %, −37 €)".
    public static func worst(_ w: StressResult.Worst) -> String {
        "Ligne la plus touchée : \(w.symbol) (\(w.movePercent > 0 ? "+" : "−")\(pc(w.movePercent)), −\(usd(w.loss)))"
    }

    /// Assumptions of the stress scenarios: how each beta was obtained, said asset by asset.
    public static func stressNote(_ p: RiskPortfolio, betas: [String: BetaEstimate]) -> String {
        var seen = Set<String>()
        let assets = p.lines.filter { seen.insert($0.key).inserted }
        let estimated = assets.filter { betas[$0.key]?.estimated == true }
        let reference = assets.filter { betas[$0.key]?.reference == true }
        let fallback = assets.filter { betas[$0.key]?.estimated != true && betas[$0.key]?.reference != true }
        var s = "Hypothèse : choc instantané, bêta constant. "
        let days = estimated.compactMap { betas[$0.key]?.days }
        if let lo = days.min(), let hi = days.max() {
            let list = estimated.map { "\($0.symbol) \(String(format: "%.2f", betas[$0.key]?.beta ?? 1))" }.joined(separator: ", ")
            s += "Bêta estimé sur \(lo)\(hi != lo ? " à \(hi)" : "") jours de rendements journaliers (\(list)). "
        }
        if !reference.isEmpty { s += "\(reference.map(\.symbol).joined(separator: ", ")) : référence elle-même, bêta 1. " }
        if !fallback.isEmpty {
            s += "Historique trop court (moins de \(RiskEngine.minBetaDays) jours communs) ou indisponible pour \(fallback.map(\.symbol).joined(separator: ", ")) : bêta 1 retenu. "
        }
        return s + "Une vraie crise peut aller plus loin : les corrélations montent quand tout baisse."
    }

    /// "Votre stop : 95,00 € (5,9 % sous le cours)."
    public static func userStop(_ stop: Double, price: Double?) -> String {
        let below = price.flatMap { $0 > 0 ? " (\(JSFormat.fr(($0 - stop) / $0 * 100, max: 1)) % sous le cours)" : nil } ?? ""
        return "Votre stop : \(Format.price(stop))\(below)."
    }
}
