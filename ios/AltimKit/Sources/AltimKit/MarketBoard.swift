import Foundation

/// The two markets of the market filter (Bitcoin for cryptos, SPY for stocks) as one line each: the « Marchés » widget
/// and the Point du jour say the same thing. nil = unknown (no candles, sources in disagreement, or a last close too
/// old), and then the filter does not apply: the asset's rule decides alone.
public enum MarketBoard {
    public static let kinds: [Kind] = [.crypto, .stock]

    public static func name(_ kind: Kind) -> String { kind == .crypto ? "Marché crypto (Bitcoin)" : "Marché actions (S&P 500)" }
    public static func shortName(_ kind: Kind) -> String { kind == .crypto ? "Crypto (BTC)" : "Actions (S&P 500)" }

    /// The full sentence (Point du jour).
    public static func text(_ kind: Kind, _ up: Bool?) -> String {
        switch up {
        case true?: return "\(name(kind)) en tendance haussière : le filtre de marché laisse acheter."
        case false?: return "\(name(kind)) hors tendance haussière : le filtre de marché suspend les achats et conseille de sortir."
        case nil: return "\(name(kind)) : état inconnu (données peu fiables ou indisponibles), le filtre de marché ne s'applique pas ; la règle de l'actif décide seule."
        }
    }

    /// The widget's few words.
    public static func shortText(_ up: Bool?) -> String {
        switch up {
        case true?: return "Haussier : achats permis"
        case false?: return "Hors tendance : pas d'achat, sortir"
        case nil: return "Inconnu : filtre non appliqué"
        }
    }

    /// The state from the benchmark's daily snapshot: nil when it is missing, unreliable or older than the stale limit
    /// of the verdicts (3 days for Bitcoin, 5 for SPY).
    public static func state(_ snap: Snapshot?, kind: Kind, now: Double = Date().timeIntervalSince1970 * 1000) -> Bool? {
        guard let snap, let last = snap.candles.last else { return nil }
        if now - last.time > (kind == .crypto ? 3.0 : 5.0) * 86_400_000 { return nil }
        let bars = snap.candles.map { Bar(time: $0.time, open: $0.open, high: $0.high, low: $0.low, close: $0.close, volume: $0.volume ?? 0) }
        return TrendEngine.marketUp(bars, kind: kind, reliability: snap.reliability, assetTime: nil)
    }

    /// Both markets read through the client (iPhone or widget, no server): two daily snapshots.
    public static func read(_ client: AltimClient) async -> [Kind: Bool?] {
        async let c = try? client.candles(TrendEngine.benchmark(.crypto), interval: Timeframe.d1.rawValue)
        async let s = try? client.candles(TrendEngine.benchmark(.stock), interval: Timeframe.d1.rawValue)
        let (cs, ss) = (await c, await s)
        return [.crypto: state(cs, kind: .crypto), .stock: state(ss, kind: .stock)]
    }
}
