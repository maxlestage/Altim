import Foundation

// « Bot Altim » v3 (additive, optional): `v3` of the report (/api/bot: horizons 20 / 60, peers ranking, corrected
// threshold, forward test), `v3` of an asset row (today's actions of the 4 headline configurations) and of a view
// (`bot` of /api/decision, items of /api/bot/views). Exact port of the v3 part of web/src/webapp/model-bot.ts and of
// BotRadarCard.tsx's pure helpers. Every field is optional-safe: a missing or odd value never breaks the screen, and a
// v1 / v2 answer reads with `v3 == nil`. Excesses are in points of % (buy: mean − baseline; sell: baseline − mean),
// `t` is by date (the verdict's), times in ms. The server computes everything; nothing here recomputes a statistic.

/// v3's candidates: family « absolute » (rise / fall of the asset) then « peers » (vs the group's median).
public enum BotV3Candidate: String, Codable, Sendable, Hashable, CaseIterable {
    case trend, v1, logit, trees, treesLong, xsMomentum, xsLogit, xsTrees

    /// V3_SHORT.
    public var short: String {
        switch self {
        case .trend: return "Tendance"
        case .v1: return "Logistique v1"
        case .logit: return "Logistique 24"
        case .trees: return "Arbres v2"
        case .treesLong: return "Arbres longs"
        case .xsMomentum: return "Momentum entre pairs"
        case .xsLogit: return "Logistique entre pairs"
        case .xsTrees: return "Arbres longs entre pairs"
        }
    }
}

public enum BotFamily: String, Codable, Sendable, Hashable, CaseIterable {
    case absolute, peers, v2

    /// FAMILY_LABEL.
    public var label: String {
        switch self {
        case .absolute: return "Hausse ou baisse de l'actif"
        case .peers: return "Classement entre pairs"
        case .v2: return "Sélection v2 (log-loss)"
        }
    }
}

// MARK: - Lenient decoding (same rules as Bot.swift)

private extension KeyedDecodingContainer {
    func opt<T: Decodable>(_ t: T.Type, _ k: Key) -> T? { (try? decodeIfPresent(t, forKey: k)) ?? nil }
    func num(_ k: Key) -> Double? { opt(Double.self, k).flatMap { $0.isFinite ? $0 : nil } }
    func int(_ k: Key) -> Int { opt(Int.self, k) ?? opt(Double.self, k).flatMap { $0.isFinite ? Int($0) : nil } ?? 0 }
    func optInt(_ k: Key) -> Int? { num(k).map { Int($0) } }
    func str(_ k: Key) -> String { opt(String.self, k) ?? "" }
    func bool(_ k: Key) -> Bool { opt(Bool.self, k) ?? false }
    func list<T: Decodable>(_ t: T.Type, _ k: Key) -> [T] { opt([Lenient<T>].self, k)?.compactMap(\.value) ?? [] }
    func action(_ k: Key) -> BotAction? { opt(String.self, k).flatMap(BotAction.init(rawValue:)) }
    func verdict(_ k: Key) -> ValidationVerdict? { opt(String.self, k).flatMap(ValidationVerdict.init(rawValue:)) }
    func v2Candidate(_ k: Key) -> BotCandidate? { opt(String.self, k).flatMap(BotCandidate.init(rawValue:)) }
    func candidate(_ k: Key) -> BotV3Candidate? { opt(String.self, k).flatMap(BotV3Candidate.init(rawValue:)) }
    func family(_ k: Key) -> BotFamily? { opt(String.self, k).flatMap(BotFamily.init(rawValue:)) }
}

private struct V3Key: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

private func box(_ decoder: Decoder) throws -> KeyedDecodingContainer<V3Key> { try decoder.container(keyedBy: V3Key.self) }
private func k(_ s: String) -> V3Key { V3Key(s) }
private func empty<T: Decodable>(_ t: T.Type) -> T {
    // swiftlint:disable:next force_try
    try! JSONDecoder().decode(T.self, from: Data("{}".utf8))
}

// MARK: - Report (`v3` of /api/bot)

/// One side of a configuration: excess in points of %, `t` by date (the verdict's), verdict at the corrected threshold
/// and at the raw t ≥ 2 (for comparison only).
public struct BotV3SideStats: Decodable, Sendable, Equatable {
    public var signals: Int
    public var mean: Double?
    public var baseline: Double?
    public var excess: Double?
    public var t: Double?
    public var dates: Int
    public var tByAsset: Double?
    public var assets: Int
    public var tPerSignal: Double?
    public var beatShare: Double?
    public var verdict: ValidationVerdict?
    public var rawVerdict: ValidationVerdict?

    public init(signals: Int = 0, mean: Double? = nil, baseline: Double? = nil, excess: Double? = nil, t: Double? = nil, dates: Int = 0,
                tByAsset: Double? = nil, assets: Int = 0, tPerSignal: Double? = nil, beatShare: Double? = nil,
                verdict: ValidationVerdict? = nil, rawVerdict: ValidationVerdict? = nil) {
        self.signals = signals
        self.mean = mean
        self.baseline = baseline
        self.excess = excess
        self.t = t
        self.dates = dates
        self.tByAsset = tByAsset
        self.assets = assets
        self.tPerSignal = tPerSignal
        self.beatShare = beatShare
        self.verdict = verdict
        self.rawVerdict = rawVerdict
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(signals: c.int(k("signals")), mean: c.num(k("mean")), baseline: c.num(k("baseline")), excess: c.num(k("excess")),
                  t: c.num(k("t")), dates: c.int(k("dates")), tByAsset: c.num(k("tByAsset")), assets: c.int(k("assets")),
                  tPerSignal: c.num(k("tPerSignal")), beatShare: c.num(k("beatShare")), verdict: c.verdict(k("verdict")),
                  rawVerdict: c.verdict(k("rawVerdict")))
    }
}

public enum BotSide: String, Sendable, Hashable, CaseIterable { case buy, sell }

public struct BotV3ConfigStats: Decodable, Sendable, Equatable {
    public var testRows: Int
    public var labelled: Int
    public var from: Double?
    public var to: Double?
    public var buy: BotV3SideStats
    public var sell: BotV3SideStats
    public var waitShare: Double?
    public var medianBotReturn: Double?
    public var medianHoldReturn: Double?
    public var beatHold: Int
    public var holdAssets: Int
    public var exit: BotExitStats?
    public var brierSkillUp: Double?
    public var brierSkillDown: Double?
    /// Family B: the same sides against the group's equal-weight mean (control added after the first real run).
    public var buyVsMean: BotV3SideStats?
    public var sellVsMean: BotV3SideStats?

    public init(buy: BotV3SideStats = BotV3SideStats(), sell: BotV3SideStats = BotV3SideStats(), buyVsMean: BotV3SideStats? = nil,
                sellVsMean: BotV3SideStats? = nil) {
        testRows = 0
        labelled = 0
        beatHold = 0
        holdAssets = 0
        self.buy = buy
        self.sell = sell
        self.buyVsMean = buyVsMean
        self.sellVsMean = sellVsMean
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        testRows = c.int(k("testRows"))
        labelled = c.int(k("labelled"))
        from = c.num(k("from"))
        to = c.num(k("to"))
        buy = c.opt(BotV3SideStats.self, k("buy")) ?? BotV3SideStats()
        sell = c.opt(BotV3SideStats.self, k("sell")) ?? BotV3SideStats()
        waitShare = c.num(k("waitShare"))
        medianBotReturn = c.num(k("medianBotReturn"))
        medianHoldReturn = c.num(k("medianHoldReturn"))
        beatHold = c.int(k("beatHold"))
        holdAssets = c.int(k("holdAssets"))
        exit = c.opt(BotExitStats.self, k("exit"))
        brierSkillUp = c.num(k("brierSkillUp"))
        brierSkillDown = c.num(k("brierSkillDown"))
        buyVsMean = c.opt(BotV3SideStats.self, k("buyVsMean"))
        sellVsMean = c.opt(BotV3SideStats.self, k("sellVsMean"))
    }

    public subscript(side: BotSide) -> BotV3SideStats { side == .buy ? buy : sell }
    /// The control against the group's mean of a side (family B only).
    public func vsMean(_ side: BotSide) -> BotV3SideStats? { side == .buy ? buyVsMean : sellVsMean }
}

/// `main`: basket, signals dated up to the pre-registration; `extra`: training assets (nested only); `forward`: after it.
public struct BotV3Config: Decodable, Sendable, Identifiable {
    public var id: String
    public var family: BotFamily
    public var candidate: BotV3Candidate?
    public var nested: Bool
    public var headline: Bool
    public var label: String
    public var trainedBlocks: Int
    public var chosenBlocks: Int
    public var main: BotV3ConfigStats
    public var extra: BotV3ConfigStats?
    public var forward: BotV3ConfigStats

    public init(id: String = "", family: BotFamily = .absolute, headline: Bool = true, label: String = "",
                main: BotV3ConfigStats = BotV3ConfigStats(), forward: BotV3ConfigStats = BotV3ConfigStats()) {
        self.id = id
        self.family = family
        nested = true
        self.headline = headline
        self.label = label
        trainedBlocks = 0
        chosenBlocks = 0
        self.main = main
        self.forward = forward
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.str(k("id"))
        family = c.family(k("family")) ?? .absolute
        candidate = c.candidate(k("candidate"))
        nested = c.bool(k("nested"))
        headline = c.bool(k("headline"))
        label = c.str(k("label"))
        trainedBlocks = c.int(k("trainedBlocks"))
        chosenBlocks = c.int(k("chosenBlocks"))
        main = c.opt(BotV3ConfigStats.self, k("main")) ?? empty(BotV3ConfigStats.self)
        extra = c.opt(BotV3ConfigStats.self, k("extra"))
        forward = c.opt(BotV3ConfigStats.self, k("forward")) ?? empty(BotV3ConfigStats.self)
    }
}

/// Economic score of a candidate on the inner validation (nil without enough signals).
public struct BotV3EconScore: Decodable, Sendable, Equatable {
    public var id: BotV3Candidate?
    public var score: Double?
    public var signals: Int
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.candidate(k("id"))
        score = c.num(k("score"))
        signals = c.int(k("signals"))
    }
}

/// One retraining: the choice of each family (v2's by log-loss, absolute and peers by the economic score).
public struct BotV3BlockOut: Decodable, Sendable, Equatable {
    public var start: Double
    public var end: Double?
    public var trainRows: Int
    public var v2: BotCandidate?
    public var absolute: BotV3Candidate?
    public var peers: BotV3Candidate?
    public var absoluteScores: [BotV3EconScore]
    public var peersScores: [BotV3EconScore]
    public var roundsUp: Int?
    public var roundsDown: Int?
    public var roundsPeers: Int?

    public init(start: Double, absolute: BotV3Candidate? = nil, peers: BotV3Candidate? = nil) {
        self.start = start
        trainRows = 0
        self.absolute = absolute
        self.peers = peers
        absoluteScores = []
        peersScores = []
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        start = c.num(k("start")) ?? 0
        end = c.num(k("end"))
        trainRows = c.int(k("trainRows"))
        v2 = c.v2Candidate(k("v2"))
        absolute = c.candidate(k("absolute"))
        peers = c.candidate(k("peers"))
        absoluteScores = c.list(BotV3EconScore.self, k("absoluteScores"))
        peersScores = c.list(BotV3EconScore.self, k("peersScores"))
        roundsUp = c.optInt(k("roundsUp"))
        roundsDown = c.optInt(k("roundsDown"))
        roundsPeers = c.optInt(k("roundsPeers"))
    }
}

/// Boosting rounds kept by early stopping over the retrainings.
public struct BotV3Rounds: Decodable, Sendable, Equatable {
    public var fits: Int
    public var min: Double?
    public var median: Double?
    public var max: Double?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        fits = c.int(k("fits"))
        min = c.num(k("min"))
        median = c.num(k("median"))
        max = c.num(k("max"))
    }
}

public struct BotV3Horizon: Decodable, Sendable, Identifiable {
    public var horizon: Int
    public var blocks: Int
    public var testFrom: Double?
    public var testTo: Double?
    public var configs: [BotV3Config]
    public var selection: [BotV3BlockOut]
    public var roundsUp: BotV3Rounds?
    public var roundsDown: BotV3Rounds?
    public var roundsPeers: BotV3Rounds?
    public var id: Int { horizon }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        horizon = c.int(k("horizon"))
        blocks = c.int(k("blocks"))
        testFrom = c.num(k("testFrom"))
        testTo = c.num(k("testTo"))
        configs = c.list(BotV3Config.self, k("configs"))
        selection = c.list(BotV3BlockOut.self, k("selection"))
        roundsUp = c.opt(BotV3Rounds.self, k("roundsUp"))
        roundsDown = c.opt(BotV3Rounds.self, k("roundsDown"))
        roundsPeers = c.opt(BotV3Rounds.self, k("roundsPeers"))
    }
}

/// Annualised (%, Sharpe without risk-free rate), max drawdown ≤ 0 (%), mean exposure 0-1.
public struct BotV3SeriesStats: Decodable, Sendable, Equatable {
    public var days: Int
    public var annualReturn: Double?
    public var annualVol: Double?
    public var sharpe: Double?
    public var maxDrawdown: Double?
    public var totalReturn: Double?
    public var meanExposure: Double?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        days = c.int(k("days"))
        annualReturn = c.num(k("annualReturn"))
        annualVol = c.num(k("annualVol"))
        sharpe = c.num(k("sharpe"))
        maxDrawdown = c.num(k("maxDrawdown"))
        totalReturn = c.num(k("totalReturn"))
        meanExposure = c.num(k("meanExposure"))
    }
}

/// The volatility-managed trend (published rule) against holding, equal-weight portfolio of the basket.
public struct BotV3VolManaged: Decodable, Sendable {
    public var assets: Int
    public var from: Double?
    public var to: Double?
    public var periods: Int
    public var hold: BotV3SeriesStats
    public var managed: BotV3SeriesStats
    public var medianSharpeHold: Double?
    public var medianSharpeManaged: Double?
    public var medianDrawdownHold: Double?
    public var medianDrawdownManaged: Double?
    public var betterSharpe: Int
    public var shallowerDrawdown: Int
    public var forwardHold: BotV3SeriesStats
    public var forwardManaged: BotV3SeriesStats
    public var text: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        assets = c.int(k("assets"))
        from = c.num(k("from"))
        to = c.num(k("to"))
        periods = c.int(k("periods"))
        hold = c.opt(BotV3SeriesStats.self, k("hold")) ?? empty(BotV3SeriesStats.self)
        managed = c.opt(BotV3SeriesStats.self, k("managed")) ?? empty(BotV3SeriesStats.self)
        medianSharpeHold = c.num(k("medianSharpeHold"))
        medianSharpeManaged = c.num(k("medianSharpeManaged"))
        medianDrawdownHold = c.num(k("medianDrawdownHold"))
        medianDrawdownManaged = c.num(k("medianDrawdownManaged"))
        betterSharpe = c.int(k("betterSharpe"))
        shallowerDrawdown = c.int(k("shallowerDrawdown"))
        forwardHold = c.opt(BotV3SeriesStats.self, k("forwardHold")) ?? empty(BotV3SeriesStats.self)
        forwardManaged = c.opt(BotV3SeriesStats.self, k("forwardManaged")) ?? empty(BotV3SeriesStats.self)
        text = c.str(k("text"))
    }
}

public struct BotV3Group: Decodable, Sendable, Identifiable {
    public var id: BotGroup
    public var label: String
    public var market: String
    public var universe: BotUniverse
    /// First day with enough peers to rank (ms).
    public var peersFrom: Double?
    public var horizons: [BotV3Horizon]
    public var volManaged: BotV3VolManaged?
    public var text: String

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = BotGroup(rawValue: c.str(k("id"))) ?? .stock
        label = c.str(k("label"))
        market = c.str(k("market"))
        universe = c.opt(BotUniverse.self, k("universe")) ?? BotUniverse(basket: 0, extra: 0, extraFailed: 0, rows: 0, dataFrom: nil, medianYears: nil, maxYears: nil)
        peersFrom = c.num(k("peersFrom"))
        horizons = c.list(BotV3Horizon.self, k("horizons"))
        volManaged = c.opt(BotV3VolManaged.self, k("volManaged"))
        text = c.str(k("text"))
    }
}

public struct BotV3CandidateInfo: Decodable, Sendable, Identifiable {
    public var id: String
    public var family: BotFamily
    public var label: String
    public var description: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.str(k("id"))
        family = c.family(k("family")) ?? .absolute
        label = c.str(k("label"))
        description = c.str(k("description"))
    }
}

/// Tests counted since v1 (Bonferroni's K).
public struct BotV3K: Decodable, Sendable, Equatable {
    public var v1: Int
    public var v2: Int
    public var v3: Int
    public var total: Int
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        v1 = c.int(k("v1"))
        v2 = c.int(k("v2"))
        v3 = c.int(k("v3"))
        total = c.int(k("total"))
    }
}

public struct BotV3Parameters: Decodable, Sendable {
    public var horizons: [Int]
    public var stockFrom: String
    public var maxTreeRows: Int
    public var longMaxRounds: Int
    public var longDepth: Int
    public var longShrinkage: Double?
    public var longMinLeaf: Int
    public var patience: Int
    public var minPeers: Int
    public var minValSignals: Int
    public var minSignals: Int
    public var nudge: Double?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        let h = c.list(Int.self, k("horizons"))
        horizons = h.isEmpty ? [20, 60] : h
        stockFrom = c.str(k("stockFrom"))
        maxTreeRows = c.int(k("maxTreeRows"))
        longMaxRounds = c.int(k("longMaxRounds"))
        longDepth = c.int(k("longDepth"))
        longShrinkage = c.num(k("longShrinkage"))
        longMinLeaf = c.int(k("longMinLeaf"))
        patience = c.int(k("patience"))
        minPeers = c.int(k("minPeers"))
        minValSignals = c.int(k("minValSignals"))
        minSignals = c.opt(Int.self, k("minSignals")) ?? 30
        nudge = c.num(k("nudge"))
    }
}

public struct BotV3Compute: Decodable, Sendable {
    public var maxRows: Int
    public var blocks: Int
    public var rowBytes: Int
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        maxRows = c.int(k("maxRows"))
        blocks = c.int(k("blocks"))
        rowBytes = c.int(k("rowBytes"))
    }
}

/// The pre-registered v3; the v2-shaped fields of the report then hold v2's selection at 20 days, at the corrected threshold.
public struct BotV3Report: Decodable, Sendable {
    public var version: Int
    /// "2026-09-30".
    public var preregDate: String
    /// Signals dated from then on make the forward test (ms).
    public var forwardFrom: Double?
    /// Rules added after the pre-registration (each one only stricter).
    public var afterPrereg: [String]
    public var k: BotV3K
    public var alpha: Double?
    /// Bonferroni's corrected threshold on the t by date (≈ 3,52 for K = 114).
    public var tRequired: Double
    public var headline: String
    public var forwardHeadline: String
    public var groups: [BotV3Group]
    public var candidates: [BotV3CandidateInfo]
    public var changes: [String]
    public var method: [String]
    public var limits: [String]
    public var parameters: BotV3Parameters
    public var compute: BotV3Compute

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        version = c.opt(Int.self, V3Key("version")) ?? 3
        preregDate = c.str(V3Key("preregDate"))
        forwardFrom = c.num(V3Key("forwardFrom"))
        afterPrereg = c.list(String.self, V3Key("afterPrereg"))
        k = c.opt(BotV3K.self, V3Key("k")) ?? empty(BotV3K.self)
        alpha = c.num(V3Key("alpha"))
        tRequired = c.num(V3Key("tRequired")) ?? 0
        headline = c.str(V3Key("headline"))
        forwardHeadline = c.str(V3Key("forwardHeadline"))
        groups = c.list(BotV3Group.self, V3Key("groups"))
        candidates = c.list(BotV3CandidateInfo.self, V3Key("candidates"))
        changes = c.list(String.self, V3Key("changes"))
        method = c.list(String.self, V3Key("method"))
        limits = c.list(String.self, V3Key("limits"))
        parameters = c.opt(BotV3Parameters.self, V3Key("parameters")) ?? empty(BotV3Parameters.self)
        compute = c.opt(BotV3Compute.self, V3Key("compute")) ?? empty(BotV3Compute.self)
    }
}

/// Today's actions of a basket asset under the 4 headline configurations.
public struct BotV3AssetNow: Decodable, Sendable, Equatable {
    public var time: Double?
    public var absolute20: BotAction?
    public var absolute60: BotAction?
    public var peers20: BotAction?
    public var peers60: BotAction?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        time = c.num(k("time"))
        absolute20 = c.action(k("absolute20"))
        absolute60 = c.action(k("absolute60"))
        peers20 = c.action(k("peers20"))
        peers60 = c.action(k("peers60"))
    }
}

// MARK: - Today's view (`v3` of a BotView)

public struct BotV3Signal: Codable, Sendable, Equatable {
    public var family: BotFamily
    public var horizon: Int
    public var candidate: BotV3Candidate?
    public var action: BotAction?
    public var buyVerdict: ProofVerdict?
    public var sellVerdict: ProofVerdict?
    public var forwardBuySignals: Int
    public var forwardSellSignals: Int
    public var contradicted: Bool
    public var counts: Bool
    /// Proven on the past but awaiting 30 confirming forward signals: shown only (since the rule of 30/09/2026).
    public var pending: Bool
    public var text: String

    enum CodingKeys: String, CodingKey {
        case family, horizon, candidate, action, buyVerdict, sellVerdict, forwardBuySignals, forwardSellSignals, contradicted, counts, pending, text
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        family = c.opt(String.self, .family).flatMap(BotFamily.init(rawValue:)) ?? .absolute
        horizon = c.int(.horizon)
        candidate = c.opt(String.self, .candidate).flatMap(BotV3Candidate.init(rawValue:))
        action = c.opt(String.self, .action).flatMap(BotAction.init(rawValue:))
        buyVerdict = c.opt(ProofVerdict.self, .buyVerdict)
        sellVerdict = c.opt(ProofVerdict.self, .sellVerdict)
        forwardBuySignals = c.int(.forwardBuySignals)
        forwardSellSignals = c.int(.forwardSellSignals)
        contradicted = c.bool(.contradicted)
        counts = c.bool(.counts)
        pending = c.bool(.pending)
        text = c.str(.text)
    }
}

/// v3 in a decision: the 4 headline configurations today; `counts` when one of them is on a confirmed side.
public struct BotV3View: Codable, Sendable, Equatable {
    public var available: Bool
    public var signals: [BotV3Signal]
    public var counts: Bool
    /// Confidence change for a buy-side verdict (± 3 at most).
    public var nudge: Double
    public var tRequired: Double
    public var note: String
    public var pro: String?
    public var con: String?
    public var conHeld: String?

    enum CodingKeys: String, CodingKey { case available, signals, counts, nudge, tRequired, note, pro, con, conHeld }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = c.bool(.available)
        signals = c.list(BotV3Signal.self, .signals)
        counts = c.bool(.counts)
        nudge = c.num(.nudge) ?? 0
        tRequired = c.num(.tRequired) ?? 0
        note = c.str(.note)
        pro = c.opt(String.self, .pro)
        con = c.opt(String.self, .con)
        conHeld = c.opt(String.self, .conHeld)
    }
}

// MARK: - Helpers (model-bot.ts v3, BotRadarCard.tsx)

extension ModelBot {
    /// "30/09/2026" from "2026-09-30".
    public static func frIso(_ s: String) -> String { s.split(separator: "-", omittingEmptySubsequences: false).reversed().joined(separator: "/") }

    /// "t = −1,2 (requis 3,52)".
    public static func tVsRequired(_ t: Double?, _ required: Double) -> String {
        "t = \(t == nil ? "—" : ModelValidation.plain(t, digits: 1)) (requis \(ModelValidation.plain(required, digits: 2)))"
    }

    /// "t par jour 3,7 (requis 3,52) · par actif 0,6": the verdict's t, the one it needs, and the t by asset.
    public static func tLine(_ s: BotV3SideStats, _ required: Double) -> String {
        "t par jour \(s.t == nil ? "—" : ModelValidation.plain(s.t, digits: 1)) (requis \(ModelValidation.plain(required, digits: 2))) · par actif \(s.tByAsset == nil ? "—" : ModelValidation.plain(s.tByAsset, digits: 1))"
    }

    /// Verdict at the corrected threshold, in French; says when only the raw t ≥ 2 was reached.
    public static func v3VerdictLabel(_ s: BotV3SideStats, _ required: Double) -> String {
        switch s.verdict {
        case .edge: return "Avantage au seuil corrigé (t ≥ \(ModelValidation.plain(required, digits: 2))), à confirmer"
        case .negative: return "Pire que la référence au seuil corrigé"
        case .insufficient: return "Trop peu de signaux pour conclure"
        default: return s.rawVerdict == .edge ? "t ≥ 2 atteint, pas le seuil corrigé : non démontré" : "Non démontré"
        }
    }

    /// A side of a configuration in one sentence: "412 achats : +0,31 point face à la médiane du groupe (t par jour 1,1 (requis 3,52) · par actif 0,4)."
    public static func v3SideText(_ family: BotFamily, _ side: BotSide, _ s: BotV3SideStats, _ required: Double) -> String {
        if s.signals == 0 { return side == .buy ? "Aucun achat." : "Aucune vente." }
        let ref = family == .peers ? "la médiane du groupe" : "une entrée au hasard"
        let plural = s.signals > 1 ? "s" : ""
        if side == .buy { return "\(s.signals) achat\(plural) : \(points(s.excess)) face à \(ref) (\(tLine(s, required)))." }
        let verb = family == .peers ? "gain à passer sur l'actif médian" : "baisse évitée"
        return "\(s.signals) vente\(plural) : \(verb) \(points(s.excess)) (\(tLine(s, required)))."
    }

    /// Forward test of a configuration so far.
    public static func forwardText(_ c: BotV3ConfigStats, _ required: Double) -> String {
        let n = c.buy.signals + c.sell.signals
        if n == 0 { return "Aucun signal jugé pour l'instant (il faut 20 à 60 jours de bourse après le signal)." }
        return "\(c.buy.signals) achat\(c.buy.signals > 1 ? "s" : "") (\(points(c.buy.excess)), \(tVsRequired(c.buy.t, required))) · \(c.sell.signals) vente\(c.sell.signals > 1 ? "s" : "") (\(points(c.sell.excess)))."
    }

    public struct VolRow: Equatable, Sendable {
        public var label: String
        public var managed: String
        public var hold: String
    }

    /// Sharpe, max drawdown, yearly return and volatility of the managed trend vs holding (equal-weight portfolio).
    public static func volRows(_ v: BotV3VolManaged) -> [VolRow] {
        [VolRow(label: "Ratio de Sharpe", managed: ModelValidation.plain(v.managed.sharpe, digits: 2), hold: ModelValidation.plain(v.hold.sharpe, digits: 2)),
         VolRow(label: "Pire baisse", managed: ModelValidation.signedPct(v.managed.maxDrawdown), hold: ModelValidation.signedPct(v.hold.maxDrawdown)),
         VolRow(label: "Rendement annuel", managed: ModelValidation.signedPct(v.managed.annualReturn), hold: ModelValidation.signedPct(v.hold.annualReturn)),
         VolRow(label: "Volatilité annuelle", managed: pct0(v.managed.annualVol), hold: pct0(v.hold.annualVol))]
    }

    /// "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)".
    public static func computeText(_ t: BotTiming?) -> String? {
        guard let t else { return nil }
        let s = Int((t.computeMs / 1000).rounded())
        let dur = s >= 60 ? "\(s / 60) min \(s % 60) s" : "\(s) s"
        let mem = t.peakRssMb.map { ", pic mémoire \(Int($0.rounded())) Mo" } ?? ""
        return "\(dur) de calcul sur \(t.threads) cœur\(t.threads > 1 ? "s" : "")\(mem) (téléchargement \(Int((t.fetchMs / 1000).rounded())) s)"
    }

    public struct HeadlineConfig: Sendable {
        public var horizon: Int
        public var config: BotV3Config
    }

    /// The 4 headline configurations of a group: (horizon, config).
    public static func headlineConfigs(_ g: BotV3Group) -> [HeadlineConfig] {
        g.horizons.flatMap { h in h.configs.filter(\.headline).map { HeadlineConfig(horizon: h.horizon, config: $0) } }
    }

    /// The figures that judge a side: family B's control against the group's mean when measured, else the side itself.
    public static func judged(_ c: BotV3ConfigStats, _ side: BotSide) -> BotV3SideStats { c.vsMean(side) ?? c[side] }

    /// A side proven at the corrected threshold (family B: against the median and the mean).
    public static func proven(_ c: BotV3ConfigStats, _ side: BotSide) -> Bool {
        c[side].verdict == .edge && judged(c, side).verdict == .edge
    }

    /// A side proven on the past and confirmed by the forward test (≥ 30 signals, excess ≥ 0 against each reference): only then does it count.
    public static func confirmed(_ c: BotV3Config, _ side: BotSide) -> Bool {
        let refs = [c.forward[side]] + [c.forward.vsMean(side)].compactMap { $0 }
        return proven(c.main, side) && refs.allSatisfy { s in s.signals >= 30 && (s.excess.map { $0 >= 0 } ?? false) }
    }

    /// « Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : … » for a proven side awaiting its forward test; nil otherwise.
    public static func pendingNote(_ c: BotV3Config, _ side: BotSide, _ required: Double) -> String? {
        if !proven(c.main, side) || confirmed(c, side) { return nil }
        let t = judged(c.main, side).t
        return "Avantage mesuré sur le passé (t = \(t == nil ? "—" : ModelValidation.plain(t, digits: 2)) contre \(ModelValidation.plain(required, digits: 2)) exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (\(c.forward[side].signals) à ce jour)."
    }

    /// Whether a v3 headline configuration has an edge on either side (at the corrected threshold, both references).
    public static func v3AnyEdge(_ r: BotV3Report) -> Bool {
        r.groups.contains { g in headlineConfigs(g).contains { proven($0.config.main, .buy) || proven($0.config.main, .sell) } }
    }

    /// Signals judged so far in the forward test (headline configurations).
    public static func forwardSignals(_ r: BotV3Report) -> Int {
        r.groups.reduce(0) { s, g in s + headlineConfigs(g).reduce(0) { $0 + $1.config.forward.buy.signals + $1.config.forward.sell.signals } }
    }

    public struct ChoiceRun: Equatable, Sendable {
        public var from: Double
        public var to: Double
        public var chosen: BotV3Candidate?
        public var count: Int
        public init(from: Double, to: Double, chosen: BotV3Candidate?, count: Int) {
            self.from = from
            self.to = to
            self.chosen = chosen
            self.count = count
        }
    }

    /// Runs of identical choices over the retrainings of one family (BotV3.tsx's `choiceRuns`).
    public static func choiceRuns(_ blocks: [BotV3BlockOut], family: BotFamily) -> [ChoiceRun] {
        var out: [ChoiceRun] = []
        for b in blocks {
            let c = family == .peers ? b.peers : b.absolute
            if let last = out.last, last.chosen == c {
                out[out.count - 1].to = b.start
                out[out.count - 1].count += 1
            } else {
                out.append(ChoiceRun(from: b.start, to: b.start, chosen: c, count: 1))
            }
        }
        return out
    }

    /// "Arbres longs (3 fois) → Tendance → aucun".
    public static func choiceRunsText(_ runs: [ChoiceRun]) -> String {
        runs.map { "\($0.chosen?.short ?? "aucun")\($0.count > 1 ? " (\($0.count) fois)" : "")" }.joined(separator: " → ")
    }

    // Texts of the v3 section (same wording as BotV3.tsx).

    /// "Bot v3 · pré-enregistré le 30/09/2026".
    public static func v3Title(_ v: BotV3Report) -> String { "Bot v3 · pré-enregistré le \(frIso(v.preregDate))" }

    /// "0 signal", "3 signaux" (the forward tile and the radar's counter; the web writes "signals", a typo).
    public static func signalsCount(_ n: Int) -> String { "\(n) \(n > 1 ? "signaux" : "signal")" }

    public static func v3ProtocolText(_ v: BotV3Report) -> String {
        "Protocole écrit avant tout calcul et figé : \(v.k.v1) tests en v1, \(v.k.v2) en v2, \(v.k.v3) en v3. Avec autant d'essais, un t de 2 arrive par hasard ; un avantage n'est dit démontré qu'au-delà de t = \(ModelValidation.plain(v.tRequired, digits: 2)) (Bonferroni, 5 %). Horizons jugés à part : \(v.parameters.horizons.map(String.init).joined(separator: " et ")) jours de bourse."
    }

    public static let afterPreregTitle = "Modifié après le pré-enregistrement :"
    public static let v3ChangesTitle = "Ce qui change avec la v3"
    public static let v3ResultsTitle = "Résultats v3 hors échantillon"

    public static func v3ResultsIntro(_ v: BotV3Report) -> String {
        "Panier fixe, signaux datés jusqu'au \(frIso(v.preregDate)) ; modèle choisi à chaque réentraînement sur une validation interne, jamais sur le test."
    }

    /// "Depuis le 30/09/2026 (test sur l'avenir)".
    public static func forwardTitle(_ v: BotV3Report) -> String { "Depuis le \(frIso(v.preregDate)) (test sur l'avenir)" }

    public static func forwardIntro(_ v: BotV3Report) -> String {
        "Le seul test vraiment neuf : ces signaux n'existaient pas quand le protocole a été figé. Même calcul, même seuil ; un avantage passé qu'il contredit (après au moins \(v.parameters.minSignals) signaux) cesse de compter."
    }

    /// "Actions et ETF américains · Classement entre pairs · 20 jours" (label bold on the screen).
    public static func forwardRowTitle(_ g: BotV3Group, _ h: HeadlineConfig) -> String { "\(g.label) · \(h.config.family.label) · \(h.horizon) jours" }

    public static let configsTitle = "Chaque configuration seule"
    public static let configsIntro = "À titre d'information, non utilisé pour choisir : chaque candidat retenu partout, et la sélection v2 refaite sur les nouvelles données."
    public static let volTitle = "Tendance à volatilité gérée"
    public static let volIntro = "Règle publiée, sans apprentissage : investi seulement au-dessus de la moyenne 200 jours, exposition réduite quand la volatilité dépasse sa médiane passée ; comparée à la simple détention, sur les mêmes jours de test."
    public static let v3MethodTitle = "Comment la v3 est jugée"

    /// "Les 8 candidats".
    public static func candidatesCount(_ v: BotV3Report) -> String { "Les \(v.candidates.count) candidats" }
    public static let v3LimitsTitle = "Limites de la v3"

    /// "Calcul : 9 min 43 s de calcul sur 1 cœur, pic mémoire 301 Mo (téléchargement 13 s) ; 200 réentraînements, jusqu'à 719 748 jours × actifs par groupe et horizon." (nil without timing).
    public static func v3ComputeLine(_ v: BotV3Report, _ timing: BotTiming?) -> String? {
        guard let c = computeText(timing) else { return nil }
        return "Calcul : \(c) ; \(v.compute.blocks) réentraînements, jusqu'à \(count(v.compute.maxRows)) jours × actifs par groupe et horizon."
    }

    /// "22 actifs testés, 72 de plus à l'entraînement · historique médian 36,7 ans · classement entre pairs possible depuis nov. 1993".
    public static func v3GroupSubtitle(_ g: BotV3Group) -> String {
        "\(g.universe.basket) actifs testés, \(g.universe.extra) de plus à l'entraînement · historique médian \(ModelValidation.plain(g.universe.medianYears, digits: 1)) ans\(g.peersFrom.map { " · classement entre pairs possible depuis \(ModelValidation.monthYear($0))" } ?? "")"
    }

    /// "À 20 jours" and "test de oct. 1995 à sept. 2026 · 31 réentraînements".
    public static func horizonTitle(_ h: BotV3Horizon) -> String { "À \(h.horizon) jours" }
    public static func horizonSubtitle(_ h: BotV3Horizon) -> String {
        "test de \(ModelValidation.monthYear(h.testFrom)) à \(ModelValidation.monthYear(h.testTo)) · \(h.blocks) réentraînements"
    }

    /// "ACHETER · face à la médiane" (family B) or "ACHETER".
    public static func sideHead(_ c: BotV3ConfigStats, _ side: BotSide) -> String {
        "\(side == .buy ? "ACHETER" : "VENDRE")\(c.vsMean(side) != nil ? " · face à la médiane" : "")"
    }

    /// Family B against the group's equal-weight mean (control added after the first real run).
    public static func controlText(_ s: BotV3SideStats, _ required: Double) -> String {
        "Face à la moyenne du groupe (contrôle ajouté après coup) : \(points(s.excess)) (\(tLine(s, required)))"
    }

    public static func controlVerdict(_ proven: Bool) -> String {
        proven ? "Sur le passé : avantage face aux deux références." : "Retenu : non démontré (il faut les deux références) ; ne compte pas."
    }

    /// "Achats cumulés / détention (médianes)": "+75 % / +1 396 %".
    public static func holdValue(_ c: BotV3ConfigStats) -> String {
        "\(ModelValidation.signedPct(c.medianBotReturn, digits: 0)) / \(ModelValidation.signedPct(c.medianHoldReturn, digits: 0))"
    }

    /// Key / value lines of a configuration alone (ConfigsCard).
    public static func configRows(_ c: BotV3Config, _ required: Double) -> [(label: String, value: String)] {
        var rows: [(label: String, value: String)] = [
            ("\(c.main.buy.signals) achats", "\(points(c.main.buy.excess)) (\(tVsRequired(c.main.buy.t, required)))"),
            ("\(c.main.sell.signals) ventes", "\(points(c.main.sell.excess)) (\(tVsRequired(c.main.sell.t, required)))"),
        ]
        if c.main.buyVsMean != nil {
            let b = judged(c.main, .buy), s = judged(c.main, .sell)
            rows.append(("Face à la moyenne", "\(points(b.excess)) / \(points(s.excess)) (t \(ModelValidation.plain(b.t, digits: 1)) / \(ModelValidation.plain(s.t, digits: 1)))"))
        }
        return rows
    }

    /// "retenu 12 fois sur 31" (nil for the nested configurations).
    public static func v3ChosenText(_ c: BotV3Config) -> String? {
        c.candidate == nil ? nil : "retenu \(c.chosenBlocks) fois sur \(c.trainedBlocks)"
    }

    /// "22 actifs du panier, de oct. 1995 à sept. 2026 · portefeuille à parts égales".
    public static func volSubtitle(_ v: BotV3VolManaged) -> String {
        "\(v.assets) actifs du panier, de \(ModelValidation.monthYear(v.from)) à \(ModelValidation.monthYear(v.to)) · portefeuille à parts égales"
    }

    public static let referenceTitle = "Référence : sélection v2 à 20 jours"

    public static func referenceText(_ v: BotV3Report) -> String {
        "Le modèle de la v2 (choisi par log-loss) refait sur les nouvelles données, jugé au seuil corrigé (t ≥ \(ModelValidation.plain(v.tRequired, digits: 2))). C'est lui qui donne les probabilités de hausse et de baisse ci-dessous ; il ne compte dans aucune décision."
    }

    /// Section label of v2's group results: renamed when v3 is present.
    public static func resultsTitle(_ r: BotReport) -> String { r.v3 != nil ? "Sélection v2 : résultats hors échantillon" : "Résultats hors échantillon" }

    /// "hausse/baisse 20 j" / "entre pairs 60 j" before a v3 action chip.
    public static func v3SignalLabel(_ s: BotV3Signal) -> String { "\(s.family == .peers ? "entre pairs" : "hausse/baisse") \(s.horizon) j" }

    // Radar card (BotRadarCard.tsx).

    /// The headline's first sentence (the verdict; the details are on the Bot screen).
    public static func firstSentence(_ s: String) -> String {
        guard let r = s.range(of: ". ") else { return s }
        return String(s[..<s.index(after: r.lowerBound)])
    }

    /// The radar's headline: v3's when present, else the report's.
    public static func radarHeadline(_ r: BotReport) -> String { firstSentence(r.v3?.headline ?? r.headline) }

    /// "Test sur l'avenir : 0 signal sur 30" (nil without v3).
    public static func radarForward(_ r: BotReport) -> String? {
        guard let v = r.v3 else { return nil }
        let n = forwardSignals(v)
        return "Test sur l'avenir : \(signalsCount(n)) sur 30"
    }

    public static let radarTitle = "Bot Altim"
    public static let radarLoading = "Chargement…"
    public static let radarPending = "Entraînement et test en cours sur le serveur (plusieurs minutes la première fois)…"
    public static let radarError = "Résultats indisponibles pour le moment."
    public static let radarLink = "Voir le bot →"
}

extension BotView {
    /// Whether the bot counts in this decision: v3's when present (then only its headline configurations can count).
    public var countsNow: Bool { v3.map(\.counts) ?? counts }

    /// Whether and how it counts: v3's note when present.
    public var noteNow: String {
        if let v = v3, !v.note.isEmpty { return v.note }
        return note
    }
}
