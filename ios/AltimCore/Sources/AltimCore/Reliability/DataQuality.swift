import Foundation

/// Quality check of a candle series before any analysis.
public struct DataQualityReport: Sendable, Hashable {
    /// 0 (unusable) to 100 (perfect).
    public let score: Double
    public let issues: [String]
    public let gaps: Int
    public let badTicks: Int
    public let isStale: Bool
}

public enum DataQuality {
    public static func assess(_ raw: [Candle], timeframe: Timeframe, assetClass: AssetClass, now: Date = Date()) -> DataQualityReport {
        let candles = raw.sanitized()
        var score = 100.0
        var issues: [String] = []

        let invalid = raw.filter { !$0.isValid }.count
        if invalid > 0 {
            score -= min(20, Double(invalid) * 2)
            issues.append("\(invalid) bougie(s) incohérente(s) écartée(s).")
        }
        guard candles.count >= 2 else {
            return DataQualityReport(score: 0, issues: issues + ["Aucune donnée exploitable."], gaps: 0, badTicks: 0, isStale: true)
        }
        if candles.count < 60 {
            score -= 40
            issues.append("Historique trop court (\(candles.count) bougies).")
        }

        // Gaps: a 24/7 market must not skip any candle.
        let step = timeframe.seconds
        var gaps = 0
        let recent = Array(candles.suffix(300))
        for i in recent.indices.dropFirst() {
            let delta = recent[i].time.timeIntervalSince(recent[i - 1].time)
            if assetClass == .crypto {
                if delta > step * 1.5 { gaps += 1 }
            } else if delta > max(step * 1.5, 4.5 * 86_400) {
                gaps += 1 // stocks: only abnormal gaps (beyond a long weekend)
            }
        }
        if gaps > 0 {
            score -= min(30, Double(gaps) * 5)
            issues.append("\(gaps) trou(s) dans l'historique.")
        }

        // Bad ticks: an extreme move immediately reversed (typical exchange error).
        let returns = zip(recent.dropFirst(), recent).map { log($0.close / $1.close) }
        var badTicks = 0
        if returns.count > 20 {
            let med = median(returns)
            let mad = median(returns.map { abs($0 - med) }) * 1.4826
            if mad > 0 {
                for i in 0..<(returns.count - 1) {
                    let r = returns[i], next = returns[i + 1]
                    if abs(r - med) > 12 * mad, abs(r) > 0.03, next * r < 0, abs(next) > abs(r) * 0.6 { badTicks += 1 }
                }
            }
        }
        if badTicks > 0 {
            score -= min(30, Double(badTicks) * 10)
            issues.append("\(badTicks) pic(s) de prix aberrant(s) aussitôt annulé(s).")
        }

        // Stale data.
        let last = candles[candles.count - 1].time
        let allowed: TimeInterval = assetClass == .crypto ? step * 3 : max(step * 3, 4 * 86_400)
        let stale = now.timeIntervalSince(last) > allowed + step
        if stale {
            score -= 40
            issues.append("Données périmées (dernière bougie : \(last.formatted(date: .abbreviated, time: .shortened))).")
        }

        // Missing volume or frozen prices.
        let withVolume = candles.filter { $0.volume > 0 }.count
        if withVolume > 0, Double(candles.count - withVolume) / Double(candles.count) > 0.2 {
            score -= 10
            issues.append("Volume absent sur une partie de l'historique.")
        }
        let flat = recent.filter { $0.high == $0.low }.count
        if Double(flat) / Double(recent.count) > 0.3 {
            score -= 15
            issues.append("Marché très peu liquide (prix figés).")
        }

        return DataQualityReport(score: max(0, score), issues: issues, gaps: gaps, badTicks: badTicks, isStale: stale)
    }

    static func median(_ values: [Double]) -> Double {
        guard !values.isEmpty else { return 0 }
        let s = values.sorted()
        return s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2
    }
}
