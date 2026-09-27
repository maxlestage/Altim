import Foundation

/// French formatting, identical to the web app (web/src/market.ts): prices in dollars, signed percentages.
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

    /// 2 decimals from 1 $, 4 from 0.01 $, 8 below (small cryptos).
    public static func price(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        let digits = abs(v) >= 1 ? 2 : abs(v) >= 0.01 ? 4 : 8
        return "\(number(v, min: digits, max: digits)) $"
    }

    /// Whole dollars from 100 $ (amounts, budgets).
    public static func money(_ v: Double) -> String {
        "\(number(v, min: 0, max: abs(v) >= 100 ? 0 : 2)) $"
    }

    /// +1,25 % / −0,40 % (true minus sign).
    public static func percent(_ v: Double?, digits: Int = 2) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v >= 0 ? "+" : "−")\(number(abs(v), min: digits, max: digits)) %"
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
