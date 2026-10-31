import Foundation

/// Altim's bot, trained on the device: a ridge regression of the next 20 days' return on 12 measures of each daily
/// candle (momentum on 4 horizons, distance to the 50- and 200-day averages, RSI, volatility, fall from the high,
/// volume), learned on the long histories of every asset of the same market (cryptos together, stocks together).
/// Walk-forward only: a prediction made on a day uses only the examples whose 20 days were already over that day.
public enum BotEngine {
    /// Days ahead that the bot predicts.
    public static let horizon = 20
    static let ridge = 1.0

    /// Indicators of one daily history.
    struct Series {
        let c: [Bar]
        let close: [Double]
        let ema50: [Double?], ema200: [Double?], rsi14: [Double?], rsi2: [Double?]
        init(_ bars: [Bar]) {
            c = Indicators.sanitize(bars)
            close = c.map(\.close)
            ema50 = Indicators.ema(close, 50); ema200 = Indicators.ema(close, 200)
            rsi14 = Indicators.rsi(close, 14); rsi2 = Indicators.rsi(close, 2)
        }
        func ret(_ i: Int, _ n: Int) -> Double { close[i] / close[i - n] - 1 }
    }

    public static let featureNames = ["rendement 5 j", "rendement 20 j", "rendement 60 j", "rendement 120 j", "écart moyenne 50 j",
                                      "écart moyenne 200 j", "RSI 14", "RSI 2", "volatilité 20 j", "recul depuis le plus haut 120 j",
                                      "moyenne 50 j / 200 j", "volume 5 j / 60 j"]

    /// The 12 measures of day `i` (closed candle), each kept within a sane range; nil before 200 days of history.
    static func features(_ s: Series, _ i: Int) -> [Double]? {
        guard i >= 200, let e50 = s.ema50[i], let e200 = s.ema200[i], let r14 = s.rsi14[i], let r2 = s.rsi2[i] else { return nil }
        func clip(_ x: Double, _ lim: Double) -> Double { x.isFinite ? max(-lim, min(lim, x)) : 0 }
        var sum = 0.0, sq = 0.0
        for k in (i - 19)...i { let r = s.close[k] / s.close[k - 1] - 1; sum += r; sq += r * r }
        let vol = sqrt(max(0, sq / 20 - (sum / 20) * (sum / 20)))
        let high = s.c[(i - 119)...i].map(\.high).max() ?? s.close[i]
        let v5 = s.c[(i - 4)...i].map(\.volume).reduce(0, +) / 5, v60 = s.c[(i - 59)...i].map(\.volume).reduce(0, +) / 60
        return [
            clip(s.ret(i, 5), 0.6), clip(s.ret(i, 20), 1), clip(s.ret(i, 60), 2), clip(s.ret(i, 120), 3),
            clip(s.close[i] / e50 - 1, 1), clip(s.close[i] / e200 - 1, 2),
            (r14 - 50) / 50, (r2 - 50) / 50,
            clip(vol * 10, 2), clip(s.close[i] / high - 1, 1),
            clip(e50 / e200 - 1, 1), v5 > 0 && v60 > 0 ? clip(log(v5 / v60), 2) : 0,
        ]
    }

    /// What a bot learns to predict.
    public enum Target: Sendable, Equatable {
        /// Return from the next open to the close `days` later (what a buyer tonight would have earned), capped.
        case gain(days: Int)
        /// Worst fall from the next open within the next `days` (≤ 0): the risk a buyer tonight would have lived.
        case fall(days: Int)
        var days: Int { switch self { case let .gain(d), let .fall(d): return d } }
        var isFall: Bool { if case .fall = self { return true } else { return false } }
    }

    static func label(_ s: Series, _ i: Int, _ target: Target = .gain(days: horizon)) -> Double? {
        let d = target.days
        guard i + d < s.c.count, s.c[i + 1].open > 0 else { return nil }
        let entry = s.c[i + 1].open
        switch target {
        case .gain: return max(-0.5, min(0.5, s.close[i + d] / entry - 1))
        case .fall: return max(-0.9, min(0, (s.c[(i + 1)...(i + d)].map(\.low).min() ?? entry) / entry - 1))
        }
    }

    /// Ridge regression grown example by example (normal equations), so retraining costs nothing.
    struct Ridge {
        let n: Int
        var xtx: [Double], xty: [Double], count = 0
        init(features: Int) {
            n = features + 1
            xtx = Array(repeating: 0, count: n * n)
            xty = Array(repeating: 0, count: n)
        }
        mutating func add(_ f: [Double], _ y: Double) {
            let x = [1] + f
            for a in 0..<n {
                xty[a] += x[a] * y
                for b in 0..<n { xtx[a * n + b] += x[a] * x[b] }
            }
            count += 1
        }
        /// Weights (intercept first), by Gaussian elimination on XᵀX + λI (the intercept is not penalised).
        func solve(lambda: Double) -> [Double]? {
            guard count > n * 10 else { return nil }
            var m = xtx, v = xty
            for a in 1..<n { m[a * n + a] += lambda }
            for col in 0..<n {
                guard let p = (col..<n).max(by: { abs(m[$0 * n + col]) < abs(m[$1 * n + col]) }), abs(m[p * n + col]) > 1e-12 else { return nil }
                if p != col {
                    for k in 0..<n { m.swapAt(col * n + k, p * n + k) }
                    v.swapAt(col, p)
                }
                for r in 0..<n where r != col {
                    let f = m[r * n + col] / m[col * n + col]
                    guard f != 0 else { continue }
                    for k in col..<n { m[r * n + k] -= f * m[col * n + k] }
                    v[r] -= f * v[col]
                }
            }
            return (0..<n).map { v[$0] / m[$0 * n + $0] }
        }
    }

    static func predict(_ w: [Double], _ f: [Double]) -> Double { w[0] + zip(w.dropFirst(), f).reduce(0) { $0 + $1.0 * $1.1 } }

    /// Walk-forward predictions for a set of histories of one market: for each history, the predicted 20-day return of
    /// each day (nil while there is not enough past to learn from). Retrained every `every` days of calendar time, on
    /// the examples whose outcome was known before that day only.
    public static func walkForward(_ histories: [[Bar]], every: Double = 30 * 86_400_000) -> [[Double?]] {
        let series = histories.map(Series.init)
        struct Example { let known: Double; let f: [Double]; let y: Double }
        var examples: [Example] = []
        var days: [(t: Double, h: Int, i: Int, f: [Double])] = []
        for (h, s) in series.enumerated() {
            for i in 0..<s.c.count {
                guard let f = features(s, i) else { continue }
                days.append((s.c[i].time, h, i, f))
                if let y = label(s, i) { examples.append(Example(known: s.c[i + horizon].time, f: f, y: y)) }
            }
        }
        examples.sort { $0.known < $1.known }
        days.sort { $0.t < $1.t }
        var out = series.map { [Double?](repeating: nil, count: $0.c.count) }
        var model = Ridge(features: featureNames.count)
        var next = 0, weights: [Double]? = nil, trainedAt = -Double.infinity
        for d in days {
            if d.t - trainedAt >= every {
                while next < examples.count, examples[next].known < d.t { model.add(examples[next].f, examples[next].y); next += 1 }
                weights = model.solve(lambda: ridge * Double(max(model.count, 1)) / 1000)
                trainedAt = d.t
            }
            if let w = weights { out[d.h][d.i] = predict(w, d.f) }
        }
        return out
    }

    // MARK: - Boosted trees (non-linear)

    /// Gradient-boosted decision stumps on the same measures: each round adds the single split (one measure, one
    /// threshold among its quantiles) that best corrects the remaining error. Captures thresholds and asymmetries a
    /// straight line cannot (e.g. "momentum positive only matters above the 200-day average").
    struct Stumps: Codable, Sendable {
        struct Stump: Codable, Sendable { let f: Int; let cut: Double; let left: Double; let right: Double }
        var base = 0.0
        var stumps: [Stump] = []

        static func train(_ x: [[Double]], _ y: [Double], rounds: Int = 120, rate: Double = 0.1, bins: Int = 16, minLeaf: Int = 200) -> Stumps {
            var m = Stumps()
            guard x.count > minLeaf * 4, let width = x.first?.count else { return m }
            m.base = y.reduce(0, +) / Double(y.count)
            var pred = [Double](repeating: m.base, count: y.count)
            // Quantile cuts and the bin of each value, computed once.
            var cuts: [[Double]] = [], bin: [[UInt8]] = []
            for f in 0..<width {
                let sorted = x.map { $0[f] }.sorted()
                let c = (1..<bins).map { sorted[$0 * sorted.count / bins] }
                cuts.append(c)
                bin.append(x.map { v in UInt8(c.firstIndex { v[f] <= $0 } ?? c.count) })
            }
            for _ in 0..<rounds {
                let r = zip(y, pred).map { $0 - $1 }
                var best: (gain: Double, f: Int, b: Int, l: Double, rr: Double)? = nil
                let total = r.reduce(0, +), n = Double(r.count)
                for f in 0..<width {
                    var sum = [Double](repeating: 0, count: bins), cnt = [Int](repeating: 0, count: bins)
                    for k in 0..<r.count { let b = Int(bin[f][k]); sum[b] += r[k]; cnt[b] += 1 }
                    var ls = 0.0, lc = 0
                    for b in 0..<(bins - 1) {
                        ls += sum[b]; lc += cnt[b]
                        let rc = r.count - lc
                        guard lc >= minLeaf, rc >= minLeaf else { continue }
                        let rs = total - ls
                        let gain = ls * ls / Double(lc) + rs * rs / Double(rc) - total * total / n
                        if gain > (best?.gain ?? 0) { best = (gain, f, b, ls / Double(lc), rs / Double(rc)) }
                    }
                }
                guard let b = best else { break }
                let s = Stump(f: b.f, cut: cuts[b.f][b.b], left: b.l * rate, right: b.rr * rate)
                m.stumps.append(s)
                for k in 0..<pred.count { pred[k] += Int(bin[b.f][k]) <= b.b ? s.left : s.right }
            }
            return m
        }

        func predict(_ f: [Double]) -> Double { stumps.reduce(base) { $0 + (f[$1.f] <= $1.cut ? $1.left : $1.right) } }
    }

    /// Walk-forward predictions of the boosted trees (retrained every `every` of calendar time on the examples
    /// already known, at most the last `window` of them).
    public static func walkForwardTrees(_ histories: [[Bar]], target: Target = .gain(days: horizon), every: Double = 90 * 86_400_000,
                                        window: Int = 60_000) -> [[Double?]] {
        walkForwardTrees(train: histories, predict: histories, target: target, every: every, window: window)
    }

    /// Walk-forward predictions for `predict`, learned on the examples of `train` only (as the app does: trained on
    /// its universe, applied to any asset of the same market).
    public static func walkForwardTrees(train: [[Bar]], predict: [[Bar]], target: Target = .gain(days: horizon),
                                        every: Double = 90 * 86_400_000, window: Int = 60_000) -> [[Double?]] {
        var examples: [(known: Double, f: [Double], y: Double)] = []
        for s in train.map(Series.init) {
            for i in 0..<s.c.count {
                if let f = features(s, i), let y = label(s, i, target) { examples.append((s.c[i + target.days].time, f, y)) }
            }
        }
        let series = predict.map(Series.init)
        var days: [(t: Double, h: Int, i: Int, f: [Double])] = []
        for (h, s) in series.enumerated() {
            for i in 0..<s.c.count { if let f = features(s, i) { days.append((s.c[i].time, h, i, f)) } }
        }
        examples.sort { $0.known < $1.known }
        days.sort { $0.t < $1.t }
        var out = series.map { [Double?](repeating: nil, count: $0.c.count) }
        var known = 0, model: Stumps? = nil, trainedAt = -Double.infinity
        for d in days {
            if d.t - trainedAt >= every {
                while known < examples.count, examples[known].known < d.t { known += 1 }
                let slice = examples[max(0, known - window)..<known]
                let m = Stumps.train(slice.map(\.f), slice.map(\.y))
                model = m.stumps.isEmpty ? nil : m
                trainedAt = d.t
            }
            if let m = model { out[d.h][d.i] = m.predict(d.f) }
        }
        return out
    }

    // MARK: - The bots used by the app

    /// A bot of the app, what it predicts and what the lab measured about it (walk-forward, two lists × two halves).
    public struct Info: Sendable {
        public let name: String, predicts: String, use: String, measured: String
    }

    /// The bots shown in the app. Tested and left out: a linear bot (worse than the rule), a 5-day bot (its
    /// predictions did not separate good and bad days consistently).
    public static let bots: [Info] = [
        Info(name: "Bot Tendance", predicts: "le rendement des 20 jours suivants",
             use: "veto : en tendance haussière, pas de nouvel achat sous −1 %, sortie sous −3 %",
             measured: "comme veto, a amélioré la règle sur les deux listes (rendement moyen +32 % contre +24 %, pertes des pires actifs réduites), surtout en crypto, et a battu 5 vetos tirés au hasard aussi souvent (+9 à +23 % sur la même liste)  ; sur un actif qu'il n'a jamais vu à l'entraînement, le veto aide encore en moyenne (+35 % contre +30 %, creux crypto baissier −30 % contre −34 %) : il apporte bien une information ; en revanche, classer les actifs par sa prévision ne marche pas (les prévisions les plus hautes n'ont pas fait mieux)"),
        Info(name: "Bot Risque", predicts: "la pire baisse dans les 20 jours suivants",
             use: "information de risque, pour régler la taille d'une position",
             measured: "fiable dans les 8 cas mesurés : quand il prévoit peu de risque, la baisse réelle a été plus faible (actions −4 % contre −8 %, crypto −13 % contre −16 % en moyenne) ; ses prévisions les plus calmes sous-estimaient la baisse réelle (crypto −8 % annoncés, −14 à −15 % vécus ; actions −3 % pour −4 %), d'où un plancher de 14 % en crypto et de 4 % en actions ; avec ce plancher, régler la taille pour ne risquer qu'environ 5 % a ramené la pire baisse de la règle de −48 % à −15 % en crypto, mais aussi son rendement de +47 % à +16 % ; sur les actions, rendement presque identique et pire baisse égale ou plus faible"),
    ]

    /// Entry blocked under this predicted 20-day return, exit under `exitBelow` (chosen once, before the test; the
    /// lab checked that nearby values give the same result: −2 / −4 %, −1 / −5 %, entry only).
    public static let entryBelow = -0.01
    public static let exitBelow = -0.03

    /// The trained bot of one market: boosted trees learned on the long daily histories of its assets.
    public struct Model: Codable, Sendable {
        let trees: Stumps
        /// Assets and examples it learned from.
        public let assets: Int, examples: Int
        public let trainedAt: Double
        /// Measures it was trained on (a bot saved by an older version with other measures is not reused).
        public var featureCount: Int? = BotEngine.featureNames.count

        /// Usable with today's measures: a bot saved by a version with other measures would read the wrong ones.
        public var isCurrent: Bool { featureCount == BotEngine.featureNames.count && trees.stumps.allSatisfy { $0.f < BotEngine.featureNames.count } }

        /// Trained on every example whose 20 days are over (the most recent 60 000 at most).
        public static func train(_ histories: [[Bar]], target: Target = .gain(days: horizon), now: Double = Date().timeIntervalSince1970 * 1000) -> Model? {
            var x: [[Double]] = [], y: [Double] = []
            for bars in histories {
                let s = Series(bars)
                for i in 0..<s.c.count { if let f = features(s, i), let l = label(s, i, target) { x.append(f); y.append(l) } }
            }
            let keep = max(0, x.count - 60_000)
            let t = Stumps.train(Array(x[keep...]), Array(y[keep...]))
            guard !t.stumps.isEmpty else { return nil }
            return Model(trees: t, assets: histories.count, examples: x.count - keep, trainedAt: now)
        }

        /// Predicted return over the next 20 days for the last closed candle of `bars`; nil under 200 days.
        public func predict(_ bars: [Bar]) -> Double? {
            let s = Series(bars)
            guard isCurrent, let f = features(s, s.c.count - 1) else { return nil }
            return trees.predict(f)
        }
    }

    /// Smallest worst fall the risk bot may announce, per market. Measured out of sample (testRiskCalibration): its
    /// calmest fifth of predictions under-estimated the real fall (crypto −8 % announced, −14 to −15 % lived; stocks
    /// −3 % announced, −4 % lived), the other fifths matched. Below these floors, the floor is the honest figure.
    public static func riskFloor(_ kind: Kind) -> Double { kind == .crypto ? 0.14 : 0.04 }

    /// The risk bot's prediction, never calmer than what its calm predictions turned out to be.
    public static func calibratedFall(_ fall: Double?, kind: Kind) -> Double? {
        fall.map { min($0, -riskFloor(kind)) }
    }

    /// Risk the user accepts per position, as a share of the planned amount (Réglages): 5 % or 10 %. Measured
    /// (testRiskSizing, rule + veto, calibrated risk): crypto +16 % / worst fall −15 % at 5 %, +33 to +35 % / −30 % at
    /// 10 %, +47 % / −48 % for a full position; stocks about the same at both.
    public static let riskBudgetKey = "altim.riskBudget"
    public static var riskBudget: Double {
        let v = UserDefaults.standard.double(forKey: riskBudgetKey)
        return v == 0.10 ? 0.10 : 0.05
    }

    /// Share (in %, 1…100) of the planned amount to commit so that the risk bot's predicted worst fall costs about
    /// `budget` of it; nil without a prediction.
    public static func size(forFall fall: Double?, budget: Double = riskBudget) -> Int? {
        guard let f = fall, f.isFinite else { return nil }
        return Int(max(1, min(100, (budget / max(abs(f), 0.01) * 100).rounded())))
    }

    /// Fall of a portfolio if every line lived, at the same time, the worst fall the risk bot predicts for it
    /// (a pessimistic bound: lines rarely all touch their low together). Lines without a prediction are left out
    /// and counted. `lines`: (value, predicted fall ≤ 0 or nil).
    public static func portfolioFall(_ lines: [(value: Double, fall: Double?)]) -> (percent: Double, amount: Double, covered: Double, missing: Int)? {
        let known = lines.filter { $0.fall != nil && $0.value > 0 }
        let total = lines.filter { $0.value > 0 }.reduce(0) { $0 + $1.value }
        let covered = known.reduce(0) { $0 + $1.value }
        guard covered > 0, total > 0 else { return nil }
        let amount = known.reduce(0) { $0 + $1.value * ($1.fall ?? 0) }
        return (amount / total * 100, amount, covered / total * 100, lines.count - known.count)
    }

    /// The rule's verdict with the bot's veto: in an up trend, no new position when the bot predicts below −1 % over
    /// 20 days (ATTENDRE, keep a position), out below −3 % (VENTE). Never turns a verdict into a buy.
    public static func apply(_ prediction: Double?, to v: TrendEngine.Verdict) -> TrendEngine.Verdict {
        var v = v
        v.bot = prediction
        guard let p = prediction, v.state == .trend || v.state == .pullback, v.action == .buy || v.action == .strongBuy else { return v }
        let pct = "\(p >= 0 ? "+" : "−")\(JSFormat.fr(abs(p) * 100, min: 1, max: 1)) %"
        if p < exitBelow {
            v.action = .sell
            v.text = "Le bot entraîné prévoit \(pct) sur 20 jours malgré la tendance : sortir. " + v.text
        } else if p < entryBelow {
            v.action = .hold
            v.text = "Le bot entraîné prévoit \(pct) sur 20 jours : conserver une position, ne pas en ouvrir. " + v.text
        }
        return v
    }
}
