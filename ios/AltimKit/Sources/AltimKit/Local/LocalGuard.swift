import Foundation

/// Market guard computed on the iPhone (core/src/engine/guard.rs): what a short-term decision cannot see on its own.
/// 1. Regime — the background trend (daily + 4 h);
/// 2. Shock — abnormal volatility, price jumps, volume spikes, compression before a breakout, news bursts;
/// 3. Reversal — a move against the trend: technical exhaustion, crowd positioning, sentiment extremes, news tone.
/// Every technical factor is weighed by how it behaved on the asset's own history.
enum GuardEngine {
    struct Headline { var title: String; var time: Double; var source: String? }

    struct Input {
        var kind: Kind
        var daily: [Bar], h4: [Bar], h1: [Bar]
        /// Perpetual funding per 8 h (fraction) and Fear & Greed (oldest first), cryptos only.
        var fundingRate: Double?
        var fearGreed: [Double] = []
        var news: [Headline] = []
        var now: Double
    }

    struct Raw { var code: String; var points: Double; var text: String }

    static let shockAgitated = 35.0, shockLevel = 65.0, reversalHigh = 50.0, minSamples = 20, unverifiableWeight = 0.75
    static let fundingHot = 0.0003, fundingVeryHot = 0.0006, fundingCold = -0.0001, fundingVeryCold = -0.0003

    // MARK: Helpers

    static func mean(_ v: ArraySlice<Double>) -> Double { v.isEmpty ? 0 : v.reduce(0, +) / Double(v.count) }
    static func std(_ v: ArraySlice<Double>) -> Double {
        guard v.count >= 2 else { return 0 }
        let m = mean(v)
        return (v.reduce(0) { $0 + ($1 - m) * ($1 - m) } / Double(v.count - 1)).squareRoot()
    }
    static func slice<T>(_ a: [T], _ start: Int, _ end: Int) -> ArraySlice<T> {
        let s = min(max(start, 0), a.count), e = min(max(end, 0), a.count)
        return s >= e ? a[0..<0] : a[s..<e]
    }
    static func one(_ v: Double) -> String { JSFormat.fr(v, min: 0, max: 1) }
    static func pct(_ v: Double) -> String { "\(JSFormat.fr(v, min: 0, max: 1)) %" }
    static func lastValue(_ s: [Double?]) -> Double? { s.last { $0 != nil } ?? nil }
    static func at(_ s: [Double?], _ i: Int) -> Double? { i >= 0 && i < s.count ? s[i] : nil }
    static func truthy(_ v: Double?) -> Double? { v.flatMap { $0 != 0 && !$0.isNaN ? $0 : nil } }

    static func percentileRank(_ values: ArraySlice<Double>, _ x: Double) -> Double {
        guard !values.isEmpty else { return 50 }
        let below = Double(values.filter { $0 < x }.count), equal = Double(values.filter { $0 == x }.count)
        return (below + 0.5 * equal) / Double(values.count) * 100
    }

    // MARK: 1. Regime

    static func regime(_ dailyRaw: [Bar], _ h4Raw: [Bar]) -> GuardReport.Regime {
        let d = Indicators.sanitize(dailyRaw)
        let closes = d.map(\.close)
        guard closes.count >= 60 else {
            return .init(trend: "range", strength: 0, text: "Historique journalier insuffisant pour juger la tendance de fond.")
        }
        let long = closes.count >= 200 ? 200 : 100
        let eL = Indicators.ema(closes, long), e50 = Indicators.ema(closes, 50)
        let close = closes[closes.count - 1]
        let l = lastValue(eL) ?? lastValue(e50) ?? .nan
        let m = lastValue(e50) ?? .nan
        let m10 = at(e50, e50.count - 11) ?? m
        let slope = truthy(m10) != nil ? (m - m10) / m10 * 100 : 0
        let a = lastValue(Indicators.adx(d, 14)) ?? 0
        var trend = "range"
        if close > l && m > l && slope > 0 { trend = "up" } else if close < l && m < l && slope < 0 { trend = "down" }
        let h4 = Indicators.sanitize(h4Raw).map(\.close)
        let aligned: Bool
        if let f20 = lastValue(Indicators.ema(h4, 20)), let f50 = lastValue(Indicators.ema(h4, 50)) {
            aligned = (trend == "up" && f20 > f50) || (trend == "down" && f20 < f50)
        } else { aligned = false }
        let strength = trend == "range" ? min(a, 40).rounded() : min(100, a * 2.5 + (aligned ? 10 : 0)).rounded()
        let text = trend == "range"
            ? "Pas de tendance de fond nette (prix \(close > l ? "au-dessus" : "en dessous") de la moyenne \(long) jours, ADX \(Int(a.rounded()))) : marché sans direction."
            : "Tendance de fond \(trend == "up" ? "haussière" : "baissière") (moyennes 50 et \(long) jours alignées, ADX \(Int(a.rounded())))\(aligned ? ", confirmée en 4 h" : ", pas encore confirmée en 4 h")."
        return .init(trend: trend, strength: strength, text: text)
    }

    // MARK: Evidence

    static func weigh(_ e: Evidence?, historical: Bool) -> (Double, String) {
        guard historical else { return (unverifiableWeight, "unverifiable") }
        guard let e, e.samples >= minSamples else { return (0.5, "unproven") }
        return e.lift < 1.1 ? (0, "rejected") : (min(1, max(0.2, (e.lift - 1) / 0.5)), "verified")
    }

    static func finalize(_ raw: [Raw], _ evidence: [String: Evidence], historical: (String) -> Bool) -> [GuardFactor] {
        raw.map { f in
            let e = evidence[f.code]
            let (w, status) = weigh(e, historical: historical(f.code))
            return GuardFactor(code: f.code, points: (f.points * w).rounded(), basePoints: f.points, text: f.text, status: status, evidence: e)
        }
    }

    struct Stats {
        var counts: [String: (n: Int, hit: Int)] = [:]
        mutating func tally(_ key: String, _ hit: Bool) {
            var s = counts[key] ?? (0, 0)
            s.n += 1
            if hit { s.hit += 1 }
            counts[key] = s
        }
        var evidence: [String: Evidence] {
            guard let b = counts["_base"], b.n > 0 else { return [:] }
            let base = Double(b.hit) / Double(b.n) * 100
            var out: [String: Evidence] = [:]
            for (k, v) in counts where k != "_base" {
                let rate = Double(v.hit) / Double(v.n) * 100
                out[k] = Evidence(samples: v.n, rate: rate, base: base, lift: base > 0 ? rate / base : 0)
            }
            return out
        }
    }

    // MARK: 2. Shock

    static func hourlyShock(_ h1: [Bar], _ r: [Double], _ i: Int) -> [Raw] {
        var f: [Raw] = []
        guard i >= 96 else { return f }
        let sLong = std(slice(r, i - 120, i - 24)), sNow = std(slice(r, i - 24, i))
        let ratio = sLong > 0 ? sNow / sLong : 1
        if ratio >= 2 { f.append(.init(code: "vol2", points: 35, text: "Volatilité des dernières 24 h \(one(ratio)) fois supérieure à la normale.")) }
        else if ratio >= 1.5 { f.append(.init(code: "vol15", points: 20, text: "Volatilité des dernières 24 h \(one(ratio)) fois supérieure à la normale.")) }
        let jump = sLong > 0 ? (slice(r, i - 6, i).map(abs).max() ?? 0) / sLong : 0
        if jump >= 4 { f.append(.init(code: "jump4", points: 35, text: "Saut de prix de \(one(jump)) écarts-types en une heure (mouvement anormal).")) }
        else if jump >= 3 { f.append(.init(code: "jump3", points: 20, text: "Mouvement horaire de \(one(jump)) écarts-types, inhabituel.")) }
        let vols = slice(h1, i - 100, i - 2).map(\.volume)
        let vNow = mean(slice(h1, i - 2, i + 1).map(\.volume)[...])
        let sv = std(vols[...])
        let z = sv > 0 ? (vNow - mean(vols[...])) / sv : 0
        if z >= 3 { f.append(.init(code: "volume", points: 15, text: "Volume des 3 dernières heures très au-dessus de la normale (\(one(z)) écarts-types).")) }
        return f
    }

    static func bandwidth(_ closes: [Double]) -> [Double?] {
        closes.indices.map { i in
            guard i >= 19 else { return nil }
            let w = closes[(i - 19)...i]
            let m = mean(w)
            return m > 0 ? 4 * std(w) / m : nil
        }
    }

    static func squeeze(_ bw: [Double?], _ i: Int) -> [Raw] {
        guard let now = at(bw, i) else { return [] }
        let hist = slice(bw, i - 200, i).compactMap { $0 }
        guard hist.count >= 100 else { return [] }
        let rank = percentileRank(hist[...], now)
        guard rank <= 10 else { return [] }
        return [.init(code: "squeeze", points: 15, text: "Volatilité anormalement comprimée (bandes de Bollinger 4 h plus étroites que \(Int((100 - rank).rounded())) % du temps) : un mouvement brutal peut suivre, sans direction connue.")]
    }

    static func logReturns(_ h1: [Bar]) -> [Double] { h1.indices.dropFirst().map { log(h1[$0].close / h1[$0 - 1].close) } }

    static func shockEvidence(_ h1: [Bar], _ h4: [Bar]) -> [String: Evidence] {
        let r = logReturns(h1)
        var s1 = Stats()
        var i = 96
        while i + 6 < r.count {
            let sLong = std(slice(r, i - 120, i - 24))
            if sLong > 0 {
                let hit = (r[i..<(i + 6)].map(abs).max() ?? 0) >= 3 * sLong
                s1.tally("_base", hit)
                for f in hourlyShock(h1, r, i) { s1.tally(f.code, hit) }
            }
            i += 1
        }
        var out = s1.evidence
        let bw = bandwidth(h4.map(\.close))
        let a = Indicators.atr(h4, 14)
        var s4 = Stats()
        i = 120
        while i + 6 < h4.count {
            if let at = truthy(a[i]) {
                let next = h4[(i + 1)...(i + 6)]
                let hit = (next.map(\.high).max() ?? 0) - (next.map(\.low).min() ?? 0) >= 4 * at
                s4.tally("_base", hit)
                for f in squeeze(bw, i) { s4.tally(f.code, hit) }
            }
            i += 1
        }
        if let e = s4.evidence["squeeze"] { out["squeeze"] = e }
        return out
    }

    static func shock(_ input: Input) -> GuardReport.Shock {
        let h1 = Indicators.sanitize(input.h1), h4 = Indicators.sanitize(input.h4)
        let r = logReturns(h1)
        var raw: [Raw] = []
        if r.count >= 96 { raw += hourlyShock(h1, r, r.count) }
        if h4.count >= 120 {
            let bw = bandwidth(h4.map(\.close))
            raw += squeeze(bw, bw.count - 1)
        }
        // News burst: headlines of the last 6 h vs the average 6 h of the period the feeds cover (a week at most; the
        // feeds read by the iPhone keep 48 h, a 7-day average would make every day look like a burst).
        let now = input.now
        let news = input.news.filter { $0.time <= now && $0.time >= now - 7 * 86_400_000 }
        if news.count >= 5 {
            let recent = news.filter { $0.time >= now - 6 * 3_600_000 }.count
            let span = max(24 * 3_600_000, now - (news.map(\.time).min() ?? now))
            let avg = Double(news.count) / (span / (6 * 3_600_000))
            let burst = avg > 0 ? Double(recent) / avg : 0
            if burst >= 3 && recent >= 4 {
                raw.append(.init(code: "newsBurst", points: 20, text: "Rafale d'actualités : \(recent) articles en 6 h, \(one(burst)) fois plus que d'habitude."))
            } else if burst >= 2 && recent >= 3 {
                raw.append(.init(code: "newsBusy", points: 10, text: "Actualité plus chargée que d'habitude (\(recent) articles en 6 h)."))
            }
        }
        let historical: Set<String> = ["vol2", "vol15", "jump4", "jump3", "volume", "squeeze", "macro"]
        let evidence = raw.isEmpty ? [:] : shockEvidence(h1, h4)
        let factors = finalize(raw, evidence) { historical.contains($0) }
        let score = min(100, factors.reduce(0) { $0 + $1.points })
        return .init(score: score, level: score >= shockLevel ? "shock" : score >= shockAgitated ? "agitated" : "calm", factors: factors)
    }

    // MARK: 3. Reversal

    static func pivots(_ values: [Double], high: Bool, k: Int, until: Int) -> [Int] {
        var out: [Int] = []
        var i = k
        while i <= until - k {
            var ok = true
            var j = i - k
            while j <= i + k && ok {
                if j != i { ok = high ? values[i] > values[j] : values[i] < values[j] }
                j += 1
            }
            if ok { out.append(i) }
            i += 1
        }
        return out
    }

    static func divergence(_ c: [Bar], _ rsi: [Double?], down: Bool, until: Int, window: Int = 60) -> Bool {
        let from = max(0, until - window)
        let values = c.map { down ? $0.high : $0.low }
        let p = pivots(values, high: down, k: 3, until: until).filter { $0 >= from }
        guard p.count >= 2, let ra = at(rsi, p[p.count - 2]), let rb = at(rsi, p[p.count - 1]) else { return false }
        let (a, b) = (p[p.count - 2], p[p.count - 1])
        return down ? values[b] > values[a] && rb < ra - 3 : values[b] < values[a] && rb > ra + 3
    }

    /// Technical reversal factors at candle i (data up to i only), on 4 h or daily candles.
    static func technical(_ c: [Bar], _ r: [Double?], _ e20: [Double?], _ a: [Double?], _ i: Int, down: Bool, four: Bool) -> [Raw] {
        var f: [Raw] = []
        let tf = four ? "4h" : "1d", label = four ? "4 h" : "journalier"
        let (hi, lo) = four ? (80.0, 20.0) : (75.0, 25.0)
        if let rv = at(r, i), down ? rv > hi : rv < lo {
            f.append(.init(code: "rsi\(tf)", points: four ? 10 : 15, text: "RSI \(label) extrême (\(Int(rv.rounded())))."))
        }
        if divergence(c, r, down: down, until: i) {
            f.append(.init(code: "div\(tf)", points: 20, text: "Divergence \(down ? "baissière" : "haussière") en \(label) : le prix fait un nouveau \(down ? "sommet" : "creux") mais pas le RSI (essoufflement)."))
        }
        if let m = at(e20, i), let atr = truthy(at(a, i)), atr > 0 {
            let ext = (c[i].close - m) / atr
            if down ? ext > 3 : ext < -3 {
                f.append(.init(code: "extension\(tf)", points: 15, text: "Prix très éloigné de sa moyenne 20 périodes en \(label) (\(one(abs(ext))) ATR)."))
            }
        }
        let k = c[i]
        let body = truthy(abs(k.close - k.open)) ?? k.close * 1e-6
        let wick = down ? k.high - max(k.open, k.close) : min(k.open, k.close) - k.low
        let vols = slice(c, i - 50, i).map(\.volume)
        let sv = std(vols[...])
        let z = sv > 0 ? (k.volume - mean(vols[...])) / sv : 0
        if wick > 2 * body && z > 2 {
            f.append(.init(code: "rejection\(tf)", points: 10, text: "Bougie de rejet sur fort volume en \(label) (longue mèche \(down ? "haute" : "basse"))."))
        }
        return f
    }

    static func reversalEvidence(_ c: [Bar], four: Bool) -> [String: Evidence] {
        let horizon = four ? 18 : 5
        guard c.count >= 100 else { return [:] }
        let closes = c.map(\.close)
        let r = Indicators.rsi(closes, 14), e20 = Indicators.ema(closes, 20), e50 = Indicators.ema(closes, 50), a = Indicators.atr(c, 14)
        var stats = Stats()
        var i = 60
        while i + horizon < c.count {
            if let m20 = e20[i], let m50 = e50[i], let at = truthy(a[i]) {
                let down = m20 > m50
                let future = c[(i + 1)...(i + horizon)]
                let hit = down ? (future.map(\.low).min() ?? .infinity) <= c[i].close - 3 * at
                    : (future.map(\.high).max() ?? 0) >= c[i].close + 3 * at
                stats.tally("_base", hit)
                for f in technical(c, r, e20, a, i, down: down, four: four) { stats.tally(f.code, hit) }
            }
            i += 1
        }
        return stats.evidence
    }

    static let negativeWords = ["hack", "hacked", "exploit", "breach", "stolen", "lawsuit", "sues", "sued", "sec charges", "investigation", "probe", "fraud",
                                "bankrupt", "bankruptcy", "insolvency", "liquidation", "liquidated", "delist", "ban", "banned", "crackdown", "crash",
                                "plunge", "plunges", "tumble", "tumbles", "sell-off", "selloff", "downgrade", "downgraded", "misses", "miss estimates",
                                "cuts guidance", "layoffs", "recall", "halt", "halted", "outage", "default", "warning", "subpoena", "indictment",
                                "sanction", "piratage", "faillite", "enquête", "plainte", "effondrement", "chute", "interdiction", "fraude"]
    static let positiveWords = ["approval", "approved", "approves", "etf inflows", "record high", "all-time high", "surge", "surges", "soars", "rally",
                                "rallies", "upgrade", "upgraded", "beats", "beat estimates", "raises guidance", "buyback", "partnership", "adoption",
                                "launch", "launches", "breakthrough", "acquisition", "wins", "approbation", "hausse", "partenariat", "rachat"]

    /// A word or phrase between two non-letters.
    static func contains(_ text: String, _ word: String) -> Bool {
        var search = text.startIndex..<text.endIndex
        while let r = text.range(of: word, range: search) {
            let before = r.lowerBound > text.startIndex ? text[text.index(before: r.lowerBound)] : " "
            let after = r.upperBound < text.endIndex ? text[r.upperBound] : " "
            if !before.isLetter && !after.isLetter { return true }
            search = r.upperBound..<text.endIndex
        }
        return false
    }

    static func newsTone(_ items: [Headline]) -> (negative: Int, positive: Int) {
        var neg = 0, pos = 0
        for it in items {
            let t = " \(it.title.lowercased()) "
            let n = negativeWords.contains { contains(t, $0) }, p = positiveWords.contains { contains(t, $0) }
            if n && !p { neg += 1 } else if p && !n { pos += 1 }
        }
        return (neg, pos)
    }

    static func reversal(_ input: Input, trend: String) -> GuardReport.Reversal {
        let d = Indicators.sanitize(input.daily), h4 = Indicators.sanitize(input.h4)
        let closes = d.map(\.close)
        // Against the background trend; in a range, against the last 5 days.
        var direction: String? = trend == "up" ? "down" : trend == "down" ? "up" : nil
        if direction == nil && closes.count > 6 {
            let mv = closes[closes.count - 1] / closes[closes.count - 6] - 1
            if abs(mv) >= 0.03 { direction = mv > 0 ? "down" : "up" }
        }
        guard let direction else { return .init(score: 0, direction: nil, factors: []) }
        let up = direction == "down"
        var raw: [Raw] = []
        if d.count > 60 {
            raw += technical(d, Indicators.rsi(closes, 14), Indicators.ema(closes, 20), Indicators.atr(d, 14), d.count - 1, down: up, four: false)
        }
        if h4.count > 60 {
            let c4 = h4.map(\.close)
            raw += technical(h4, Indicators.rsi(c4, 14), Indicators.ema(c4, 20), Indicators.atr(h4, 14), h4.count - 1, down: up, four: true)
        }
        // Crowd positioning (crypto perpetuals).
        if let f = input.fundingRate {
            let text = "\(JSFormat.fr(f * 100, min: 0, max: 4)) % par 8 h"
            if up && f >= fundingVeryHot {
                raw.append(.init(code: "funding", points: 25, text: "Financement des contrats perpétuels très élevé (\(text)) : les acheteurs à levier sont surchargés, risque de liquidations en cascade."))
            } else if up && f >= fundingHot {
                raw.append(.init(code: "funding", points: 15, text: "Financement élevé (\(text)) : beaucoup d'acheteurs à levier."))
            } else if !up && f <= fundingVeryCold {
                raw.append(.init(code: "funding", points: 25, text: "Financement très négatif (\(text)) : les vendeurs à découvert sont surchargés, risque de rachat brutal (short squeeze)."))
            } else if !up && f <= fundingCold {
                raw.append(.init(code: "funding", points: 15, text: "Financement négatif (\(text)) : beaucoup de vendeurs à découvert."))
            }
        }
        // Sentiment, contrarian at the extremes.
        if let v = input.fearGreed.last {
            let vs = Int(v.rounded())
            if up && v >= 80 { raw.append(.init(code: "fearGreed", points: 15, text: "Avidité extrême (Fear & Greed \(vs)) : historiquement proche des sommets.")) }
            else if up && v >= 75 { raw.append(.init(code: "fearGreed", points: 8, text: "Forte avidité (Fear & Greed \(vs)).")) }
            else if !up && v <= 20 { raw.append(.init(code: "fearGreed", points: 15, text: "Peur extrême (Fear & Greed \(vs)) : historiquement proche des creux.")) }
            else if !up && v <= 25 { raw.append(.init(code: "fearGreed", points: 8, text: "Forte peur (Fear & Greed \(vs)).")) }
        }
        // News tone against the trend (last 24 h).
        let tone = newsTone(input.news.filter { $0.time >= input.now - 86_400_000 && $0.time <= input.now })
        if up && tone.negative >= 2 && tone.negative > tone.positive {
            raw.append(.init(code: "newsTone", points: 15, text: "\(tone.negative) actualités négatives en 24 h alors que la tendance est haussière."))
        }
        if !up && tone.positive >= 2 && tone.positive > tone.negative {
            raw.append(.init(code: "newsTone", points: 15, text: "\(tone.positive) actualités positives en 24 h alors que la tendance est baissière."))
        }
        let isTechnical = { (code: String) in code.hasSuffix("4h") || code.hasSuffix("1d") }
        var evidence: [String: Evidence] = [:]
        if raw.contains(where: { isTechnical($0.code) && $0.code.hasSuffix("1d") }) { evidence.merge(reversalEvidence(d, four: false)) { a, _ in a } }
        if raw.contains(where: { isTechnical($0.code) && $0.code.hasSuffix("4h") }) { evidence.merge(reversalEvidence(h4, four: true)) { a, _ in a } }
        let factors = finalize(raw, evidence, historical: isTechnical)
        return .init(score: min(100, factors.reduce(0) { $0 + $1.points }), direction: direction, factors: factors)
    }

    // MARK: Everything together

    static func run(_ input: Input) -> (GuardReport.Regime, GuardReport.Shock, GuardReport.Reversal, GuardReport.Policy) {
        let reg = regime(input.daily, input.h4)
        let sh = shock(input)
        let rev = reversal(input, trend: reg.trend)
        var notes: [String] = []
        var scalping = "ok", size = 1.0, stop = 1.0
        if sh.level == "shock" {
            (scalping, size, stop) = ("pause", 0, 2)
            notes.append("Marché en choc : suspendre les nouvelles positions à court terme, laisser passer la tempête.")
        } else if sh.level == "agitated" {
            (scalping, size, stop) = ("reduce", 0.5, 1.5)
            notes.append("Marché agité : diviser la taille des positions par deux et élargir les stops (×1,5) pour ne pas être sorti par le bruit.")
        }
        if rev.score >= reversalHigh, let dir = rev.direction {
            if scalping == "ok" { (scalping, size, stop) = ("reduce", 0.5, 1) }
            notes.append("Risque de retournement à la \(dir == "down" ? "baisse" : "hausse") élevé : ne pas ouvrir de position dans le sens de la tendance actuelle, resserrer les stops des positions existantes.")
        }
        if notes.isEmpty { notes.append("Conditions normales : pas de signal de choc ni de retournement.") }
        if reg.trend != "range" {
            notes.append("Pour le long terme : \(reg.trend == "up" ? "privilégier les achats sur repli" : "privilégier la prudence, les rebonds sont fragiles") tant que la tendance de fond tient.")
        }
        return (reg, sh, rev, .init(scalping: scalping, sizeMultiplier: size, stopMultiplier: stop, notes: notes))
    }
}

extension LocalBackend {
    /// `/api/guard`: daily, 4 h and 1 h candles, the asset's headlines (news engine), and for cryptos the OKX perpetual
    /// funding rate and the Fear & Greed index (alternative.me). A missing extra input only removes its factor.
    func guardReport(_ a: Asset) async throws -> GuardReport {
        async let d1 = snapshot(a, .d1)
        async let h4 = try? snapshot(a, .h4)
        async let h1 = try? snapshot(a, .h1)
        async let news = news([a])
        async let funding: Double? = a.kind == .crypto ? Self.funding(a.symbol, fetcher) : nil
        async let fearGreed: [Double] = a.kind == .crypto ? Self.fearGreed(fetcher) : []
        async let q = quotes([a])
        let daily = try await d1
        let report = await news
        let headlines = report.items.filter { $0.assets.contains(a.id) }.map { GuardEngine.Headline(title: $0.title, time: $0.time, source: $0.source) }
        let now = nowMs()
        let input = GuardEngine.Input(kind: a.kind, daily: daily.candles, h4: await h4?.candles ?? [], h1: await h1?.candles ?? [],
                                      fundingRate: await funding, fearGreed: await fearGreed, news: headlines, now: now)
        let (regime, shock, reversal, policy) = GuardEngine.run(input)
        let macro = try? await macroReport()
        let recent = headlines.filter { $0.time >= now - 86_400_000 }
        return GuardReport(symbol: a.symbol, kind: a.kind, price: await q.first?.price ?? daily.candles.last?.close, asOf: now,
                           regime: regime, shock: shock, reversal: reversal, policy: policy,
                           inputs: .init(fearGreed: input.fearGreed.last, news24h: recent.count,
                                         headlines: recent.prefix(6).map { .init(title: $0.title, time: $0.time, source: $0.source) },
                                         vix: macro?.values["vix"]?.value),
                           macro: macro)
    }

    static func funding(_ symbol: String, _ f: MarketFetcher) async -> Double? {
        guard let d = try? await f.json("https://www.okx.com/api/v5/public/funding-rate?instId=\(symbol)-USDT-SWAP") else { return nil }
        let v = num(obj(arr(at(d, "data"))?.first)?["fundingRate"])
        return v.isFinite ? v : nil
    }

    static func fearGreed(_ f: MarketFetcher) async -> [Double] {
        guard let d = try? await f.json("https://api.alternative.me/fng/?limit=30") else { return [] }
        return (arr(at(d, "data")) ?? []).compactMap { obj($0) }.map { num($0["value"]) }.filter(\.isFinite).reversed()
    }
}
