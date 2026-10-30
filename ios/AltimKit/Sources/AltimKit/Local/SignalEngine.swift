import Foundation

// On-device signal engine: the same computation as the server's (core/src/engine/signal.rs and reliability.rs), so the
// app reads the markets and judges them itself, without the Altim server.

/// One candle as the engines use it (time in ms, volume 0 when the source gives none).
public struct Bar: Sendable, Equatable {
    public var time: Double
    public var open: Double
    public var high: Double
    public var low: Double
    public var close: Double
    public var volume: Double
    public init(time: Double, open: Double, high: Double, low: Double, close: Double, volume: Double) {
        self.time = time
        self.open = open
        self.high = high
        self.low = low
        self.close = close
        self.volume = volume
    }
    public var candle: Candle { Candle(time: time, open: open, high: high, low: low, close: close, volume: volume) }
}

public struct SignalFactor: Codable, Sendable, Equatable {
    public var name: String
    public var score: Double
    public var weight: Double
    public var detail: String
}

public struct LocalSignal: Sendable, Equatable {
    public var action: Action
    public var score: Double
    public var confidence: Double
    public var price: Double
    public var time: Double
    public var factors: [SignalFactor]
    public var stopLoss: Double
    public var takeProfit: Double
    public var hasPlan: Bool
    public var warnings: [String]
}

public struct QualityReport: Sendable, Equatable {
    public var score: Double
    public var issues: [String]
    public var gaps: Int
    public var badTicks: Int
    public var stale: Bool
}

enum Indicators {
    typealias Series = [Double?]

    static func isValid(_ c: Bar) -> Bool {
        [c.open, c.high, c.low, c.close, c.volume].allSatisfy(\.isFinite) && c.low > 0 && c.high >= c.low
            && c.high >= max(c.open, c.close) && c.low <= min(c.open, c.close) && c.volume >= 0
    }

    /// Valid candles, sorted, without duplicates (the first of equal times is kept).
    static func sanitize(_ candles: [Bar]) -> [Bar] {
        var v = candles.filter(isValid)
        v = v.enumerated().sorted { $0.element.time != $1.element.time ? $0.element.time < $1.element.time : $0.offset < $1.offset }.map(\.element)
        var out: [Bar] = []
        out.reserveCapacity(v.count)
        for c in v where out.last?.time != c.time { out.append(c) }
        return out
    }

    static func sign(_ x: Double) -> Double { x > 0 ? 1 : x < 0 ? -1 : 0 }
    static func clamp(_ x: Double, _ lo: Double, _ hi: Double) -> Double { min(hi, max(lo, x)) }

    static func sma(_ values: [Double], _ period: Int) -> Series {
        var out = Series(repeating: nil, count: values.count)
        guard period > 0, values.count >= period else { return out }
        var sum = values[..<period].reduce(0, +)
        out[period - 1] = sum / Double(period)
        for i in period..<values.count {
            sum += values[i] - values[i - period]
            out[i] = sum / Double(period)
        }
        return out
    }

    static func ema(_ values: Series, _ period: Int) -> Series {
        var out = Series(repeating: nil, count: values.count)
        guard let start = values.firstIndex(where: { $0 != nil }), values.count - start >= period else { return out }
        let k = 2 / (Double(period) + 1)
        var prev = 0.0
        for j in 0..<period { prev += values[start + j] ?? .nan }
        prev /= Double(period)
        out[start + period - 1] = prev
        for i in (start + period)..<values.count {
            guard let v = values[i] else { return out }
            prev = v * k + prev * (1 - k)
            out[i] = prev
        }
        return out
    }

    static func ema(_ values: [Double], _ period: Int) -> Series { ema(values.map { Optional($0) }, period) }

    static func rsi(_ closes: [Double], _ period: Int) -> Series {
        var out = Series(repeating: nil, count: closes.count)
        guard closes.count > period else { return out }
        var gain = 0.0, loss = 0.0
        for i in 1...period {
            let c = closes[i] - closes[i - 1]
            if c >= 0 { gain += c } else { loss -= c }
        }
        let p = Double(period)
        var ag = gain / p, al = loss / p
        func value(_ g: Double, _ l: Double) -> Double { l == 0 ? (g == 0 ? 50 : 100) : 100 - 100 / (1 + g / l) }
        out[period] = value(ag, al)
        for i in (period + 1)..<max(period + 1, closes.count) {
            let c = closes[i] - closes[i - 1]
            ag = (ag * (p - 1) + max(c, 0)) / p
            al = (al * (p - 1) + max(-c, 0)) / p
            out[i] = value(ag, al)
        }
        return out
    }

    static func trueRange(_ c: [Bar]) -> [Double] {
        c.indices.map { i in
            i == 0 ? c[i].high - c[i].low
                : max(c[i].high - c[i].low, abs(c[i].high - c[i - 1].close), abs(c[i].low - c[i - 1].close))
        }
    }

    static func atr(_ c: [Bar], _ period: Int) -> Series {
        var out = Series(repeating: nil, count: c.count)
        guard period > 0, c.count >= period else { return out }
        let tr = trueRange(c)
        let p = Double(period)
        var prev = tr[..<period].reduce(0, +) / p
        out[period - 1] = prev
        for i in period..<c.count {
            prev = (prev * (p - 1) + tr[i]) / p
            out[i] = prev
        }
        return out
    }

    static func adx(_ c: [Bar], _ period: Int) -> Series {
        let n = c.count
        var out = Series(repeating: nil, count: n)
        guard n > period * 2 else { return out }
        let tr = trueRange(c)
        var pdm = [Double](repeating: 0, count: n), mdm = [Double](repeating: 0, count: n)
        for i in 1..<n {
            let up = c[i].high - c[i - 1].high
            let down = c[i - 1].low - c[i].low
            pdm[i] = up > down && up > 0 ? up : 0
            mdm[i] = down > up && down > 0 ? down : 0
        }
        var sTr = 0.0, sP = 0.0, sM = 0.0
        for i in 1...period {
            sTr += tr[i]
            sP += pdm[i]
            sM += mdm[i]
        }
        var dx = Series(repeating: nil, count: n)
        func update(_ i: Int) {
            let p = sTr == 0 ? 0 : 100 * sP / sTr
            let m = sTr == 0 ? 0 : 100 * sM / sTr
            dx[i] = p + m == 0 ? 0 : 100 * abs(p - m) / (p + m)
        }
        update(period)
        let pf = Double(period)
        for i in (period + 1)..<n {
            sTr = sTr - sTr / pf + tr[i]
            sP = sP - sP / pf + pdm[i]
            sM = sM - sM / pf + mdm[i]
            update(i)
        }
        let first = 2 * period - 1
        var prev = 0.0
        for i in period...first { prev += dx[i] ?? 0 }
        prev /= pf
        out[first] = prev
        for i in (first + 1)..<max(first + 1, n) {
            prev = (prev * (pf - 1) + (dx[i] ?? 0)) / pf
            out[i] = prev
        }
        return out
    }

    static func smaOptional(_ values: Series, _ period: Int) -> Series {
        var out = Series(repeating: nil, count: values.count)
        guard period > 0 else { return out }
        for i in max(period - 1, 0)..<max(period - 1, values.count) {
            var sum = 0.0
            var ok = true
            for v in values[(i + 1 - period)...i] {
                guard let v else { ok = false; break }
                sum += v
            }
            if ok { out[i] = sum / Double(period) }
        }
        return out
    }

    static func stochastic(_ c: [Bar], _ period: Int, _ smoothK: Int, _ smoothD: Int) -> (Series, Series) {
        var raw = Series(repeating: nil, count: c.count)
        for i in max(period - 1, 0)..<max(period - 1, c.count) {
            var hh = -Double.infinity, ll = Double.infinity
            for x in c[(i + 1 - period)...i] {
                hh = max(hh, x.high)
                ll = min(ll, x.low)
            }
            raw[i] = hh - ll == 0 ? 50 : (c[i].close - ll) / (hh - ll) * 100
        }
        let k = smaOptional(raw, smoothK)
        return (k, smaOptional(k, smoothD))
    }

    static func obv(_ c: [Bar]) -> [Double] {
        var out = [Double](repeating: 0, count: c.count)
        for i in c.indices.dropFirst() {
            let d = c[i].close - c[i - 1].close
            out[i] = out[i - 1] + (d > 0 ? c[i].volume : d < 0 ? -c[i].volume : 0)
        }
        return out
    }

    static func slope(_ values: ArraySlice<Double>) -> Double? {
        guard values.count >= 2 else { return nil }
        let n = Double(values.count)
        let mx = (n - 1) / 2
        let my = values.reduce(0, +) / n
        var num = 0.0, den = 0.0
        for (i, y) in values.enumerated() {
            num += (Double(i) - mx) * (y - my)
            den += (Double(i) - mx) * (Double(i) - mx)
        }
        return den > 0 ? num / den : nil
    }

    /// Median of finite values (mean of the two middle ones), 0 when empty.
    static func median(_ v: [Double]) -> Double {
        guard !v.isEmpty else { return 0 }
        let s = v.sorted()
        let m = s.count / 2
        return s.count % 2 == 0 ? (s[m - 1] + s[m]) / 2 : s[m]
    }
}

/// Number with `digits` decimals, written like JavaScript's toFixed (dot separator).
func fixed(_ x: Double, _ digits: Int) -> String { String(format: "%.\(digits)f", x) }

public enum SignalEngine {
    static let wTrend = 2.0, wMacd = 1.5, wRsi = 1.5, wStoch = 1.0, wBoll = 0.75, wVolume = 1.0, wHigher = 1.5

    /// Technical signal of the last closed candle; nil under 60 valid candles.
    public static func analyze(_ raw: [Bar], higher: [Bar]? = nil, intervalMs: Double? = nil, now: Double = Date().timeIntervalSince1970 * 1000) -> LocalSignal? {
        typealias I = Indicators
        let candles = I.sanitize(raw)
        guard candles.count >= 60 else { return nil }
        let closes = candles.map(\.close)
        let last = candles.count - 1
        let price = closes[last]
        var factors: [SignalFactor] = []
        var warnings: [String] = []
        let adxNow = I.adx(candles, 14)[last]
        let atrNow = I.atr(candles, 14)[last] ?? 0

        // 1. Trend
        let e20 = I.ema(closes, 20)[last], e50 = I.ema(closes, 50)[last], e200 = I.ema(closes, 200)[last]
        let (fast, slow, label) = e200 != nil ? (e50, e200, "EMA 50/200") : (e20, e50, "EMA 20/50")
        if e200 == nil { warnings.append("Moins de 200 bougies : tendance de long terme estimée sur EMA 20/50.") }
        if let fast, let slow {
            let rawTrend = 0.5 * I.sign(fast - slow) + 0.5 * I.sign(price - slow)
            let strength = adxNow.map { I.clamp(($0 - 15) / 20, 0.3, 1) } ?? 0.6
            let dir = rawTrend > 0 ? "haussière" : rawTrend < 0 ? "baissière" : "neutre"
            let adxText = adxNow.map { " · ADX \(fixed($0, 0))" } ?? ""
            factors.append(.init(name: "Tendance", score: I.clamp(rawTrend * strength, -1, 1), weight: wTrend, detail: "\(label) \(dir)\(adxText)"))
        }
        if let a = adxNow, a < 18 { warnings.append("Marché sans tendance (ADX < 18) : signaux moins fiables.") }

        // 2. MACD
        let f12 = I.ema(closes, 12), s26 = I.ema(closes, 26)
        let line: I.Series = zip(f12, s26).map { f, s in if let f, let s { return f - s } else { return nil } }
        let sig = I.ema(line, 9)
        let hist: I.Series = zip(line, sig).map { l, s in if let l, let s { return l - s } else { return nil } }
        if let h = hist[last], let hp = hist[last - 1] {
            var score = 0.5 * I.sign(h)
            var detail = h >= 0 ? "Histogramme positif" : "Histogramme négatif"
            var cross: Double?
            for i in max(last - 3, 1)...last {
                guard let a = hist[i - 1], let b = hist[i] else { continue }
                if a <= 0 && b > 0 { cross = 1 } else if a >= 0 && b < 0 { cross = -1 }
            }
            if let c = cross {
                score = c
                detail = c > 0 ? "Croisement haussier récent" : "Croisement baissier récent"
            } else if h > hp {
                score += 0.25
                detail += ", en hausse"
            } else if h < hp {
                score -= 0.25
                detail += ", en baisse"
            }
            factors.append(.init(name: "MACD", score: I.clamp(score, -1, 1), weight: wMacd, detail: detail))
        }

        // 3. RSI
        let r = I.rsi(closes, 14)
        if let rv = r[last] {
            let prev = r[last - 1] ?? rv
            let score: Double
            if rv < 30 { score = min(0.5 + (30 - rv) / 20, 1) * (rv > prev ? 1 : 0.7) }
            else if rv > 70 { score = -min(0.5 + (rv - 70) / 20, 1) * (rv < prev ? 1 : 0.7) }
            else { score = (rv - 50) / 40 }
            let zone = rv < 30 ? "survente" : rv > 70 ? "surachat" : "zone neutre"
            factors.append(.init(name: "RSI", score: I.clamp(score, -1, 1), weight: wRsi, detail: "RSI \(fixed(rv, 1)) (\(zone))"))
        }

        // 4. Stochastic
        let (sk, sd) = I.stochastic(candles, 14, 3, 3)
        if let k = sk[last], let d = sd[last], let kp = sk[last - 1], let dp = sd[last - 1] {
            var score = 0.0
            var detail = "%K \(fixed(k, 0)) / %D \(fixed(d, 0))"
            if kp <= dp && k > d && k < 30 {
                score = 1
                detail += " · croisement haussier en survente"
            } else if kp >= dp && k < d && k > 70 {
                score = -1
                detail += " · croisement baissier en surachat"
            } else if k < 20 { score = 0.5 } else if k > 80 { score = -0.5 }
            factors.append(.init(name: "Stochastique", score: score, weight: wStoch, detail: detail))
        }

        // 5. Bollinger
        if let mid = I.sma(closes, 20)[last] {
            let variance = closes[(last - 19)...last].reduce(0) { $0 + ($1 - mid) * ($1 - mid) }
            let sd = (variance / 20).squareRoot()
            let up = mid + 2 * sd, lo = mid - 2 * sd
            if up > lo {
                let pb = (price - lo) / (up - lo)
                let score = pb < 0 ? 0.8 : pb > 1 ? -0.8 : (0.5 - pb) * 0.6
                factors.append(.init(name: "Bollinger", score: score, weight: wBoll, detail: "%B \(fixed(pb, 2))"))
            }
        }

        // 6. Volume
        let o = I.obv(candles).suffix(20)
        let avgVol = candles.suffix(20).reduce(0) { $0 + $1.volume } / 20
        if let s = I.slope(o), avgVol > 0 {
            let perBar = s / avgVol
            factors.append(.init(name: "Volume (OBV)", score: I.clamp(perBar / 0.3, -1, 1), weight: wVolume, detail: perBar >= 0 ? "Accumulation" : "Distribution"))
        } else {
            warnings.append("Volume indisponible : facteur volume ignoré.")
        }

        // 7. Higher-timeframe confirmation
        var higherScore: Double?
        let hi = I.sanitize(higher ?? [])
        if hi.count >= 50 {
            let hc = hi.map(\.close)
            let hl = hc.count - 1
            if let hf = I.ema(hc, 20)[hl], let hs = I.ema(hc, 50)[hl] {
                let sc = 0.5 * I.sign(hf - hs) + 0.5 * I.sign(hc[hl] - hs)
                higherScore = sc
                factors.append(.init(name: "UT supérieure", score: sc, weight: wHigher,
                                     detail: sc > 0 ? "Tendance de fond haussière" : sc < 0 ? "Tendance de fond baissière" : "Neutre"))
            }
        }

        let tw = factors.reduce(0) { $0 + $1.weight }
        let score = tw > 0 ? factors.reduce(0) { $0 + $1.score * $1.weight } / tw * 100 : 0
        let directional = factors.filter { $0.score != 0 }
        let dw = directional.reduce(0) { $0 + $1.weight }
        let aw = directional.filter { I.sign($0.score) == I.sign(score) }.reduce(0) { $0 + $1.weight }
        let agreement = dw > 0 ? aw / dw : 0
        let confidence = I.clamp(abs(score) * 1.25, 0, 100) * (0.5 + 0.5 * agreement)

        var action: Action = score >= 50 ? .strongBuy : score >= 25 ? .buy : score <= -50 ? .strongSell : score <= -25 ? .sell : .hold
        // Never buy against the underlying trend: the signal is downgraded.
        if let h = higherScore {
            if action.isBuy && h < 0 {
                action = action == .strongBuy ? .buy : .hold
                warnings.append("Achat contre la tendance de l'unité supérieure : signal rétrogradé.")
            } else if action.isSell && h > 0 {
                action = action == .strongSell ? .sell : .hold
                warnings.append("Vente contre la tendance de l'unité supérieure : signal rétrogradé.")
            }
        }
        if atrNow > 0 && atrNow / price > 0.08 {
            warnings.append("Volatilité extrême (ATR \(fixed(atrNow / price * 100, 1)) % du prix) : réduisez la taille.")
        }
        let lastTime = candles[last].time
        if let iv = intervalMs, iv != 0, now - lastTime > iv * 3 {
            warnings.append("Données possiblement périmées (dernière bougie ancienne).")
        }
        let dist = 2 * atrNow
        let sell = action.isSell
        return LocalSignal(action: action, score: score, confidence: confidence, price: price, time: lastTime, factors: factors,
                           stopLoss: sell ? price + dist : max(price - dist, price * 0.01),
                           takeProfit: sell ? max(price - dist * 2, price * 0.01) : price + dist * 2,
                           hasPlan: atrNow > 0, warnings: warnings)
    }

    // MARK: Data quality and reliability

    public static func assessQuality(_ raw: [Bar], intervalMs: Double, kind: Kind, now: Double = Date().timeIntervalSince1970 * 1000) -> QualityReport {
        let candles = Indicators.sanitize(raw)
        var score = 100.0
        var issues: [String] = []
        let invalid = raw.count - candles.count
        if invalid > 0 {
            score -= min(Double(invalid) * 2, 20)
            issues.append("\(invalid) bougie(s) incohérente(s) écartée(s).")
        }
        guard candles.count >= 2 else {
            issues.append("Aucune donnée exploitable.")
            return QualityReport(score: 0, issues: issues, gaps: 0, badTicks: 0, stale: true)
        }
        if candles.count < 60 {
            score -= 40
            issues.append("Historique trop court (\(candles.count) bougies).")
        }
        let recent = Array(candles.suffix(300))
        var gaps = 0
        for i in recent.indices.dropFirst() {
            let delta = recent[i].time - recent[i - 1].time
            let tooFar = kind == .crypto ? delta > intervalMs * 1.5 : delta > max(intervalMs * 1.5, 4.5 * 86_400_000)
            if tooFar { gaps += 1 }
        }
        if gaps > 0 {
            score -= min(Double(gaps) * 5, 30)
            issues.append("\(gaps) trou(s) dans l'historique.")
        }
        let returns = recent.indices.dropFirst().map { log(recent[$0].close / recent[$0 - 1].close) }
        var badTicks = 0
        if returns.count > 20 {
            let med = Indicators.median(returns)
            let mad = Indicators.median(returns.map { abs($0 - med) }) * 1.4826
            if mad > 0 {
                for i in 0..<(returns.count - 1) {
                    let r = returns[i], next = returns[i + 1]
                    if abs(r - med) > 12 * mad && abs(r) > 0.03 && next * r < 0 && abs(next) > abs(r) * 0.6 { badTicks += 1 }
                }
            }
        }
        if badTicks > 0 {
            score -= min(Double(badTicks) * 10, 30)
            issues.append("\(badTicks) pic(s) de prix aberrant(s) aussitôt annulé(s).")
        }
        let lastTime = candles[candles.count - 1].time
        let allowed = kind == .crypto ? intervalMs * 3 : max(intervalMs * 3, 4 * 86_400_000)
        let stale = now - lastTime > allowed + intervalMs
        if stale {
            score -= 40
            issues.append("Données périmées.")
        }
        let withVolume = candles.filter { $0.volume > 0 }.count
        if withVolume > 0 && Double(candles.count - withVolume) / Double(candles.count) > 0.2 {
            score -= 10
            issues.append("Volume absent sur une partie de l'historique.")
        }
        if Double(recent.filter { $0.high == $0.low }.count) / Double(recent.count) > 0.3 {
            score -= 15
            issues.append("Marché très peu liquide (prix figés).")
        }
        return QualityReport(score: max(score, 0), issues: issues, gaps: gaps, badTicks: badTicks, stale: stale)
    }

    /// Score cap by number of independent agreeing sources (index = sources, 5 and more = 100).
    static let reliabilityCap: [Double] = [40, 40, 60, 75, 90, 100]

    public static func reliability(quality: Double, independent: Int, conflict: Bool) -> Reliability {
        // A decision needs several independent confirmations: 1 source = no advice, 3 minimum for "high".
        let cap = reliabilityCap[min(max(independent, 0), 5)]
        let score = conflict ? min(quality, 30) : min(quality, cap)
        let level = score >= 75 ? "high" : score >= 50 ? "medium" : "low"
        return Reliability(score: score, level: level, independent: independent, conflict: conflict)
    }

    /// Never a BUY/SELL on doubtful data.
    public static func gate(_ s: LocalSignal, _ rel: Reliability, issues: [String]) -> LocalSignal {
        var out = s
        switch rel.level {
        case "low":
            if out.action != .hold {
                out.action = .hold
                out.warnings.insert("Données insuffisamment fiables (\(Int(rel.score.rounded()))/100) : signal suspendu.", at: 0)
            }
        case "medium":
            if out.action == .strongBuy { out.action = .buy }
            if out.action == .strongSell { out.action = .sell }
            out.warnings.append("Fiabilité moyenne des données (\(rel.independent ?? 0) source(s) indépendante(s)).")
        default: break
        }
        out.warnings += issues
        out.confidence = s.confidence * rel.score / 100
        return out
    }
}

extension Action {
    var isBuy: Bool { self == .buy || self == .strongBuy }
    var isSell: Bool { self == .sell || self == .strongSell }
}
