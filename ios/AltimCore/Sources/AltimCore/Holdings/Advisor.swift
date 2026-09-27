import Foundation

/// Altim's advice on an asset, in plain language (port of `web/src/engine/advice.ts`).
/// Altim never places orders: it advises.
public enum Advisor {
    public enum Tone: String, Sendable { case buy, hold, sell, unknown }

    public struct Advice: Sendable, Hashable {
        public let tone: Tone
        public let title: String
        public let points: [String]
        public let entry: Double?
        public let stop: Double?
        public let target: Double?
        /// Prudent amount (USD) if the user's wealth is known.
        public let amount: Double?
        /// Corresponding quantity (whole shares for a stock).
        public var quantity: Double? = nil
    }

    /// Track record of the buy signals on one asset (backtest on the same candles, fees included).
    public struct TrackRecord: Sendable, Hashable {
        public let trades: Int
        public let winRate: Double
        public let avgReturn: Double
        public init(trades: Int, winRate: Double, avgReturn: Double) {
            self.trades = trades
            self.winRate = winRate
            self.avgReturn = avgReturn
        }
        public init(_ r: Backtester.Result) {
            let n = r.trades.count
            trades = n
            winRate = n > 0 ? Double(r.trades.filter { $0.returnPercent > 0 }.count) / Double(n) * 100 : 0
            avgReturn = n > 0 ? r.trades.reduce(0) { $0 + $1.returnPercent } / Double(n) : 0
        }
    }

    /// Market guard context (MarketGuard): shock level and counter-trend reversal risk.
    public struct GuardContext: Sendable, Hashable {
        public let shock: MarketGuard.ShockLevel
        public let reversalScore: Int
        public let reversalDirection: MarketGuard.Direction?
        public init(shock: MarketGuard.ShockLevel, reversalScore: Int, reversalDirection: MarketGuard.Direction?) {
            self.shock = shock
            self.reversalScore = reversalScore
            self.reversalDirection = reversalDirection
        }
        public init(_ r: MarketGuard.Result) {
            self.init(shock: r.shock.level, reversalScore: r.reversal.score, reversalDirection: r.reversal.direction)
        }
    }

    /// Below this track record on the asset itself, a buy signal is not advised (same rule in advice.ts).
    public static let minTrackTrades = 5
    public static let minWinRate = 40.0

    static func usd(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(0...(v >= 100 ? 0 : 2))).locale(Locale(identifier: "fr_FR"))) + " $"
    }

    /// Price to the cent above 1 $, 4 significant digits below (0,000009312 $ for PEPE, not 0,00001 $).
    static func px(_ v: Double) -> String {
        let fr = Locale(identifier: "fr_FR")
        return (v >= 1 ? v.formatted(.number.precision(.fractionLength(2)).locale(fr))
                       : v.formatted(.number.precision(.significantDigits(1...4)).locale(fr))) + " $"
    }

    static func two(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(0...2)).locale(Locale(identifier: "fr_FR")))
    }

    static func pct(_ v: Double) -> String { "\(v >= 0 ? "+" : "−")\(one(abs(v))) %" }

    /// Quantity to buy or sell: whole shares for a stock, 6 significant digits (rounded down) for a crypto.
    public static func quantity(amount: Double, price: Double, kind: AssetClass) -> Double {
        guard amount > 0, price > 0 else { return 0 }
        let raw = amount / price
        if kind == .stock { return (raw + 1e-9).rounded(.down) }
        let step = pow(10, floor(log10(raw)) - 5)
        return ((raw / step + 1e-9).rounded(.down)) * step
    }

    public static func quantityText(_ q: Double, kind: AssetClass, symbol: String) -> String {
        if kind == .stock { return "\(Int(q)) action\(q > 1 ? "s" : "") \(symbol)" }
        return q.formatted(.number.precision(.significantDigits(1...6)).locale(Locale(identifier: "fr_FR"))) + " \(symbol)"
    }

    static func one(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "fr_FR")))
    }

    public static func advise(signal: Signal?, reliability: ReliabilityLevel?, price: Double?,
                              line: HoldingsAnalyzer.Line?, capital: Double?, risk: RiskSettings,
                              track: TrackRecord? = nil, symbol: String = "", kind: AssetClass = .crypto,
                              guard guardContext: GuardContext? = nil) -> Advice {
        let reversalDown = guardContext.map { $0.reversalDirection == .down && $0.reversalScore >= MarketGuard.reversalHigh } ?? false
        let agitated = guardContext?.shock == .agitated
        // Asset already held: the portfolio analysis takes precedence.
        if let line {
            let tone: Tone = switch line.recommendation {
            case .sell, .protect: .sell
            case .strengthen: .buy
            case .unknown: .unknown
            case .lighten, .hold: .hold
            }
            var points = ["Vous en détenez \(line.quantity.formatted(.number.precision(.fractionLength(0...8)).locale(Locale(identifier: "fr_FR")))) (\(usd(line.value)), \(line.pnl >= 0 ? "+" : "−")\(usd(abs(line.pnl))) depuis l'achat), soit \(one(line.weight)) % de votre patrimoine."]
            points += line.reasons.map { HoldingsAnalyzer.reasonText[$0] ?? $0 }
            if line.recommendation == .lighten && line.trimValue > 0 {
                let q = line.price.map { quantity(amount: line.trimValue, price: $0, kind: line.kind) } ?? 0
                points.append("Montant à alléger conseillé : environ \(usd(line.trimValue))\(q > 0 ? ", soit \(quantityText(q, kind: line.kind, symbol: line.symbol))" : "").")
            }
            if let stop = line.stop {
                points.append("Stop de protection conseillé : \(px(stop)) (si le cours passe dessous, sortir limite la perte à ≈ \(usd(line.lossAtStop ?? 0))).")
            }
            if guardContext?.shock == .shock {
                points.append("Marché en choc (mouvements anormaux) : ne renforcez pas maintenant, vérifiez que votre stop est bien en place.")
            }
            if reversalDown {
                points.append("Risque de retournement à la baisse élevé (\(guardContext!.reversalScore)/100) : resserrez votre stop ou prenez une partie de vos gains.")
            }
            let finalTone: Tone = tone == .buy && (guardContext?.shock == .shock || reversalDown) ? .hold : tone
            return Advice(tone: finalTone, title: line.recommendation.label, points: points, entry: nil, stop: line.stop, target: nil, amount: nil)
        }

        guard let signal, let price, price > 0 else {
            return Advice(tone: .unknown, title: "Pas de conseil pour l'instant",
                          points: ["Historique ou cours insuffisant pour analyser cet actif."], entry: nil, stop: nil, target: nil, amount: nil)
        }
        if reliability == .low {
            return Advice(tone: .unknown, title: "Pas de conseil pour l'instant",
                          points: ["Les sources de données sont absentes ou en désaccord : Altim préfère ne rien conseiller plutôt que de mal conseiller."],
                          entry: nil, stop: nil, target: nil, amount: nil)
        }
        let caution = reliability == .medium ? ["Fiabilité des données moyenne (peu de sources indépendantes) : restez prudent."] : []

        // Market guard: no buy in a shock or when a reversal against the rise is likely.
        if signal.action == .buy || signal.action == .strongBuy {
            if guardContext?.shock == .shock {
                return Advice(tone: .hold, title: "Attendre : marché en choc",
                              points: ["Les indicateurs sont à l'achat, mais le marché fait des mouvements anormaux (volatilité, sauts de prix) : attendez que la tempête passe."] + caution,
                              entry: nil, stop: nil, target: nil, amount: nil)
            }
            if reversalDown {
                return Advice(tone: .hold, title: "Attendre : risque de retournement",
                              points: ["Les indicateurs sont à l'achat, mais un retournement à la baisse est probable (\(guardContext!.reversalScore)/100 : excès, foule trop optimiste ou actualités défavorables). Attendez qu'il se produise ou soit écarté."] + caution,
                              entry: nil, stop: nil, target: nil, amount: nil)
            }
        }
        // A buy signal must have worked on this very asset: otherwise, wait.
        if signal.action == .buy || signal.action == .strongBuy, let t = track, t.trades >= minTrackTrades,
           t.winRate < minWinRate || t.avgReturn <= 0 {
            return Advice(tone: .hold, title: "Attendre : signal peu fiable sur cet actif",
                          points: ["Les indicateurs sont à l'achat, mais sur l'historique de cet actif ce signal n'a réussi que \(Int(t.winRate.rounded())) % du temps (\(t.trades) signaux, \(pct(t.avgReturn)) en moyenne par signal, frais inclus).",
                                   "Altim préfère attendre un meilleur point d'entrée plutôt que de suivre un signal qui a surtout échoué ici."] + caution,
                          entry: nil, stop: nil, target: nil, amount: nil)
        }

        switch signal.action {
        case .buy, .strongBuy:
            var points: [String] = []
            var amount: Double?
            var qty: Double?
            var entry: Double?, stop: Double?, target: Double?
            if let p = signal.plan {
                let distance = p.entry - p.stopLoss
                let plan = TradePlan(entry: price, stopLoss: price - distance, takeProfit: price + distance * 2)
                if plan.stopLoss > 0 {
                    entry = plan.entry; stop = plan.stopLoss; target = plan.takeProfit
                    points.append("Zone d'entrée : autour de \(px(plan.entry)). Stop conseillé : \(px(plan.stopLoss)). Objectif : \(px(plan.takeProfit)) (gain potentiel \(one(plan.riskReward)) fois le risque).")
                    if let capital, capital > 0, let size = RiskManager(settings: risk).positionSize(equity: capital, plan: plan) {
                        // Agitated market: half the amount (same rule as the guard's policy for bots).
                        let q = quantity(amount: size.notional * (agitated ? 0.5 : 1), price: plan.entry, kind: kind)
                        qty = q
                        // Whole shares: the amount and the loss at the stop are those of the rounded quantity.
                        let scale = q > 0 && (kind == .stock || agitated) ? q * plan.entry / size.notional : agitated ? 0.5 : 1
                        amount = size.notional * scale
                        let qtyText = q > 0 && !symbol.isEmpty ? ", soit \(quantityText(q, kind: kind, symbol: symbol))"
                            : kind == .stock && !symbol.isEmpty ? " (moins d'une action entière : il faudrait des fractions d'action)" : ""
                        points.append("Avec votre patrimoine (\(usd(capital))), n'y consacrez pas plus d'environ \(usd(size.notional * scale))\(qtyText) : si le stop est touché, la perte resterait limitée à ≈ \(usd(size.riskAmount * scale)) (\(two(risk.riskPerTradePercent * scale)) % du patrimoine, frais inclus).")
                    } else {
                        points.append("Renseignez vos avoirs dans « Mes avoirs » pour obtenir un montant adapté à votre patrimoine.")
                    }
                }
            }
            if agitated { points.append("Marché agité : le montant conseillé est divisé par deux, et un stop plus large évite d'être sorti par le bruit.") }
            points.append("Confiance du signal : \(Int(signal.confidence.rounded())) %. \(signal.action == .strongBuy ? "Les indicateurs sont largement d'accord." : "Signal modéré : entrez progressivement.")")
            if let t = track, t.trades >= minTrackTrades {
                points.append("Sur l'historique de cet actif : \(t.trades) signaux d'achat, \(Int(t.winRate.rounded())) % gagnants, \(pct(t.avgReturn)) en moyenne par signal (frais inclus). Les performances passées ne préjugent pas des performances futures.")
            } else if let t = track {
                points.append("Seulement \(t.trades) signal\(t.trades > 1 ? "s" : "") d'achat dans l'historique de cet actif : pas assez pour juger de sa fiabilité, prudence.")
            }
            return Advice(tone: .buy, title: signal.action == .strongBuy ? "Achat envisageable (signal fort)" : "Achat envisageable",
                          points: points + caution, entry: entry, stop: stop, target: target, amount: amount, quantity: qty)
        case .sell, .strongSell:
            return Advice(tone: .sell, title: "À éviter pour l'instant",
                          points: ["Tendance baissière : ce n'est pas le moment d'acheter.",
                                   "Si vous en détenez, ajoutez-le dans « Mes avoirs » pour savoir s'il faut protéger ou alléger votre position."] + caution,
                          entry: nil, stop: nil, target: nil, amount: nil)
        case .hold:
            let orientation = signal.score > 10 ? "légère orientation haussière" : signal.score < -10 ? "légère orientation baissière" : "neutre"
            return Advice(tone: .hold, title: "Attendre",
                          points: ["Pas de signal clair (score \(signal.score >= 0 ? "+" : "")\(Int(signal.score.rounded()))/100, \(orientation)).",
                                   "Mieux vaut attendre un signal d'achat confirmé par plusieurs indicateurs."] + caution,
                          entry: nil, stop: nil, target: nil, amount: nil)
        }
    }
}
