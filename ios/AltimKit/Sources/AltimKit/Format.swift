import Foundation

/// French formatting, identical to the web app (web/src/market.ts): prices in the display currency (Money: the dollar
/// amounts of the sources converted to euros at the current rate, "$" without a rate), signed percentages.
public enum Format {
    private static func number(_ v: Double, min: Int, max: Int) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.numberStyle = .decimal
        f.minimumFractionDigits = min
        f.maximumFractionDigits = max
        // Narrow no-break space as the thousands separator, like Intl in the browser.
        f.groupingSeparator = "\u{202F}"
        f.decimalSeparator = ","
        return f.string(from: NSNumber(value: v)) ?? String(v)
    }

    /// A dollar price in the display currency: 2 decimals from 1, 4 from 0.01, 8 below (small cryptos).
    public static func price(_ v: Double?) -> String {
        guard let usd = v, usd.isFinite else { return "—" }
        let v = Money.toDisplay(usd)
        let digits = abs(v) >= 1 ? 2 : abs(v) >= 0.01 ? 4 : 8
        return "\(number(v, min: digits, max: digits)) \(Money.symbol())"
    }

    /// A dollar amount in the display currency, whole from 100 (amounts, budgets).
    public static func money(_ usd: Double) -> String {
        let v = Money.toDisplay(usd)
        return "\(number(v, min: 0, max: abs(v) >= 100 ? 0 : 2)) \(Money.symbol())"
    }

    /// +1,25 % / −0,40 % (true minus sign).
    public static func percent(_ v: Double?, digits: Int = 2) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v >= 0 ? "+" : "−")\(number(abs(v), min: digits, max: digits)) %"
    }

    /// Large amounts and counts: "421 Md€", "3,2 Md€", "38 M€", "19,9 M" (unit ""); below a million, whole numbers.
    /// With the default unit "$" the dollar amount is shown in the display currency.
    public static func large(_ value: Double?, unit: String = "$") -> String {
        guard let value, value.isFinite else { return "—" }
        let money = unit == "$"
        let v = money ? Money.toDisplay(value) : value
        let unit = money ? Money.symbol() : unit
        let a = abs(v)
        let n: String
        if a >= 1e9 {
            n = "\(number(v / 1e9, min: 0, max: a >= 1e11 ? 0 : 1)) Md"
        } else if a >= 1e6 {
            n = "\(number(v / 1e6, min: 0, max: a >= 1e8 ? 0 : 1)) M"
        } else {
            return unit.isEmpty ? number(v, min: 0, max: a >= 100 ? 0 : 2) : "\(number(v, min: 0, max: a >= 100 ? 0 : 2)) \(unit)"
        }
        return n + unit
    }

    public static func plain(_ v: Double, digits: Int = 2) -> String { number(v, min: 0, max: digits) }

    /// Quantity of units: up to 8 decimals for cryptos, no trailing zeros.
    public static func quantity(_ v: Double) -> String { number(v, min: 0, max: 8) }

    public static func date(_ ms: Double, time: Bool = false) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.timeZone = TimeZone(identifier: "Europe/Paris")
        f.dateFormat = time ? "d MMM yyyy 'à' HH:mm" : "d MMMM yyyy"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }
}
