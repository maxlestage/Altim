import Foundation

// « Bot Altim » (GET /api/bot, 202 {pending:true} while the first training runs; GET /api/bot/views?symbols=; `bot` of
// /api/decision): two logistic regressions trained on the validation's basket and judged walk-forward on periods they
// had not seen, saying ACHETER / ATTENDRE / VENDRE at 20 days. JSON contract (every field optional-safe: a missing or
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

/// ACHETER signals out of sample (non-overlapping per asset). excess = meanNet − baselineNet (same asset and period).
public struct BotBuyStats: Decodable, Sendable, Equatable {
    public var signals: Int
    public var meanNet: Double?
    public var baselineNet: Double?
    public var allDaysNet: Double?
    public var excess: Double?
    public var tStat: Double?
    public var rawTStat: Double?
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
    public var fallRate: Double?
    public var baselineFallRate: Double?
    public var meanDrawdown: Double?
    public var baselineDrawdown: Double?
    public var verdict: ValidationVerdict?
    public var verdictLabel: String

    public init(signals: Int, meanAfter: Double?, baselineAfter: Double?, allDaysAfter: Double?, avoided: Double?, tStat: Double?,
                fallRate: Double?, baselineFallRate: Double?, meanDrawdown: Double?, baselineDrawdown: Double?,
                verdict: ValidationVerdict?, verdictLabel: String) {
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
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(signals: c.int(k("signals")), meanAfter: c.num(k("meanAfter")), baselineAfter: c.num(k("baselineAfter")),
                  allDaysAfter: c.num(k("allDaysAfter")), avoided: c.num(k("avoided")), tStat: c.num(k("tStat")), fallRate: c.num(k("fallRate")),
                  baselineFallRate: c.num(k("baselineFallRate")), meanDrawdown: c.num(k("meanDrawdown")),
                  baselineDrawdown: c.num(k("baselineDrawdown")), verdict: c.verdict(k("verdict")), verdictLabel: c.str(k("verdictLabel")))
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

/// The model trained on the whole known history (today's view).
public struct BotLiveModel: Decodable, Sendable {
    public var trainedRows: Int
    public var trainedFrom: Double?
    public var trainedTo: Double?
    public var up: BotModelOut
    public var down: BotModelOut
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        trainedRows = c.int(k("trainedRows"))
        trainedFrom = c.num(k("trainedFrom"))
        trainedTo = c.num(k("trainedTo"))
        up = c.opt(BotModelOut.self, k("up")) ?? emptyObject(BotModelOut.self)
        down = c.opt(BotModelOut.self, k("down")) ?? emptyObject(BotModelOut.self)
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

    enum CodingKeys: String, CodingKey {
        case available, group, groupLabel, inBasket, action, actionLabel, up, down, thresholdUp, thresholdDown, baseUp, baseDown
        case buyVerdict, sellVerdict, counts, time, contributions, text, note, asOf, link, symbol, kind
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
    }

    /// With an action: shown with its probabilities; without: only the text.
    public var hasAction: Bool { available && action != nil }

    /// Colour of the decision block's edge, as the web's `dec-proof` classes: not available, counts, does not count.
    public enum Tone: String, Sendable { case na, edge, unproven }
    public var tone: Tone { !hasAction ? .na : counts ? .edge : .unproven }

    /// "compte" / "ne compte pas" (only with an action).
    public var countsLabel: String { counts ? "compte" : "ne compte pas" }
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

    /// "108 achats : +1,7 % en moyenne sur 20 jours, contre +1,85 % pour une entrée au hasard … (−0,15 point, t = −0,1)."
    public static func buyText(_ b: BotBuyStats) -> String {
        if b.signals == 0 { return "Aucun achat pendant les périodes de test." }
        return "\(b.signals) achat\(b.signals > 1 ? "s" : "") : \(ModelValidation.signedPct(b.meanNet, digits: 2)) en moyenne sur 20 jours, contre \(ModelValidation.signedPct(b.baselineNet, digits: 2)) pour une entrée au hasard sur le même actif et la même période (\(points(b.excess)), \(tText(b.tStat)))."
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
        return "\(s.signals) vente\(s.signals > 1 ? "s" : "") : le cours a fait \(ModelValidation.signedPct(s.meanAfter, digits: 2)) dans les 20 jours suivants, contre \(ModelValidation.signedPct(s.baselineAfter, digits: 2)) après un jour au hasard (\(diff), \(tText(s.tStat)))."
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

    public static let intro = "Un modèle appris sur l'historique de 34 actifs fixés d'avance, qui dit ACHETER, ATTENDRE ou VENDRE à 20 jours. Il n'est jugé que sur des périodes qu'il n'avait pas vues. Altim ne passe aucun ordre."
    public static let pendingText = "Entraînement et test sur tout le panier en cours (environ une minute la première fois)…"
    public static let settingsText = "Un modèle appris qui dit ACHETER, ATTENDRE ou VENDRE, jugé seulement sur des périodes qu'il n'avait pas vues, avec ses résultats réels et ses limites."
    public static let calibrationIntro = "Quand le bot annonce une probabilité, la fréquence observée ensuite devrait être proche. Chaque ligne : jours de test dont la probabilité tombait dans la tranche."
    public static let assetsIntro = "Dans l'ordre du panier, jamais classés par performance. « Aujourd'hui » : avis du modèle entraîné sur tout l'historique connu."
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

    /// "22 actifs · test du oct. 2024 au sept. 2026 · 4 réentraînements".
    public static func groupSubtitle(_ g: BotGroupStat) -> String {
        "\(g.assets) actif\(g.assets > 1 ? "s" : "") · test du \(ModelValidation.monthYear(g.testFrom)) au \(ModelValidation.monthYear(g.testTo)) · \(g.trainedBlocks) réentraînement\(g.trainedBlocks > 1 ? "s" : "")"
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
        s += " · attente \(pct0(a.waitShare)) · achats cumulés \(ModelValidation.signedPct(a.botReturn)), détention \(ModelValidation.signedPct(a.holdReturn)) (\(a.source))"
        return s
    }

    /// Row of "Vos actifs aujourd'hui": probabilities and thresholds, or the reason there is none.
    public static func viewText(_ v: BotView) -> String {
        guard v.hasAction else { return v.text }
        return "Hausse \(pct0(v.up)) (seuil \(pct0(v.thresholdUp))), baisse \(pct0(v.down)) (seuil \(pct0(v.thresholdDown)))\(v.inBasket ? "" : " · hors du panier testé")"
    }

    /// Decision line: "Probabilités à 20 jours : hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)".
    public static func probabilitiesText(_ v: BotView) -> String {
        "Probabilités à 20 jours : hausse \(pct0(v.up)) (seuil \(pct0(v.thresholdUp))), baisse \(pct0(v.down)) (seuil \(pct0(v.thresholdDown)))"
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

    /// "Panier fixé le 29/09/2026. Source : ….".
    public static func footer(_ r: BotReport) -> String { "Panier fixé le \(ModelValidation.basketDate(r.basketFixedOn)). Source : \(r.source)." }
}
