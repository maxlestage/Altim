import Foundation

// Shape of GET /api/decision (backend/src/engine/decision_types.rs): one decision per asset, with its reasons, what
// would change it and what could make it wrong. Informational by default; "personal" only when the user's average
// cost or portfolio weights are passed (the server never keeps them).
//
// Enums decode an unknown value as `.unknown` (a new server value never breaks decoding); the French labels shown
// come from the server (`label`, `levelLabel`) whenever it gives them.

public struct Decision: Codable, Sendable {
    public enum Verdict: String, Codable, Sendable {
        case buy, buyZone, wait, noPosition, trim, sell, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var label: String {
            switch self {
            case .buy: return "ACHETER"
            case .buyZone: return "ZONE D'ACHAT"
            case .wait: return "ATTENDRE"
            case .noPosition: return "AUCUNE POSITION"
            case .trim: return "ALLÉGER"
            case .sell: return "VENDRE"
            case .unknown: return "—"
            }
        }
    }

    /// 🟢 strong · 🟡 moderate · ⚪ waiting · 🟠 highRisk · 🔴 exit
    public enum Level: String, Codable, Sendable {
        case strong, moderate, waiting, highRisk, exit, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var emoji: String {
            switch self {
            case .strong: return "🟢"
            case .moderate: return "🟡"
            case .waiting, .unknown: return "⚪"
            case .highRisk: return "🟠"
            case .exit: return "🔴"
            }
        }
        public var label: String {
            switch self {
            case .strong: return "Signal fort"
            case .moderate: return "Signal modéré"
            case .waiting: return "Attente"
            case .highRisk: return "Risque élevé"
            case .exit: return "Sortie / risque d'invalidation"
            case .unknown: return "Niveau inconnu"
            }
        }
    }

    public enum Status: String, Codable, Sendable {
        case positive, neutral, negative, unavailable, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var label: String {
            switch self {
            case .positive: return "Favorable"
            case .neutral: return "Neutre"
            case .negative: return "Défavorable"
            case .unavailable, .unknown: return "Non disponible"
            }
        }
        public var tone: Tone {
            switch self {
            case .positive: return .good
            case .negative: return .bad
            case .neutral: return .warn
            case .unavailable, .unknown: return .neutral
            }
        }
    }

    public struct Family: Codable, Sendable, Identifiable {
        /// "trend" | "momentum" | "volume" | "volatility" | "valuation" | "fundamentals" | "macro" | "sentiment" | "news" | "liquidity" | "onchain"
        public var key: String
        public var label: String
        /// −100 … +100; nil when unavailable.
        public var score: Double?
        public var status: Status
        public var summary: String
        public var points: [String]
        public var source: String
        public var id: String { key }
    }

    /// A reason that forbids buying now; all checks are listed, active or not.
    public struct Veto: Codable, Sendable, Identifiable {
        public var code: String
        public var label: String
        public var active: Bool
        /// false: no source to check it, shown as "non vérifiable".
        public var verifiable: Bool
        public var detail: String
        public var id: String { code }
    }

    public enum StepState: String, Codable, Sendable {
        case ok, no, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
    }

    public struct Step: Codable, Sendable {
        public var label: String
        public var state: StepState
        public var detail: String
    }

    public struct Setup: Codable, Sendable {
        public var name: String
        public var steps: [Step]
        public var met: Int
        public var total: Int
    }

    /// Entry plan: buy zone, stop, two targets, risk/reward (gain to target 1 ÷ risk to the stop).
    public struct Plan: Codable, Sendable {
        public var zoneFrom: Double
        public var zoneTo: Double
        public var entry: Double
        public var stop: Double
        public var target1: Double
        public var target2: Double?
        public var riskPct: Double
        public var reward1Pct: Double
        public var reward2Pct: Double?
        public var riskReward: Double
        public var minRiskReward: Double
        public var acceptable: Bool
        public var horizon: String
    }

    public struct Condition: Codable, Sendable {
        public var text: String
        public var level: Double?
    }

    public enum ScenarioKind: String, Codable, Sendable {
        case bull, neutral, bear, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
    }

    public struct Scenario: Codable, Sendable {
        public var kind: ScenarioKind
        public var title: String
        public var condition: String
        public var consequence: String
        public var level: Double?
    }

    public enum Uncertainty: String, Codable, Sendable {
        case low, medium, high, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var label: String {
            switch self {
            case .low: return "faible"
            case .medium: return "moyenne"
            case .high: return "élevée"
            case .unknown: return "inconnue"
            }
        }
    }

    /// "Pourquoi pas ?": what could make this decision wrong.
    public struct WhyNot: Codable, Sendable {
        public var risks: [String]
        public var uncertainty: Uncertainty
        public var invalidation: [String]
    }

    public struct EarningsDate: Codable, Sendable {
        /// ms, New York date.
        public var date: Double
        /// Estimated by the data vendor, not announced by the company.
        public var estimated: Bool
    }

    public struct EarningsSurprise: Codable, Sendable {
        public var quarter: String
        public var eps: Double
        public var consensus: Double
        public var surprisePct: Double
    }

    public struct Revisions: Codable, Sendable {
        public var monthAgo: Double
        public var now: Double
        public var changePct: Double
    }

    /// Company figures from its filings, trailing twelve months. nil = not reported.
    public struct StockFundamentals: Codable, Sendable {
        public var period: String
        public var revenue: Double?
        public var revenueGrowth: Double?
        public var netIncome: Double?
        public var eps: Double?
        public var epsGrowth: Double?
        public var grossMargin: Double?
        public var operatingMargin: Double?
        public var netMargin: Double?
        public var freeCashFlow: Double?
        public var fcfMargin: Double?
        public var debt: Double?
        public var cash: Double?
        public var netDebt: Double?
        public var roe: Double?
        public var per: Double?
        public var peg: Double?
        public var evEbitda: Double?
        public var dividendYield: Double?
        public var shareChange: Double?
        public var nextEarnings: EarningsDate?
        public var surprises: [EarningsSurprise]
        public var revisions: Revisions?
        public var sectorNote: String
        public var source: String
        // Added later (absent from older answers, hence optional).
        /// Price ÷ sales and price ÷ book (ratios).
        public var ps: Double?
        public var pb: Double?
        /// Return on invested capital, % (operating income after tax ÷ (debt + equity − cash)).
        public var roic: Double?
        /// Tax rate used for the ROIC, %.
        public var roicTaxRate: Double?
        /// true: the 21 % US federal statutory rate (effective rate not computable).
        public var roicTaxStatutory: Bool?
        /// End of the last period filed and date of that filing (ms, midnight UTC).
        public var periodEnd: Double?
        public var filedAt: Double?
        public var sector: Sector?
        public var valuationHistory: ValuationHistory?
        public var peers: PeerComparison?
        /// Valuation vs growth in one sentence.
        public var valuationVerdict: String?
        /// Management guidance, or why it is not given.
        public var guidance: String?
    }

    /// Sector of a company: its SIC code as filed at the SEC.
    public struct Sector: Codable, Sendable {
        public var label: String
        public var sic: String
        public var sicDescription: String
        public var source: String
    }

    /// A valuation ratio against its own daily history; `percentile`: % of days at or below today's value.
    public struct RatioHistory: Codable, Sendable {
        public var current: Double
        public var median: Double
        public var min: Double
        public var max: Double
        public var percentile: Double
        public var days: Double
        public var from: Double
        public var to: Double
    }

    /// P/E and P/S against their own history (up to 5 years of daily closes).
    public struct ValuationHistory: Codable, Sendable {
        public var per: RatioHistory?
        public var ps: RatioHistory?
        public var method: String
        public var source: String
    }

    /// A comparable company (same activity), figures from its own filings.
    public struct Peer: Codable, Sendable {
        public var symbol: String
        public var name: String
        public var per: Double?
        public var ps: Double?
        public var operatingMargin: Double?
        public var netMargin: Double?
        public var revenueGrowth: Double?
        public var periodEnd: Double
    }

    /// Comparison with companies of the same activity: medians of the peers whose figures were actually read.
    public struct PeerComparison: Codable, Sendable {
        public var group: String
        public var peers: [Peer]
        public var medianPer: Double?
        public var medianPs: Double?
        public var medianOperatingMargin: Double?
        public var medianNetMargin: Double?
        public var medianRevenueGrowth: Double?
        public var date: Double
        public var source: String
    }

    /// Developer activity of the project's code (CoinGecko, else its main GitHub repository).
    public struct DevActivity: Codable, Sendable {
        public var repo: String?
        public var commits4w: Double?
        public var pullRequestsMerged: Double?
        public var contributors: Double?
        public var stars: Double?
        public var additions4w: Double?
        public var deletions4w: Double?
        public var smartContractPlatform: Bool?
        public var source: String
    }

    /// Stablecoins in circulation (USD value): the crypto market's cash.
    public struct StablecoinFlows: Codable, Sendable {
        /// "Tous réseaux" or the chain's name.
        public var scope: String
        public var date: Double
        public var total: Double
        public var change7d: Double?
        public var change7dPct: Double?
        public var change30d: Double?
        public var change30dPct: Double?
        public var source: String
    }

    /// Token and network figures. nil = not given by a free verifiable source.
    public struct CryptoFundamentals: Codable, Sendable {
        public var marketCap: Double?
        public var fdv: Double?
        public var mcFdv: Double?
        public var circulatingSupply: Double?
        public var totalSupply: Double?
        public var maxSupply: Double?
        public var circulatingPct: Double?
        public var tvl: Double?
        public var fees30d: Double?
        public var btcDominance: Double?
        /// Funding of perpetual futures per 8 hours, as a fraction (0.0000688 = 0.00688 %).
        public var fundingRate: Double?
        public var openInterest: Double?
        public var txPerDay: Double?
        public var hashRate: Double?
        public var unlocks: String
        public var source: String
        // Added later (absent from older answers, hence optional).
        public var devActivity: DevActivity?
        /// All chains, and the asset's own chain when it is one.
        public var stablecoins: StablecoinFlows?
        public var chainStablecoins: StablecoinFlows?
        /// What is not covered and why. Present in every newer answer: it tells a missing `devActivity` (unavailable)
        /// from an older server that did not know the field.
        public var notCovered: String?
        public var knowsDevActivity: Bool { notCovered != nil }
    }

    /// Tagged by "kind" ("stock" | "crypto"); another kind decodes as `.unknown`.
    public enum Fundamentals: Codable, Sendable {
        case stock(StockFundamentals)
        case crypto(CryptoFundamentals)
        case unknown

        private enum Key: String, CodingKey { case kind }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: Key.self)
            switch try c.decodeIfPresent(String.self, forKey: .kind) {
            case "stock": self = .stock(try StockFundamentals(from: decoder))
            case "crypto": self = .crypto(try CryptoFundamentals(from: decoder))
            default: self = .unknown
            }
        }

        public func encode(to encoder: Encoder) throws {
            switch self {
            case let .stock(s):
                try s.encode(to: encoder)
                var c = encoder.container(keyedBy: Key.self)
                try c.encode("stock", forKey: .kind)
            case let .crypto(x):
                try x.encode(to: encoder)
                var c = encoder.container(keyedBy: Key.self)
                try c.encode("crypto", forKey: .kind)
            case .unknown:
                var c = encoder.container(keyedBy: Key.self)
                try c.encode("unknown", forKey: .kind)
            }
        }
    }

    public struct Liquidity: Codable, Sendable {
        /// Already in % (0.0012 = 0,0012 %).
        public var spreadPct: Double?
        public var dailyValue: Double?
        public var relativeVolume: Double?
        public var source: String
    }

    /// Past behaviour of the signal on this asset (walk-forward, fees and slippage included).
    public struct Track: Codable, Sendable {
        public var period: String
        public var trades: Int
        public var winRate: Double
        public var avgWin: Double?
        public var avgLoss: Double?
        public var profitFactor: Double?
        public var sharpe: Double?
        public var sortino: Double?
        public var maxDrawdown: Double
        public var totalReturn: Double
        public var buyAndHold: Double
        public var feesPct: Double
        public var slippagePct: Double
        public var losingStreak: Int
        public var note: String
        // Added later (absent from older answers): spread cost, expectancy, R multiples, results by market regime.
        /// Full bid/ask spread used (%), measured on the order book or assumed.
        public var spreadPct: Double?
        public var spreadMeasured: Bool?
        public var spreadNote: String?
        /// % per trade, costs included.
        public var expectancy: Double?
        /// Average of (trade return ÷ initial risk to the stop).
        public var avgR: Double?
        public var regimes: [RegimeStat]?
        /// Daily candles tested (after the warm-up).
        public var testedBars: Int?
        /// How the test avoids flattering itself.
        public var biasNotes: [String]?

        /// Older answers (before these fields) show nothing more.
        public var hasDetails: Bool { regimes != nil || expectancy != nil }
        /// French flat tax (PFU) on the net gain, paid at the end: an assumption, not the user's own situation.
        public static let flatTax = 30.0
        /// Return of the signal after the flat tax (a loss is not taxed).
        public var afterTaxReturn: Double { totalReturn > 0 ? totalReturn * (1 - Self.flatTax / 100) : totalReturn }
    }

    /// Results of the signal by market regime on the signal candle (read without future data).
    public struct RegimeStat: Codable, Sendable {
        /// "bull" | "bear" | "range" | "crisis" | "unknown"
        public var regime: String
        public var label: String
        public var trades: Int
        public var winRate: Double?
        public var avgReturn: Double?
        /// Fewer than 5 trades: "échantillon trop faible".
        public var lowSample: Bool
    }

    public enum ExitKind: String, Codable, Sendable {
        case profit, defensive, macro, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var label: String {
            switch self {
            case .profit: return "Prise de bénéfices"
            case .defensive: return "Protection"
            case .macro: return "Choc de marché"
            case .unknown: return "Sortie"
            }
        }
    }

    /// A step of a progressive exit ("vendre 20 % si …").
    public struct Exit: Codable, Sendable {
        public var kind: ExitKind
        public var share: Double
        public var trigger: String
        public var price: Double?
        /// Condition already met now.
        public var now: Bool
    }

    /// Only when the average cost was passed.
    public struct Position: Codable, Sendable {
        public var cost: Double
        public var pnlPct: Double?
        public var advice: String
        public var exits: [Exit]
    }

    /// Only when the portfolio weights were passed: exposure to the same risk factor.
    public struct Exposure: Codable, Sendable {
        public var factor: String
        public var weight: Double
        public var assets: [String]
        public var correlation: Double?
        public var warning: String?
    }

    public struct DataSource: Codable, Sendable {
        public var name: String
        public var ok: Bool
        public var detail: String
    }

    public var symbol: String
    public var kind: Kind
    public var name: String
    public var asOf: Double
    public var price: Double?
    /// "informational" | "personal"
    public var mode: String
    public var verdict: Verdict
    public var label: String
    public var level: Level
    public var levelLabel: String
    /// 0–100.
    public var confidence: Double
    public var confidenceText: String
    public var headline: String
    public var families: [Family]
    public var vetoes: [Veto]
    public var blocked: Bool
    public var setup: Setup
    public var plan: Plan?
    public var whyWait: [String]
    public var toBuy: [Condition]
    public var toSell: [Condition]
    public var scenarios: [Scenario]
    public var pros: [String]
    public var cons: [String]
    public var whyNot: WhyNot
    public var fundamentals: Fundamentals?
    public var liquidity: Liquidity?
    public var track: Track?
    public var position: Position?
    public var exposure: Exposure?
    public var sources: [DataSource]
    public var disclaimer: String

    public var isPersonal: Bool { mode == "personal" }
    /// Active vetoes first, then the checked ones, the unverifiable last.
    public var sortedVetoes: [Veto] {
        vetoes.enumerated().sorted { a, b in
            let ra = a.element.active ? 0 : a.element.verifiable ? 1 : 2
            let rb = b.element.active ? 0 : b.element.verifiable ? 1 : 2
            return (ra, a.offset) < (rb, b.offset)
        }.map(\.element)
    }
}

// MARK: - Personal inputs

/// Share of the portfolio held in one asset, sent as `weights=` (a percentage, never a quantity nor an amount).
public struct DecisionWeight: Sendable, Equatable {
    public var asset: Asset
    /// 0–100, one decimal.
    public var weight: Double
    public init(asset: Asset, weight: Double) {
        self.asset = asset
        self.weight = weight
    }
}

public enum DecisionInputs {
    /// Share of each held asset in the total portfolio value (quantity × price; lines of the same asset added up),
    /// largest first, at most `limit` (the server takes 20). Lines without a price are left out; nothing without a total.
    public static func weights(_ holdings: [Holding], prices: [String: Double], limit: Int = 20) -> [DecisionWeight] {
        var values: [String: (asset: Asset, value: Double)] = [:]
        for h in holdings {
            guard let p = prices[h.asset.id], p.isFinite, p > 0, h.quantity.isFinite, h.quantity > 0 else { continue }
            values[h.asset.id, default: (h.asset, 0)].value += p * h.quantity
        }
        let total = values.values.map(\.value).reduce(0, +)
        guard total > 0 else { return [] }
        return values.values
            .map { DecisionWeight(asset: $0.asset, weight: (($0.value / total * 100) * 10).rounded() / 10) }
            .filter { $0.weight > 0 }
            .sorted { ($0.weight, $1.asset.id) > ($1.weight, $0.asset.id) }
            .prefix(max(0, limit))
            .map { $0 }
    }

    /// Average purchase price of this asset over its lines (weighted by quantity); nil when a line has none.
    public static func cost(of asset: Asset, in holdings: [Holding]) -> Double? {
        let lines = holdings.filter { $0.asset.id == asset.id && $0.quantity > 0 }
        guard !lines.isEmpty, lines.allSatisfy({ ($0.averagePrice ?? 0) > 0 }) else { return nil }
        let quantity = lines.map(\.quantity).reduce(0, +)
        let spent = lines.map { $0.quantity * $0.averagePrice! }.reduce(0, +)
        return quantity > 0 ? spent / quantity : nil
    }

    /// "BTC:crypto:35.2,AAPL:stock:10"
    public static func weightsParameter(_ weights: [DecisionWeight]) -> String {
        weights.map { "\($0.asset.symbol):\($0.asset.kind.rawValue):\(number($0.weight, digits: 1))" }.joined(separator: ",")
    }

    /// Plain decimal with a dot, no exponent nor grouping ("275.5", "0.00001234").
    static func number(_ v: Double, digits: Int) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.numberStyle = .decimal
        f.usesGroupingSeparator = false
        f.minimumFractionDigits = 0
        f.maximumFractionDigits = digits
        f.decimalSeparator = "."
        return f.string(from: NSNumber(value: v)) ?? String(v)
    }
}

extension AltimClient {
    static func decisionQuery(_ asset: Asset, cost: Double?, weights: [DecisionWeight]) -> [String: String] {
        var q = ["symbol": asset.symbol, "kind": asset.kind.rawValue]
        if let cost, cost.isFinite, cost > 0 { q["cost"] = DecisionInputs.number(cost, digits: 10) }
        let w = Array(weights.filter { $0.weight.isFinite && $0.weight > 0 }.prefix(20))
        if !w.isEmpty { q["weights"] = DecisionInputs.weightsParameter(w) }
        return q
    }

    /// Decision on one asset. Without `cost` and `weights`: informational (market data only). With them (the average
    /// purchase price and the portfolio's percentages, never quantities): personal, used for this answer only.
    public func decision(asset: Asset, cost: Double? = nil, weights: [DecisionWeight] = []) async throws -> Decision {
        try await getDecision(Self.decisionQuery(asset, cost: cost, weights: weights))
    }
}
