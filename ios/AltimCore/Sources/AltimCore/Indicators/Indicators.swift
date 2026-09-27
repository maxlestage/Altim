import Foundation

/// Indicateurs techniques classiques.
///
/// Toutes les fonctions renvoient un tableau de même longueur que l'entrée ;
/// les valeurs non encore calculables (période de chauffe) valent `nil`.
/// Les formules suivent les définitions de référence (Wilder pour RSI/ATR/ADX).
public enum Indicators {

    // MARK: - Moyennes

    public static func sma(_ values: [Double], period: Int) -> [Double?] {
        guard period > 0 else { return Array(repeating: nil, count: values.count) }
        var result = [Double?](repeating: nil, count: values.count)
        guard values.count >= period else { return result }
        var sum = values[0..<period].reduce(0, +)
        result[period - 1] = sum / Double(period)
        for i in period..<values.count {
            sum += values[i] - values[i - period]
            result[i] = sum / Double(period)
        }
        return result
    }

    /// EMA initialisée par la SMA des `period` premières valeurs définies.
    public static func ema(_ values: [Double?], period: Int) -> [Double?] {
        var result = [Double?](repeating: nil, count: values.count)
        guard period > 0, let start = values.firstIndex(where: { $0 != nil }) else { return result }
        let defined = values[start...].map { $0 ?? .nan }
        guard defined.count >= period else { return result }
        let k = 2.0 / Double(period + 1)
        var prev = defined[0..<period].reduce(0, +) / Double(period)
        guard prev.isFinite else { return result }
        result[start + period - 1] = prev
        for j in period..<defined.count {
            let v = defined[j]
            guard v.isFinite else { return result }
            prev = v * k + prev * (1 - k)
            result[start + j] = prev
        }
        return result
    }

    public static func ema(_ values: [Double], period: Int) -> [Double?] {
        ema(values.map { Optional($0) }, period: period)
    }

    // MARK: - Momentum

    /// RSI de Wilder.
    public static func rsi(_ closes: [Double], period: Int = 14) -> [Double?] {
        var result = [Double?](repeating: nil, count: closes.count)
        guard period > 0, closes.count > period else { return result }
        var gain = 0.0, loss = 0.0
        for i in 1...period {
            let change = closes[i] - closes[i - 1]
            if change >= 0 { gain += change } else { loss -= change }
        }
        var avgGain = gain / Double(period)
        var avgLoss = loss / Double(period)
        result[period] = rsiValue(avgGain, avgLoss)
        if closes.count > period + 1 {
            for i in (period + 1)..<closes.count {
                let change = closes[i] - closes[i - 1]
                avgGain = (avgGain * Double(period - 1) + max(change, 0)) / Double(period)
                avgLoss = (avgLoss * Double(period - 1) + max(-change, 0)) / Double(period)
                result[i] = rsiValue(avgGain, avgLoss)
            }
        }
        return result
    }

    private static func rsiValue(_ avgGain: Double, _ avgLoss: Double) -> Double {
        if avgLoss == 0 { return avgGain == 0 ? 50 : 100 }
        let rs = avgGain / avgLoss
        return 100 - 100 / (1 + rs)
    }

    public struct MACD: Sendable {
        public let line: [Double?]
        public let signal: [Double?]
        public let histogram: [Double?]
    }

    public static func macd(_ closes: [Double], fast: Int = 12, slow: Int = 26, signal: Int = 9) -> MACD {
        let fastEMA = ema(closes, period: fast)
        let slowEMA = ema(closes, period: slow)
        let line: [Double?] = zip(fastEMA, slowEMA).map { f, s in
            guard let f, let s else { return nil }
            return f - s
        }
        let signalLine = ema(line, period: signal)
        let histogram: [Double?] = zip(line, signalLine).map { l, s in
            guard let l, let s else { return nil }
            return l - s
        }
        return MACD(line: line, signal: signalLine, histogram: histogram)
    }

    public struct Stochastic: Sendable {
        public let k: [Double?]
        public let d: [Double?]
    }

    /// Stochastique lent (%K lissé, %D = moyenne de %K).
    public static func stochastic(_ candles: [Candle], period: Int = 14, smoothK: Int = 3, smoothD: Int = 3) -> Stochastic {
        let n = candles.count
        var raw = [Double?](repeating: nil, count: n)
        if n >= period {
            for i in (period - 1)..<n {
                let window = candles[(i - period + 1)...i]
                let hh = window.map(\.high).max()!
                let ll = window.map(\.low).min()!
                raw[i] = hh - ll == 0 ? 50 : (candles[i].close - ll) / (hh - ll) * 100
            }
        }
        let k = smaOptional(raw, period: smoothK)
        let d = smaOptional(k, period: smoothD)
        return Stochastic(k: k, d: d)
    }

    // MARK: - Volatilité

    public struct Bands: Sendable {
        public let upper: [Double?]
        public let middle: [Double?]
        public let lower: [Double?]
    }

    public static func bollinger(_ closes: [Double], period: Int = 20, multiplier: Double = 2) -> Bands {
        let middle = sma(closes, period: period)
        var upper = [Double?](repeating: nil, count: closes.count)
        var lower = [Double?](repeating: nil, count: closes.count)
        for i in closes.indices {
            guard let m = middle[i] else { continue }
            let window = closes[(i - period + 1)...i]
            let variance = window.reduce(0) { $0 + ($1 - m) * ($1 - m) } / Double(period)
            let sd = variance.squareRoot()
            upper[i] = m + multiplier * sd
            lower[i] = m - multiplier * sd
        }
        return Bands(upper: upper, middle: middle, lower: lower)
    }

    public static func trueRange(_ candles: [Candle]) -> [Double] {
        candles.indices.map { i in
            let c = candles[i]
            guard i > 0 else { return c.high - c.low }
            let prevClose = candles[i - 1].close
            return max(c.high - c.low, abs(c.high - prevClose), abs(c.low - prevClose))
        }
    }

    /// ATR de Wilder.
    public static func atr(_ candles: [Candle], period: Int = 14) -> [Double?] {
        var result = [Double?](repeating: nil, count: candles.count)
        guard period > 0, candles.count >= period else { return result }
        let tr = trueRange(candles)
        var prev = tr[0..<period].reduce(0, +) / Double(period)
        result[period - 1] = prev
        for i in period..<candles.count {
            prev = (prev * Double(period - 1) + tr[i]) / Double(period)
            result[i] = prev
        }
        return result
    }

    // MARK: - Tendance

    public struct DirectionalMovement: Sendable {
        public let adx: [Double?]
        public let plusDI: [Double?]
        public let minusDI: [Double?]
    }

    /// ADX / DI+ / DI- de Wilder.
    public static func adx(_ candles: [Candle], period: Int = 14) -> DirectionalMovement {
        let n = candles.count
        var adx = [Double?](repeating: nil, count: n)
        var plusDI = [Double?](repeating: nil, count: n)
        var minusDI = [Double?](repeating: nil, count: n)
        guard period > 0, n > period * 2 else { return DirectionalMovement(adx: adx, plusDI: plusDI, minusDI: minusDI) }

        let tr = trueRange(candles)
        var plusDM = [Double](repeating: 0, count: n)
        var minusDM = [Double](repeating: 0, count: n)
        for i in 1..<n {
            let up = candles[i].high - candles[i - 1].high
            let down = candles[i - 1].low - candles[i].low
            plusDM[i] = (up > down && up > 0) ? up : 0
            minusDM[i] = (down > up && down > 0) ? down : 0
        }

        var smTR = tr[1...period].reduce(0, +)
        var smPlus = plusDM[1...period].reduce(0, +)
        var smMinus = minusDM[1...period].reduce(0, +)
        var dx = [Double?](repeating: nil, count: n)

        func update(_ i: Int) {
            let pdi = smTR == 0 ? 0 : 100 * smPlus / smTR
            let mdi = smTR == 0 ? 0 : 100 * smMinus / smTR
            plusDI[i] = pdi
            minusDI[i] = mdi
            dx[i] = (pdi + mdi) == 0 ? 0 : 100 * abs(pdi - mdi) / (pdi + mdi)
        }
        update(period)
        for i in (period + 1)..<n {
            smTR = smTR - smTR / Double(period) + tr[i]
            smPlus = smPlus - smPlus / Double(period) + plusDM[i]
            smMinus = smMinus - smMinus / Double(period) + minusDM[i]
            update(i)
        }

        let firstADX = 2 * period - 1
        var prev = dx[period...firstADX].compactMap { $0 }.reduce(0, +) / Double(period)
        adx[firstADX] = prev
        if n > firstADX + 1 {
            for i in (firstADX + 1)..<n {
                prev = (prev * Double(period - 1) + (dx[i] ?? 0)) / Double(period)
                adx[i] = prev
            }
        }
        return DirectionalMovement(adx: adx, plusDI: plusDI, minusDI: minusDI)
    }

    // MARK: - Volume

    public static func obv(_ candles: [Candle]) -> [Double] {
        var result = [Double](repeating: 0, count: candles.count)
        for i in candles.indices.dropFirst() {
            let delta = candles[i].close - candles[i - 1].close
            result[i] = result[i - 1] + (delta > 0 ? candles[i].volume : delta < 0 ? -candles[i].volume : 0)
        }
        return result
    }

    // MARK: - Utilitaires

    /// Pente de la régression linéaire des `period` dernières valeurs, divisée par leur moyenne absolue
    /// (pente relative par barre, comparable d'un actif à l'autre).
    public static func normalizedSlope(_ values: [Double], period: Int) -> Double? {
        guard period > 1, values.count >= period else { return nil }
        let window = Array(values.suffix(period))
        let n = Double(period)
        let meanX = (n - 1) / 2
        let meanY = window.reduce(0, +) / n
        var num = 0.0, den = 0.0
        for (i, y) in window.enumerated() {
            let dx = Double(i) - meanX
            num += dx * (y - meanY)
            den += dx * dx
        }
        let scale = window.reduce(0) { $0 + abs($1) } / n
        guard den > 0, scale > 0 else { return 0 }
        return (num / den) / scale
    }

    static func smaOptional(_ values: [Double?], period: Int) -> [Double?] {
        var result = [Double?](repeating: nil, count: values.count)
        guard period > 0 else { return result }
        for i in values.indices where i >= period - 1 {
            let window = values[(i - period + 1)...i]
            guard window.allSatisfy({ $0 != nil }) else { continue }
            result[i] = window.compactMap { $0 }.reduce(0, +) / Double(period)
        }
        return result
    }
}
