import Foundation

// « Bot Altim » (GET /api/bot, 202 {pending:true} while the first training runs; GET /api/bot/views?symbols=; `bot` of
// /api/decision): two logistic regressions trained on the validation's basket and judged walk-forward on periods they
// had not seen, saying ACHETER / ATTENDRE / VENDRE at 20 days. v2 fields are additive and optional (a v1 answer still
// reads): `version`, `changes`, per group `universe`, `dataYears`, `selection`, `candidates`, `holdout`, `extra`,
// `market`, `clustered` t in each side, `exit` of the sell side, the live model's `candidate`. JSON contract (every field optional-safe: a missing or
// odd value never breaks the screen nor the decision) and the pure helpers of the screen and of the decision line,
// exact port of web/src/webapp/model-bot.ts. Returns and probabilities are in %, "points" are differences of %
// (signal − random day), times in ms. The server computes everything; nothing here recomputes a statistic.

public enum BotGroup: String, Codable, Sendable, Hashable {
    case stock, crypto
    public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .stock }
}

public enum BotAction: String, Codable, Sendable, Hashable, CaseIterable {
    case buy, wait, sell

    /// "ACHETER", "ATTENDRE", "VENDRE".
    public var label: String {
        switch self {
        case .buy: return "ACHETER"
        case .wait: return "ATTENDRE"
        case .sell: return "VENDRE"
        }
    }
}

/// The fixed candidate models of v2: v1's logistic regression (16 features), the same on 24, boosted trees, the trend rule.
public enum BotCandidate: String, Codable, Sendable, Hashable, CaseIterable {
    case v1, logit, trees, trend

    /// CANDIDATE_SHORT: "Logistique v1", "Logistique 24", "Arbres", "Tendance".
    public var short: String {
        switch self {
        case .v1: return "Logistique v1"
        case .logit: return "Logistique 24"
        case .trees: return "Arbres"
        case .trend: return "Tendance"
        }
    }
}

// MARK: - Lenient decoding

private extension KeyedDecodingContainer {
    func opt<T: Decodable>(_ t: T.Type, _ k: Key) -> T? { (try? decodeIfPresent(t, forKey: k)) ?? nil }
    /// A finite number, or nil (null, missing, not a number).
    func num(_ k: Key) -> Double? { opt(Double.self, k).flatMap { $0.isFinite ? $0 : nil } }
    func int(_ k: Key) -> Int { opt(Int.self, k) ?? opt(Double.self, k).flatMap { $0.isFinite ? Int($0) : nil } ?? 0 }
    func str(_ k: Key) -> String { opt(String.self, k) ?? "" }
    func bool(_ k: Key) -> Bool { opt(Bool.self, k) ?? false }
    func list<T: Decodable>(_ t: T.Type, _ k: Key) -> [T] { opt([Lenient<T>].self, k)?.compactMap(\.value) ?? [] }
    func action(_ k: Key) -> BotAction? { opt(String.self, k).flatMap(BotAction.init(rawValue:)) }
    func verdict(_ k: Key) -> ValidationVerdict? { opt(String.self, k).flatMap(ValidationVerdict.init(rawValue:)) }
    func candidate(_ k: Key) -> BotCandidate? { opt(String.self, k).flatMap(BotCandidate.init(rawValue:)) }
}

private struct BotKey: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

private typealias BBox = KeyedDecodingContainer<BotKey>
private func box(_ decoder: Decoder) throws -> BBox { try decoder.container(keyedBy: BotKey.self) }
private func k(_ s: String) -> BotKey { BotKey(s) }

private func emptyObject<T: Decodable>(_ t: T.Type) -> T {
    // swiftlint:disable:next force_try
    try! JSONDecoder().decode(T.self, from: Data("{}".utf8))
}

// MARK: - Report (/api/bot)

/// Probability range [from, to) in %, rows = labelled test days, predicted = mean probability, realised = share.
public struct BotBucket: Decodable, Sendable, Equatable {
    public var from: Double
    public var to: Double
    public var label: String
    public var rows: Int
    public var predicted: Double?
    public var realised: Double?

    public init(from: Double, to: Double, label: String, rows: Int, predicted: Double?, realised: Double?) {
        self.from = from
        self.to = to
        self.label = label
        self.rows = rows
        self.predicted = predicted
        self.realised = realised
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(from: c.num(k("from")) ?? 0, to: c.num(k("to")) ?? 0, label: c.str(k("label")), rows: c.int(k("rows")),
                  predicted: c.num(k("predicted")), realised: c.num(k("realised")))
    }
}

/// t of a side's excesses: by date (the verdict's since v2), by asset, per signal (v1's).
public struct BotClustered: Decodable, Sendable, Equatable {
    public var byDate: Double?
    public var dates: Int
    public var byAsset: Double?
    public var assets: Int
    public var perSignal: Double?

    public init(byDate: Double?, dates: Int, byAsset: Double?, assets: Int, perSignal: Double?) {
        self.byDate = byDate
        self.dates = dates
        self.byAsset = byAsset
        self.assets = assets
        self.perSignal = perSignal
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(byDate: c.num(k("byDate")), dates: c.int(k("dates")), byAsset: c.num(k("byAsset")), assets: c.int(k("assets")),
                  perSignal: c.num(k("perSignal")))
    }
}

/// Holding vs leaving for 20 days at each VENDRE signal (medians over the assets; drawdowns ≤ 0 in %).
public struct BotExitStats: Decodable, Sendable, Equatable {
    public var assets: Int
    public var outShare: Double?
    public var medianHoldMaxDrawdown: Double?
    public var medianBotMaxDrawdown: Double?
    public var medianDrawdownAvoided: Double?
    public var medianHoldReturn: Double?
    public var medianBotReturn: Double?
    public var beatHold: Int

    public init(assets: Int, outShare: Double?, medianHoldMaxDrawdown: Double?, medianBotMaxDrawdown: Double?, medianDrawdownAvoided: Double?,
                medianHoldReturn: Double?, medianBotReturn: Double?, beatHold: Int) {
        self.assets = assets
        self.outShare = outShare
        self.medianHoldMaxDrawdown = medianHoldMaxDrawdown
        self.medianBotMaxDrawdown = medianBotMaxDrawdown
        self.medianDrawdownAvoided = medianDrawdownAvoided
        self.medianHoldReturn = medianHoldReturn
        self.medianBotReturn = medianBotReturn
        self.beatHold = beatHold
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(assets: c.int(k("assets")), outShare: c.num(k("outShare")), medianHoldMaxDrawdown: c.num(k("medianHoldMaxDrawdown")),
                  medianBotMaxDrawdown: c.num(k("medianBotMaxDrawdown")), medianDrawdownAvoided: c.num(k("medianDrawdownAvoided")),
                  medianHoldReturn: c.num(k("medianHoldReturn")), medianBotReturn: c.num(k("medianBotReturn")), beatHold: c.int(k("beatHold")))
    }
}

/// ACHETER signals out of sample (non-overlapping per asset). excess = meanNet − baselineNet (same asset and period).
public struct BotBuyStats: Decodable, Sendable, Equatable {
    public var signals: Int
    public var meanNet: Double?
    public var baselineNet: Double?
    public var allDaysNet: Double?
    public var excess: Double?
    public var tStat: Double?
    public var rawTStat: Double?
    /// v2: the t by date / by asset / per signal (`tStat` is then the by-date t); nil in a v1 answer.
    public var clustered: BotClustered?
    public var hitRate: Double?
    public var baselineHitRate: Double?
    public var medianBotReturn: Double?
    public var medianHoldReturn: Double?
    public var beatHold: Int
    public var assets: Int
    public var verdict: ValidationVerdict?
    public var verdictLabel: String

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        signals = c.int(k("signals"))
        meanNet = c.num(k("meanNet"))
        baselineNet = c.num(k("baselineNet"))
        allDaysNet = c.num(k("allDaysNet"))
        excess = c.num(k("excess"))
        tStat = c.num(k("tStat"))
        rawTStat = c.num(k("rawTStat"))
        clustered = c.opt(BotClustered.self, k("clustered"))
        hitRate = c.num(k("hitRate"))
        baselineHitRate = c.num(k("baselineHitRate"))
        medianBotReturn = c.num(k("medianBotReturn"))
        medianHoldReturn = c.num(k("medianHoldReturn"))
        beatHold = c.int(k("beatHold"))
        assets = c.int(k("assets"))
        verdict = c.verdict(k("verdict"))
        verdictLabel = c.str(k("verdictLabel"))
    }
}

/// VENDRE signals: what holding made over the next 20 days after them vs a random day; avoided = baseline − after.
public struct BotSellStats: Decodable, Sendable, Equatable {
    public var signals: Int
    public var meanAfter: Double?
    public var baselineAfter: Double?
    public var allDaysAfter: Double?
    public var avoided: Double?
    public var tStat: Double?
    /// v2 only (see BotBuyStats.clustered).
    public var clustered: BotClustered?
    public var fallRate: Double?
    public var baselineFallRate: Double?
    public var meanDrawdown: Double?
    public var baselineDrawdown: Double?
    /// v2 only: what leaving 20 days at each VENDRE did to the holding.
    public var exit: BotExitStats?
    public var verdict: ValidationVerdict?
    public var verdictLabel: String

    public init(signals: Int, meanAfter: Double?, baselineAfter: Double?, allDaysAfter: Double?, avoided: Double?, tStat: Double?,
                fallRate: Double?, baselineFallRate: Double?, meanDrawdown: Double?, baselineDrawdown: Double?,
                verdict: ValidationVerdict?, verdictLabel: String, clustered: BotClustered? = nil, exit: BotExitStats? = nil) {
        self.signals = signals
        self.meanAfter = meanAfter
        self.baselineAfter = baselineAfter
        self.allDaysAfter = allDaysAfter
        self.avoided = avoided
        self.tStat = tStat
        self.fallRate = fallRate
        self.baselineFallRate = baselineFallRate
        self.meanDrawdown = meanDrawdown
        self.baselineDrawdown = baselineDrawdown
        self.verdict = verdict
        self.verdictLabel = verdictLabel
        self.clustered = clustered
        self.exit = exit
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(signals: c.int(k("signals")), meanAfter: c.num(k("meanAfter")), baselineAfter: c.num(k("baselineAfter")),
                  allDaysAfter: c.num(k("allDaysAfter")), avoided: c.num(k("avoided")), tStat: c.num(k("tStat")), fallRate: c.num(k("fallRate")),
                  baselineFallRate: c.num(k("baselineFallRate")), meanDrawdown: c.num(k("meanDrawdown")),
                  baselineDrawdown: c.num(k("baselineDrawdown")), verdict: c.verdict(k("verdict")), verdictLabel: c.str(k("verdictLabel")),
                  clustered: c.opt(BotClustered.self, k("clustered")), exit: c.opt(BotExitStats.self, k("exit")))
    }
}

public struct BotWaitStats: Decodable, Sendable, Equatable {
    public var days: Int
    public var share: Double?
    public var meanNet: Double?
    public var baselineNet: Double?

    public init(days: Int, share: Double?, meanNet: Double?, baselineNet: Double?) {
        self.days = days
        self.share = share
        self.meanNet = meanNet
        self.baselineNet = baselineNet
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(days: c.int(k("days")), share: c.num(k("share")), meanNet: c.num(k("meanNet")), baselineNet: c.num(k("baselineNet")))
    }
}

/// Statistics of a group (or of all groups together).
public struct BotStats: Decodable, Sendable {
    public var testRows: Int
    public var labelled: Int
    public var buy: BotBuyStats
    public var sell: BotSellStats
    public var wait: BotWaitStats
    /// Brier skill against the training base rate (%): > 0 better, < 0 worse.
    public var brierSkillUp: Double?
    public var brierSkillDown: Double?
    public var calibrationUp: [BotBucket]
    public var calibrationDown: [BotBucket]

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        testRows = c.int(k("testRows"))
        labelled = c.int(k("labelled"))
        buy = c.opt(BotBuyStats.self, k("buy")) ?? emptyObject(BotBuyStats.self)
        sell = c.opt(BotSellStats.self, k("sell")) ?? emptyObject(BotSellStats.self)
        wait = c.opt(BotWaitStats.self, k("wait")) ?? emptyObject(BotWaitStats.self)
        brierSkillUp = c.num(k("brierSkillUp"))
        brierSkillDown = c.num(k("brierSkillDown"))
        calibrationUp = c.list(BotBucket.self, k("calibrationUp"))
        calibrationDown = c.list(BotBucket.self, k("calibrationDown"))
    }
}

public struct BotWeight: Decodable, Sendable {
    public var id: String
    public var coef: Double
    public var mean: Double
    public var sd: Double
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.str(k("id"))
        coef = c.num(k("coef")) ?? 0
        mean = c.num(k("mean")) ?? 0
        sd = c.num(k("sd")) ?? 0
    }
}

public struct BotModelOut: Decodable, Sendable {
    public var baseRate: Double
    public var threshold: Double
    public var intercept: Double
    public var weights: [BotWeight]
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        baseRate = c.num(k("baseRate")) ?? 0
        threshold = c.num(k("threshold")) ?? 0
        intercept = c.num(k("intercept")) ?? 0
        weights = c.list(BotWeight.self, k("weights"))
    }
}

/// Inner-validation log-loss of one candidate (lower is better).
public struct BotCandidateScore: Decodable, Sendable, Equatable {
    public var id: BotCandidate?
    public var logLoss: Double?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.candidate(k("id"))
        logLoss = c.num(k("logLoss"))
    }
}

/// Probabilities of the trend rule (v2 candidate without learning).
public struct BotTrendModel: Decodable, Sendable {
    public var probs: [Double]
    public var baseRate: Double
    public var rows: Int
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        probs = c.list(Double.self, k("probs"))
        baseRate = c.num(k("baseRate")) ?? 0
        rows = c.int(k("rows"))
    }
}

/// A live side model of v2: `logit` weights, `trees` (compact nodes, only flagged here) or `trend` probabilities.
public struct BotLiveSide: Decodable, Sendable {
    public var baseRate: Double
    public var threshold: Double
    public var logit: BotModelOut?
    /// Whether the side is boosted trees (their nodes are not read by the app).
    public var hasTrees: Bool
    public var trend: BotTrendModel?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        baseRate = c.num(k("baseRate")) ?? 0
        threshold = c.num(k("threshold")) ?? 0
        logit = c.opt(BotModelOut.self, k("logit"))
        hasTrees = c.contains(k("trees")) && ((try? c.decodeNil(forKey: k("trees"))) ?? true) == false
        trend = c.opt(BotTrendModel.self, k("trend"))
    }
}

/// The model trained on the whole known history (today's view).
public struct BotLiveModel: Decodable, Sendable {
    public var trainedRows: Int
    public var trainedFrom: Double?
    public var trainedTo: Double?
    public var up: BotModelOut
    public var down: BotModelOut
    /// v2: the candidate chosen on the last known year, the inner-validation scores and both side models.
    public var candidate: BotCandidate?
    public var scores: [BotCandidateScore]
    public var upModel: BotLiveSide?
    public var downModel: BotLiveSide?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        trainedRows = c.int(k("trainedRows"))
        trainedFrom = c.num(k("trainedFrom"))
        trainedTo = c.num(k("trainedTo"))
        up = c.opt(BotModelOut.self, k("up")) ?? emptyObject(BotModelOut.self)
        down = c.opt(BotModelOut.self, k("down")) ?? emptyObject(BotModelOut.self)
        candidate = c.candidate(k("candidate"))
        scores = c.list(BotCandidateScore.self, k("scores"))
        upModel = c.opt(BotLiveSide.self, k("upModel"))
        downModel = c.opt(BotLiveSide.self, k("downModel"))
    }
}

/// One candidate's own walk-forward result: for information only, never used to choose.
public struct BotCandidateStat: Decodable, Sendable, Identifiable {
    public var id: BotCandidate
    public var label: String
    public var description: String
    public var trainedBlocks: Int
    public var chosenBlocks: Int
    public var stats: BotStats
    public var buy: BotBuyStats { stats.buy }
    public var sell: BotSellStats { stats.sell }
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.candidate(k("id")) ?? .v1
        label = c.str(k("label"))
        description = c.str(k("description"))
        trainedBlocks = c.int(k("trainedBlocks"))
        chosenBlocks = c.int(k("chosenBlocks"))
        stats = try BotStats(from: decoder)
    }
}

/// One retraining: test period [start, end), training rows, inner-validation log-loss of each candidate, choice.
public struct BotBlockOut: Decodable, Sendable, Equatable {
    public var start: Double
    public var end: Double?
    public var trainRows: Int
    public var chosen: BotCandidate?
    public var scores: [BotCandidateScore]

    public init(start: Double, end: Double?, trainRows: Int, chosen: BotCandidate?, scores: [BotCandidateScore] = []) {
        self.start = start
        self.end = end
        self.trainRows = trainRows
        self.chosen = chosen
        self.scores = scores
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(start: c.num(k("start")) ?? 0, end: c.num(k("end")), trainRows: c.int(k("trainRows")), chosen: c.candidate(k("chosen")),
                  scores: c.list(BotCandidateScore.self, k("scores")))
    }
}

/// Training universe of a group: basket assets, extra assets (and how many failed), rows, history span.
public struct BotUniverse: Decodable, Sendable, Equatable {
    public var basket: Int
    public var extra: Int
    public var extraFailed: Int
    public var rows: Int
    public var dataFrom: Double?
    public var medianYears: Double?
    public var maxYears: Double?

    public init(basket: Int, extra: Int, extraFailed: Int, rows: Int, dataFrom: Double?, medianYears: Double?, maxYears: Double?) {
        self.basket = basket
        self.extra = extra
        self.extraFailed = extraFailed
        self.rows = rows
        self.dataFrom = dataFrom
        self.medianYears = medianYears
        self.maxYears = maxYears
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(basket: c.int(k("basket")), extra: c.int(k("extra")), extraFailed: c.int(k("extraFailed")), rows: c.int(k("rows")),
                  dataFrom: c.num(k("dataFrom")), medianYears: c.num(k("medianYears")), maxYears: c.num(k("maxYears")))
    }
}

/// The last 12 months, shown apart (nothing is chosen on them).
public struct BotHoldout: Decodable, Sendable {
    public var from: Double?
    public var to: Double?
    public var stats: BotStats
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        from = c.num(k("from"))
        to = c.num(k("to"))
        stats = try BotStats(from: decoder)
    }
}

/// Stocks or cryptos: the statistics are flattened in the same object on the server.
public struct BotGroupStat: Decodable, Sendable, Identifiable {
    public var id: BotGroup
    public var label: String
    public var assets: Int
    public var testFrom: Double?
    public var testTo: Double?
    public var blocks: Int
    public var trainedBlocks: Int
    public var stats: BotStats
    public var model: BotLiveModel?
    public var text: String
    /// v2 (nil / empty in a v1 answer).
    public var universe: BotUniverse?
    public var dataYears: Double?
    /// One per retraining block, in time order.
    public var selection: [BotBlockOut]
    /// Each candidate alone, for information only (fixed order v1, logit, trees, trend).
    public var candidates: [BotCandidateStat]
    public var holdout: BotHoldout?
    /// The nested result on the extra training assets (out of sample too, not the headline).
    public var extra: BotStats?
    /// "S&P 500 (SPY)".
    public var market: String?

    public var buy: BotBuyStats { stats.buy }
    public var sell: BotSellStats { stats.sell }
    public var wait: BotWaitStats { stats.wait }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = BotGroup(rawValue: c.str(k("id"))) ?? .stock
        label = c.str(k("label"))
        assets = c.int(k("assets"))
        testFrom = c.num(k("testFrom"))
        testTo = c.num(k("testTo"))
        blocks = c.int(k("blocks"))
        trainedBlocks = c.int(k("trainedBlocks"))
        stats = try BotStats(from: decoder)
        model = c.opt(BotLiveModel.self, k("model"))
        text = c.str(k("text"))
        universe = c.opt(BotUniverse.self, k("universe"))
        dataYears = c.num(k("dataYears"))
        selection = c.list(BotBlockOut.self, k("selection"))
        candidates = c.list(BotCandidateStat.self, k("candidates"))
        holdout = c.opt(BotHoldout.self, k("holdout"))
        extra = c.opt(BotStats.self, k("extra"))
        market = c.opt(String.self, k("market"))
    }
}

/// Today's action of an asset of the basket.
public struct BotNowView: Decodable, Sendable {
    public var time: Double?
    public var action: BotAction?
    public var up: Double?
    public var down: Double?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        time = c.num(k("time"))
        action = c.action(k("action"))
        up = c.num(k("up"))
        down = c.num(k("down"))
    }
}

public struct BotAssetRow: Decodable, Sendable, Identifiable {
    public var symbol: String
    public var name: String
    public var kind: Kind
    public var assetClass: ValidationClass
    public var group: BotGroup
    public var testRows: Int
    public var testFrom: Double?
    public var testTo: Double?
    public var buys: Int
    public var buyMean: Double?
    public var buyExcess: Double?
    public var sells: Int
    public var sellAvoided: Double?
    public var waitShare: Double?
    public var botReturn: Double?
    public var holdReturn: Double?
    public var now: BotNowView
    public var source: String
    /// v2: history span (years, first day), time out of the market after VENDRE (%), worst falls holding / with the bot (%).
    public var years: Double?
    public var dataFrom: Double?
    public var outShare: Double?
    public var holdMaxDrawdown: Double?
    public var botMaxDrawdown: Double?
    /// v3: today's actions of the 4 headline configurations (nil in a v1 / v2 answer).
    public var v3: BotV3AssetNow?
    /// v4: the selective bots speaking today on it (nil before v4).
    public var v4: BotV4AssetNow?
    public var id: String { "\(kind.rawValue):\(symbol)" }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name.isEmpty ? symbol : name) }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        symbol = c.str(k("symbol"))
        name = c.str(k("name"))
        assetClass = ValidationClass(rawValue: c.str(k("class"))) ?? .stock
        kind = Kind(rawValue: c.str(k("kind"))) ?? (assetClass == .stock ? .stock : .crypto)
        group = BotGroup(rawValue: c.str(k("group"))) ?? (kind == .crypto ? .crypto : .stock)
        testRows = c.int(k("testRows"))
        testFrom = c.num(k("testFrom"))
        testTo = c.num(k("testTo"))
        buys = c.int(k("buys"))
        buyMean = c.num(k("buyMean"))
        buyExcess = c.num(k("buyExcess"))
        sells = c.int(k("sells"))
        sellAvoided = c.num(k("sellAvoided"))
        waitShare = c.num(k("waitShare"))
        botReturn = c.num(k("botReturn"))
        holdReturn = c.num(k("holdReturn"))
        now = c.opt(BotNowView.self, k("now")) ?? emptyObject(BotNowView.self)
        source = c.str(k("source"))
        years = c.num(k("years"))
        dataFrom = c.num(k("dataFrom"))
        outShare = c.num(k("outShare"))
        holdMaxDrawdown = c.num(k("holdMaxDrawdown"))
        botMaxDrawdown = c.num(k("botMaxDrawdown"))
        v3 = c.opt(BotV3AssetNow.self, k("v3"))
        v4 = c.opt(BotV4AssetNow.self, k("v4"))
    }
}

public struct BotFeature: Decodable, Sendable {
    public var id: String
    public var label: String
    public var help: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.str(k("id"))
        label = c.str(k("label"))
        help = c.str(k("help"))
    }
}

public struct BotParameters: Decodable, Sendable {
    public var horizonDays: Int
    public var retrainEvery: Int
    public var purgeDays: Int
    public var minTrainDays: Int
    public var warmup: Int
    public var l2: Double?
    /// Points of % above the training base rate.
    public var thresholdMargin: Double?
    /// Round-trip cost (fees, slippage, spread) by group, %.
    public var costStockPct: Double?
    public var costCryptoPct: Double?
    public var minSignals: Int
    public var tEdge: Double?
    public var nudge: Double?
    /// v2 (nil in a v1 answer).
    public var innerValidationDays: Int?
    public var trainStride: Int?
    public var holdoutDays: Int?
    public var stockYears: Int?
    public var trees: Int?
    public var treeDepth: Int?
    public var shrinkage: Double?
    public var minLeaf: Int?
    /// How the candidate is chosen at each retraining.
    public var selection: String?
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        horizonDays = c.opt(Int.self, k("horizonDays")) ?? 20
        retrainEvery = c.int(k("retrainEvery"))
        purgeDays = c.int(k("purgeDays"))
        minTrainDays = c.int(k("minTrainDays"))
        warmup = c.int(k("warmup"))
        l2 = c.num(k("l2"))
        thresholdMargin = c.num(k("thresholdMargin"))
        costStockPct = c.num(k("costStockPct"))
        costCryptoPct = c.num(k("costCryptoPct"))
        minSignals = c.int(k("minSignals"))
        tEdge = c.num(k("tEdge"))
        nudge = c.num(k("nudge"))
        func optInt(_ s: String) -> Int? { c.num(k(s)).map { Int($0) } }
        innerValidationDays = optInt("innerValidationDays")
        trainStride = optInt("trainStride")
        holdoutDays = optInt("holdoutDays")
        stockYears = optInt("stockYears")
        trees = optInt("trees")
        treeDepth = optInt("treeDepth")
        shrinkage = c.num(k("shrinkage"))
        minLeaf = optInt("minLeaf")
        selection = c.opt(String.self, k("selection"))
    }
}

public struct BotReport: Decodable, Sendable {
    public var asOf: Double?
    public var basketFixedOn: String
    /// Plain-French verdict.
    public var headline: String
    public var overall: BotStats
    /// Stocks then cryptos (those with at least one asset).
    public var groups: [BotGroupStat]
    /// Basket order, never ranked by performance.
    public var assets: [BotAssetRow]
    /// Same shape as the validation's failures.
    public var failures: [ValidationFailure]
    public var features: [BotFeature]
    public var parameters: BotParameters
    public var method: [String]
    public var limits: [String]
    public var source: String
    /// v2: 2 (nil in a v1 answer), what changed since v1, extra assets that could not be read, when the extra universe
    /// was fixed, how long the download and the computation took.
    public var version: Int?
    public var changes: [String]
    public var extraFailures: [ValidationFailure]
    public var extraFixedOn: String?
    public var timing: BotTiming?
    /// Since v3: the pre-registered v3; the v2-shaped fields then hold v2's selection at 20 days, at the corrected threshold.
    public var v3: BotV3Report?
    /// Since v4: the pre-registered selective bots (precision first, forward test from 02/10/2026).
    public var v4: BotV4Report?

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        asOf = c.num(k("asOf"))
        basketFixedOn = c.str(k("basketFixedOn"))
        headline = c.str(k("headline"))
        overall = c.opt(BotStats.self, k("overall")) ?? emptyObject(BotStats.self)
        groups = c.list(BotGroupStat.self, k("groups"))
        assets = c.list(BotAssetRow.self, k("assets"))
        failures = c.list(ValidationFailure.self, k("failures"))
        features = c.list(BotFeature.self, k("features"))
        parameters = c.opt(BotParameters.self, k("parameters")) ?? emptyObject(BotParameters.self)
        method = c.list(String.self, k("method"))
        limits = c.list(String.self, k("limits"))
        source = c.str(k("source"))
        version = c.num(k("version")).map { Int($0) }
        changes = c.list(String.self, k("changes"))
        extraFailures = c.list(ValidationFailure.self, k("extraFailures"))
        extraFixedOn = c.opt(String.self, k("extraFixedOn"))
        timing = c.opt(BotTiming.self, k("timing"))
        v3 = c.opt(BotV3Report.self, k("v3"))
        v4 = c.opt(BotV4Report.self, k("v4"))
    }
}

public struct BotTiming: Decodable, Sendable {
    public var fetchMs: Double
    public var computeMs: Double
    public var threads: Int
    /// Since v3: resident memory before the computation and its peak (MB; nil when not measured).
    public var rssBeforeMb: Double?
    public var peakRssMb: Double?

    public init(fetchMs: Double, computeMs: Double, threads: Int, rssBeforeMb: Double? = nil, peakRssMb: Double? = nil) {
        self.fetchMs = fetchMs
        self.computeMs = computeMs
        self.threads = threads
        self.rssBeforeMb = rssBeforeMb
        self.peakRssMb = peakRssMb
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(fetchMs: c.num(k("fetchMs")) ?? 0, computeMs: c.num(k("computeMs")) ?? 0, threads: c.int(k("threads")),
                  rssBeforeMb: c.num(k("rssBeforeMb")), peakRssMb: c.num(k("peakRssMb")))
    }
}

// MARK: - Today's view (`bot` of /api/decision, /api/bot/views)

/// One of the top 3 contributions to the probability of the action's model.
public struct BotContribution: Codable, Sendable, Equatable {
    public var id: String
    public var label: String
    public var value: Double
    public var valueText: String
    public var weight: Double
    /// "up": pushes the probability up, "down": down.
    public var effect: String
    public var text: String

    enum CodingKeys: String, CodingKey { case id, label, value, valueText, weight, effect, text }

    public init(id: String, label: String, value: Double, valueText: String, weight: Double, effect: String, text: String) {
        self.id = id
        self.label = label
        self.value = value
        self.valueText = valueText
        self.weight = weight
        self.effect = effect
        self.text = text
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(id: c.str(.id), label: c.str(.label), value: c.num(.value) ?? 0, valueText: c.str(.valueText), weight: c.num(.weight) ?? 0,
                  effect: c.str(.effect), text: c.str(.text))
    }
}

/// The bot's view of one asset now, from the cached report only. Older servers send no `bot` (nil in the decision);
/// inside the block every field falls back to the backend's default when missing or odd.
public struct BotView: Codable, Sendable, Equatable {
    /// False when no report is cached yet or the group has no model (the text says so).
    public var available: Bool
    public var group: BotGroup
    public var groupLabel: String
    /// Whether the asset is in the basket the bot was trained and tested on.
    public var inBasket: Bool
    /// nil without enough history.
    public var action: BotAction?
    public var actionLabel: String?
    /// Probabilities of a rise and of a fall over 20 days, thresholds and training base rates, %.
    public var up: Double?
    public var down: Double?
    public var thresholdUp: Double?
    public var thresholdDown: Double?
    public var baseUp: Double?
    public var baseDown: Double?
    /// Out-of-sample verdicts of the group's two sides.
    public var buyVerdict: ProofVerdict?
    public var sellVerdict: ProofVerdict?
    /// Today's action is on a side with an out-of-sample edge: it counts (a little) in the decision.
    public var counts: Bool
    public var time: Double?
    public var contributions: [BotContribution]
    public var text: String
    /// Whether and how it counts.
    public var note: String
    /// The report's time (ms).
    public var asOf: Double?
    public var link: String
    /// Items of /api/bot/views only.
    public var symbol: String?
    public var kind: Kind?
    /// v2: the candidate model behind today's view and its label ("Régression logistique 24 mesures").
    public var model: BotCandidate?
    public var modelLabel: String?
    /// Since v3: the 4 headline configurations today; `counts` and `note` are then theirs.
    public var v3: BotV3View?
    /// Since v4: the group's selective bots today; the server's `counts` and `note` then include them.
    public var v4: BotV4View?

    enum CodingKeys: String, CodingKey {
        case available, group, groupLabel, inBasket, action, actionLabel, up, down, thresholdUp, thresholdDown, baseUp, baseDown
        case buyVerdict, sellVerdict, counts, time, contributions, text, note, asOf, link, symbol, kind, model, modelLabel, v3, v4
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = c.bool(.available)
        group = c.opt(String.self, .group).flatMap(BotGroup.init(rawValue:)) ?? .stock
        groupLabel = c.str(.groupLabel)
        inBasket = c.bool(.inBasket)
        action = c.action(.action)
        actionLabel = c.opt(String.self, .actionLabel)
        up = c.num(.up)
        down = c.num(.down)
        thresholdUp = c.num(.thresholdUp)
        thresholdDown = c.num(.thresholdDown)
        baseUp = c.num(.baseUp)
        baseDown = c.num(.baseDown)
        buyVerdict = c.opt(ProofVerdict.self, .buyVerdict)
        sellVerdict = c.opt(ProofVerdict.self, .sellVerdict)
        counts = c.bool(.counts)
        time = c.num(.time)
        contributions = c.list(BotContribution.self, .contributions)
        text = c.str(.text)
        note = c.str(.note)
        asOf = c.num(.asOf)
        link = c.str(.link)
        symbol = c.opt(String.self, .symbol)
        kind = c.opt(String.self, .kind).flatMap(Kind.init(rawValue:))
        model = c.opt(String.self, .model).flatMap(BotCandidate.init(rawValue:))
        modelLabel = c.opt(String.self, .modelLabel)
        v3 = c.opt(BotV3View.self, .v3)
        v4 = c.opt(BotV4View.self, .v4)
    }

    /// With an action: shown with its probabilities; without: only the text.
    public var hasAction: Bool { available && action != nil }

    /// Colour of the decision block's edge, as the web's `dec-proof` classes: not available, counts, does not count.
    public enum Tone: String, Sendable { case na, edge, unproven }
    public var tone: Tone { !hasAction ? .na : countsNow ? .edge : .unproven }

    /// "compte" / "ne compte pas" (only with an action; v3's `counts` when present).
    public var countsLabel: String { countsNow ? "compte" : "ne compte pas" }
}

/// Answer of /api/bot/views.
public struct BotViews: Decodable, Sendable {
    public var asOf: Double?
    public var views: [BotView]
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        asOf = c.num(k("asOf"))
        views = c.list(BotView.self, k("views"))
    }
}

public enum BotResult: Sendable {
    case ready(BotReport)
    /// The first training and test over the whole basket is still running (≈ 1 min): ask again.
    case pending
}

extension AltimClient {
    /// « Bot Altim » trained and tested walk-forward on the validation's basket (heavy: 202 `.pending` until ready).
    public func bot() async throws -> BotResult {
        let (data, status) = try await getWithStatus(ModelBot.path, [:])
        if status == 202 { return .pending }
        return .ready(try decodeResponse(BotReport.self, data, status))
    }

    /// Today's view of the bot for these assets (20 at most, cached report only).
    public func botViews(_ assets: [Asset]) async throws -> BotViews {
        let (data, status) = try await getWithStatus(ModelBot.viewsPath, ModelBot.viewsQuery(assets))
        return try decodeResponse(BotViews.self, data, status)
    }
}

// MARK: - Helpers of the screen and of the decision line

public enum ModelBot {
    public static let path = "/api/bot"
    public static let viewsPath = "/api/bot/views"

    /// `symbols=BTC:crypto,AAPL:stock` (20 at most, as the server).
    public static func viewsQuery(_ assets: [Asset]) -> [String: String] {
        ["symbols": assets.prefix(20).map { "\($0.symbol):\($0.kind.rawValue)" }.joined(separator: ",")]
    }

    static let nnbsp = "\u{202F}"

    /// "+0,52 point", "−2,44 points", "—".
    public static func points(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v < 0 ? "−" : v > 0 ? "+" : "")\(JSFormat.fr(abs(v), max: 2)) point\(abs(v) >= 2 ? "s" : "")"
    }

    /// "54 %", "—".
    public static func pct0(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(JSFormat.fr(v, max: 0))\(nnbsp)%"
    }

    /// "12 504" (grouped like the browser's fr-FR).
    public static func count(_ n: Int) -> String { JSFormat.fr(Double(n), max: 0) }

    static func tText(_ t: Double?) -> String { t.map { "t = \(ModelValidation.plain($0, digits: 1))" } ?? "t non calculable" }

    /// v2 (clustered present): the t is by date.
    static func sideT(_ t: Double?, _ c: BotClustered?) -> String {
        if c != nil, let t { return "t par jour = \(ModelValidation.plain(t, digits: 1))" }
        return tText(t)
    }

    /// "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8" (the verdict reads the first).
    public static func clusteredText(_ c: BotClustered?, _ t: Double?) -> String {
        guard let c else { return "t = \(t == nil ? "—" : ModelValidation.plain(t, digits: 1))" }
        func f(_ v: Double?) -> String { v == nil ? "—" : ModelValidation.plain(v, digits: 1) }
        return "t par jour \(f(c.byDate)) (\(c.dates) jours) · par actif \(f(c.byAsset)) · par signal \(f(c.perSignal))"
    }

    /// What leaving at each VENDRE did to the holding: time out, worst fall, return (medians over the assets).
    public static func exitText(_ e: BotExitStats?) -> String? {
        guard let e, e.assets != 0 else { return nil }
        return "En sortant 20 jours à chaque VENDRE : hors marché \(pct0(e.outShare)) du temps ; pire baisse médiane \(ModelValidation.signedPct(e.medianBotMaxDrawdown)) contre \(ModelValidation.signedPct(e.medianHoldMaxDrawdown)) en gardant ; rendement médian \(ModelValidation.signedPct(e.medianBotReturn, digits: 0)) contre \(ModelValidation.signedPct(e.medianHoldReturn, digits: 0)) (mieux que garder : \(e.beatHold) actif\(e.beatHold > 1 ? "s" : "") sur \(e.assets))."
    }

    /// "22 actifs du panier et 72 de plus à l'entraînement · historique médian 20,1 ans (le plus long 20,1 ans) · marché : S&P 500 (SPY)".
    public static func dataText(_ g: BotGroupStat) -> String? {
        guard let u = g.universe else { return nil }
        func y(_ v: Double?) -> String { v == nil ? "—" : "\(ModelValidation.plain(v, digits: 1)) ans" }
        let failed = u.extraFailed > 0 ? " (\(u.extraFailed) indisponible\(u.extraFailed > 1 ? "s" : ""))" : ""
        let market = (g.market ?? "").isEmpty ? "" : " · marché : \(g.market ?? "")"
        return "\(u.basket) actif\(u.basket > 1 ? "s" : "") du panier et \(u.extra) de plus à l'entraînement\(failed) · historique médian \(y(u.medianYears)) (le plus long \(y(u.maxYears)))\(market)"
    }

    /// Consecutive retrainings with the same choice: `from` / `to` = start of the first / last retraining of the run.
    public struct SelectionRun: Equatable, Sendable {
        public var from: Double
        public var to: Double
        public var chosen: BotCandidate?
        public var count: Int
        public init(from: Double, to: Double, chosen: BotCandidate?, count: Int) {
            self.from = from
            self.to = to
            self.chosen = chosen
            self.count = count
        }
    }

    /// Retrainings grouped by consecutive identical choices.
    public static func selectionRuns(_ blocks: [BotBlockOut]) -> [SelectionRun] {
        var out: [SelectionRun] = []
        for b in blocks {
            if let last = out.last, last.chosen == b.chosen {
                out[out.count - 1].to = b.start
                out[out.count - 1].count += 1
            } else {
                out.append(SelectionRun(from: b.start, to: b.start, chosen: b.chosen, count: 1))
            }
        }
        return out
    }

    /// "oct. 2009 → oct. 2016 (8 fois)" or "oct. 2009".
    public static func runPeriod(_ x: SelectionRun) -> String {
        "\(ModelValidation.monthYear(x.from))\(x.count > 1 ? " → \(ModelValidation.monthYear(x.to)) (\(x.count) fois)" : "")"
    }

    /// "Tendance", or "aucun (trop peu de données)".
    public static func runChoice(_ x: SelectionRun) -> String { x.chosen?.short ?? "aucun (trop peu de données)" }

    /// "Aujourd'hui : Logistique 24, choisi de la même façon sur la dernière année connue." (nil without a live candidate).
    public static func liveModelText(_ g: BotGroupStat) -> String? {
        g.model?.candidate.map { "Aujourd'hui : \($0.short), choisi de la même façon sur la dernière année connue." }
    }

    /// "retenu 4 fois sur 17".
    public static func chosenText(_ c: BotCandidateStat) -> String { "retenu \(c.chosenBlocks) fois sur \(c.trainedBlocks)" }

    /// Key / value lines of a candidate alone.
    public static func candidateRows(_ c: BotCandidateStat) -> [(label: String, value: String)] {
        [("\(c.buy.signals) achats · écart au hasard", "\(points(c.buy.excess)) (t \(ModelValidation.plain(c.buy.tStat, digits: 1)))"),
         ("\(c.sell.signals) ventes · baisse évitée", "\(points(c.sell.avoided)) (t \(ModelValidation.plain(c.sell.tStat, digits: 1)))"),
         ("Précision hausse / baisse", "\(ModelValidation.plain(c.stats.brierSkillUp, digits: 1)) % / \(ModelValidation.plain(c.stats.brierSkillDown, digits: 1)) %")]
    }

    /// "Achats : avantage non démontré" (the verdict's label, first letter lowered).
    public static func sideVerdictLabel(_ side: String, _ label: String) -> String {
        "\(side) : \(label.prefix(1).lowercased())\(label.dropFirst())"
    }

    /// "29/09/2025" (UTC day, like the web's toLocaleDateString fr-FR in UTC), "?" without a time.
    public static func day(_ ms: Double?) -> String {
        guard let ms, ms.isFinite else { return "?" }
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let d = cal.dateComponents([.year, .month, .day], from: Date(timeIntervalSince1970: ms / 1000))
        return String(format: "%02d/%02d/%d", d.day ?? 0, d.month ?? 0, d.year ?? 0)
    }

    /// "Actions et ETF américains · 12 derniers mois".
    public static func holdoutTitle(_ g: BotGroupStat) -> String { "\(g.label) · 12 derniers mois" }

    public static func holdoutNote(_ h: BotHoldout) -> String {
        "Du \(day(h.from)) au \(day(h.to)), présentés à part (même modèle choisi ; rien n'est choisi sur cette période)."
    }

    /// "Actions et ETF américains · actifs d'entraînement hors panier".
    public static func extraTitle(_ g: BotGroupStat) -> String { "\(g.label) · actifs d'entraînement hors panier" }

    public static func extraNote(_ g: BotGroupStat) -> String {
        "\(g.universe?.extra ?? 0) actifs fixés d'avance, hors du test principal ; eux aussi jugés hors échantillon."
    }

    /// "Univers élargi : 2 actifs indisponibles (X, Y)." (nil when none).
    public static func extraFailuresText(_ r: BotReport) -> String? {
        let f = r.extraFailures
        guard !f.isEmpty else { return nil }
        return "Univers élargi : \(f.count) actif\(f.count > 1 ? "s" : "") indisponible\(f.count > 1 ? "s" : "") (\(f.map(\.symbol).joined(separator: ", ")))."
    }

    /// Groups that carry each candidate alone (v2).
    public static func withCandidates(_ r: BotReport) -> [BotGroupStat] { r.groups.filter { !$0.candidates.isEmpty } }

    /// Whether the « Dernière année et univers élargi » section has something to show.
    public static func hasSubResults(_ r: BotReport) -> Bool { r.groups.contains { $0.holdout != nil || $0.extra != nil } }

    /// "108 achats : +1,7 % en moyenne sur 20 jours, contre +1,85 % pour une entrée au hasard … (−0,15 point, t = −0,1)."
    public static func buyText(_ b: BotBuyStats) -> String {
        if b.signals == 0 { return "Aucun achat pendant les périodes de test." }
        return "\(b.signals) achat\(b.signals > 1 ? "s" : "") : \(ModelValidation.signedPct(b.meanNet, digits: 2)) en moyenne sur 20 jours, contre \(ModelValidation.signedPct(b.baselineNet, digits: 2)) pour une entrée au hasard sur le même actif et la même période (\(points(b.excess)), \(sideT(b.tStat, b.clustered)))."
    }

    /// What followed the VENDRE signals, said without a sign to decode.
    public static func sellText(_ s: BotSellStats) -> String {
        if s.signals == 0 { return "Aucune vente pendant les périodes de test." }
        let diff: String
        if let a = s.avoided {
            diff = a >= 0
                ? "cours ensuite inférieur de \(points(a).replacingOccurrences(of: "+", with: ""))"
                : "cours ensuite supérieur de \(points(-a).replacingOccurrences(of: "+", with: ""))"
        } else {
            diff = ""
        }
        return "\(s.signals) vente\(s.signals > 1 ? "s" : "") : le cours a fait \(ModelValidation.signedPct(s.meanAfter, digits: 2)) dans les 20 jours suivants, contre \(ModelValidation.signedPct(s.baselineAfter, digits: 2)) après un jour au hasard (\(diff), \(sideT(s.tStat, s.clustered)))."
    }

    public static func waitText(_ w: BotWaitStats) -> String {
        if w.days == 0 { return "Jamais sur ATTENDRE pendant les tests." }
        return "ATTENDRE \(pct0(w.share)) des jours testés, suivis en moyenne de \(ModelValidation.signedPct(w.meanNet, digits: 2)) (tous les jours : \(ModelValidation.signedPct(w.baselineNet, digits: 2)))."
    }

    /// Calibration buckets that have days in them.
    public static func calibrationRows(_ b: [BotBucket]) -> [BotBucket] { b.filter { $0.rows > 0 } }

    /// "pas mieux que la fréquence de base", "1,1 % moins bien que la fréquence de base" from a Brier skill (%).
    public static func skillText(_ skill: Double?) -> String {
        guard let skill else { return "non calculable" }
        if abs(skill) < 0.5 { return "pas mieux que la fréquence de base" }
        return skill > 0
            ? "\(JSFormat.fr(skill, max: 1))\(nnbsp)% mieux que la fréquence de base"
            : "\(JSFormat.fr(-skill, max: 1))\(nnbsp)% moins bien que la fréquence de base"
    }

    /// Decision line: "ATTENDRE · hausse 54 %, baisse 44 %" (the text when there is no action).
    public static func summary(_ v: BotView) -> String {
        guard v.available, let a = v.action else { return v.text }
        return "\(a.label) · hausse \(pct0(v.up)), baisse \(pct0(v.down))"
    }

    /// Whether any group has an edge on either side (then the headline says which one counts).
    public static func anyEdge(_ r: BotReport) -> Bool {
        r.groups.contains { $0.buy.verdict == .edge || $0.sell.verdict == .edge }
    }

    // Texts of the screen (same wording as Bot.tsx).

    public static let intro = "Des modèles appris sur de longs historiques (actions depuis 1990, cryptos depuis leur cotation), qui disent ACHETER, ATTENDRE ou VENDRE à 20 et 60 jours. Jugés seulement sur des périodes qu'ils n'avaient pas vues, sur 34 actifs fixés d'avance, avec un seuil corrigé des essais multiples. Altim ne passe aucun ordre."
    public static let pendingText = "Téléchargement des historiques, entraînement et test en cours (plusieurs minutes la première fois)…"
    public static let changesTitle = "Ce qui change avec la v2"
    public static let resultsIntro = "Modèle choisi à chaque réentraînement sur une validation interne (jamais sur le test), testé sur les actifs du panier."
    public static let candidatesTitle = "Chaque modèle seul"
    public static let candidatesIntro = "À titre d'information, non utilisé pour choisir : ce qu'aurait donné chaque candidat retenu partout, sur les mêmes jours."
    public static let subResultsTitle = "Dernière année et univers élargi"
    public static let selectionTitle = "Modèle retenu à chaque réentraînement"
    public static let settingsText = "Un modèle appris qui dit ACHETER, ATTENDRE ou VENDRE, jugé seulement sur des périodes qu'il n'avait pas vues, avec ses résultats réels et ses limites."
    public static let calibrationIntro = "Quand le bot annonce une probabilité, la fréquence observée ensuite devrait être proche. Chaque ligne : jours de test dont la probabilité tombait dans la tranche."
    public static let assetsIntro = "Dans l'ordre du panier, jamais classés par performance. « Aujourd'hui » : avis du modèle retenu, entraîné sur tout l'historique connu."
    public static let linkText = "Voir le bot et ses résultats →"

    /// "Actifs": "33 / 34".
    public static func assetsTile(_ r: BotReport) -> String { "\(r.assets.count) / \(r.assets.count + r.failures.count)" }

    /// "Achats / ventes": "207 / 54".
    public static func signalsTile(_ r: BotReport) -> String { "\(r.overall.buy.signals) / \(r.overall.sell.signals)" }

    /// "Horizon 20 jours · seuil : … · coûts d'un aller-retour 0,31 % (actions), 0,32 % (cryptos)." (without the date).
    public static func parametersText(_ r: BotReport) -> String {
        let p = r.parameters
        return "Horizon \(p.horizonDays) jours · seuil : fréquence d'entraînement + \(ModelValidation.plain(p.thresholdMargin, digits: 0)) points · coûts d'un aller-retour \(ModelValidation.plain(p.costStockPct, digits: 2)) % (actions), \(ModelValidation.plain(p.costCryptoPct, digits: 2)) % (cryptos)."
    }

    /// "1 actif non utilisé :".
    public static func failuresTitle(_ n: Int) -> String { "\(n) actif\(n > 1 ? "s" : "") non utilisé\(n > 1 ? "s" : "") :" }

    /// "22 actifs testés · test du oct. 2024 au sept. 2026 · 4 réentraînements".
    public static func groupSubtitle(_ g: BotGroupStat) -> String {
        "\(g.assets) actif\(g.assets > 1 ? "s" : "") testé\(g.assets > 1 ? "s" : "") · test du \(ModelValidation.monthYear(g.testFrom)) au \(ModelValidation.monthYear(g.testTo)) · \(g.trainedBlocks) réentraînement\(g.trainedBlocks > 1 ? "s" : "")"
    }

    /// Key / value lines under each side of a group card.
    public static func buyRows(_ b: BotBuyStats) -> [(label: String, value: String)] {
        [("Achats gagnants / jours gagnants", "\(pct0(b.hitRate)) / \(pct0(b.baselineHitRate))"),
         ("Achats cumulés / détention (médianes)", "\(ModelValidation.signedPct(b.medianBotReturn)) / \(ModelValidation.signedPct(b.medianHoldReturn))")]
    }

    public static func sellRows(_ s: BotSellStats) -> [(label: String, value: String)] {
        [("Suivies d'une baisse / tous les jours", "\(pct0(s.fallRate)) / \(pct0(s.baselineFallRate))"),
         ("Pire recul moyen ensuite / au hasard", "\(ModelValidation.signedPct(s.meanDrawdown)) / \(ModelValidation.signedPct(s.baselineDrawdown))")]
    }

    /// "Actions et ETF américains · hausse".
    public static func calibrationTitle(_ g: BotGroupStat, up: Bool) -> String { "\(g.label) · \(up ? "hausse" : "baisse")" }

    /// "Précision : 1,1 % moins bien que la fréquence de base."
    public static func precisionText(_ skill: Double?) -> String { "Précision : \(skillText(skill))." }

    /// "12 504 jours".
    public static func bucketDays(_ b: BotBucket) -> String { "\(count(b.rows)) jours" }

    /// "prévu 55 % · observé 55 %".
    public static func bucketText(_ b: BotBucket) -> String { "prévu \(pct0(b.predicted)) · observé \(pct0(b.realised))" }

    /// "Aujourd'hui : hausse 56 %, baisse 40 %".
    public static func todayText(_ a: BotAssetRow) -> String { "Aujourd'hui : hausse \(pct0(a.now.up)), baisse \(pct0(a.now.down))" }

    /// Test line of an asset row.
    public static func assetTestText(_ a: BotAssetRow) -> String {
        var s = "Test : \(a.buys) achat\(a.buys > 1 ? "s" : "")"
        if let e = a.buyExcess { s += " (\(points(e)) vs hasard)" }
        s += " · \(a.sells) vente\(a.sells > 1 ? "s" : "")"
        if let v = a.sellAvoided { s += " (cours ensuite \(points(-v)) vs hasard)" }
        s += " · attente \(pct0(a.waitShare)) · achats cumulés \(ModelValidation.signedPct(a.botReturn)), détention \(ModelValidation.signedPct(a.holdReturn))"
        if a.outShare != nil {
            s += " · hors marché après VENDRE \(pct0(a.outShare)) du temps, pire baisse \(ModelValidation.signedPct(a.botMaxDrawdown)) contre \(ModelValidation.signedPct(a.holdMaxDrawdown)) en gardant"
        }
        s += " (\(a.source)\(a.years.map { ", \(ModelValidation.plain($0, digits: 1)) ans" } ?? ""))"
        return s
    }

    /// Row of "Vos actifs aujourd'hui": probabilities and thresholds, or the reason there is none.
    public static func viewText(_ v: BotView) -> String {
        guard v.hasAction else { return v.text }
        return "Hausse \(pct0(v.up)) (seuil \(pct0(v.thresholdUp))), baisse \(pct0(v.down)) (seuil \(pct0(v.thresholdDown)))\(modelSuffix(v) ?? "")\(v.inBasket ? "" : " · hors du panier testé")"
    }

    /// Decision line: "Probabilités à 20 jours : hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)".
    public static func probabilitiesText(_ v: BotView) -> String {
        "Probabilités à 20 jours : hausse \(pct0(v.up)) (seuil \(pct0(v.thresholdUp))), baisse \(pct0(v.down)) (seuil \(pct0(v.thresholdDown)))"
    }

    /// " · Régression logistique 24 mesures": the v2 model behind the view (nil in a v1 answer).
    public static func modelSuffix(_ v: BotView) -> String? {
        guard let l = v.modelLabel, !l.isEmpty else { return nil }
        return " · \(l)"
    }

    /// " · modèle des cryptos, non testé sur cet actif" when the asset is outside the basket.
    public static func outOfBasketText(_ v: BotView) -> String? {
        v.inBasket ? nil : " · modèle des \(v.group == .crypto ? "cryptos" : "actions"), non testé sur cet actif"
    }

    /// "RSI 14 : 40 (pèse contre la hausse) · …", nil without contributions.
    public static func contributionsText(_ v: BotView) -> String? {
        v.contributions.isEmpty ? nil : v.contributions.map(\.text).joined(separator: " · ")
    }

    /// " · entraîné le 29/09 à 13:36" after the link; nil without a report.
    public static func trainedText(_ v: BotView) -> String? { v.asOf.map { " · entraîné le \(DecisionGuidance.shortDateTime($0))" } }

    /// "Panier fixé le 29/09/2026, univers élargi le 29/09/2026. Source : …. Calcul : 13 s de téléchargement, 35 s d'entraînement et de test."
    /// (the timing is in the v3 section when v3 is present).
    public static func footer(_ r: BotReport) -> String {
        let extra = (r.extraFixedOn ?? "").isEmpty ? "" : ", univers élargi le \(ModelValidation.basketDate(r.extraFixedOn ?? ""))"
        let timing = r.v3 != nil ? "" : r.timing.map {
            " Calcul : \(Int(($0.fetchMs / 1000).rounded())) s de téléchargement, \(Int(($0.computeMs / 1000).rounded())) s d'entraînement et de test."
        } ?? ""
        return "Panier fixé le \(ModelValidation.basketDate(r.basketFixedOn))\(extra). Source : \(r.source).\(timing)"
    }
}
