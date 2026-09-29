import Foundation

// « Validation du modèle » (GET /api/validation, 202 {pending:true} while the first computation runs): the signal's
// backtest replayed on a basket fixed in advance, pooled by asset class, by market regime and overall. JSON contract
// (every field optional-safe: a missing or odd value never breaks the screen) and the pure helpers of the screen
// (sorting, French texts), exact port of web/src/webapp/model-validation.ts. Percentages are in %, times in ms. The
// server computes everything; nothing here recomputes a statistic.

public enum ValidationClass: String, Sendable, Hashable, CaseIterable {
    case stock, btc, eth, altcoin

    /// Chip and row label.
    public var short: String {
        switch self {
        case .stock: return "Actions"
        case .btc: return "Bitcoin"
        case .eth: return "Ethereum"
        case .altcoin: return "Altcoins"
        }
    }
}

public enum ValidationRegime: String, Sendable, Hashable {
    case bull, bear, range, crisis, unknown
}

public enum ValidationVerdict: String, Sendable, Hashable {
    case insufficient, edge, negative, unproven
}

// MARK: - Contract

private extension KeyedDecodingContainer {
    func opt<T: Decodable>(_ t: T.Type, _ k: Key) -> T? { (try? decodeIfPresent(t, forKey: k)) ?? nil }
    /// A finite number, or nil (null, missing, not a number).
    func num(_ k: Key) -> Double? { opt(Double.self, k).flatMap { $0.isFinite ? $0 : nil } }
    func int(_ k: Key) -> Int { opt(Int.self, k) ?? opt(Double.self, k).flatMap { $0.isFinite ? Int($0) : nil } ?? 0 }
    func str(_ k: Key) -> String { opt(String.self, k) ?? "" }
    func bool(_ k: Key) -> Bool { opt(Bool.self, k) ?? false }
    func list<T: Decodable>(_ t: T.Type, _ k: Key) -> [T] { opt([Lenient<T>].self, k)?.compactMap(\.value) ?? [] }
}

private struct AnyKey: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

private typealias VBox = KeyedDecodingContainer<AnyKey>
private func box(_ decoder: Decoder) throws -> VBox { try decoder.container(keyedBy: AnyKey.self) }
private func k(_ s: String) -> AnyKey { AnyKey(s) }

/// Trade statistics pooled over every trade of a group (each trade weighs the same).
public struct ValidationPooled: Decodable, Sendable, Equatable {
    public var trades: Int
    public var winRate: Double?
    public var profitFactor: Double?
    /// Mean net return per trade (%), costs included.
    public var expectancy: Double?
    public var avgR: Double?
    /// Mean ÷ standard error of the net trade returns.
    public var tStat: Double?

    public init(trades: Int, winRate: Double?, profitFactor: Double?, expectancy: Double?, avgR: Double?, tStat: Double?) {
        self.trades = trades
        self.winRate = winRate
        self.profitFactor = profitFactor
        self.expectancy = expectancy
        self.avgR = avgR
        self.tStat = tStat
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(trades: c.int(k("trades")), winRate: c.num(k("winRate")), profitFactor: c.num(k("profitFactor")), expectancy: c.num(k("expectancy")),
                  avgR: c.num(k("avgR")), tStat: c.num(k("tStat")))
    }
}

public struct ValidationRegimeGroup: Decodable, Sendable, Equatable {
    public var regime: ValidationRegime
    public var label: String
    /// Flattened on the server (same object).
    public var pooled: ValidationPooled
    /// Asset-days in this regime (regime read the day before).
    public var days: Int
    /// Assets with ≥ 20 days in it; those where the signal made more than buy-and-hold over these days.
    public var assets: Int
    public var beatHold: Int
    public var beatShare: Double?
    /// Medians over those assets, compounded over the regime's days (%).
    public var medianSignal: Double?
    public var medianHold: Double?
    public var lowSample: Bool
    public var verdict: ValidationVerdict
    public var verdictLabel: String

    public init(regime: ValidationRegime, label: String, pooled: ValidationPooled, days: Int, assets: Int, beatHold: Int, beatShare: Double?,
                medianSignal: Double?, medianHold: Double?, lowSample: Bool, verdict: ValidationVerdict, verdictLabel: String) {
        self.regime = regime
        self.label = label
        self.pooled = pooled
        self.days = days
        self.assets = assets
        self.beatHold = beatHold
        self.beatShare = beatShare
        self.medianSignal = medianSignal
        self.medianHold = medianHold
        self.lowSample = lowSample
        self.verdict = verdict
        self.verdictLabel = verdictLabel
    }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        self.init(regime: ValidationRegime(rawValue: c.str(k("regime"))) ?? .unknown, label: c.str(k("label")), pooled: try ValidationPooled(from: decoder),
                  days: c.int(k("days")), assets: c.int(k("assets")), beatHold: c.int(k("beatHold")), beatShare: c.num(k("beatShare")),
                  medianSignal: c.num(k("medianSignal")), medianHold: c.num(k("medianHold")), lowSample: c.bool(k("lowSample")),
                  verdict: ValidationVerdict(rawValue: c.str(k("verdict"))) ?? .unproven, verdictLabel: c.str(k("verdictLabel")))
    }
}

/// A symbol and its value ("pire actif").
public struct ValidationNamed: Decodable, Sendable, Equatable {
    public var symbol: String
    public var value: Double
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        guard let v = c.num(k("value")) else { throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "value")) }
        symbol = c.str(k("symbol"))
        value = v
    }
}

public struct ValidationGroup: Decodable, Sendable, Identifiable {
    /// "all" or the asset class.
    public var id: String
    public var label: String
    public var pooled: ValidationPooled
    public var assets: Int
    public var years: Double?
    public var medianReturn: Double?
    public var worstReturn: ValidationNamed?
    public var medianHold: Double?
    /// ≤ 0.
    public var medianDrawdown: Double?
    public var worstDrawdown: ValidationNamed?
    public var medianHoldDrawdown: Double?
    public var medianSharpe: Double?
    public var medianSortino: Double?
    public var medianExposure: Double?
    public var beatHold: Int
    public var beatShare: Double?
    public var medianAfterTax: Double?
    public var medianHoldAfterTax: Double?
    public var lowSample: Bool
    public var verdict: ValidationVerdict
    public var verdictLabel: String
    public var regimes: [ValidationRegimeGroup]

    /// The class of this group, nil for "all".
    public var assetClass: ValidationClass? { ValidationClass(rawValue: id) }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        id = c.opt(String.self, k("id")) ?? "all"
        label = c.str(k("label"))
        pooled = try ValidationPooled(from: decoder)
        assets = c.int(k("assets"))
        years = c.num(k("years"))
        medianReturn = c.num(k("medianReturn"))
        worstReturn = c.opt(ValidationNamed.self, k("worstReturn"))
        medianHold = c.num(k("medianHold"))
        medianDrawdown = c.num(k("medianDrawdown"))
        worstDrawdown = c.opt(ValidationNamed.self, k("worstDrawdown"))
        medianHoldDrawdown = c.num(k("medianHoldDrawdown"))
        medianSharpe = c.num(k("medianSharpe"))
        medianSortino = c.num(k("medianSortino"))
        medianExposure = c.num(k("medianExposure"))
        beatHold = c.int(k("beatHold"))
        beatShare = c.num(k("beatShare"))
        medianAfterTax = c.num(k("medianAfterTax"))
        medianHoldAfterTax = c.num(k("medianHoldAfterTax"))
        lowSample = c.bool(k("lowSample"))
        verdict = ValidationVerdict(rawValue: c.str(k("verdict"))) ?? .unproven
        verdictLabel = c.str(k("verdictLabel"))
        regimes = c.list(ValidationRegimeGroup.self, k("regimes"))
    }

    /// Empty group (a report without "overall").
    public static let empty: ValidationGroup = {
        // swiftlint:disable:next force_try
        try! JSONDecoder().decode(ValidationGroup.self, from: Data("{}".utf8))
    }()
}

public struct ValidationRegimeDays: Decodable, Sendable, Equatable {
    public var regime: ValidationRegime
    public var days: Int
    public var signal: Double
    public var hold: Double
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        regime = ValidationRegime(rawValue: c.str(k("regime"))) ?? .unknown
        days = c.int(k("days"))
        signal = c.num(k("signal")) ?? 0
        hold = c.num(k("hold")) ?? 0
    }
}

/// Kind of an asset of the basket: the one sent, else deduced from its class.
private func kindOf(_ c: VBox, _ cls: ValidationClass) -> Kind {
    Kind(rawValue: c.str(k("kind"))) ?? (cls == .stock ? .stock : .crypto)
}

public struct ValidationAsset: Decodable, Sendable, Identifiable {
    public var symbol: String
    public var name: String
    public var kind: Kind
    public var assetClass: ValidationClass
    public var group: String
    public var from: Double
    public var to: Double
    public var bars: Int
    public var trades: Int
    public var winRate: Double?
    public var profitFactor: Double?
    public var expectancy: Double?
    public var avgR: Double?
    public var maxDrawdown: Double
    public var holdMaxDrawdown: Double
    public var sharpe: Double?
    public var sortino: Double?
    public var totalReturn: Double
    public var buyAndHold: Double
    public var exposure: Double
    public var beatHold: Bool
    public var afterTax: Double
    public var holdAfterTax: Double
    public var lowSample: Bool
    public var regimeDays: [ValidationRegimeDays]
    public var source: String
    public var id: String { symbol }

    /// Signal − buy-and-hold (% points).
    public var gap: Double { totalReturn - buyAndHold }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name.isEmpty ? symbol : name) }

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        symbol = c.str(k("symbol"))
        name = c.str(k("name"))
        assetClass = ValidationClass(rawValue: c.str(k("class"))) ?? .stock
        kind = kindOf(c, assetClass)
        group = c.str(k("group"))
        from = c.num(k("from")) ?? 0
        to = c.num(k("to")) ?? 0
        bars = c.int(k("bars"))
        trades = c.int(k("trades"))
        winRate = c.num(k("winRate"))
        profitFactor = c.num(k("profitFactor"))
        expectancy = c.num(k("expectancy"))
        avgR = c.num(k("avgR"))
        maxDrawdown = c.num(k("maxDrawdown")) ?? 0
        holdMaxDrawdown = c.num(k("holdMaxDrawdown")) ?? 0
        sharpe = c.num(k("sharpe"))
        sortino = c.num(k("sortino"))
        totalReturn = c.num(k("totalReturn")) ?? 0
        buyAndHold = c.num(k("buyAndHold")) ?? 0
        exposure = c.num(k("exposure")) ?? 0
        beatHold = c.bool(k("beatHold"))
        afterTax = c.num(k("afterTax")) ?? 0
        holdAfterTax = c.num(k("holdAfterTax")) ?? 0
        lowSample = c.bool(k("lowSample"))
        regimeDays = c.list(ValidationRegimeDays.self, k("regimeDays"))
        source = c.str(k("source"))
    }
}

public struct ValidationFailure: Decodable, Sendable {
    public var symbol: String
    public var name: String
    public var kind: Kind
    public var assetClass: ValidationClass
    public var error: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        symbol = c.str(k("symbol"))
        name = c.str(k("name"))
        assetClass = ValidationClass(rawValue: c.str(k("class"))) ?? .stock
        kind = kindOf(c, assetClass)
        error = c.str(k("error"))
    }
}

public struct ValidationBasketEntry: Decodable, Sendable {
    public var symbol: String
    public var name: String
    public var kind: Kind
    public var assetClass: ValidationClass
    public var group: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        symbol = c.str(k("symbol"))
        name = c.str(k("name"))
        assetClass = ValidationClass(rawValue: c.str(k("class"))) ?? .stock
        kind = kindOf(c, assetClass)
        group = c.str(k("group"))
    }
}

public struct ValidationParameters: Decodable, Sendable {
    public var feesPct: Double?
    public var slippagePct: Double?
    public var spreadStockPct: Double?
    public var spreadCryptoPct: Double?
    public var lookback: Int
    public var warmup: Int
    public var rewardRisk: Double?
    /// Illustrative flat tax (30 % when missing).
    public var taxRatePct: Double
    public var minTrades: Int
    public var tEdge: Double?
    public var regimeRule: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        feesPct = c.num(k("feesPct"))
        slippagePct = c.num(k("slippagePct"))
        spreadStockPct = c.num(k("spreadStockPct"))
        spreadCryptoPct = c.num(k("spreadCryptoPct"))
        lookback = c.int(k("lookback"))
        warmup = c.int(k("warmup"))
        rewardRisk = c.num(k("rewardRisk"))
        taxRatePct = c.num(k("taxRatePct")) ?? 30
        minTrades = c.int(k("minTrades"))
        tEdge = c.num(k("tEdge"))
        regimeRule = c.str(k("regimeRule"))
    }

    static let empty: ValidationParameters = {
        // swiftlint:disable:next force_try
        try! JSONDecoder().decode(ValidationParameters.self, from: Data("{}".utf8))
    }()
}

public struct ValidationOutOfSample: Decodable, Sendable {
    /// "verified" or "notVerifiable".
    public var status: String
    public var note: String
    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        status = c.opt(String.self, k("status")) ?? "notVerifiable"
        note = c.str(k("note"))
    }
    init(status: String, note: String) {
        self.status = status
        self.note = note
    }
}

public struct ValidationReport: Decodable, Sendable {
    public var asOf: Double?
    public var basketFixedOn: String
    public var headline: String
    public var from: Double?
    public var to: Double?
    public var years: Double?
    public var minYears: Double?
    public var maxYears: Double?
    public var overall: ValidationGroup
    public var classes: [ValidationGroup]
    /// Basket order (by class), never ranked by performance.
    public var assets: [ValidationAsset]
    public var failures: [ValidationFailure]
    public var basket: [ValidationBasketEntry]
    public var parameters: ValidationParameters
    public var protections: [String]
    public var outOfSample: ValidationOutOfSample
    public var limits: [String]
    public var source: String

    public init(from decoder: Decoder) throws {
        let c = try box(decoder)
        asOf = c.num(k("asOf"))
        basketFixedOn = c.str(k("basketFixedOn"))
        headline = c.str(k("headline"))
        from = c.num(k("from"))
        to = c.num(k("to"))
        years = c.num(k("years"))
        minYears = c.num(k("minYears"))
        maxYears = c.num(k("maxYears"))
        overall = c.opt(ValidationGroup.self, k("overall")) ?? .empty
        classes = c.list(ValidationGroup.self, k("classes"))
        assets = c.list(ValidationAsset.self, k("assets"))
        failures = c.list(ValidationFailure.self, k("failures"))
        basket = c.list(ValidationBasketEntry.self, k("basket"))
        parameters = c.opt(ValidationParameters.self, k("parameters")) ?? .empty
        protections = c.list(String.self, k("protections"))
        outOfSample = c.opt(ValidationOutOfSample.self, k("outOfSample")) ?? ValidationOutOfSample(status: "notVerifiable", note: "")
        limits = c.list(String.self, k("limits"))
        source = c.str(k("source"))
    }

    /// "Tous" then the classes: the chips of the regime section.
    public var groups: [ValidationGroup] { [overall] + classes }
}

public enum ValidationResult: Sendable {
    case ready(ValidationReport)
    /// The first computation over the whole basket is still running (≈ 30 s): ask again.
    case pending
}

extension AltimClient {
    /// Signal validated on a fixed basket (heavy: 202 `.pending` until the first computation is ready, ask again).
    public func validation() async throws -> ValidationResult {
        let (data, status) = try await getWithStatus(ModelValidation.path, [:])
        if status == 202 { return .pending }
        return .ready(try decodeResponse(ValidationReport.self, data, status))
    }
}

// MARK: - Helpers of the screen

public enum ModelValidation {
    public static let path = "/api/validation"

    public static let classOrder: [ValidationClass] = [.stock, .btc, .eth, .altcoin]

    public enum SortKey: String, Sendable, CaseIterable, Hashable {
        case byClass = "class", name, gap

        public var label: String {
            switch self {
            case .byClass: return "Par classe"
            case .name: return "Par nom"
            case .gap: return "Écart avec la détention"
            }
        }
    }

    /// Rows in the chosen order: by class (the basket's own order, the default), by name, or by the gap signal −
    /// buy-and-hold (largest first). Stable: equal rows keep the basket's order.
    public static func sortAssets(_ assets: [ValidationAsset], _ key: SortKey) -> [ValidationAsset] {
        let rows = Array(assets.enumerated())
        let sorted: [(offset: Int, element: ValidationAsset)]
        switch key {
        case .name:
            sorted = rows.sorted { a, b in
                let o = a.element.symbol.compare(b.element.symbol, options: [.caseInsensitive, .diacriticInsensitive])
                return o == .orderedSame ? a.offset < b.offset : o == .orderedAscending
            }
        case .gap:
            sorted = rows.sorted { a, b in a.element.gap == b.element.gap ? a.offset < b.offset : a.element.gap > b.element.gap }
        case .byClass:
            let rank = { (c: ValidationClass) in classOrder.firstIndex(of: c) ?? classOrder.count }
            sorted = rows.sorted { a, b in
                let ra = rank(a.element.assetClass), rb = rank(b.element.assetClass)
                return ra == rb ? a.offset < b.offset : ra < rb
            }
        }
        return sorted.map(\.element)
    }

    static let nnbsp = "\u{202F}"

    /// "+12,3 %", "−4 %", "—".
    public static func signedPct(_ v: Double?, digits: Int = 1) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v < 0 ? "−" : v > 0 ? "+" : "")\(JSFormat.fr(abs(v), max: digits))\(nnbsp)%"
    }

    /// "1,16", "−0,4", "—".
    public static func plain(_ v: Double?, digits: Int = 2) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v < 0 ? "−" : "")\(JSFormat.fr(abs(v), max: digits))"
    }

    /// "4,8 ans", "1 an", "—".
    public static func years(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(JSFormat.fr(v, max: 1))\(nnbsp)an\(v >= 2 ? "s" : "")"
    }

    /// Tone of a verdict for the chips: good only for a measured positive gain, warning for a measured loss.
    public static func verdictTone(_ v: ValidationVerdict) -> Tone {
        switch v {
        case .edge: return .good
        case .negative: return .warn
        case .insufficient, .unproven: return .neutral
        }
    }

    /// "6 sur 34 (18 %)".
    public static func beatText(_ beat: Int, _ n: Int, _ share: Double?) -> String {
        if n == 0 { return "aucun actif comparable" }
        return "\(beat) sur \(n)\(share.map { " (\(JSFormat.fr($0, max: 0))\(nnbsp)%)" } ?? "")"
    }

    /// What the regime days say, in one sentence.
    public static func regimeDaysText(_ g: ValidationRegimeGroup) -> String {
        if g.assets == 0 { return "Aucun actif n'a passé 20 jours dans ce régime : pas de comparaison avec la détention." }
        return "Pendant ces jours, le signal a fait mieux que la détention sur \(beatText(g.beatHold, g.assets, g.beatShare)) actifs (médianes : signal \(signedPct(g.medianSignal)), détention \(signedPct(g.medianHold)))."
    }

    /// Trades line of a pooled group: "650 trades · réussite 38 % · espérance +0,2 % · t = 2,2".
    public static func pooledText(_ p: ValidationPooled) -> String {
        if p.trades == 0 { return "Aucun trade." }
        var parts = ["\(p.trades) trade\(p.trades > 1 ? "s" : "")", "réussite \(JSFormat.fr(p.winRate ?? 0, max: 0))\(nnbsp)%", "espérance \(signedPct(p.expectancy, digits: 2))"]
        if let pf = p.profitFactor { parts.append("facteur de profit \(plain(pf))") }
        if let t = p.tStat { parts.append("t = \(plain(t, digits: 1))") }
        return parts.joined(separator: " · ")
    }

    static let months = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."]

    /// "déc. 2021" (UTC), "?" when unknown.
    public static func monthYear(_ ms: Double?) -> String {
        guard let ms, ms.isFinite else { return "?" }
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let d = cal.dateComponents([.year, .month], from: Date(timeIntervalSince1970: ms / 1000))
        return "\(months[((d.month ?? 1) - 1) % 12]) \(d.year ?? 0)"
    }

    /// Regimes of a group, the "unknown" one (history too short to classify) last.
    public static func regimesOf(_ g: ValidationGroup) -> [ValidationRegimeGroup] {
        g.regimes.filter { $0.regime != .unknown } + g.regimes.filter { $0.regime == .unknown }
    }

    /// Headline tile "Historique": "0,5 à 4,8 ans" when the assets' histories differ, else "4,8 ans".
    public static func historyTile(_ r: ValidationReport) -> String {
        if let lo = r.minYears, let hi = r.maxYears, hi - lo >= 0.2 { return "\(plain(lo, digits: 1)) à \(years(hi))" }
        return years(r.years)
    }

    /// "Actifs testés": "34 / 34".
    public static func testedTile(_ r: ValidationReport) -> String { "\(r.overall.assets) / \(r.basket.count)" }

    /// "Période : déc. 2021 – sept. 2026".
    public static func periodText(_ r: ValidationReport) -> String { "\(monthYear(r.from)) – \(monthYear(r.to))" }

    /// "2026-09-29" → "29/09/2026".
    public static func basketDate(_ iso: String) -> String { iso.split(separator: "-").reversed().joined(separator: "/") }

    /// "2 actifs non testés :".
    public static func failuresTitle(_ n: Int) -> String { "\(n) actif\(n > 1 ? "s" : "") non testé\(n > 1 ? "s" : "") :" }

    /// "22 actifs · historique médian 4,8 ans".
    public static func groupSubtitle(_ g: ValidationGroup) -> String { "\(g.assets) actif\(g.assets > 1 ? "s" : "") · historique médian \(years(g.years))" }

    /// Key / value lines of a class card, same labels and order as the web; tone = sign of the value when it matters.
    public static func groupRows(_ g: ValidationGroup, taxRate: Double) -> [(label: String, value: String, tone: Double?)] {
        var rows: [(label: String, value: String, tone: Double?)] = [
            ("Rendement médian du signal", signedPct(g.medianReturn), g.medianReturn),
            ("Détention médiane", signedPct(g.medianHold), g.medianHold),
        ]
        if let w = g.worstReturn { rows.append(("Pire actif (\(w.symbol))", signedPct(w.value), w.value)) }
        rows.append(("Recul max médian (signal / détention)", "\(signedPct(g.medianDrawdown)) / \(signedPct(g.medianHoldDrawdown))", nil))
        if let w = g.worstDrawdown { rows.append(("Pire recul (\(w.symbol))", signedPct(w.value), nil)) }
        rows.append(("Sharpe / Sortino médians", "\(plain(g.medianSharpe)) / \(plain(g.medianSortino))", nil))
        rows.append(("Multiple de R moyen", g.pooled.avgR.map { "\(plain($0)) R" } ?? "—", nil))
        rows.append(("Temps investi médian", g.medianExposure.map { "\(plain($0, digits: 0))\(nnbsp)%" } ?? "—", nil))
        rows.append(("Après impôt \(plain(taxRate, digits: 0))\(nnbsp)% (signal / détention)", "\(signedPct(g.medianAfterTax)) / \(signedPct(g.medianHoldAfterTax))", nil))
        return rows
    }

    /// "3 922 jours-actifs dans ce régime."
    public static func regimeDays(_ g: ValidationRegimeGroup) -> String { "\(JSFormat.fr(Double(g.days), max: 0)) jours-actifs dans ce régime." }

    /// "(moins bien, écart −86,6 %)".
    public static func assetGapText(_ a: ValidationAsset) -> String { "(\(a.beatHold ? "mieux" : "moins bien"), écart \(signedPct(a.gap)))" }

    /// Details line of an asset row.
    public static func assetDetails(_ a: ValidationAsset) -> String {
        let span = (a.to - a.from) / (365.25 * 86_400_000)
        return "\(a.trades) trade\(a.trades > 1 ? "s" : "") · réussite \(a.winRate.map { "\(plain($0, digits: 0))\(nnbsp)%" } ?? "—") · facteur de profit \(plain(a.profitFactor)) · espérance \(signedPct(a.expectancy, digits: 2)) · recul max \(signedPct(a.maxDrawdown)) (détention \(signedPct(a.holdMaxDrawdown))) · Sharpe \(plain(a.sharpe)) · Sortino \(plain(a.sortino)) · \(years(span)) (\(a.source))"
    }

    /// "AAPL · Actions · Technologie" chip.
    public static func assetTag(_ a: ValidationAsset) -> String { "\(a.assetClass.short) · \(a.group)" }

    /// Costs paragraph at the bottom of "Biais et limites".
    public static func costsText(_ r: ValidationReport) -> String {
        let p = r.parameters
        return "Coûts par ordre : frais \(plain(p.feesPct, digits: 3))\(nnbsp)%, glissement \(plain(p.slippagePct, digits: 3))\(nnbsp)%, écart achat/vente supposé \(plain(p.spreadStockPct, digits: 3))\(nnbsp)% (actions) ou \(plain(p.spreadCryptoPct, digits: 3))\(nnbsp)% (cryptos), moitié payée à chaque ordre. Panier fixé le \(basketDate(r.basketFixedOn)). Source : \(r.source)."
    }

    /// Text of the regime section footer.
    public static func regimeFooter(_ r: ValidationReport) -> String {
        "\(r.parameters.regimeRule) Régime lu la veille, sans données futures ; « inconnu » tant que l'historique est trop court pour le classer."
    }

    public static let pendingText = "Calcul sur tout le panier en cours (environ 30 secondes la première fois)…"
}
