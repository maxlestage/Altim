import Foundation

// Sector exposure of the portfolio held: stocks grouped by sector (as returned by GET /api/sectors), cryptos and cash
// as their own blocks, stocks without a known sector as "Secteur inconnu".
// Exact port of web/src/engine/sectors.ts: same blocks, weights, effective sectors, warnings and French texts.
// Pure, deterministic functions. The iPhone has no cash field in its holdings: the card passes 0 and says so.

// MARK: - Contract of GET /api/sectors?symbols=AAPL,NVDA (≤ 50 stocks)

/// "nasdaq": sector of the Nasdaq screener; "sec": SIC division filed at the SEC; "etf": fund spanning several sectors.
public enum SectorClassification: String, Codable, Sendable, CaseIterable {
    case nasdaq, sec, etf
}

/// Sector of one symbol; every field optional-safe (a missing or unexpected value reads as unknown).
public struct SectorItem: Decodable, Sendable, Equatable {
    public struct Sec: Decodable, Sendable, Equatable {
        public var label: String?
        public var sic: String?
        public var sicDescription: String?
        public var source: String?
    }

    public struct Nasdaq: Decodable, Sendable, Equatable {
        public var sector: String?
        public var sectorFr: String?
        public var industry: String?
    }

    public var symbol: String
    /// French label ("Technologie", "Finance et immobilier", "ETF / fonds indiciel (plusieurs secteurs)"), nil when unknown.
    public var sector: String?
    public var classification: SectorClassification?
    /// Named source of the label, nil when unknown.
    public var source: String?
    /// Why no source covers it (nil when classified).
    public var reason: String?
    public var etf: Bool
    public var sec: Sec?
    public var nasdaq: Nasdaq?

    public init(symbol: String, sector: String?, classification: SectorClassification?, source: String? = nil, reason: String? = nil,
                etf: Bool = false, sec: Sec? = nil, nasdaq: Nasdaq? = nil) {
        self.symbol = symbol
        self.sector = sector
        self.classification = classification
        self.source = source
        self.reason = reason
        self.etf = etf
        self.sec = sec
        self.nasdaq = nasdaq
    }

    enum CodingKeys: String, CodingKey { case symbol, sector, classification, source, reason, etf, sec, nasdaq }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        func opt<T: Decodable>(_ t: T.Type, _ k: CodingKeys) -> T? { (try? c.decodeIfPresent(t, forKey: k)) ?? nil }
        symbol = opt(String.self, .symbol) ?? ""
        sector = opt(String.self, .sector).flatMap { $0.isEmpty ? nil : $0 }
        classification = opt(String.self, .classification).flatMap(SectorClassification.init(rawValue:))
        source = opt(String.self, .source)
        reason = opt(String.self, .reason)
        etf = opt(Bool.self, .etf) ?? false
        sec = opt(Sec.self, .sec)
        nasdaq = opt(Nasdaq.self, .nasdaq)
    }
}

public struct SectorsReport: Decodable, Sendable {
    public struct SourceState: Decodable, Sendable, Equatable {
        public var name: String
        public var ok: Bool
        public var error: String?

        enum CodingKeys: String, CodingKey { case name, ok, error }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            name = ((try? c.decodeIfPresent(String.self, forKey: .name)) ?? nil) ?? ""
            ok = ((try? c.decodeIfPresent(Bool.self, forKey: .ok)) ?? nil) ?? false
            error = (try? c.decodeIfPresent(String.self, forKey: .error)) ?? nil
        }
    }

    public var asOf: Double?
    public var items: [SectorItem]
    public var sources: [SourceState]

    enum CodingKeys: String, CodingKey { case asOf, items, sources }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        asOf = (try? c.decodeIfPresent(Double.self, forKey: .asOf)) ?? nil
        // An unreadable item is dropped (its stock then reads "absent de la réponse du serveur"), not the whole answer.
        items = (((try? c.decodeIfPresent([Lossy<SectorItem>].self, forKey: .items)) ?? nil) ?? []).compactMap(\.value).filter { !$0.symbol.isEmpty }
        sources = (((try? c.decodeIfPresent([Lossy<SourceState>].self, forKey: .sources)) ?? nil) ?? []).compactMap(\.value)
    }

    /// Items keyed by symbol (the first one wins).
    public var bySymbol: [String: SectorItem] { Dictionary(items.map { ($0.symbol, $0) }, uniquingKeysWith: { a, _ in a }) }
}

private struct Lossy<T: Decodable>: Decodable {
    let value: T?
    init(from decoder: Decoder) throws { value = try? T(from: decoder) }
}

// MARK: - Exposure

public enum SectorBlockKind: String, Sendable {
    case sector, etf, crypto, cash, unknown
}

public struct ExposureBlock: Sendable, Equatable, Identifiable {
    public var key: String
    public var label: String
    public var kind: SectorBlockKind
    public var value: Double
    /// % of the whole portfolio (cash included).
    public var weight: Double
    /// % of the stock part (stocks only, else nil).
    public var stockWeight: Double?
    public var symbols: [String]
    public var id: String { key }
}

public struct SectorInsight: Sendable, Equatable {
    public enum Level: String, Sendable { case warning, info }
    public enum Code: String, Sendable { case sectorHeavy = "sector_heavy", sectorHeavyStocks = "sector_heavy_stocks", sectorEffective = "sector_effective",
                                              sectorEtf = "sector_etf", sectorUnknown = "sector_unknown" }
    public var level: Level
    public var code: Code
    public var sector: String? = nil
    /// % of the whole portfolio.
    public var weight: Double? = nil
    /// % of the stock part.
    public var share: Double? = nil
    public var effective: Double? = nil
    public var symbols: String? = nil
}

public struct SectorExposure: Sendable {
    public var total: Double
    public var stockValue: Double
    /// Sorted by weight, heaviest first.
    public var blocks: [ExposureBlock]
    /// 1 / Σ w² over the stocks with an operating sector (ETFs and unknown left out); nil without any.
    public var effectiveSectors: Double?
    /// % of the stock part whose operating sector is known (ETFs and unknown excluded).
    public var classifiedShare: Double
    public var unknown: [(symbol: String, reason: String)]
    /// Number of stocks per classification, for the source line.
    public var bySource: [SectorClassification: Int]
    public var insights: [SectorInsight]
}

public enum Sectors {
    /// A sector above this share of the whole portfolio is flagged.
    public static let maxWeight = 35.0
    /// A sector above this share of the stock part is flagged (from 2 stock lines).
    public static let maxStockShare = 50.0

    public static let etfBlockLabel = "ETF / fonds (plusieurs secteurs)"
    public static let unknownLabel = "Secteur inconnu"

    /// A line of the portfolio with its current value (USD).
    public struct Line: Sendable {
        public var symbol: String
        public var kind: Kind
        public var value: Double
        public init(symbol: String, kind: Kind, value: Double) {
            self.symbol = symbol
            self.kind = kind
            self.value = value
        }
    }

    /// Exposure by sector. `sectors`: items of /api/sectors by symbol, nil when the call failed (`failure` then says why:
    /// every stock is "Secteur inconnu").
    public static func exposure(_ lines: [Line], cash: Double, sectors: [String: SectorItem]?,
                                failure: String = "classement sectoriel indisponible") -> SectorExposure {
        let safeCash = max(0, cash)
        let total = lines.reduce(0) { $0 + max(0, $1.value) } + safeCash
        var blocks: [String: (label: String, kind: SectorBlockKind, value: Double, symbols: [String])] = [:]
        var order: [String] = []
        var unknown: [(symbol: String, reason: String)] = []
        var bySource: [SectorClassification: Int] = [.nasdaq: 0, .sec: 0, .etf: 0]
        var counted = Set<String>()
        func add(_ key: String, _ label: String, _ kind: SectorBlockKind, _ value: Double, _ symbol: String? = nil) {
            var b = blocks[key] ?? (label, kind, 0, [])
            if blocks[key] == nil { order.append(key) }
            b.value += value
            if let symbol, !b.symbols.contains(symbol) { b.symbols.append(symbol) }
            blocks[key] = b
        }
        var stockValue = 0.0
        for l in lines {
            let value = max(0, l.value)
            if l.kind == .crypto {
                add("crypto", "Crypto", .crypto, value, l.symbol)
                continue
            }
            stockValue += value
            let s = sectors?[l.symbol]
            if let s, let sector = s.sector, let cls = s.classification {
                if !counted.contains(l.symbol) { bySource[cls, default: 0] += 1 }
                counted.insert(l.symbol)
                if cls == .etf { add("etf", etfBlockLabel, .etf, value, l.symbol) }
                // Nasdaq and SEC are two classifications: never merged, the SEC's labelled "(SIC)".
                else { add("\(cls.rawValue):\(sector)", cls == .sec ? "\(sector) (SIC)" : sector, .sector, value, l.symbol) }
            } else {
                add("unknown", unknownLabel, .unknown, value, l.symbol)
                let reason = s?.reason ?? (sectors != nil ? "absent de la réponse du serveur" : failure)
                // A Map on the web: the last reason of a symbol wins, at the place of its first insertion.
                if let i = unknown.firstIndex(where: { $0.symbol == l.symbol }) { unknown[i].reason = reason } else { unknown.append((l.symbol, reason)) }
            }
        }
        if safeCash > 0 { add("cash", "Liquidités", .cash, safeCash) }

        func pct(_ v: Double, _ of: Double) -> Double { of > 0 ? v / of * 100 : 0 }
        let fr = Locale(identifier: "fr_FR")
        let out: [ExposureBlock] = order.compactMap { key -> ExposureBlock? in
            guard let b = blocks[key], b.value > 0 else { return nil }
            let inStocks = b.kind == .sector || b.kind == .etf || b.kind == .unknown
            return ExposureBlock(key: key, label: b.label, kind: b.kind, value: b.value, weight: pct(b.value, total),
                                 stockWeight: inStocks ? pct(b.value, stockValue) : nil, symbols: b.symbols)
        }
        .enumerated()
        .sorted { x, y in
            let (a, b) = (x.element, y.element)
            if a.weight != b.weight { return a.weight > b.weight }
            let c = a.label.compare(b.label, locale: fr)
            return c == .orderedSame ? x.offset < y.offset : c == .orderedAscending
        }
        .map(\.element)

        let sectorBlocks = out.filter { $0.kind == .sector }
        let classified = sectorBlocks.reduce(0) { $0 + $1.value }
        let hhi = classified > 0 ? sectorBlocks.reduce(0) { $0 + pow($1.value / classified, 2) } : 0
        let effectiveSectors: Double? = hhi > 0 ? 1 / hhi : nil
        let stockLines = Set(lines.filter { $0.kind == .stock && $0.value > 0 }.map(\.symbol)).count

        var insights: [SectorInsight] = []
        if let top = sectorBlocks.first {
            if top.weight > maxWeight {
                insights.append(SectorInsight(level: .warning, code: .sectorHeavy, sector: top.label, weight: top.weight))
            } else if stockLines >= 2, let share = top.stockWeight, share > maxStockShare {
                insights.append(SectorInsight(level: .warning, code: .sectorHeavyStocks, sector: top.label, share: share))
            }
        }
        if let e = effectiveSectors, sectorBlocks.reduce(0, { $0 + $1.symbols.count }) >= 2, e < 2 {
            insights.append(SectorInsight(level: .info, code: .sectorEffective, effective: e))
        }
        if let etf = out.first(where: { $0.kind == .etf }) {
            insights.append(SectorInsight(level: .info, code: .sectorEtf, weight: etf.weight, symbols: etf.symbols.joined(separator: ", ")))
        }
        if let unk = out.first(where: { $0.kind == .unknown }) {
            insights.append(SectorInsight(level: .info, code: .sectorUnknown, weight: unk.weight, symbols: unk.symbols.joined(separator: ", ")))
        }

        return SectorExposure(total: total, stockValue: stockValue, blocks: out, effectiveSectors: effectiveSectors,
                              classifiedShare: pct(classified, stockValue), unknown: unknown, bySource: bySource, insights: insights)
    }

    // MARK: Texts (web: sectorInsightText, CLASSIFICATION_SOURCE, SectorCard.tsx)

    /// "12,3 %" (fr-FR, one decimal at most).
    public static func pc(_ v: Double) -> String { "\(JSFormat.fr(v, max: 1)) %" }

    public static func text(_ i: SectorInsight) -> String {
        switch i.code {
        case .sectorHeavy:
            return "\(i.sector ?? "") pèse \(pc(i.weight ?? 0)) de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois."
        case .sectorHeavyStocks:
            return "\(i.sector ?? "") représente \(pc(i.share ?? 0)) de vos actions : leur diversification sectorielle est faible."
        case .sectorEffective:
            return "Vos actions classées équivalent à \(JSFormat.fr(i.effective ?? 0, max: 1)) secteur(s) de même poids."
        case .sectorEtf:
            return "ETF (\(i.symbols ?? ""), \(pc(i.weight ?? 0))) : leur répartition par secteur n'est pas couverte (composition non lue)."
        case .sectorUnknown:
            return "Secteur non couvert pour \(i.symbols ?? "") (\(pc(i.weight ?? 0)))."
        }
    }

    public static func classificationSource(_ c: SectorClassification) -> String {
        switch c {
        case .nasdaq: return "Nasdaq (secteur du screener)"
        case .sec: return "SEC EDGAR (code SIC, grandes divisions)"
        case .etf: return "Nasdaq Trader / SEC (ETF)"
        }
    }

    /// "Nasdaq (secteur du screener) · 3 actions ; Nasdaq Trader / SEC (ETF) · 1 action" (empty without any).
    public static func sourceLine(_ by: [SectorClassification: Int]) -> String {
        SectorClassification.allCases.compactMap { k -> String? in
            guard let n = by[k], n > 0 else { return nil }
            return "\(classificationSource(k)) · \(n) action\(n > 1 ? "s" : "")"
        }
        .joined(separator: " ; ")
    }

    /// Source line of the card (web SectorCard.tsx), with what the iPhone does not know: its cash.
    public static func footnote(_ e: SectorExposure) -> String {
        let src = sourceLine(e.bySource)
        return "Sources : \(src.isEmpty ? "aucune action à classer" : src). Crypto : vos avoirs. Les secteurs SEC (codes SIC) sont de grandes divisions, affichés à part de ceux du Nasdaq. Les liquidités ne sont pas saisies sur l'iPhone : pourcentages calculés sur vos lignes."
    }

    /// Stocks to classify: unique, sorted, 50 at most (the server's limit).
    public static func stockSymbols(_ lines: [Line]) -> [String] {
        Array(Set(lines.filter { $0.kind == .stock }.map(\.symbol)).sorted().prefix(50))
    }
}

extension AltimClient {
    /// Sector of each stock (Nasdaq screener, SEC SIC code, ETF flag); 50 at most.
    public func sectors(_ symbols: [String]) async throws -> SectorsReport {
        try await getSectors(["symbols": symbols.prefix(50).joined(separator: ",")])
    }
}
