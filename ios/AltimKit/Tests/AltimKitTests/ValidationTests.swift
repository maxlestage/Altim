import Foundation
import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimKit

/// Same cases as web/test/validation.test.ts on the real /api/validation answer (backend/tests/samples/validation.json),
/// plus the optional-safe decoding and the 202 "pending" answer.
final class ValidationTests: XCTestCase {
    func sample() throws -> ValidationReport {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "validation", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(ValidationReport.self, from: Data(contentsOf: url))
    }

    func regime(_ json: String) throws -> ValidationRegimeGroup {
        try JSONDecoder().decode(ValidationRegimeGroup.self, from: Data(json.utf8))
    }

    let bear = #"""
    {"regime":"bear","label":"Marché baissier","trades":7,"winRate":42.9,"profitFactor":0.8,"expectancy":-0.31,"avgR":-0.1,"tStat":-0.4,"days":3922,"assets":20,
     "beatHold":7,"beatShare":35,"medianSignal":-1.5,"medianHold":-9.25,"lowSample":true,"verdict":"insufficient","verdictLabel":"Échantillon trop faible"}
    """#

    // MARK: Contract sample

    func testEveryBasketAssetIsTestedOrListedGroupsAddUp() throws {
        let r = try sample()
        XCTAssertEqual(ModelValidation.path, "/api/validation")
        XCTAssertTrue(AltimClient.cacheable.contains("/api/validation"))
        XCTAssertEqual(r.assets.count + r.failures.count, r.basket.count)
        XCTAssertEqual(r.basket.count, 34)
        XCTAssertEqual(r.overall.pooled.trades, r.assets.reduce(0) { $0 + $1.trades })
        XCTAssertEqual(r.overall.pooled.trades, 822)
        XCTAssertEqual(r.classes.reduce(0) { $0 + $1.assets }, r.overall.assets)
        XCTAssertEqual(r.overall.regimes.reduce(0) { $0 + $1.pooled.trades }, r.overall.pooled.trades)
        XCTAssertEqual(r.overall.beatHold, r.assets.filter(\.beatHold).count)
        XCTAssertEqual(r.overall.beatHold, 6)
        XCTAssertEqual(r.classes.map(\.id), ModelValidation.classOrder.map(\.rawValue).filter { c in r.classes.contains { $0.id == c } })
        XCTAssertEqual(r.classes.compactMap(\.assetClass), [.stock, .btc, .eth, .altcoin])
        XCTAssertEqual(r.outOfSample.status, "notVerifiable")
        XCTAssertEqual(r.parameters.taxRatePct, 30)
        XCTAssertTrue(r.headline.hasPrefix("Sur \(r.overall.assets) actifs"))
        XCTAssertEqual(r.overall.id, "all")
        XCTAssertNil(r.overall.assetClass)
        XCTAssertEqual(r.overall.verdict, .unproven)
        XCTAssertEqual(r.overall.verdictLabel, "Avantage non démontré")
        XCTAssertEqual(r.overall.worstReturn?.symbol, "LINK")
        XCTAssertEqual(r.groups.count, 5)
        XCTAssertFalse(r.protections.isEmpty)
        XCTAssertFalse(r.limits.isEmpty)
        XCTAssertFalse(r.outOfSample.note.isEmpty)
        let btc = try XCTUnwrap(r.assets.first { $0.symbol == "BTC" })
        XCTAssertEqual(btc.kind, .crypto)
        XCTAssertEqual(btc.assetClass, .btc)
        XCTAssertEqual(r.assets[0].asset, Asset(symbol: "AAPL", kind: .stock, name: "Apple"))
    }

    func testListKeepsBasketOrderByDefaultNeverRankedByPerformance() throws {
        let r = try sample()
        XCTAssertEqual(ModelValidation.sortAssets(r.assets, .byClass).map(\.symbol), r.assets.map(\.symbol))
        let byGap = ModelValidation.sortAssets(r.assets, .gap).map(\.gap)
        XCTAssertEqual(byGap, byGap.sorted(by: >))
        let byName = ModelValidation.sortAssets(r.assets, .name).map(\.symbol)
        XCTAssertEqual(byName, byName.sorted())
        XCTAssertEqual(Array(byName.prefix(5)), ["AAPL", "ADA", "AMZN", "AVAX", "BA"])
        XCTAssertEqual(r.assets[0].symbol, r.basket[0].symbol)
        XCTAssertEqual(ModelValidation.SortKey.allCases.map(\.label), ["Par classe", "Par nom", "Écart avec la détention"])
    }

    // MARK: Texts

    func testNumbersInFrenchWithSigns() throws {
        let g = try regime(bear)
        XCTAssertEqual(ModelValidation.signedPct(12.345), "+12,3\u{202F}%")
        XCTAssertEqual(ModelValidation.signedPct(-4), "−4\u{202F}%")
        XCTAssertEqual(ModelValidation.signedPct(nil), "—")
        XCTAssertEqual(ModelValidation.beatText(6, 34, 17.6), "6 sur 34 (18\u{202F}%)")
        XCTAssertEqual(ModelValidation.beatText(0, 0, nil), "aucun actif comparable")
        XCTAssertEqual(ModelValidation.pooledText(g.pooled), "7 trades · réussite 43\u{202F}% · espérance −0,31\u{202F}% · facteur de profit 0,8 · t = −0,4")
        var none = g.pooled
        none.trades = 0
        XCTAssertEqual(ModelValidation.pooledText(none), "Aucun trade.")
        XCTAssertTrue(ModelValidation.regimeDaysText(g).contains("7 sur 20 (35\u{202F}%) actifs"))
        var empty = g
        empty.assets = 0
        XCTAssertTrue(ModelValidation.regimeDaysText(empty).contains("Aucun actif"))
        XCTAssertEqual(ModelValidation.verdictTone(.edge), .good)
        XCTAssertEqual(ModelValidation.verdictTone(.negative), .warn)
        XCTAssertEqual(ModelValidation.verdictTone(.unproven), .neutral)
        XCTAssertEqual(ModelValidation.regimeDays(g), "3\u{202F}922 jours-actifs dans ce régime.")
    }

    func testHeadlineTexts() throws {
        let r = try sample()
        XCTAssertEqual(ModelValidation.historyTile(r), "0,5 à 4,8\u{202F}ans")
        XCTAssertEqual(ModelValidation.testedTile(r), "34 / 34")
        XCTAssertEqual(ModelValidation.periodText(r), "déc. 2021 – sept. 2026")
        XCTAssertEqual(ModelValidation.basketDate(r.basketFixedOn), "29/09/2026")
        XCTAssertEqual(ModelValidation.years(1), "1\u{202F}an")
        XCTAssertEqual(ModelValidation.monthYear(nil), "?")
        XCTAssertEqual(ModelValidation.failuresTitle(1), "1 actif non testé :")
        XCTAssertEqual(ModelValidation.failuresTitle(2), "2 actifs non testés :")
        XCTAssertTrue(ModelValidation.costsText(r).hasPrefix("Coûts par ordre : frais 0,1\u{202F}%, glissement 0,05\u{202F}%, écart achat/vente supposé 0,01\u{202F}% (actions) ou 0,02\u{202F}% (cryptos)"))
        XCTAssertTrue(ModelValidation.costsText(r).contains("Panier fixé le 29/09/2026."))
        XCTAssertTrue(ModelValidation.regimeFooter(r).hasSuffix("« inconnu » tant que l'historique est trop court pour le classer."))
        let stocks = r.classes[0]
        XCTAssertEqual(ModelValidation.groupSubtitle(stocks), "22 actifs · historique médian \(ModelValidation.years(stocks.years))")
        let rows = ModelValidation.groupRows(r.overall, taxRate: 30)
        XCTAssertEqual(rows.first?.label, "Rendement médian du signal")
        XCTAssertEqual(rows.first?.value, "−9,7\u{202F}%")
        XCTAssertTrue(rows.contains { $0.label == "Pire actif (LINK)" && $0.value == "−83,7\u{202F}%" })
        XCTAssertEqual(rows.last?.label, "Après impôt 30\u{202F}% (signal / détention)")
        let aapl = r.assets[0]
        XCTAssertEqual(ModelValidation.assetGapText(aapl), "(moins bien, écart −86,5\u{202F}%)")
        XCTAssertEqual(ModelValidation.assetTag(aapl), "Actions · Technologie")
        XCTAssertTrue(ModelValidation.assetDetails(aapl).hasPrefix("36 trades · réussite 42\u{202F}% · facteur de profit 1,16 · espérance +0,47\u{202F}%"))
        XCTAssertTrue(ModelValidation.assetDetails(aapl).hasSuffix("(StockAnalysis)"))
        XCTAssertTrue(r.assets.contains(where: \.lowSample))
    }

    func testUnknownRegimeLast() throws {
        let r = try sample()
        let order = ModelValidation.regimesOf(r.overall).map(\.regime)
        XCTAssertEqual(order, [.bull, .bear, .range, .crisis, .unknown])
        var g = r.overall
        g.regimes = [g.regimes[4]] + Array(g.regimes.prefix(4))
        XCTAssertEqual(ModelValidation.regimesOf(g).map(\.regime), [.bull, .bear, .range, .crisis, .unknown])
    }

    // MARK: Robustness

    func testEveryFieldOptionalSafe() throws {
        let r = try JSONDecoder().decode(ValidationReport.self, from: Data("{}".utf8))
        XCTAssertTrue(r.assets.isEmpty)
        XCTAssertEqual(r.overall.pooled.trades, 0)
        XCTAssertEqual(r.parameters.taxRatePct, 30)
        XCTAssertEqual(ModelValidation.historyTile(r), "—")
        XCTAssertEqual(ModelValidation.periodText(r), "? – ?")
        let odd = #"""
        {"overall":{"id":"all","trades":"x","verdict":"magic","regimes":[{"regime":"moon","trades":3},42],"worstReturn":{"symbol":"X"}},
         "assets":[{"symbol":"NEW","class":"altcoin","totalReturn":5,"buyAndHold":null},"oops"],"classes":null}
        """#
        let o = try JSONDecoder().decode(ValidationReport.self, from: Data(odd.utf8))
        XCTAssertEqual(o.overall.verdict, .unproven)
        XCTAssertEqual(o.overall.regimes.map(\.regime), [.unknown])
        XCTAssertNil(o.overall.worstReturn)
        XCTAssertEqual(o.assets.count, 1)
        XCTAssertEqual(o.assets[0].kind, .crypto)
        XCTAssertEqual(o.assets[0].gap, 5)
        XCTAssertTrue(o.classes.isEmpty)
    }

    func testPendingThenReadyThenOffline() async throws {
        AltimClient.retryDelays = [0.01, 0.01]
        StubProtocol.calls = 0
        StubProtocol.urls = []
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("altim-validation-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [StubProtocol.self]
        let c = AltimClient(baseURL: URL(string: "https://x.herokuapp.com")!, credentials: nil, session: AltimClient.makeSession(configuration: config),
                            cache: FileResponseCache(directory: dir))
        let url = try XCTUnwrap(Bundle.module.url(forResource: "validation", withExtension: "json", subdirectory: "Fixtures"))
        let body = try String(contentsOf: url, encoding: .utf8)
        StubProtocol.script = [.status(202, #"{"pending":true}"#), .status(200, body), .offline, .offline, .offline]
        guard case .pending = try await c.validation() else { return XCTFail("202 is pending") }
        guard case let .ready(r) = try await c.validation() else { return XCTFail("200 is ready") }
        XCTAssertEqual(r.assets.count, 34)
        XCTAssertEqual(StubProtocol.urls.last?.path, "/api/validation")
        // Offline: the last good answer.
        guard case let .ready(cached) = try await c.validation() else { return XCTFail("cached answer") }
        XCTAssertEqual(cached.overall.pooled.trades, 822)
        StubProtocol.script = []
    }
}
