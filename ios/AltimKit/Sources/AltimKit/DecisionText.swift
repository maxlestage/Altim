import Foundation

/// Numbers written like `toLocaleString("fr-FR")` in the browser: narrow no-break space between thousands, comma,
/// halves rounded away from zero (as Intl does), no trailing zeros beyond `min`.
enum JSFormat {
    static func fr(_ v: Double, min: Int = 0, max: Int) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.numberStyle = .decimal
        f.minimumFractionDigits = min
        f.maximumFractionDigits = max
        f.roundingMode = .halfUp
        f.groupingSeparator = "\u{202F}"
        f.decimalSeparator = ","
        f.minusSign = "-"
        let s = f.string(from: NSNumber(value: v)) ?? String(v)
        // "-0" after rounding a tiny negative number: written 0, as the browser does.
        return s == "-0" ? "0" : s
    }
}

/// Texts of the decision card's newer figures (fundamentals, track record), identical to the web
/// (web/src/webapp/decision.ts and DecisionCard.tsx): "—" when a figure is missing.
public enum DecisionText {
    static let nnbsp = "\u{202F}"

    /// "12,3 %", "−4 %", "+2,5 %" with `sign`.
    public static func pct(_ v: Double?, digits: Int = 1, sign: Bool = false) -> String {
        guard let v, v.isFinite else { return "—" }
        let s = v < 0 ? "−" : sign && v > 0 ? "+" : ""
        return "\(s)\(JSFormat.fr(abs(v), max: digits))\(nnbsp)%"
    }

    /// Plain number (ratios, counts).
    public static func num(_ v: Double?, digits: Int = 2) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v < 0 ? "−" : "")\(JSFormat.fr(abs(v), max: digits))"
    }

    /// Large counts: "19,93 millions".
    public static func count(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        if v >= 1e9 { return "\(JSFormat.fr(v / 1e9, max: 2)) milliards" }
        if v >= 1e6 { return "\(JSFormat.fr(v / 1e6, max: 2)) millions" }
        return JSFormat.fr(v, max: 0)
    }

    /// "421 Md$", "3,2 Md$", "38,5 M$", "12,4 k$".
    public static func usdCompact(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        let a = abs(v)
        let (div, unit): (Double, String) = a >= 1e9 ? (1e9, "Md$") : a >= 1e6 ? (1e6, "M$") : a >= 1e4 ? (1e3, "k$") : (1, "$")
        let x = v / div
        let digits = abs(x) >= 100 ? 0 : abs(x) >= 10 ? 1 : 2
        return "\(JSFormat.fr(x, max: digits))\(nnbsp)\(unit)"
    }

    /// "+2,8 Md$" / "−271 M$".
    public static func signedUsd(_ v: Double?) -> String {
        guard let v else { return "—" }
        return "\(v < 0 ? "−" : v > 0 ? "+" : "")\(usdCompact(abs(v)))"
    }

    /// Date of a filing or a price, New York time (UTC for a date at midnight UTC): "28 juin 2026".
    public static func nyDate(_ ms: Double) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.timeZone = TimeZone(identifier: ms.truncatingRemainder(dividingBy: 86_400_000) == 0 ? "UTC" : "America/New_York")
        f.dateFormat = "d MMMM yyyy"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }

    /// Two rows per ratio: today vs median and range, then where today stands in the window.
    public static func historyRows(_ name: String, _ r: Decision.RatioHistory?) -> [(String, String)] {
        guard let r else { return [] }
        return [
            ("\(name) sur la période", "\(num(r.current, digits: 1)) aujourd'hui · médiane \(num(r.median, digits: 1)) · de \(num(r.min, digits: 1)) à \(num(r.max, digits: 1))"),
            ("Centile du \(name)", "plus haut que \(num(r.percentile, digits: 0)) % des \(count(r.days)) jours (\(nyDate(r.from)) – \(nyDate(r.to)))"),
        ]
    }

    /// Total and 7 / 30-day changes of the stablecoins in circulation (a missing change is nil).
    public static func stableRows(_ s: Decision.StablecoinFlows?, label: String) -> [(String, String?)] {
        guard let s else { return [] }
        return [
            ("Stablecoins (\(label))", "\(usdCompact(s.total)) au \(nyDate(s.date))"),
            ("… sur 7 jours", s.change7d.map { "\(signedUsd($0)) (\(pct(s.change7dPct, digits: 2, sign: true)))" }),
            ("… sur 30 jours", s.change30d.map { "\(signedUsd($0)) (\(pct(s.change30dPct, digits: 2, sign: true)))" }),
        ]
    }

    /// "18,2 % (impôt 16,1 %, taux effectif)", nil without ROIC.
    public static func roic(_ f: Decision.StockFundamentals) -> String? {
        guard let r = f.roic else { return nil }
        let tax = f.roicTaxStatutory == true ? " : taux légal américain, taux effectif non calculable" : ", taux effectif"
        return "\(pct(r, digits: 1)) (impôt \(pct(f.roicTaxRate, digits: 1))\(tax))"
    }

    /// One comparable company in a sentence.
    public static func peer(_ p: Decision.Peer) -> String {
        "\(p.name) (\(p.symbol)) : PER \(num(p.per, digits: 1)), P/S \(num(p.ps, digits: 1)), marge opérationnelle \(pct(p.operatingMargin)), chiffre d'affaires \(pct(p.revenueGrowth, digits: 1, sign: true)) sur un an"
    }
}
