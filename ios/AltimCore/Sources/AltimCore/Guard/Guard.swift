import Foundation

/// Market guard (port of `web/src/engine/guard.ts`, checked against it by a shared fixture).
///
/// 1. Regime   — the background trend (daily + 4 h), for the long-term bot.
/// 2. Shock    — abnormal volatility, price jumps, volume spikes, compression, news bursts, VIX:
///               when to reduce or pause short-term trading.
/// 3. Reversal — a move against the current trend: technical exhaustion, crowd positioning, sentiment, news tone.
///
/// Nothing predicts a true surprise. Every candle-based factor is self-validated on the asset's own history:
/// a factor that never announced anything on this asset is not counted.
public enum MarketGuard {
    public enum Trend: String, Codable, Sendable { case up, down, range }
    public enum ShockLevel: String, Codable, Sendable { case calm, agitated, shock }
    public enum Direction: String, Codable, Sendable { case down, up }
    public enum FactorStatus: String, Codable, Sendable { case verified, unproven, rejected, unverifiable }
    public enum Scalping: String, Codable, Sendable { case ok, reduce, pause }

    public struct Positioning: Sendable {
        public var fundingRate: Double?
        public var longShortRatio: [Double]
        public var openInterest: [Double]
        public init(fundingRate: Double? = nil, longShortRatio: [Double] = [], openInterest: [Double] = []) {
            self.fundingRate = fundingRate
            self.longShortRatio = longShortRatio
            self.openInterest = openInterest
        }
    }

    public struct Sentiment: Sendable {
        public var fearGreed: [Double]
        public var socialBullish: Double?
        public var socialSample: Int
        public init(fearGreed: [Double] = [], socialBullish: Double? = nil, socialSample: Int = 0) {
            self.fearGreed = fearGreed
            self.socialBullish = socialBullish
            self.socialSample = socialSample
        }
    }

    public struct NewsItem: Sendable, Hashable, Codable {
        public let title: String
        public let time: Date
        public let source: String?
        public init(title: String, time: Date, source: String? = nil) {
            self.title = title
            self.time = time
            self.source = source
        }
    }

    public struct Input: Sendable {
        public var kind: AssetClass
        public var daily: [Candle]
        public var h4: [Candle]
        public var h1: [Candle]
        public var positioning: Positioning?
        public var sentiment: Sentiment?
        public var news: [NewsItem]
        public var vix: [Double]
        public var now: Date
        public init(kind: AssetClass, daily: [Candle], h4: [Candle], h1: [Candle], positioning: Positioning? = nil,
                    sentiment: Sentiment? = nil, news: [NewsItem] = [], vix: [Double] = [], now: Date = Date()) {
            self.kind = kind
            self.daily = daily
            self.h4 = h4
            self.h1 = h1
            self.positioning = positioning
            self.sentiment = sentiment
            self.news = news
            self.vix = vix
            self.now = now
        }
    }

    public struct Evidence: Sendable, Hashable, Codable {
        public let samples: Int
        public let rate: Double
        public let base: Double
        public let lift: Double
    }

    public struct Factor: Sendable, Hashable, Identifiable {
        public let code: String
        public let points: Int
        public let basePoints: Int
        public let text: String
        public let status: FactorStatus
        public let evidence: Evidence?
        public var id: String { code }
    }

    public struct Regime: Sendable, Hashable { public let trend: Trend; public let strength: Int; public let text: String }
    public struct Shock: Sendable, Hashable { public let score: Int; public let level: ShockLevel; public let factors: [Factor] }
    public struct Reversal: Sendable, Hashable { public let score: Int; public let direction: Direction?; public let factors: [Factor] }
    public struct Policy: Sendable, Hashable {
        public let scalping: Scalping
        public let sizeMultiplier: Double
        public let stopMultiplier: Double
        public let notes: [String]
    }

    public struct Result: Sendable, Hashable {
        public let regime: Regime
        public let shock: Shock
        public let reversal: Reversal
        public let policy: Policy
    }

    // MARK: Thresholds (identical in guard.ts)

    public static let shockAgitated = 35
    public static let shockLevel = 65
    public static let reversalHigh = 50
    public static let minSamples = 20
    public static let unverifiableWeight = 0.75
    public static let fundingHot = 0.0003, fundingVeryHot = 0.0006, fundingCold = -0.0001, fundingVeryCold = -0.0003

    // MARK: Helpers

    static func mean(_ v: [Double]) -> Double { v.isEmpty ? 0 : v.reduce(0, +) / Double(v.count) }
    static func std(_ v: [Double]) -> Double {
        guard v.count >= 2 else { return 0 }
        let m = mean(v)
        return (v.reduce(0) { $0 + ($1 - m) * ($1 - m) } / Double(v.count - 1)).squareRoot()
    }
    static func lastValue(_ s: [Double?]) -> Double? { s.last { $0 != nil } ?? nil }
    static let fr = Locale(identifier: "fr_FR")
    static func one(_ v: Double) -> String { v.formatted(.number.precision(.fractionLength(0...1)).locale(fr)) }
    static func pct(_ v: Double) -> String { one(v) + " %" }
    /// JavaScript's Math.round (half up) for the scores and points.
    static func jsRound(_ x: Double) -> Int { Int((x + 0.5).rounded(.down)) }

    /// Percentile rank (0-100) of x among the values.
    public static func percentileRank(_ values: [Double], _ x: Double) -> Double {
        guard !values.isEmpty else { return 50 }
        let below = Double(values.filter { $0 < x }.count), equal = Double(values.filter { $0 == x }.count)
        return (below + 0.5 * equal) / Double(values.count) * 100
    }

    static func slice<T>(_ a: [T], _ from: Int, _ to: Int) -> [T] {
        let lo = max(0, min(from, a.count)), hi = max(lo, min(to, a.count))
        return Array(a[lo..<hi])
    }

    // MARK: 1. Regime

    public static func regime(daily raw: [Candle], h4 h4Raw: [Candle]) -> Regime {
        let d = raw.sanitized()
        let closes = d.map(\.close)
        guard closes.count >= 60 else {
            return Regime(trend: .range, strength: 0, text: "Historique journalier insuffisant pour juger la tendance de fond.")
        }
        let long = closes.count >= 200 ? 200 : 100
        let eL = Indicators.ema(closes, period: long), e50 = Indicators.ema(closes, period: 50)
        let close = closes.last!
        let m = lastValue(e50)!
        let l = lastValue(eL) ?? m
        let m10 = (e50.count >= 11 ? e50[e50.count - 11] : nil) ?? m
        let slope = m10 != 0 ? (m - m10) / m10 * 100 : 0
        let a = lastValue(Indicators.adx(d).adx) ?? 0
        var trend: Trend = .range
        if close > l && m > l && slope > 0 { trend = .up } else if close < l && m < l && slope < 0 { trend = .down }
        let h4 = h4Raw.sanitized().map(\.close)
        let f20 = lastValue(Indicators.ema(h4, period: 20)), f50 = lastValue(Indicators.ema(h4, period: 50))
        var aligned = false
        if let f20, let f50 { aligned = (trend == .up && f20 > f50) || (trend == .down && f20 < f50) }
        let strength = trend == .range ? jsRound(min(a, 40)) : jsRound(min(100, a * 2.5 + (aligned ? 10 : 0)))
        let text = trend == .range
            ? "Pas de tendance de fond nette (prix \(close > l ? "au-dessus" : "en dessous") de la moyenne \(long) jours, ADX \(jsRound(a))) : marché sans direction."
            : "Tendance de fond \(trend == .up ? "haussière" : "baissière") (moyennes 50 et \(long) jours alignées, ADX \(jsRound(a)))\(aligned ? ", confirmée en 4 h" : ", pas encore confirmée en 4 h")."
        return Regime(trend: trend, strength: strength, text: text)
    }

    // MARK: News tone

    static let negative = [
        "hack", "hacked", "exploit", "breach", "stolen", "lawsuit", "sues", "sued", "sec charges", "investigation", "probe", "fraud",
        "bankrupt", "bankruptcy", "insolvency", "liquidation", "liquidated", "delist", "ban", "banned", "crackdown", "crash", "plunge",
        "plunges", "tumble", "tumbles", "sell-off", "selloff", "downgrade", "downgraded", "misses", "miss estimates", "cuts guidance",
        "layoffs", "recall", "halt", "halted", "outage", "default", "warning", "subpoena", "indictment", "sanction",
        "piratage", "faillite", "enquête", "plainte", "effondrement", "chute", "interdiction", "fraude",
    ]
    static let positive = [
        "approval", "approved", "approves", "etf inflows", "record high", "all-time high", "surge", "surges", "soars", "rally",
        "rallies", "upgrade", "upgraded", "beats", "beat estimates", "raises guidance", "buyback", "partnership", "adoption",
        "launch", "launches", "breakthrough", "acquisition", "wins",
        "approbation", "hausse", "partenariat", "rachat",
    ]
    static let toneRegex: (neg: [NSRegularExpression], pos: [NSRegularExpression]) = {
        let make = { (w: String) in try! NSRegularExpression(pattern: "[^a-zà-ü]\(NSRegularExpression.escapedPattern(for: w))[^a-zà-ü]") }
        return (negative.map(make), positive.map(make))
    }()

    public static func newsTone(_ items: [NewsItem]) -> (negative: Int, positive: Int) {
        var neg = 0, pos = 0
        for it in items {
            let t = " \(it.title.lowercased()) "
            let range = NSRange(t.startIndex..., in: t)
            let n = toneRegex.neg.contains { $0.firstMatch(in: t, range: range) != nil }
            let p = toneRegex.pos.contains { $0.firstMatch(in: t, range: range) != nil }
            if n && !p { neg += 1 } else if p && !n { pos += 1 }
        }
        return (neg, pos)
    }

    // MARK: Evidence

    public static func weigh(_ e: Evidence?, historical: Bool) -> (weight: Double, status: FactorStatus) {
        guard historical else { return (unverifiableWeight, .unverifiable) }
        guard let e, e.samples >= minSamples else { return (0.5, .unproven) }
        if e.lift < 1.1 { return (0, .rejected) }
        return (min(1, max(0.2, (e.lift - 1) / 0.5)), .verified)
    }

    struct Raw { let code: String; let points: Int; let text: String }

    static func finalize(_ raw: [Raw], _ evidence: [String: Evidence], historical: (String) -> Bool) -> [Factor] {
        raw.map { f in
            let e = evidence[f.code]
            let w = weigh(e, historical: historical(f.code))
            return Factor(code: f.code, points: jsRound(Double(f.points) * w.weight), basePoints: f.points, text: f.text, status: w.status, evidence: e)
        }
    }

    struct Tally {
        var stats: [String: (n: Int, hit: Int)] = [:]
        var order: [String] = []
        mutating func add(_ key: String, _ hit: Bool) {
            if stats[key] == nil { order.append(key) }
            var s = stats[key] ?? (0, 0)
            s.n += 1
            if hit { s.hit += 1 }
            stats[key] = s
        }
        var evidence: [String: Evidence] {
            guard let base = stats["_base"], base.n > 0 else { return [:] }
            let b = Double(base.hit) / Double(base.n) * 100
            var out: [String: Evidence] = [:]
            for (k, v) in stats where k != "_base" {
                let rate = Double(v.hit) / Double(v.n) * 100
                out[k] = Evidence(samples: v.n, rate: rate, base: b, lift: b > 0 ? rate / b : 0)
            }
            return out
        }
    }

    // MARK: 2. Shock

    /// Hourly factors at candle i (r[j] = log(close j+1 / close j)), no look-ahead.
    static func hourlyShockFactors(_ h1: [Candle], _ r: [Double], _ i: Int) -> [Raw] {
        var f: [Raw] = []
        guard i >= 96 else { return f }
        let sLong = std(slice(r, max(0, i - 120), i - 24)), sNow = std(slice(r, i - 24, i))
        let ratio = sLong > 0 ? sNow / sLong : 1
        if ratio >= 2 { f.append(Raw(code: "vol2", points: 35, text: "Volatilité des dernières 24 h \(one(ratio)) fois supérieure à la normale.")) }
        else if ratio >= 1.5 { f.append(Raw(code: "vol15", points: 20, text: "Volatilité des dernières 24 h \(one(ratio)) fois supérieure à la normale.")) }
        let jump = sLong > 0 ? slice(r, i - 6, i).map(abs).max()! / sLong : 0
        if jump >= 4 { f.append(Raw(code: "jump4", points: 35, text: "Saut de prix de \(one(jump)) écarts-types en une heure (mouvement anormal).")) }
        else if jump >= 3 { f.append(Raw(code: "jump3", points: 20, text: "Mouvement horaire de \(one(jump)) écarts-types, inhabituel.")) }
        let vols = slice(h1, max(0, i - 100), i - 2).map(\.volume)
        let vNow = mean(slice(h1, i - 2, i + 1).map(\.volume))
        let z = std(vols) > 0 ? (vNow - mean(vols)) / std(vols) : 0
        if z >= 3 { f.append(Raw(code: "volume", points: 15, text: "Volume des 3 dernières heures très au-dessus de la normale (\(one(z)) écarts-types).")) }
        return f
    }

    static func bandwidth(_ closes: [Double]) -> [Double?] {
        closes.indices.map { i in
            guard i >= 19 else { return nil }
            let w = Array(closes[(i - 19)...i]), m = mean(w)
            return m > 0 ? 4 * std(w) / m : nil
        }
    }

    static func squeezeFactor(_ bw: [Double?], _ i: Int) -> [Raw] {
        guard let now = bw[i] else { return [] }
        let hist = slice(bw, max(0, i - 200), i).compactMap { $0 }
        guard hist.count >= 100 else { return [] }
        let rank = percentileRank(hist, now)
        return rank <= 10
            ? [Raw(code: "squeeze", points: 15, text: "Volatilité anormalement comprimée (bandes de Bollinger 4 h plus étroites que \(jsRound(100 - rank)) % du temps) : un mouvement brutal peut suivre, sans direction connue.")]
            : []
    }

    public static func shockEvidence(h1 h1Raw: [Candle], h4 h4Raw: [Candle]) -> [String: Evidence] {
        var out: [String: Evidence] = [:]
        let h1 = h1Raw.sanitized()
        let r = h1.count > 1 ? (1..<h1.count).map { log(h1[$0].close / h1[$0 - 1].close) } : []
        var t1 = Tally()
        if r.count > 102 {
            for i in 96..<(r.count - 6) {
                let sLong = std(slice(r, max(0, i - 120), i - 24))
                guard sLong > 0 else { continue }
                let hit = slice(r, i, i + 6).map(abs).max()! >= 3 * sLong
                t1.add("_base", hit)
                for f in hourlyShockFactors(h1, r, i) { t1.add(f.code, hit) }
            }
        }
        out.merge(t1.evidence) { a, _ in a }
        let h4 = h4Raw.sanitized()
        let bw = bandwidth(h4.map(\.close)), a = Indicators.atr(h4)
        var t4 = Tally()
        if h4.count > 126 {
            for i in 120..<(h4.count - 6) {
                guard let at = a[i], at != 0 else { continue }
                let next = Array(h4[(i + 1)...(i + 6)])
                let hit = next.map(\.high).max()! - next.map(\.low).min()! >= 4 * at
                t4.add("_base", hit)
                for f in squeezeFactor(bw, i) { t4.add(f.code, hit) }
            }
        }
        if let sq = t4.evidence["squeeze"] { out["squeeze"] = sq }
        return out
    }

    public static func shock(_ input: Input) -> Shock {
        let h1 = input.h1.sanitized(), h4 = input.h4.sanitized()
        let r = h1.count > 1 ? (1..<h1.count).map { log(h1[$0].close / h1[$0 - 1].close) } : []
        var raw: [Raw] = []
        if r.count >= 96 { raw += hourlyShockFactors(h1, r, r.count) }
        if h4.count >= 120 {
            let bw = bandwidth(h4.map(\.close))
            raw += squeezeFactor(bw, bw.count - 1)
        }
        let now = input.now
        let week = input.news.filter { $0.time <= now && $0.time >= now.addingTimeInterval(-7 * 86_400) }
        if week.count >= 5 {
            let recent = week.filter { $0.time >= now.addingTimeInterval(-6 * 3600) }.count
            let avg = Double(week.count) / 28
            let burst = avg > 0 ? Double(recent) / avg : 0
            if burst >= 3 && recent >= 4 { raw.append(Raw(code: "newsBurst", points: 20, text: "Rafale d'actualités : \(recent) articles en 6 h, \(one(burst)) fois plus que d'habitude.")) }
            else if burst >= 2 && recent >= 3 { raw.append(Raw(code: "newsBusy", points: 10, text: "Actualité plus chargée que d'habitude (\(recent) articles en 6 h).")) }
        }
        if input.kind == .stock, input.vix.count >= 2 {
            let v = input.vix.last!, prev = input.vix[input.vix.count - 2]
            if v >= 30 { raw.append(Raw(code: "vixHigh", points: 20, text: "Peur généralisée sur les marchés (VIX à \(one(v))).")) }
            if prev > 0 && v / prev - 1 >= 0.2 { raw.append(Raw(code: "vixJump", points: 15, text: "Le VIX a bondi de \(pct((v / prev - 1) * 100)) en une séance.")) }
        }
        let historical: Set<String> = ["vol2", "vol15", "jump4", "jump3", "volume", "squeeze"]
        let factors = finalize(raw, raw.isEmpty ? [:] : shockEvidence(h1: h1, h4: h4)) { historical.contains($0) }
        let score = min(100, factors.reduce(0) { $0 + $1.points })
        let level: ShockLevel = score >= shockLevel ? .shock : score >= shockAgitated ? .agitated : .calm
        return Shock(score: score, level: level, factors: factors)
    }

    // MARK: 3. Reversal

    /// Swing highs / lows: index whose value is the extreme of `k` bars on each side.
    public static func pivots(_ values: [Double], high: Bool, k: Int = 3, until: Int? = nil) -> [Int] {
        let u = until ?? values.count - 1
        guard u - k >= k else { return [] }
        return (k...(u - k)).filter { i in
            (i - k...i + k).allSatisfy { j in j == i || (high ? values[i] > values[j] : values[i] < values[j]) }
        }
    }

    public static func divergence(_ c: [Candle], _ rsi: [Double?], _ direction: Direction, until: Int? = nil, window: Int = 60) -> Bool {
        let u = until ?? c.count - 1
        let from = max(0, u - window)
        let values = direction == .down ? c.map(\.high) : c.map(\.low)
        let p = pivots(values, high: direction == .down, k: 3, until: u).filter { $0 >= from }
        guard p.count >= 2 else { return false }
        let a = p[p.count - 2], b = p[p.count - 1]
        guard let ra = rsi[a], let rb = rsi[b] else { return false }
        return direction == .down ? values[b] > values[a] && rb < ra - 3 : values[b] < values[a] && rb > ra + 3
    }

    static func technicalReversal(_ c: [Candle], _ r: [Double?], _ e20: [Double?], _ a: [Double?], _ i: Int,
                                  _ direction: Direction, _ tf: String) -> [Raw] {
        var f: [Raw] = []
        let up = direction == .down
        let label = tf == "4h" ? "4 h" : "journalier"
        let (hi, lo): (Double, Double) = tf == "4h" ? (80, 20) : (75, 25)
        if let rv = r[i], up ? rv > hi : rv < lo {
            f.append(Raw(code: "rsi\(tf)", points: tf == "4h" ? 10 : 15, text: "RSI \(label) extrême (\(jsRound(rv)))."))
        }
        if divergence(c, r, direction, until: i) {
            f.append(Raw(code: "div\(tf)", points: 20, text: "Divergence \(up ? "baissière" : "haussière") en \(label) : le prix fait un nouveau \(up ? "sommet" : "creux") mais pas le RSI (essoufflement)."))
        }
        if let m = e20[i], let at = a[i], at > 0 {
            let ext = (c[i].close - m) / at
            if up ? ext > 3 : ext < -3 {
                f.append(Raw(code: "extension\(tf)", points: 15, text: "Prix très éloigné de sa moyenne 20 périodes en \(label) (\(one(abs(ext))) ATR)."))
            }
        }
        let k = c[i]
        let body = abs(k.close - k.open) == 0 ? k.close * 1e-6 : abs(k.close - k.open)
        let wick = up ? k.high - max(k.open, k.close) : min(k.open, k.close) - k.low
        let vols = slice(c, max(0, i - 50), i).map(\.volume)
        let z = std(vols) > 0 ? (k.volume - mean(vols)) / std(vols) : 0
        if wick > 2 * body && z > 2 {
            f.append(Raw(code: "rejection\(tf)", points: 10, text: "Bougie de rejet sur fort volume en \(label) (longue mèche \(up ? "haute" : "basse"))."))
        }
        return f
    }

    public static func reversalEvidence(_ raw: [Candle], tf: String) -> [String: Evidence] {
        let c = raw.sanitized()
        let horizon = tf == "4h" ? 18 : 5
        guard c.count >= 100 else { return [:] }
        let closes = c.map(\.close)
        let r = Indicators.rsi(closes), e20 = Indicators.ema(closes, period: 20), e50 = Indicators.ema(closes, period: 50), a = Indicators.atr(c)
        var t = Tally()
        for i in 60..<(c.count - horizon) {
            guard let m20 = e20[i], let m50 = e50[i], let at = a[i], at != 0 else { continue }
            let direction: Direction = m20 > m50 ? .down : .up
            let future = Array(c[(i + 1)...(i + horizon)])
            let hit = direction == .down
                ? future.map(\.low).min()! <= c[i].close - 3 * at
                : future.map(\.high).max()! >= c[i].close + 3 * at
            t.add("_base", hit)
            for f in technicalReversal(c, r, e20, a, i, direction, tf) { t.add(f.code, hit) }
        }
        return t.evidence
    }

    public static func reversal(_ input: Input, trend: Trend) -> Reversal {
        let d = input.daily.sanitized(), h4 = input.h4.sanitized()
        let closesD = d.map(\.close)
        var direction: Direction? = trend == .up ? .down : trend == .down ? .up : nil
        if direction == nil, closesD.count > 6 {
            let move = closesD.last! / closesD[closesD.count - 6] - 1
            if abs(move) >= 0.03 { direction = move > 0 ? .down : .up }
        }
        guard let direction else { return Reversal(score: 0, direction: nil, factors: []) }
        let up = direction == .down
        var raw: [Raw] = []
        if d.count > 60 {
            raw += technicalReversal(d, Indicators.rsi(closesD), Indicators.ema(closesD, period: 20), Indicators.atr(d), d.count - 1, direction, "1d")
        }
        if h4.count > 60 {
            let c4 = h4.map(\.close)
            raw += technicalReversal(h4, Indicators.rsi(c4), Indicators.ema(c4, period: 20), Indicators.atr(h4), h4.count - 1, direction, "4h")
        }

        let p = input.positioning
        if let fr = p?.fundingRate {
            let frText = (fr * 100).formatted(.number.precision(.fractionLength(0...4)).locale(fr_)) + " % par 8 h"
            if up && fr >= fundingVeryHot { raw.append(Raw(code: "funding", points: 25, text: "Financement des contrats perpétuels très élevé (\(frText)) : les acheteurs à levier sont surchargés, risque de liquidations en cascade.")) }
            else if up && fr >= fundingHot { raw.append(Raw(code: "funding", points: 15, text: "Financement élevé (\(frText)) : beaucoup d'acheteurs à levier.")) }
            else if !up && fr <= fundingVeryCold { raw.append(Raw(code: "funding", points: 25, text: "Financement très négatif (\(frText)) : les vendeurs à découvert sont surchargés, risque de rachat brutal (short squeeze).")) }
            else if !up && fr <= fundingCold { raw.append(Raw(code: "funding", points: 15, text: "Financement négatif (\(frText)) : beaucoup de vendeurs à découvert.")) }
        }
        let ls = p?.longShortRatio ?? []
        if ls.count >= 24 {
            let rank = percentileRank(Array(ls.dropLast()), ls.last!)
            if up && rank >= 90 { raw.append(Raw(code: "longShort", points: 10, text: "Ratio acheteurs/vendeurs à \(one(ls.last!)), parmi les plus hauts du mois : la foule est déjà acheteuse.")) }
            if !up && rank <= 10 { raw.append(Raw(code: "longShort", points: 10, text: "Ratio acheteurs/vendeurs à \(one(ls.last!)), parmi les plus bas du mois : la foule est déjà vendeuse.")) }
        }
        let oi = p?.openInterest ?? []
        if oi.count >= 25 && h4.count > 7 {
            let oiChange = oi.last! / oi[oi.count - 25] - 1
            let priceChange = abs(h4.last!.close / h4[h4.count - 7].close - 1)
            if oiChange >= 0.1 && priceChange < 0.01 {
                raw.append(Raw(code: "openInterest", points: 10, text: "Positions à levier en hausse de \(pct(oiChange * 100)) en 24 h sans que le prix avance : situation fragile."))
            }
        }

        if let v = input.sentiment?.fearGreed.last {
            let s = String(Int(v))
            if up && v >= 80 { raw.append(Raw(code: "fearGreed", points: 15, text: "Avidité extrême (Fear & Greed \(s)) : historiquement proche des sommets.")) }
            else if up && v >= 75 { raw.append(Raw(code: "fearGreed", points: 8, text: "Forte avidité (Fear & Greed \(s)).")) }
            else if !up && v <= 20 { raw.append(Raw(code: "fearGreed", points: 15, text: "Peur extrême (Fear & Greed \(s)) : historiquement proche des creux.")) }
            else if !up && v <= 25 { raw.append(Raw(code: "fearGreed", points: 8, text: "Forte peur (Fear & Greed \(s)).")) }
        }
        if let sb = input.sentiment?.socialBullish, (input.sentiment?.socialSample ?? 0) >= 20 {
            if up && sb >= 85 { raw.append(Raw(code: "social", points: 10, text: "Réseaux sociaux quasi unanimement optimistes (\(jsRound(sb)) % haussiers sur StockTwits).")) }
            if !up && sb <= 30 { raw.append(Raw(code: "social", points: 10, text: "Réseaux sociaux très pessimistes (\(jsRound(sb)) % haussiers seulement sur StockTwits).")) }
        }

        let now = input.now
        let tone = newsTone(input.news.filter { $0.time >= now.addingTimeInterval(-86_400) && $0.time <= now })
        if up && tone.negative >= 2 && tone.negative > tone.positive {
            raw.append(Raw(code: "newsTone", points: 15, text: "\(tone.negative) actualités négatives en 24 h alors que la tendance est haussière."))
        }
        if !up && tone.positive >= 2 && tone.positive > tone.negative {
            raw.append(Raw(code: "newsTone", points: 15, text: "\(tone.positive) actualités positives en 24 h alors que la tendance est baissière."))
        }

        let isTechnical = { (code: String) in code.hasSuffix("4h") || code.hasSuffix("1d") }
        var evidence: [String: Evidence] = [:]
        if raw.contains(where: { $0.code.hasSuffix("1d") }) { evidence.merge(reversalEvidence(d, tf: "1d")) { a, _ in a } }
        if raw.contains(where: { $0.code.hasSuffix("4h") }) { evidence.merge(reversalEvidence(h4, tf: "4h")) { _, b in b } }
        let factors = finalize(raw, evidence, historical: isTechnical)
        return Reversal(score: min(100, factors.reduce(0) { $0 + $1.points }), direction: direction, factors: factors)
    }
    static let fr_ = Locale(identifier: "fr_FR")

    // MARK: Everything together

    public static func evaluate(_ input: Input) -> Result {
        let reg = regime(daily: input.daily, h4: input.h4)
        let sh = shock(input)
        let rev = reversal(input, trend: reg.trend)
        var notes: [String] = []
        var scalping = Scalping.ok, size = 1.0, stop = 1.0
        if sh.level == .shock {
            (scalping, size, stop) = (.pause, 0, 2)
            notes.append("Marché en choc : suspendre les nouvelles positions à court terme, laisser passer la tempête.")
        } else if sh.level == .agitated {
            (scalping, size, stop) = (.reduce, 0.5, 1.5)
            notes.append("Marché agité : diviser la taille des positions par deux et élargir les stops (×1,5) pour ne pas être sorti par le bruit.")
        }
        if rev.score >= reversalHigh, let dir = rev.direction {
            if scalping == .ok { (scalping, size, stop) = (.reduce, 0.5, 1) }
            notes.append("Risque de retournement à la \(dir == .down ? "baisse" : "hausse") élevé : ne pas ouvrir de position dans le sens de la tendance actuelle, resserrer les stops des positions existantes.")
        }
        if notes.isEmpty { notes.append("Conditions normales : pas de signal de choc ni de retournement.") }
        if reg.trend != .range {
            notes.append("Pour le long terme : \(reg.trend == .up ? "privilégier les achats sur repli" : "privilégier la prudence, les rebonds sont fragiles") tant que la tendance de fond tient.")
        }
        return Result(regime: reg, shock: sh, reversal: rev,
                      policy: Policy(scalping: scalping, sizeMultiplier: size, stopMultiplier: stop, notes: notes))
    }
}
