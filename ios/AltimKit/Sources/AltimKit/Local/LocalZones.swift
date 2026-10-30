import Foundation

/// Buy zones by horizon with Fibonacci retracements, computed on the iPhone (core/src/engine/fibonacci.rs).
///
/// After a rise from a low L to a high H, the price often comes back to 38.2 %–65 % of the move before resuming (the
/// "golden pocket" is 61.8 %–65 %). Short term: 4 h candles over ~2 weeks; medium: daily over ~4 months; long: weekly
/// over ~2 years. Each zone is checked on the asset's own history, without look-ahead.
enum FibEngine {
    struct Config {
        let horizon: String, label: String, unit: String, holding: String
        let window: Int, outcome: Int, minAtr: Double
    }

    static let short = Config(horizon: "short", label: "Court terme", unit: "4 h", holding: "quelques jours à 2 semaines", window: 90, outcome: 30, minAtr: 4)
    static let medium = Config(horizon: "medium", label: "Moyen terme", unit: "1 j", holding: "quelques semaines à quelques mois", window: 120, outcome: 40, minAtr: 4)
    static let long = Config(horizon: "long", label: "Long terme", unit: "1 sem.", holding: "plusieurs mois à plusieurs années", window: 104, outcome: 26, minAtr: 3)

    static let ratios: [Double] = [0.236, 0.382, 0.5, 0.618, 0.65, 0.786]
    static let extensions: [Double] = [1.272, 1.618]

    struct Swing: Equatable {
        var up: Bool
        var low: Double, high: Double
        var lowIndex: Int, highIndex: Int
    }

    /// Last significant move known at candle `until` within `window` candles (see the Rust reference for the rules).
    static func swing(_ c: [Bar], until: Int, window: Int, minAtr: Double, atr: [Double?]) -> Swing? {
        let start = max(until + 1 - window, 0)
        guard until >= start + 20, until < atr.count, let a = atr[until] else { return nil }
        func argmax(_ from: Int) -> Int {
            var k = from
            for i in from...until where c[i].high >= c[k].high { k = i }
            return k
        }
        var lo = start
        for i in start...until where c[i].low <= c[lo].low { lo = i }
        let hi = argmax(start)
        func make(_ l: Int, _ h: Int, _ up: Bool) -> Swing { Swing(up: up, low: c[l].low, high: c[h].high, lowIndex: l, highIndex: h) }
        if lo < hi { return c[hi].high - c[lo].low >= minAtr * a ? make(lo, hi, true) : nil }
        if lo == until { return c[hi].high - c[lo].low >= minAtr * a ? make(lo, hi, false) : nil }
        let fall = c[hi].high - c[lo].low
        let hi2 = argmax(lo + 1)
        let rebound = c[hi2].high - c[lo].low
        if rebound >= minAtr * a && rebound >= 0.382 * fall { return make(lo, hi2, true) }
        return fall >= minAtr * a ? make(lo, hi, false) : nil
    }

    static func level(_ s: Swing, _ r: Double) -> Double { s.high - r * (s.high - s.low) }

    /// 1 when the price reaches `up` before `down` within `bars` candles (pessimistic when both in one candle).
    static func outcome(_ c: [Bar], _ i: Int, up: Double, down: Double, bars: Int) -> Double {
        let end = min(c.count - 1, i + bars)
        guard end > i else { return 0 }
        for x in c[(i + 1)...end] {
            if x.low <= down { return 0 }
            if x.high >= up { return 1 }
        }
        return 0
    }

    /// Every first entry into the 38.2–65 % zone of an up move known at that time; success = back to the high before
    /// breaking the low. Base = the same distances from every candle of the history.
    static func evidence(_ c: [Bar], _ cfg: Config) -> Evidence? {
        let a = Indicators.atr(c, 14)
        var events: [(Int, Double, Double)] = []
        var seen = Set<Int>()
        let end = c.count - cfg.outcome
        guard end > 30 else { return nil }
        for i in 30..<end {
            guard let s = swing(c, until: i, window: cfg.window, minAtr: cfg.minAtr, atr: a), s.up, !seen.contains(s.highIndex), s.highIndex != i else { continue }
            let top = level(s, 0.382), bottom = level(s, 0.65)
            let p = c[i].close
            if p <= top && p >= bottom && c[i - 1].close > top {
                seen.insert(s.highIndex)
                events.append((i, s.high / p, s.low / p))
            }
        }
        guard !events.isEmpty else { return nil }
        let wins = events.reduce(0.0) { $0 + outcome(c, $1.0, up: c[$1.0].close * $1.1, down: c[$1.0].close * $1.2, bars: cfg.outcome) }
        var baseWins = 0.0, baseN = 0.0
        for (_, up, down) in events {
            for j in 30..<end {
                baseWins += outcome(c, j, up: c[j].close * up, down: c[j].close * down, bars: cfg.outcome)
                baseN += 1
            }
        }
        let rate = wins / Double(events.count) * 100
        let base = baseN != 0 ? baseWins / baseN * 100 : 0
        let lift = base > 0 ? rate / base : rate > 0 ? 2 : 1
        return Evidence(samples: events.count, rate: rate, base: base, lift: lift)
    }

    static func px(_ v: Double) -> String { Money.price(v) }
    static func pct(_ v: Double) -> String { "\(JSFormat.fr(v, min: 0, max: 1)) %" }

    static func state(_ s: Swing, _ p: Double) -> (status: String, distance: Double?, text: String) {
        let top = level(s, 0.382), goldenTop = level(s, 0.618), bottom = level(s, 0.65)
        let status = p > top ? "above" : p >= goldenTop ? "inZone" : p >= bottom ? "golden" : p >= s.low ? "deep" : "broken"
        let distance = status == "above" ? (1 - top / p) * 100 : nil
        let text: String
        switch status {
        case "above": text = "Prix au-dessus de la zone : attendre un repli vers \(px(top)) – \(px(bottom)) (−\(pct(distance ?? 0)) pour l'atteindre)."
        case "inZone": text = "Prix dans la zone d'achat (38,2 % – 61,8 % du mouvement de \(px(s.low)) à \(px(s.high)))."
        case "golden": text = "Prix dans la « zone d'or » (61,8 % – 65 %), le repli le plus surveillé par les traders."
        case "deep": text = "Repli profond (au-delà de 65 %) : dernier soutien avant \(px(s.low)) ; un passage sous ce plus bas invalide la zone."
        default: text = "Le plus bas du mouvement (\(px(s.low))) est cassé : zone invalidée, attendre un nouveau point bas."
        }
        return (status, distance, text)
    }

    static func zone(_ raw: [Bar], _ cfg: Config, price: Double?, withEvidence: Bool) -> FibZone {
        let c = Indicators.sanitize(raw)
        func empty(_ status: String, _ levels: [FibZone.Level], _ text: String) -> FibZone {
            FibZone(horizon: cfg.horizon, label: cfg.label, unit: cfg.unit, holding: cfg.holding, status: status, levels: levels, zone: nil,
                    golden: nil, invalidation: nil, targets: [], distance: nil, evidence: nil, text: text, macroNote: nil)
        }
        let s = c.count >= 30 ? swing(c, until: c.count - 1, window: cfg.window, minAtr: cfg.minAtr, atr: Indicators.atr(c, 14)) : nil
        guard let s else {
            return empty("none", [], c.count < 30 ? "Historique insuffisant pour cet horizon." : "Pas de mouvement assez net pour tracer des niveaux.")
        }
        let p = price ?? c[c.count - 1].close
        if !s.up {
            return empty("downtrend", ratios.map { .init(ratio: $0, price: s.low + $0 * (s.high - s.low)) },
                         "Mouvement baissier en cours (de \(px(s.high)) à \(px(s.low))) : pas de zone d'achat en repli. Les niveaux de Fibonacci au-dessus du prix sont des résistances ; attendre qu'un nouveau creux se forme et que le prix reparte.")
        }
        let st = state(s, p)
        return FibZone(horizon: cfg.horizon, label: cfg.label, unit: cfg.unit, holding: cfg.holding, status: st.status,
                       levels: ratios.map { .init(ratio: $0, price: level(s, $0)) },
                       zone: Band(from: level(s, 0.65), to: level(s, 0.382)), golden: Band(from: level(s, 0.65), to: level(s, 0.618)),
                       invalidation: s.low, targets: [s.high] + extensions.map { s.low + $0 * (s.high - s.low) },
                       distance: st.distance, evidence: withEvidence ? evidence(c, cfg) : nil, text: st.text, macroNote: nil)
    }

    /// The three horizons from 4 h, daily and weekly candles.
    static func zones(h4: [Bar], daily: [Bar], weekly: [Bar], price: Double?) -> [FibZone] {
        [zone(h4, short, price: price, withEvidence: true), zone(daily, medium, price: price, withEvidence: true),
         zone(weekly, long, price: price, withEvidence: true)]
    }
}
