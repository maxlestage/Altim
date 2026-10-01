import Foundation

// « Bots sélectifs » (v4, additive, optional): `v4` of the report (/api/bot: 16 bots that speak rarely, each with its
// measured precision, Wilson interval, signals per year and status), `v4` of an asset row (the bots speaking today on
// it) and of a view (`bot` of /api/decision, items of /api/bot/views: the group's 8 bots today). Port of the Yew
// front's `app/bot/v4.rs` and of `altim_core::engine::bot_v4`'s texts. Every field is optional-safe: a v1 – v3
// answer reads with `v4 == nil`. Rates and precisions in %, excesses in points of %, times in ms. The server computes
// everything; nothing here recomputes a statistic.

/// What a bot predicts: rise / fall of the asset, top / bottom of its group's ranking.
public enum BotV4Side: String, Codable, Sendable, Hashable, CaseIterable {
    case rise, fall, top, bottom

    public var label: String {
        switch self {
        case .rise: return "Hausse"
        case .fall: return "Baisse"
        case .top: return "Haut du classement"
        case .bottom: return "Bas du classement"
        }
    }

    /// The avis when it speaks.
    public var avis: String {
        switch self {
        case .rise: return "ACHETER (hausse attendue)"
        case .fall: return "VENDRE (baisse attendue)"
        case .top: return "ACHETER (dans le haut de son groupe)"
        case .bottom: return "ALLÉGER (dans le bas de son groupe)"
        }
    }
}

/// « prouvé » (précis and confirmed on the forward test: only then it counts), « en attente », « contredit », « non prouvé ».
public enum BotV4Status: String, Codable, Sendable, Hashable, CaseIterable {
    case proven, pending, contradicted, notProven

    public var label: String {
        switch self {
        case .proven: return "prouvé"
        case .pending: return "en attente"
        case .contradicted: return "contredit"
        case .notProven: return "non prouvé"
        }
    }
}

// MARK: - Lenient decoding (same rules as Bot.swift)

private extension KeyedDecodingContainer {
    func opt<T: Decodable>(_ t: T.Type, _ k: Key) -> T? { (try? decodeIfPresent(t, forKey: k)) ?? nil }
    func num(_ k: Key) -> Double? { opt(Double.self, k).flatMap { $0.isFinite ? $0 : nil } }
    func int(_ k: Key) -> Int { opt(Int.self, k) ?? opt(Double.self, k).flatMap { $0.isFinite ? Int($0) : nil } ?? 0 }
    func str(_ k: Key) -> String { opt(String.self, k) ?? "" }
    func bool(_ k: Key) -> Bool { opt(Bool.self, k) ?? false }
    func list<T: Decodable>(_ t: T.Type, _ k: Key) -> [T] { opt([Lenient<T>].self, k)?.compactMap(\.value) ?? [] }
    func action(_ k: Key) -> BotAction? { opt(String.self, k).flatMap(BotAction.init(rawValue:)) }
    func side(_ k: Key) -> BotV4Side { opt(String.self, k).flatMap(BotV4Side.init(rawValue:)) ?? .rise }
    func status(_ k: Key) -> BotV4Status { opt(String.self, k).flatMap(BotV4Status.init(rawValue:)) ?? .notProven }
}

private struct V4Key: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

private func box(_ decoder: Decoder) throws -> KeyedDecodingContainer<V4Key> { try decoder.container(keyedBy: V4Key.self) }
private func k(_ s: String) -> V4Key { V4Key(s) }

// MARK: - Report (`v4` of /api/bot)

/// A bot's figures on one period: precision with its Wilson 95 % interval, base rate, random entry and the higher of
/// the two (`reference`), t by date, mean excess vs a random entry, signals per year, coverage, mean shown probability.
public struct BotV4Stats: Decodable, Sendable, Equatable {
    public var signals: Int
    public var hits: Int
    public var precision: Double?
    public var wilsonLow: Double?
    public var wilsonHigh: Double?
    public var baseRate: Double?
    public var randomRate: Double?
    public var reference: Double?
    public var t: Double?
    public var excess: Double?
    public var excessMedian: Double?
    public var perYear: Double?
    public var coverage: Double?
    public var meanProb: Double?
    public var from: Double?
    public var to: Double?
    /// "precise" / "notProven" / "insufficient" / "contrary".
    public var verdict: String

    public init(signals: Int = 0, hits: Int = 0, precision: Double? = nil, wilsonLow: Double? = nil, wilsonHigh: Double? = nil,
                reference: Double? = nil, perYear: Double? = nil) {
        self.signals = signals
        self.hits = hits
        self.precision = precision
        self.wilsonLow = wilsonLow
        self.wilsonHigh = wilsonHigh
        self.reference = reference
        self.perYear = perYear
        verdict = "insufficient"
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        signals = c.int(k("signals"))
        hits = c.int(k("hits"))
        precision = c.num(k("precision"))
        wilsonLow = c.num(k("wilsonLow"))
        wilsonHigh = c.num(k("wilsonHigh"))
        baseRate = c.num(k("baseRate"))
        randomRate = c.num(k("randomRate"))
        reference = c.num(k("reference"))
        t = c.num(k("t"))
        excess = c.num(k("excess"))
        excessMedian = c.num(k("excessMedian"))
        perYear = c.num(k("perYear"))
        coverage = c.num(k("coverage"))
        meanProb = c.num(k("meanProb"))
        from = c.num(k("from"))
        to = c.num(k("to"))
        verdict = c.str(k("verdict"))
    }
}

public struct BotV4Bot: Decodable, Sendable, Identifiable {
    public var id: String
    public var group: BotGroup
    public var horizon: Int
    public var side: BotV4Side
    public var label: String
    public var hit: String
    public var models: String
    public var blocks: Int
    public var openBlocks: Int
    public var main: BotV4Stats
    public var extra: BotV4Stats
    public var forward: BotV4Stats
    public var status: BotV4Status
    /// Today: the gate's level (%; nil: closed), the basket assets it speaks on.
    public var todayLevel: Double?
    public var today: [String]
    public var text: String

    public init(id: String, group: BotGroup = .stock, horizon: Int = 20, side: BotV4Side = .rise, main: BotV4Stats = BotV4Stats(),
                status: BotV4Status = .notProven, todayLevel: Double? = nil, today: [String] = []) {
        self.id = id
        self.group = group
        self.horizon = horizon
        self.side = side
        label = "\(side.label) à \(horizon) jours"
        hit = ""
        models = ""
        blocks = 0
        openBlocks = 0
        self.main = main
        extra = BotV4Stats()
        forward = BotV4Stats()
        self.status = status
        self.todayLevel = todayLevel
        self.today = today
        text = ""
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.str(k("id"))
        group = BotGroup(rawValue: c.str(k("group"))) ?? .stock
        horizon = c.int(k("horizon"))
        side = c.side(k("side"))
        label = c.str(k("label"))
        hit = c.str(k("hit"))
        models = c.str(k("models"))
        blocks = c.int(k("blocks"))
        openBlocks = c.int(k("openBlocks"))
        main = c.opt(BotV4Stats.self, k("main")) ?? BotV4Stats()
        extra = c.opt(BotV4Stats.self, k("extra")) ?? BotV4Stats()
        forward = c.opt(BotV4Stats.self, k("forward")) ?? BotV4Stats()
        status = c.status(k("status"))
        todayLevel = c.num(k("todayLevel"))
        today = c.list(String.self, k("today"))
        text = c.str(k("text"))
    }
}

public struct BotV4K: Decodable, Sendable, Equatable {
    public var v1: Int
    public var v2: Int
    public var v3: Int
    public var v4: Int
    public var total: Int
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        v1 = c.int(k("v1"))
        v2 = c.int(k("v2"))
        v3 = c.int(k("v3"))
        v4 = c.int(k("v4"))
        total = c.int(k("total"))
    }
}

public struct BotV4Report: Decodable, Sendable {
    public var preregDate: String
    public var afterPrereg: [String]
    public var k: BotV4K?
    public var tRequired: Double
    public var headline: String
    public var forwardHeadline: String
    public var bots: [BotV4Bot]
    public var method: [String]
    public var limits: [String]
    public var minSignals: Int

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        preregDate = c.str(V4Key("preregDate"))
        afterPrereg = c.list(String.self, V4Key("afterPrereg"))
        k = c.opt(BotV4K.self, V4Key("k"))
        tRequired = c.num(V4Key("tRequired")) ?? 0
        headline = c.str(V4Key("headline"))
        forwardHeadline = c.str(V4Key("forwardHeadline"))
        bots = c.list(BotV4Bot.self, V4Key("bots"))
        method = c.list(String.self, V4Key("method"))
        limits = c.list(String.self, V4Key("limits"))
        let p = try? c.nestedContainer(keyedBy: V4Key.self, forKey: V4Key("parameters"))
        minSignals = p?.int(V4Key("minSignals")) ?? 30
    }
}

/// The bots speaking today on a basket asset (ids).
public struct BotV4AssetNow: Decodable, Sendable, Equatable {
    public var time: Double?
    public var bots: [String]
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        time = c.num(k("time"))
        bots = c.list(String.self, k("bots"))
    }
}

// MARK: - Decision view (`v4` of a bot view)

public struct BotV4Signal: Codable, Sendable, Equatable {
    public var id: String
    public var label: String
    public var horizon: Int
    public var side: BotV4Side
    /// nil: « pas d'avis ».
    public var action: BotAction?
    public var probability: Double?
    public var precision: Double?
    public var wilsonLow: Double?
    public var wilsonHigh: Double?
    public var reference: Double?
    public var perYear: Double?
    public var status: BotV4Status
    public var counts: Bool
    public var text: String

    public init(id: String, side: BotV4Side, action: BotAction?, status: BotV4Status = .notProven, precision: Double? = nil,
                wilsonLow: Double? = nil, wilsonHigh: Double? = nil, reference: Double? = nil, perYear: Double? = nil) {
        self.id = id
        label = side.label
        horizon = 20
        self.side = side
        self.action = action
        self.status = status
        self.precision = precision
        self.wilsonLow = wilsonLow
        self.wilsonHigh = wilsonHigh
        self.reference = reference
        self.perYear = perYear
        counts = false
        text = ""
    }

    enum CodingKeys: String, CodingKey {
        case id, label, horizon, side, action, probability, precision, wilsonLow, wilsonHigh, reference, perYear, status, counts, text
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = c.str(.id)
        label = c.str(.label)
        horizon = c.int(.horizon)
        side = c.side(.side)
        action = c.action(.action)
        probability = c.num(.probability)
        precision = c.num(.precision)
        wilsonLow = c.num(.wilsonLow)
        wilsonHigh = c.num(.wilsonHigh)
        reference = c.num(.reference)
        perYear = c.num(.perYear)
        status = c.status(.status)
        counts = c.bool(.counts)
        text = c.str(.text)
    }
}

public struct BotV4View: Codable, Sendable, Equatable {
    public var available: Bool
    public var signals: [BotV4Signal]
    public var counts: Bool
    public var nudge: Double
    public var note: String

    public init(available: Bool = true, signals: [BotV4Signal] = [], counts: Bool = false, note: String = "") {
        self.available = available
        self.signals = signals
        self.counts = counts
        nudge = 0
        self.note = note
    }

    enum CodingKeys: String, CodingKey { case available, signals, counts, nudge, note }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = c.bool(.available)
        signals = c.list(BotV4Signal.self, .signals)
        counts = c.bool(.counts)
        nudge = c.num(.nudge) ?? 0
        note = c.str(.note)
    }

    /// The bots speaking today.
    public var speaking: [BotV4Signal] { signals.filter { $0.action != nil } }
}

// MARK: - Texts (same as the Yew front)

public enum ModelBotV4 {
    public static let sectionTitle = "Bots sélectifs"
    public static let intro = "Chaque bot parle rarement : seulement quand deux modèles sont d'accord sur un score extrême, sinon « pas d'avis ». Précision = part des signaux qui ont réussi, avec son intervalle à 95 % ; « précis » seulement si le bas de l'intervalle dépasse le hasard et que le t dépasse le seuil corrigé."
    public static let forwardTitle = "Depuis le 02/10/2026 (test sur l'avenir)"
    public static let methodTitle = "Comment les bots sélectifs sont jugés"

    public static func title(_ r: BotV4Report) -> String { "Bots sélectifs · pré-enregistrés le \(ModelBot.frIso(r.preregDate))" }

    /// "62 %" (nil: "—").
    public static func pct(_ v: Double?) -> String {
        guard let v else { return "—" }
        return "\(ModelValidation.plain(v, digits: 0))\u{a0}%"
    }

    /// "62 % (57–67 %)" or "aucun signal".
    public static func precisionShort(_ s: BotV4Stats) -> String {
        s.signals == 0 ? "aucun signal" : "\(pct(s.precision)) (\(pct(s.wilsonLow))–\(pct(s.wilsonHigh)))"
    }

    /// The avis of today on the basket.
    public static func todayText(_ b: BotV4Bot) -> String {
        if b.todayLevel == nil { return "pas d'avis (seuil fermé sur la dernière année)" }
        return b.today.isEmpty ? "pas d'avis" : "\(b.side.avis) : \(b.today.joined(separator: ", "))"
    }

    public static func rhythm(_ s: BotV4Stats) -> String {
        guard let y = s.perYear else { return "\(s.signals) · rythme —" }
        return "\(s.signals) · \(ModelValidation.plain(y, digits: 1)) signaux par an"
    }

    public static func tText(_ s: BotV4Stats, _ required: Double) -> String {
        "\(s.t.map { ModelValidation.plain($0, digits: 1) } ?? "—") (requis \(ModelValidation.plain(required, digits: 2)))"
    }

    /// Bots precise on the past (pending, proven or contradicted), out of all.
    public static func preciseCount(_ r: BotV4Report) -> String {
        "\(r.bots.filter { $0.status != .notProven }.count) / \(r.bots.count)"
    }

    /// "Actions et ETF américains · 20 jours": cards of four bots, in the report's order.
    public static func cards(_ r: BotV4Report) -> [(title: String, bots: [BotV4Bot])] {
        var out: [(title: String, bots: [BotV4Bot])] = []
        for b in r.bots {
            let t = "\(b.group == .crypto ? "Cryptos (bitcoin, ether, altcoins)" : "Actions et ETF américains") · \(b.horizon) jours"
            if let last = out.indices.last, out[last].title == t { out[last].bots.append(b) } else { out.append((t, [b])) }
        }
        return out
    }

    /// The decision's line head: "pas d'avis aujourd'hui (8 bots)" / "2 avis sur 8 bots".
    public static func viewHead(_ v: BotV4View) -> String {
        let n = v.speaking.count
        return n == 0 ? "pas d'avis aujourd'hui (\(v.signals.count) bots)" : "\(n) avis sur \(v.signals.count) bots"
    }

    public static func signalText(_ s: BotV4Signal) -> String {
        "\(s.side.avis) · précision mesurée \(pct(s.precision)) (\(pct(s.wilsonLow))–\(pct(s.wilsonHigh))) contre \(pct(s.reference)) au hasard · \(s.perYear.map { ModelValidation.plain($0, digits: 1) } ?? "—") signaux par an"
    }

    /// « Hausse 20 j » from a bot's id.
    public static func shortLabel(_ id: String) -> String {
        let parts = id.split(separator: "-", maxSplits: 2).map(String.init)
        guard parts.count == 3, let side = BotV4Side(rawValue: parts[2]) else { return id }
        return "\(side.label) \(parts[1]) j"
    }
}
