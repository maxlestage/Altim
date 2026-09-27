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
    }

    static func usd(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(0...(v >= 100 ? 0 : 2))).locale(Locale(identifier: "fr_FR"))) + " $"
    }

    static func px(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(0...(v >= 1 ? 2 : 6))).locale(Locale(identifier: "fr_FR"))) + " $"
    }

    static func one(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "fr_FR")))
    }

    public static func advise(signal: Signal?, reliability: ReliabilityLevel?, price: Double?,
                              line: HoldingsAnalyzer.Line?, capital: Double?, risk: RiskSettings) -> Advice {
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
                points.append("Montant à alléger conseillé : environ \(usd(line.trimValue)).")
            }
            if let stop = line.stop {
                points.append("Stop de protection conseillé : \(px(stop)) (si le cours passe dessous, sortir limite la perte à ≈ \(usd(line.lossAtStop ?? 0))).")
            }
            return Advice(tone: tone, title: line.recommendation.label, points: points, entry: nil, stop: line.stop, target: nil, amount: nil)
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

        switch signal.action {
        case .buy, .strongBuy:
            var points: [String] = []
            var amount: Double?
            var entry: Double?, stop: Double?, target: Double?
            if let p = signal.plan {
                let distance = p.entry - p.stopLoss
                let plan = TradePlan(entry: price, stopLoss: price - distance, takeProfit: price + distance * 2)
                if plan.stopLoss > 0 {
                    entry = plan.entry; stop = plan.stopLoss; target = plan.takeProfit
                    points.append("Zone d'entrée : autour de \(px(plan.entry)). Stop conseillé : \(px(plan.stopLoss)). Objectif : \(px(plan.takeProfit)) (gain potentiel \(one(plan.riskReward)) fois le risque).")
                    if let capital, capital > 0, let size = RiskManager(settings: risk).positionSize(equity: capital, plan: plan) {
                        amount = size.notional
                        points.append("Avec votre patrimoine (\(usd(capital))), n'y consacrez pas plus d'environ \(usd(size.notional)) : si le stop est touché, la perte resterait limitée à ≈ \(usd(size.riskAmount)) (\(one(risk.riskPerTradePercent)) % du patrimoine).")
                    } else {
                        points.append("Renseignez vos avoirs dans « Mes avoirs » pour obtenir un montant adapté à votre patrimoine.")
                    }
                }
            }
            points.append("Confiance du signal : \(Int(signal.confidence.rounded())) %. \(signal.action == .strongBuy ? "Les indicateurs sont largement d'accord." : "Signal modéré : entrez progressivement.")")
            return Advice(tone: .buy, title: signal.action == .strongBuy ? "Achat envisageable (signal fort)" : "Achat envisageable",
                          points: points + caution, entry: entry, stop: stop, target: target, amount: amount)
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
