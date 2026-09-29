import Foundation

/// Numbers written like `toLocaleString("fr-FR")` in the browser: narrow no-break space between thousands, comma,
/// halves away from zero, no trailing zeros beyond `min`. Intl rounds the shortest decimal writing of the double
/// (9.01 − 95.56 = −86.55 → "86,6", although the exact binary value is 86.5499…), so the same is done here.
/// Done by hand: NumberFormatter rounds differently between Linux and Apple systems.
enum JSFormat {
    static func fr(_ v: Double, min: Int = 0, max: Int) -> String {
        guard v.isFinite else { return String(v) }
        let parts = shortestDecimal(abs(v)).split(separator: ".", omittingEmptySubsequences: false)
        var intDigits = Array(parts[0]).map { Int(String($0))! }
        let frac = parts.count > 1 ? Array(parts[1]).map { Int(String($0))! } : []
        var kept = Array(frac.prefix(max))
        while kept.count < max { kept.append(0) }
        let rest = Array(frac.dropFirst(max))
        // Half or more of the next unit: away from zero (the tail decides exact halves: they round up too).
        if let first = rest.first, first >= 5 {
            var digits = intDigits + kept
            var i = digits.count - 1
            while i >= 0 {
                if digits[i] == 9 { digits[i] = 0; i -= 1 } else { digits[i] += 1; break }
            }
            if i < 0 { digits.insert(1, at: 0) }
            intDigits = Array(digits.prefix(digits.count - max))
            kept = Array(digits.suffix(max))
        }
        while kept.count > min, kept.last == 0 { kept.removeLast() }
        // Thousands grouped by three with a narrow no-break space.
        var grouped = ""
        for (k, d) in intDigits.enumerated() {
            if k > 0 && (intDigits.count - k) % 3 == 0 { grouped += "\u{202F}" }
            grouped += String(d)
        }
        let body = kept.isEmpty ? grouped : "\(grouped),\(kept.map(String.init).joined())"
        // A tiny negative number rounded to zero is written without its sign, as the browser does.
        let zero = intDigits.allSatisfy { $0 == 0 } && kept.allSatisfy { $0 == 0 }
        return v < 0 && !zero ? "-\(body)" : body
    }

    /// Shortest decimal that reads back as the same double ("86.55", "0.00001", "12345678901234567000"), without
    /// exponent: Swift's `description` is the shortest round-trip writing, expanded here when it uses "e".
    static func shortestDecimal(_ v: Double) -> String {
        let s = "\(v)"
        guard let e = s.firstIndex(where: { $0 == "e" || $0 == "E" }) else { return s }
        let mantissa = String(s[..<e])
        let exp = Int(s[s.index(after: e)...]) ?? 0
        let mParts = mantissa.split(separator: ".", omittingEmptySubsequences: false)
        let intPart = String(mParts[0])
        let fracPart = mParts.count > 1 ? String(mParts[1]) : ""
        var digits = intPart + fracPart
        var point = intPart.count + exp
        if point <= 0 {
            digits = String(repeating: "0", count: 1 - point) + digits
            point = 1
        } else if point > digits.count {
            digits += String(repeating: "0", count: point - digits.count)
        }
        let head = String(digits.prefix(point))
        let tail = String(digits.dropFirst(point))
        return tail.isEmpty ? head : "\(head).\(tail)"
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

    /// Large dollar amounts in the display currency: "421 Md€", "3,2 Md€", "38,5 M€", "12,4 k€".
    public static func usdCompact(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        return Money.compact(v, sep: nnbsp)
    }

    /// "+2,8 Md€" / "−271 M€".
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
