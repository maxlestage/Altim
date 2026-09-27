import Foundation

/// Moteur de signaux par confluence.
///
/// Chaque indicateur donne un score entre -1 et +1, pondéré, puis agrégé en un score de -100 à +100.
/// Un achat n'est proposé que si plusieurs familles d'indicateurs indépendantes (tendance, momentum,
/// volume, volatilité) vont dans le même sens. Aucune bougie en formation n'est utilisée (pas de "repaint").
public struct SignalEngine: Sendable {

    public struct Configuration: Codable, Hashable, Sendable {
        public var buyThreshold: Double = 25
        public var strongThreshold: Double = 50
        /// Stop = entrée − `stopATR` × ATR.
        public var stopATR: Double = 2
        /// Objectif = entrée + `rewardRisk` × distance du stop.
        public var rewardRisk: Double = 2
        /// Nombre minimum de bougies clôturées pour émettre un signal.
        public var minimumCandles: Int = 60

        public var weightTrend: Double = 2.0
        public var weightMACD: Double = 1.5
        public var weightRSI: Double = 1.5
        public var weightStochastic: Double = 1.0
        public var weightBollinger: Double = 0.75
        public var weightVolume: Double = 1.0
        public var weightHigherTimeframe: Double = 1.5

        public init() {}
    }

    public enum EngineError: Error, Equatable, LocalizedError {
        case notEnoughData(have: Int, need: Int)

        public var errorDescription: String? {
            switch self {
            case let .notEnoughData(have, need):
                return "Historique insuffisant : \(have) bougies, \(need) nécessaires."
            }
        }
    }

    public var configuration: Configuration

    public init(configuration: Configuration = Configuration()) {
        self.configuration = configuration
    }

    /// Analyse une série de bougies.
    /// - Parameters:
    ///   - candles: historique (les bougies non clôturées ou invalides sont ignorées).
    ///   - higherTimeframe: historique de l'unité de temps supérieure, pour confirmer la tendance.
    ///   - timeframe: unité de temps, pour détecter des données périmées.
    ///   - now: date courante (injectable pour les tests).
    public func analyze(_ candles: [Candle], higherTimeframe: [Candle]? = nil,
                        timeframe: Timeframe? = nil, now: Date = Date()) throws -> Signal {
        let data = candles.sanitized()
        let cfg = configuration
        guard data.count >= cfg.minimumCandles else {
            throw EngineError.notEnoughData(have: data.count, need: cfg.minimumCandles)
        }

        let closes = data.closes
        let last = data.count - 1
        let price = closes[last]
        var factors: [SignalFactor] = []
        var warnings: [String] = []

        let dmi = Indicators.adx(data)
        let adx = dmi.adx[last]
        let atr = Indicators.atr(data)[last] ?? 0

        // 1. Tendance : EMA 50 / 200 (ou 20 / 50 si historique court), modulée par l'ADX.
        let ema50 = Indicators.ema(closes, period: 50)[last]
        let ema200 = Indicators.ema(closes, period: 200)[last]
        let ema20 = Indicators.ema(closes, period: 20)[last]
        let (fastTrend, slowTrend, trendLabel): (Double?, Double?, String) =
            ema200 != nil ? (ema50, ema200, "EMA 50/200") : (ema20, ema50, "EMA 20/50")
        if ema200 == nil {
            warnings.append("Moins de 200 bougies : tendance de long terme estimée sur EMA 20/50.")
        }
        if let fast = fastTrend, let slow = slowTrend {
            let raw = 0.5 * sign(fast - slow) + 0.5 * sign(price - slow)
            let strength = adx.map { clamp(($0 - 15) / 20, 0.3, 1) } ?? 0.6
            let direction = raw > 0 ? "haussière" : raw < 0 ? "baissière" : "neutre"
            let adxText = adx.map { String(format: " · ADX %.0f", $0) } ?? ""
            factors.append(SignalFactor(name: "Tendance", score: raw * strength, weight: cfg.weightTrend,
                                        detail: "\(trendLabel) \(direction)\(adxText)"))
        }
        if let adx, adx < 18 {
            warnings.append("Marché sans tendance (ADX < 18) : signaux moins fiables.")
        }

        // 2. MACD : croisements récents et pente de l'histogramme.
        let macd = Indicators.macd(closes)
        if let h = macd.histogram[last], let hPrev = macd.histogram[last - 1] {
            var score = 0.5 * sign(h)
            var detail = h >= 0 ? "Histogramme positif" : "Histogramme négatif"
            let recent = (max(1, last - 3)...last).compactMap { i -> Double? in
                guard let a = macd.histogram[i - 1], let b = macd.histogram[i] else { return nil }
                if a <= 0 && b > 0 { return 1 }
                if a >= 0 && b < 0 { return -1 }
                return nil
            }
            if let cross = recent.last {
                score = cross
                detail = cross > 0 ? "Croisement haussier récent" : "Croisement baissier récent"
            } else if h > hPrev {
                score += 0.25
                detail += ", en hausse"
            } else if h < hPrev {
                score -= 0.25
                detail += ", en baisse"
            }
            factors.append(SignalFactor(name: "MACD", score: score, weight: cfg.weightMACD, detail: detail))
        }

        // 3. RSI : survente / surachat, sinon momentum doux.
        let rsiSeries = Indicators.rsi(closes)
        if let rsi = rsiSeries[last] {
            let prev = rsiSeries[last - 1] ?? rsi
            let score: Double
            if rsi < 30 {
                score = min(1, 0.5 + (30 - rsi) / 20) * (rsi > prev ? 1 : 0.7)
            } else if rsi > 70 {
                score = -min(1, 0.5 + (rsi - 70) / 20) * (rsi < prev ? 1 : 0.7)
            } else {
                score = (rsi - 50) / 40
            }
            let zone = rsi < 30 ? "survente" : rsi > 70 ? "surachat" : "zone neutre"
            factors.append(SignalFactor(name: "RSI", score: score, weight: cfg.weightRSI,
                                        detail: String(format: "RSI %.1f (%@)", rsi, zone)))
        }

        // 4. Stochastique : croisements en zones extrêmes.
        let stoch = Indicators.stochastic(data)
        if let k = stoch.k[last], let d = stoch.d[last], let kPrev = stoch.k[last - 1], let dPrev = stoch.d[last - 1] {
            var score = 0.0
            var detail = String(format: "%%K %.0f / %%D %.0f", k, d)
            if kPrev <= dPrev && k > d && k < 30 {
                score = 1; detail += " · croisement haussier en survente"
            } else if kPrev >= dPrev && k < d && k > 70 {
                score = -1; detail += " · croisement baissier en surachat"
            } else if k < 20 {
                score = 0.5
            } else if k > 80 {
                score = -0.5
            }
            factors.append(SignalFactor(name: "Stochastique", score: score, weight: cfg.weightStochastic, detail: detail))
        }

        // 5. Bollinger : retour à la moyenne.
        let bands = Indicators.bollinger(closes)
        if let up = bands.upper[last], let low = bands.lower[last], up > low {
            let pb = (price - low) / (up - low)
            let score: Double = pb < 0 ? 0.8 : pb > 1 ? -0.8 : (0.5 - pb) * 0.6
            factors.append(SignalFactor(name: "Bollinger", score: score, weight: cfg.weightBollinger,
                                        detail: String(format: "%%B %.2f", pb)))
        }

        // 6. Volume : pente de l'OBV exprimée en volume moyen par bougie.
        let obv = Indicators.obv(data)
        let avgVolume = data.suffix(20).map(\.volume).reduce(0, +) / 20
        if avgVolume > 0, let slope = rawSlope(Array(obv.suffix(20))) {
            let perBar = slope / avgVolume
            let score = clamp(perBar / 0.3, -1, 1)
            factors.append(SignalFactor(name: "Volume (OBV)", score: score, weight: cfg.weightVolume,
                                        detail: perBar >= 0 ? "Accumulation" : "Distribution"))
        } else {
            warnings.append("Volume indisponible : facteur volume ignoré.")
        }

        // 7. Confirmation par l'unité de temps supérieure.
        var higherScore: Double?
        if let higher = higherTimeframe?.sanitized(), higher.count >= 50 {
            let hc = higher.closes
            let hl = hc.count - 1
            let fast = Indicators.ema(hc, period: 20)[hl]
            let slow = Indicators.ema(hc, period: 50)[hl]
            if let fast, let slow {
                let s = 0.5 * sign(fast - slow) + 0.5 * sign(hc[hl] - slow)
                higherScore = s
                factors.append(SignalFactor(name: "UT supérieure", score: s, weight: cfg.weightHigherTimeframe,
                                            detail: s > 0 ? "Tendance de fond haussière" : s < 0 ? "Tendance de fond baissière" : "Neutre"))
            }
        }

        // Agrégation.
        let totalWeight = factors.reduce(0) { $0 + $1.weight }
        let score = totalWeight > 0 ? factors.reduce(0) { $0 + $1.score * $1.weight } / totalWeight * 100 : 0
        let directional = factors.filter { $0.score != 0 }
        let agreeingWeight = directional.filter { sign($0.score) == sign(score) }.reduce(0) { $0 + $1.weight }
        let directionalWeight = directional.reduce(0) { $0 + $1.weight }
        let agreement = directionalWeight > 0 ? agreeingWeight / directionalWeight : 0
        let confidence = clamp(abs(score) * 1.25, 0, 100) * (0.5 + 0.5 * agreement)

        var action: SignalAction
        switch score {
        case cfg.strongThreshold...: action = .strongBuy
        case cfg.buyThreshold...: action = .buy
        case ...(-cfg.strongThreshold): action = .strongSell
        case ...(-cfg.buyThreshold): action = .sell
        default: action = .hold
        }

        // Ne jamais acheter contre la tendance de fond : on rétrograde le signal.
        if let h = higherScore {
            if action.isBuy && h < 0 {
                action = action == .strongBuy ? .buy : .hold
                warnings.append("Achat contre la tendance de l'unité supérieure : signal rétrogradé.")
            } else if action.isSell && h > 0 {
                action = action == .strongSell ? .sell : .hold
                warnings.append("Vente contre la tendance de l'unité supérieure : signal rétrogradé.")
            }
        }

        if atr > 0, atr / price > 0.08 {
            warnings.append(String(format: "Volatilité extrême (ATR %.1f %% du prix) : réduisez la taille.", atr / price * 100))
        }
        if let timeframe, now.timeIntervalSince(data[last].time) > timeframe.seconds * 3 {
            warnings.append("Données possiblement périmées (dernière bougie ancienne).")
        }

        var plan: TradePlan?
        if atr > 0 {
            let distance = cfg.stopATR * atr
            if action.isSell {
                plan = TradePlan(entry: price, stopLoss: price + distance, takeProfit: max(price - distance * cfg.rewardRisk, price * 0.01))
            } else {
                plan = TradePlan(entry: price, stopLoss: max(price - distance, price * 0.01), takeProfit: price + distance * cfg.rewardRisk)
            }
        }

        return Signal(action: action, score: score, confidence: confidence, price: price, time: data[last].time,
                      factors: factors, plan: plan, warnings: warnings)
    }

    private func rawSlope(_ values: [Double]) -> Double? {
        guard values.count > 1 else { return nil }
        let n = Double(values.count)
        let meanX = (n - 1) / 2
        let meanY = values.reduce(0, +) / n
        var num = 0.0, den = 0.0
        for (i, y) in values.enumerated() {
            num += (Double(i) - meanX) * (y - meanY)
            den += (Double(i) - meanX) * (Double(i) - meanX)
        }
        return den > 0 ? num / den : nil
    }
}

@inline(__always) func sign(_ x: Double) -> Double { x > 0 ? 1 : x < 0 ? -1 : 0 }
@inline(__always) func clamp(_ x: Double, _ lo: Double, _ hi: Double) -> Double { min(hi, max(lo, x)) }
