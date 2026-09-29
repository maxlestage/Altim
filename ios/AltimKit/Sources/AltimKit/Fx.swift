import Foundation

/// EUR/USD rate of the app (/api/fx: Yahoo Finance, else ECB, else Frankfurter), as on the web (web/src/webapp/fx.ts).
/// The last valid rate is kept on the iPhone (7 days at most, shown with its source and time) so a server hiccup does
/// not switch the amounts back to dollars; older, or never read, the app says "Taux EUR/USD indisponible" and shows
/// dollars.
public enum Fx {
    public static let refreshInterval: TimeInterval = 10 * 60
    static let keepMs: Double = 7 * 86_400_000

    /// `/api/fx` body (rate null with `error` when no source answered).
    public struct Response: Codable, Sendable, Equatable {
        public var base: String?
        public var quote: String?
        public var rate: Double?
        public var usdPerEur: Double?
        public var time: Double?
        public var source: String?
        public var fetchedAt: Double?
        public var stale: Bool?
        public var error: String?
    }

    /// A valid rate from the response, else nil (never a default).
    public static func parse(_ x: Response?, now: Date = Date()) -> FxRate? {
        guard let x, let rate = x.rate, rate.isFinite, rate > 0, rate <= 5, let time = x.time, let source = x.source else { return nil }
        return FxRate(rate: rate, usdPerEur: x.usdPerEur ?? 1 / rate, time: time, source: source,
                      fetchedAt: x.fetchedAt ?? now.timeIntervalSince1970 * 1000, stale: x.stale ?? false)
    }

    public static func parse(_ data: Data?, now: Date = Date()) -> FxRate? {
        guard let data, let r = try? JSONDecoder().decode(Response.self, from: data) else { return nil }
        return parse(r, now: now)
    }

    /// The saved rate when younger than 7 days (read by Altim, not quoted: a weekend's Friday close stays valid).
    public static func saved(_ data: Data?, now: Date = Date()) -> FxRate? {
        guard var r = parse(data, now: now), now.timeIntervalSince1970 * 1000 - r.fetchedAt < keepMs else { return nil }
        r.stale = true
        return r
    }
}

extension AltimClient {
    /// `GET /api/fx`: the raw answer (kept by the app for the next launch) and its valid rate, nil when unavailable.
    /// Served from the offline cache like the other reads: its `fetchedAt` then tells how old it is.
    public func fx() async throws -> (data: Data, response: Fx.Response, rate: FxRate?) {
        let (data, status) = try await getWithStatus("/api/fx", [:])
        let r = try decodeResponse(Fx.Response.self, data, status)
        // A saved answer older than 7 days (offline) is not a rate any more.
        let rate = Fx.parse(r).flatMap { Date().timeIntervalSince1970 * 1000 - $0.fetchedAt < Fx.keepMs ? $0 : nil }
        return (data, r, rate)
    }

    /// `cur=USD` on the routes whose texts carry amounts when the display is in dollars (the server writes euros by
    /// default when it has a rate).
    static func currencyQuery() -> [String: String] { Money.displayCurrency == .usd ? ["cur": "USD"] : [:] }
}
