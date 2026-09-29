import Foundation
import XCTest
@testable import AltimKit

/// Same cases and expected numbers as web/test/sectors.test.ts (web/src/engine/sectors.ts).
final class SectorsTests: XCTestCase {
    static func item(_ symbol: String, _ sector: String?, _ c: SectorClassification?, _ reason: String? = nil) -> SectorItem {
        SectorItem(symbol: symbol, sector: sector, classification: c, source: c == nil ? nil : "test", reason: reason, etf: c == .etf)
    }

    static let sectors: [String: SectorItem] = [
        "AAPL": item("AAPL", "Technologie", .nasdaq),
        "NVDA": item("NVDA", "Technologie", .nasdaq),
        "JPM": item("JPM", "Finance", .nasdaq),
        "BRK-B": item("BRK-B", "Finance et immobilier", .sec),
        "SPY": item("SPY", "ETF / fonds indiciel (plusieurs secteurs)", .etf),
        "ABCD": item("ABCD", nil, nil, "Secteur non couvert : aucun dépôt à la SEC"),
    ]

    static func line(_ s: String, _ k: Kind, _ v: Double) -> Sectors.Line { Sectors.Line(symbol: s, kind: k, value: v) }

    func testBlocksSectorsEtfCryptoCashUnknownHeaviestFirst() throws {
        let e = Sectors.exposure([
            Self.line("AAPL", .stock, 3000), Self.line("NVDA", .stock, 1000), Self.line("JPM", .stock, 1000), Self.line("SPY", .stock, 1000),
            Self.line("ABCD", .stock, 500), Self.line("BTC", .crypto, 2000), Self.line("ETH", .crypto, 500),
        ], cash: 1000, sectors: Self.sectors)
        XCTAssertEqual(e.total, 10000)
        XCTAssertEqual(e.stockValue, 6500)
        XCTAssertEqual(e.blocks.map(\.label), ["Technologie", "Crypto", Sectors.etfBlockLabel, "Finance", "Liquidités", Sectors.unknownLabel])
        XCTAssertEqual(e.blocks.map(\.weight), [40, 25, 10, 10, 10, 5])
        let tech = e.blocks[0]
        XCTAssertEqual(try XCTUnwrap(tech.stockWeight), 4000.0 / 6500 * 100, accuracy: 1e-9)
        XCTAssertEqual(tech.symbols, ["AAPL", "NVDA"])
        XCTAssertNil(e.blocks.first { $0.kind == .crypto }?.stockWeight)
        // Weights add up to 100 %.
        XCTAssertEqual(e.blocks.reduce(0) { $0 + $1.weight }, 100, accuracy: 1e-9)
        // Effective sectors over Technologie 4000 / Finance 1000: 1 / (0.8² + 0.2²).
        XCTAssertEqual(try XCTUnwrap(e.effectiveSectors), 1 / 0.68, accuracy: 1e-9)
        XCTAssertEqual(e.classifiedShare, 5000.0 / 6500 * 100, accuracy: 1e-9)
        XCTAssertEqual(e.bySource, [.nasdaq: 3, .sec: 0, .etf: 1])
        XCTAssertEqual(e.unknown.map(\.symbol), ["ABCD"])
        XCTAssertEqual(e.unknown.map(\.reason), ["Secteur non couvert : aucun dépôt à la SEC"])
        XCTAssertEqual(e.insights.map(\.code.rawValue), ["sector_heavy", "sector_effective", "sector_etf", "sector_unknown"])
        XCTAssertEqual(Sectors.text(e.insights[0]),
                       "Technologie pèse 40 % de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois.")
        XCTAssertEqual(Sectors.text(e.insights[1]), "Vos actions classées équivalent à 1,5 secteur(s) de même poids.")
        XCTAssertEqual(Sectors.text(e.insights[2]), "ETF (SPY, 10 %) : leur répartition par secteur n'est pas couverte (composition non lue).")
        XCTAssertEqual(Sectors.text(e.insights[3]), "Secteur non couvert pour ABCD (5 %).")
        XCTAssertEqual(Sectors.sourceLine(e.bySource), "Nasdaq (secteur du screener) · 3 actions ; Nasdaq Trader / SEC (ETF) · 1 action")
    }

    func testStockPartConcentrationSecAndNasdaqNeverMerged() throws {
        let e = Sectors.exposure([
            Self.line("AAPL", .stock, 2000), Self.line("JPM", .stock, 1000), Self.line("BRK-B", .stock, 500), Self.line("BTC", .crypto, 6500),
        ], cash: 0, sectors: Self.sectors)
        XCTAssertEqual(e.blocks.map(\.label), ["Crypto", "Technologie", "Finance", "Finance et immobilier (SIC)"])
        XCTAssertEqual(e.insights.first, SectorInsight(level: .warning, code: .sectorHeavyStocks, sector: "Technologie", share: 2000.0 / 3500 * 100))
        XCTAssertEqual(Sectors.text(e.insights[0]), "Technologie représente 57,1 % de vos actions : leur diversification sectorielle est faible.")
        XCTAssertEqual(try XCTUnwrap(e.effectiveSectors), 1 / (pow(2 / 3.5, 2) + pow(1 / 3.5, 2) + pow(0.5 / 3.5, 2)), accuracy: 1e-9)
        // One stock line only: 100 % of the stock part is not flagged (the line concentration is, elsewhere).
        let one = Sectors.exposure([Self.line("AAPL", .stock, 1000), Self.line("BTC", .crypto, 9000)], cash: 0, sectors: Self.sectors)
        XCTAssertEqual(one.insights, [])
    }

    func testServerUnreachableEveryStockUnknownWithTheReason() {
        let e = Sectors.exposure([Self.line("AAPL", .stock, 1000), Self.line("BTC", .crypto, 1000)], cash: 0, sectors: nil, failure: "HTTP 502")
        XCTAssertEqual(e.blocks.map(\.kind), [.crypto, .unknown])
        XCTAssertEqual(e.blocks.map(\.weight), [50, 50])
        XCTAssertNil(e.effectiveSectors)
        XCTAssertEqual(e.classifiedShare, 0)
        XCTAssertEqual(e.unknown.map(\.symbol), ["AAPL"])
        XCTAssertEqual(e.unknown.map(\.reason), ["HTTP 502"])
        // Empty portfolio: no block, no division by zero.
        let empty = Sectors.exposure([], cash: 0, sectors: [:])
        XCTAssertEqual(empty.total, 0)
        XCTAssertEqual(empty.blocks.count, 0)
        XCTAssertNil(empty.effectiveSectors)
    }

    func testStockMissingFromTheAnswer() {
        let e = Sectors.exposure([Self.line("MSFT", .stock, 100)], cash: 0, sectors: [:])
        XCTAssertEqual(e.unknown.map(\.reason), ["absent de la réponse du serveur"])
        XCTAssertEqual(Sectors.sourceLine(e.bySource), "")
        XCTAssertTrue(Sectors.footnote(e).hasPrefix("Sources : aucune action à classer."))
    }

    func testDecodesTheRouteAndToleratesMissingOrOddFields() throws {
        let json = """
        {"asOf": 1790000000000, "items": [
          {"symbol": "NVDA", "sector": "Technologie", "classification": "nasdaq", "source": "Nasdaq (secteur du screener des actions US)",
           "reason": null, "etf": false,
           "sec": {"label": "Industrie manufacturière", "sic": "3674", "sicDescription": "Semiconductors & Related Devices", "source": "SEC EDGAR (code SIC déclaré)"},
           "nasdaq": {"sector": "Technology", "sectorFr": "Technologie", "industry": "Semiconductors"}},
          {"symbol": "SPY", "sector": "ETF / fonds indiciel (plusieurs secteurs)", "classification": "etf", "etf": true, "sec": null, "nasdaq": null},
          {"symbol": "ABCD", "sector": null, "classification": null, "reason": "Secteur non couvert : aucun dépôt à la SEC"},
          {"symbol": "ODD", "sector": "Autre", "classification": "gics"},
          {"sector": "Sans symbole"},
          42
        ], "sources": [{"name": "SEC EDGAR", "ok": false, "error": "HTTP 503"}, {"name": "Nasdaq"}]}
        """
        let r = try JSONDecoder().decode(SectorsReport.self, from: Data(json.utf8))
        XCTAssertEqual(r.asOf, 1_790_000_000_000)
        XCTAssertEqual(r.items.map(\.symbol), ["NVDA", "SPY", "ABCD", "ODD"])
        let by = r.bySymbol
        XCTAssertEqual(by["NVDA"]?.nasdaq?.industry, "Semiconductors")
        XCTAssertEqual(by["NVDA"]?.sec?.sic, "3674")
        XCTAssertEqual(by["SPY"]?.classification, .etf)
        XCTAssertEqual(by["SPY"]?.etf, true)
        XCTAssertNil(by["ODD"]?.classification)
        XCTAssertEqual(r.sources.map(\.name), ["SEC EDGAR", "Nasdaq"])
        XCTAssertEqual(r.sources.first?.error, "HTTP 503")
        XCTAssertEqual(r.sources.last?.ok, false)
        // An unknown classification counts as unknown.
        let e = Sectors.exposure([Self.line("ODD", .stock, 100), Self.line("NVDA", .stock, 100)], cash: 0, sectors: by)
        XCTAssertEqual(e.blocks.map(\.label), ["Secteur inconnu", "Technologie"])
        XCTAssertEqual(e.unknown.map(\.reason), ["absent de la réponse du serveur"])
        // An empty object decodes too.
        let blank = try JSONDecoder().decode(SectorsReport.self, from: Data("{}".utf8))
        XCTAssertTrue(blank.items.isEmpty && blank.sources.isEmpty && blank.asOf == nil)
    }

    func testStockSymbolsUniqueSortedAtMostFifty() {
        let lines = (0..<60).map { Self.line(String(format: "S%02d", 59 - $0), .stock, 1) } + [Self.line("S00", .stock, 1), Self.line("BTC", .crypto, 1)]
        let s = Sectors.stockSymbols(lines)
        XCTAssertEqual(s.count, 50)
        XCTAssertEqual(s.first, "S00")
        XCTAssertEqual(s.last, "S49")
    }
}
