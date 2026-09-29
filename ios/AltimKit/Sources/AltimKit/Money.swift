import Foundation

/// Display currency of every amount, identical to the web (web/src/money.ts): engines and the server compute in
/// dollars (the quote currency of the sources), the display converts to euros with the EUR/USD rate of /api/fx.
/// Process-wide state, set by the app (`Money.setDisplay`) when the setting or the rate changes; without a rate the
/// amounts stay in dollars with "$" (a rate is never made up).
public enum Currency: String, Codable, Sendable, CaseIterable, Hashable {
    case eur = "EUR"
    case usd = "USD"

    public var symbol: String { self == .eur ? "€" : "$" }
}

/// `GET /api/fx` with a rate (euros for 1 dollar).
public struct FxRate: Codable, Sendable, Equatable, Hashable {
    public var rate: Double
    public var usdPerEur: Double
    /// Time of the quote (ms).
    public var time: Double
    /// "Yahoo Finance", "BCE" or "Frankfurter (BCE)".
    public var source: String
    public var fetchedAt: Double
    public var stale: Bool

    public init(rate: Double, usdPerEur: Double, time: Double, source: String, fetchedAt: Double, stale: Bool) {
        self.rate = rate
        self.usdPerEur = usdPerEur
        self.time = time
        self.source = source
        self.fetchedAt = fetchedAt
        self.stale = stale
    }
}

/// What the formatters read: the currency chosen in Réglages and the last known rate (thread-safe: the background
/// checks format notification bodies too).
final class MoneyState: @unchecked Sendable {
    private let lock = NSLock()
    private var wanted: Currency = .usd
    private var fx: FxRate?

    func set(_ want: Currency, _ rate: FxRate?) {
        lock.lock()
        wanted = want
        fx = rate.flatMap { $0.rate.isFinite && $0.rate > 0 ? $0 : nil }
        lock.unlock()
    }

    var value: (wanted: Currency, fx: FxRate?) {
        lock.lock()
        defer { lock.unlock() }
        return (wanted, fx)
    }
}

public enum Money {
    static let state = MoneyState()
    public static let nbsp = "\u{00A0}"

    /// Sets what the formatters below use: the currency chosen in Réglages and the last known rate.
    public static func setDisplay(_ want: Currency, _ rate: FxRate?) { state.set(want, rate) }

    public static var currentFx: FxRate? { state.value.fx }
    /// The currency chosen in Réglages (shown only when a rate allows it, see `displayCurrency`).
    public static var wanted: Currency { state.value.wanted }

    /// Currency actually shown: euros only when chosen and a rate is known.
    public static var displayCurrency: Currency {
        let s = state.value
        return s.wanted == .eur && s.fx != nil ? .eur : .usd
    }

    public static func symbol(_ c: Currency? = nil) -> String { (c ?? displayCurrency).symbol }

    /// `v` from one currency to another at the current rate; NaN when no rate allows it.
    public static func convert(_ v: Double, from: Currency, to: Currency) -> Double {
        if from == to { return v }
        guard let fx = currentFx else { return .nan }
        return from == .usd ? v * fx.rate : v / fx.rate
    }

    /// Dollars → display currency.
    public static func toDisplay(_ usd: Double) -> Double { convert(usd, from: .usd, to: displayCurrency) }
    /// Display currency → dollars (what the user typed, for the engines and the server).
    public static func fromDisplay(_ v: Double) -> Double { convert(v, from: displayCurrency, to: .usd) }

    static func fr(_ v: Double, _ min: Int, _ max: Int) -> String { JSFormat.fr(v, min: min, max: max) }

    /// A dollar amount in the display currency with `min`–`max` decimals: "212,40 €" (or "$" without a rate).
    public static func money(_ usd: Double, min: Int = 2, max: Int? = nil, sep: String = nbsp) -> String {
        "\(fr(toDisplay(usd), min, max ?? min))\(sep)\(symbol())"
    }

    /// A dollar amount in the display currency with the caller's number format (applied to the converted value).
    public static func moneyFmt(_ usd: Double, sep: String = nbsp, _ fmt: (Double) -> String) -> String {
        "\(fmt(toDisplay(usd)))\(sep)\(symbol())"
    }

    /// A price with `formatPrice` digits (2 from 1, 4 from 0.01, 8 below), chosen on the converted value.
    public static func price(_ usd: Double, sep: String = nbsp) -> String {
        let v = toDisplay(usd)
        let digits = v >= 1 ? 2 : v >= 0.01 ? 4 : 8
        return "\(fr(v, digits, digits))\(sep)\(symbol())"
    }

    /// Large amounts: "421 Md€", "3,16 Md€", "850 M€", "12,5 k€".
    public static func compact(_ usd: Double, sep: String = nbsp) -> String {
        let v = toDisplay(usd)
        let a = abs(v)
        let sym = symbol()
        let (div, unit): (Double, String) = a >= 1e9 ? (1e9, "Md\(sym)") : a >= 1e6 ? (1e6, "M\(sym)") : a >= 1e4 ? (1e3, "k\(sym)") : (1, sym)
        let x = v / div
        let digits = abs(x) >= 100 ? 0 : abs(x) >= 10 ? 1 : 2
        return "\(fr(x, 0, digits))\(sep)\(unit)"
    }

    /// A threshold kept in its own currency ("80 000,00 €"), as typed: price alerts.
    public static func threshold(_ v: Double, _ c: Currency) -> String {
        "\(fr(v, v >= 1 ? 2 : 4, v >= 1 ? 2 : 8)) \(c.symbol)"
    }

    /// "1 $ = 0,881 € · Yahoo Finance, 14:05" (or the date when not today); nil without a rate.
    public static func fxLine(_ r: FxRate?, now: Date = Date(), timeZone: TimeZone = .current) -> String? {
        guard let r else { return nil }
        let d = Date(timeIntervalSince1970: r.time / 1000)
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = timeZone
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.timeZone = timeZone
        f.dateFormat = cal.isDate(d, inSameDayAs: now) ? "HH:mm" : "d MMM HH:mm"
        return "1 $ = \(fr(r.rate, 3, 4)) € · \(r.source), \(f.string(from: d))"
    }

    /// Currency of a stored amount: the one it was typed in; old data without the tag is in dollars.
    public static func stored(_ c: Currency?) -> Currency { c ?? .usd }

    /// A saved amount shown in the display currency (NaN when no rate allows it).
    public static func shown(_ v: Double, _ c: Currency?) -> Double { convert(v, from: stored(c), to: displayCurrency) }

    /// A number typed by the user ("1 234,5", "80 000 €") or nil.
    public static func parse(_ text: String) -> Double? {
        let t = text.replacingOccurrences(of: "\u{202F}", with: "").replacingOccurrences(of: "\u{00A0}", with: "")
            .replacingOccurrences(of: " ", with: "").replacingOccurrences(of: "€", with: "").replacingOccurrences(of: "$", with: "")
            .replacingOccurrences(of: ",", with: ".")
        guard !t.isEmpty, let v = Double(t), v.isFinite else { return nil }
        return v
    }

    /// A saved amount put back in an input, in its display value ("" when not finite): "212,4".
    public static func inputText(_ v: Double) -> String {
        guard v.isFinite else { return "" }
        return JSFormat.shortestDecimal(abs((v * 1e8).rounded() / 1e8)).replacingOccurrences(of: ".", with: ",").prefixedMinus(v < 0)
    }

    /// A price put back in an input, rounded like a price (2 decimals from 1, 4 from 0.01, 8 below), without
    /// thousands separator: "88162,4", "0,000012".
    public static func inputPrice(_ v: Double) -> String {
        guard v.isFinite else { return "" }
        let d = abs(v) >= 1 ? 2 : abs(v) >= 0.01 ? 4 : 8
        return JSFormat.fr(v, min: 0, max: d).replacingOccurrences(of: "\u{202F}", with: "")
    }

    /// "Taux EUR/USD indisponible : montants affichés en $." when euros are asked but no rate is known, else the rate
    /// line with "(dernier taux connu)" for a saved one; nil when dollars are chosen.
    public static func note() -> String? {
        let s = state.value
        guard s.wanted == .eur else { return nil }
        guard let fx = s.fx, let line = fxLine(fx) else { return "Taux EUR/USD indisponible : montants affichés en $." }
        return line + (fx.stale ? " (dernier taux connu)" : "")
    }
}

private extension String {
    func prefixedMinus(_ yes: Bool) -> String { yes && self != "0" ? "-" + self : self }
}

/// An amount saved with the currency it was typed in (Sélection budget): older versions saved a bare number, in
/// dollars.
public struct StoredAmount: Codable, Sendable, Equatable {
    public var amount: Double
    public var currency: Currency

    public init(amount: Double, currency: Currency) {
        self.amount = amount
        self.currency = currency
    }

    /// The saved amount and its currency tag (absent = dollars); nil when not a positive number.
    public static func read(amount: Double?, currency: String?) -> StoredAmount? {
        guard let amount, amount.isFinite, amount > 0 else { return nil }
        return StoredAmount(amount: amount, currency: currency == Currency.eur.rawValue ? .eur : .usd)
    }

    /// In dollars at the current rate (NaN when no rate allows it).
    public var usd: Double { Money.convert(amount, from: currency, to: .usd) }
}
