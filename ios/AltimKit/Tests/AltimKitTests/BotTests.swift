import Foundation
import XCTest
@testable import AltimKit

/// « Bot Altim »: same cases as web/test/bot.test.ts on the real /api/bot answers (backend/tests/samples/bot.json, v2,
/// and bot-v1.json, copied as Fixtures/bot.json and Fixtures/bot-v1.json), plus the optional-safe decoding,
/// /api/bot/views and the `bot` field of a decision.
final class BotTests: XCTestCase {
    func raw(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func sample() throws -> BotReport { try JSONDecoder().decode(BotReport.self, from: raw("bot")) }
    func sampleV1() throws -> BotReport { try JSONDecoder().decode(BotReport.self, from: raw("bot-v1")) }

    /// Narrow no-break spaces (before %) read as plain spaces in the expectations.
    func n(_ s: String) -> String { s.replacingOccurrences(of: "\u{202F}", with: " ") }

    // MARK: Contract sample (v1: still read)

    func testGroupsAssetsAndSignalsAddUp() throws {
        let r = try sampleV1()
        XCTAssertEqual(ModelBot.path, "/api/bot")
        XCTAssertTrue(AltimClient.cacheable.contains("/api/bot"))
        XCTAssertEqual(r.groups.map(\.id), [.stock, .crypto])
        XCTAssertEqual(r.assets.count + r.failures.count, 34)
        XCTAssertEqual(r.overall.buy.signals, r.groups.reduce(0) { $0 + $1.buy.signals })
        XCTAssertEqual(r.overall.sell.signals, r.groups.reduce(0) { $0 + $1.sell.signals })
        XCTAssertEqual(r.overall.buy.signals, 207)
        XCTAssertEqual(r.overall.labelled, 12504)
        for g in r.groups {
            XCTAssertEqual(g.stats.calibrationUp.reduce(0) { $0 + $1.rows }, g.stats.labelled)
            XCTAssertEqual(try XCTUnwrap(g.model).up.weights.count, r.features.count)
            if let m = g.buy.meanNet, let b = g.buy.baselineNet, let e = g.buy.excess { XCTAssertLessThan(abs(m - b - e), 0.02) }
            XCTAssertFalse(g.text.isEmpty)
        }
        XCTAssertEqual(r.features.count, 16)
        XCTAssertEqual(r.parameters.horizonDays, 20)
        XCTAssertEqual(r.parameters.minSignals, 30)
        XCTAssertEqual(r.parameters.costStockPct, 0.31)
        XCTAssertFalse(r.method.isEmpty)
        XCTAssertFalse(r.limits.isEmpty)
        // The real run found no out-of-sample edge: the headline says it does not count.
        XCTAssertFalse(ModelBot.anyEdge(r))
        XCTAssertTrue(r.headline.hasPrefix("Hors échantillon, le bot n'a pas fait mieux"))
        XCTAssertTrue(r.headline.contains("il ne compte pas dans les décisions"))
    }

    func testFirstAssetAndGroupDecodeExactly() throws {
        let r = try sampleV1()
        let a = try XCTUnwrap(r.assets.first)
        XCTAssertEqual(a.symbol, "AAPL")
        XCTAssertEqual(a.kind, .stock)
        XCTAssertEqual(a.assetClass, .stock)
        XCTAssertEqual(a.group, .stock)
        XCTAssertEqual(a.now.action, .wait)
        XCTAssertEqual(a.now.up, 56)
        XCTAssertEqual(a.asset, Asset(symbol: "AAPL", kind: .stock, name: "Apple"))
        let g = r.groups[0]
        XCTAssertEqual(g.label, "Actions et ETF américains")
        XCTAssertEqual(g.buy.verdict, .unproven)
        XCTAssertEqual(g.buy.verdictLabel, "Avantage non démontré")
        XCTAssertEqual(r.groups[1].sell.verdict, .insufficient)
        XCTAssertNil(r.overall.calibrationUp[0].predicted)
    }

    // MARK: Texts

    func testPointsSkillCalibrationRows() {
        XCTAssertEqual(ModelBot.points(0.52), "+0,52 point")
        XCTAssertEqual(ModelBot.points(-2.44), "−2,44 points")
        XCTAssertEqual(ModelBot.points(nil), "—")
        XCTAssertEqual(n(ModelBot.skillText(-1.1)), "1,1 % moins bien que la fréquence de base")
        XCTAssertEqual(n(ModelBot.skillText(2)), "2 % mieux que la fréquence de base")
        XCTAssertEqual(ModelBot.skillText(0.2), "pas mieux que la fréquence de base")
        XCTAssertEqual(ModelBot.skillText(nil), "non calculable")
        XCTAssertEqual(ModelBot.calibrationRows([BotBucket(from: 0, to: 30, label: "x", rows: 0, predicted: nil, realised: nil)]), [])
        XCTAssertEqual(ModelBot.viewsQuery([Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), Asset(symbol: "AAPL", kind: .stock, name: "Apple")]),
                       ["symbols": "BTC:crypto,AAPL:stock"])
        XCTAssertEqual(ModelBot.viewsQuery(Array(repeating: Asset(symbol: "X", kind: .stock, name: "X"), count: 25))["symbols"]?.split(separator: ",").count, 20)
        XCTAssertEqual(ModelBot.count(12504), "12\u{202F}504")
        XCTAssertEqual(BotAction.sell.label, "VENDRE")
    }

    func testGroupTextsOfTheRealSample() throws {
        let r = try sampleV1()
        let s = r.groups[0], c = r.groups[1]
        XCTAssertEqual(n(ModelBot.buyText(s.buy)),
                       "108 achats : +1,7 % en moyenne sur 20 jours, contre +1,85 % pour une entrée au hasard sur le même actif et la même période (−0,15 point, t = −0,1).")
        XCTAssertEqual(n(ModelBot.sellText(c.sell)),
                       "7 ventes : le cours a fait −1,01 % dans les 20 jours suivants, contre −3,45 % après un jour au hasard (cours ensuite supérieur de 2,44 points, t = −0,7).")
        XCTAssertEqual(n(ModelBot.waitText(s.wait)), "ATTENDRE 85 % des jours testés, suivis en moyenne de +0,98 % (tous les jours : +1,39 %).")
        XCTAssertEqual(ModelBot.groupSubtitle(s), "22 actifs testés · test du oct. 2024 au sept. 2026 · 4 réentraînements")
        XCTAssertEqual(ModelBot.calibrationTitle(s, up: false), "Actions et ETF américains · baisse")
        XCTAssertEqual(ModelBot.calibrationRows(s.stats.calibrationUp).map(\.label), ["40 à 50 %", "50 à 60 %", "60 à 70 %", "70 % et plus"])
        XCTAssertEqual(n(ModelBot.precisionText(s.stats.brierSkillUp)), "Précision : 1,1 % moins bien que la fréquence de base.")
        XCTAssertEqual(n(ModelBot.buyRows(s.buy)[0].value), "46 % / 55 %")
        XCTAssertEqual(ModelBot.assetsTile(r), "34 / 34")
        XCTAssertEqual(ModelBot.signalsTile(r), "207 / 54")
        XCTAssertEqual(ModelBot.parametersText(r), "Horizon 20 jours · seuil : fréquence d'entraînement + 5 points · coûts d'un aller-retour 0,31 % (actions), 0,32 % (cryptos).")
        XCTAssertTrue(ModelBot.footer(r).hasPrefix("Panier fixé le 29/09/2026. Source : "))
        XCTAssertFalse(ModelBot.footer(r).contains("Calcul"))
        let a = r.assets[0]
        XCTAssertEqual(n(ModelBot.todayText(a)), "Aujourd'hui : hausse 56 %, baisse 40 %")
        XCTAssertEqual(n(ModelBot.assetTestText(a)),
                       "Test : 3 achats (−7,12 points vs hasard) · 3 ventes (cours ensuite −1,42 point vs hasard) · attente 90 % · achats cumulés −19,6 %, détention +45,7 % (Robinhood)")
    }

    func testSellTextSaysBetterOrWorseWithoutASign() throws {
        var s = try JSONDecoder().decode(BotSellStats.self, from: Data(#"""
        {"signals":7,"meanAfter":-1.01,"baselineAfter":-3.45,"allDaysAfter":1.6,"avoided":-2.44,"tStat":-0.66,"fallRate":71.4,"baselineFallRate":45.7,
         "meanDrawdown":-11,"baselineDrawdown":-8.8,"verdict":"insufficient","verdictLabel":"Trop peu de ventes pour conclure"}
        """#.utf8))
        XCTAssertTrue(n(ModelBot.sellText(s)).contains("cours ensuite supérieur de 2,44 points"))
        s.avoided = 1.2
        XCTAssertTrue(n(ModelBot.sellText(s)).contains("cours ensuite inférieur de 1,2 point"))
        s.signals = 0
        XCTAssertEqual(ModelBot.sellText(s), "Aucune vente pendant les périodes de test.")
        var b = try sampleV1().groups[0].buy
        b.signals = 0
        XCTAssertEqual(ModelBot.buyText(b), "Aucun achat pendant les périodes de test.")
        XCTAssertEqual(ModelBot.waitText(BotWaitStats(days: 0, share: nil, meanNet: nil, baselineNet: nil)), "Jamais sur ATTENDRE pendant les tests.")
    }

    func testV1LeavesTheV2PartsOut() throws {
        let r = try sampleV1()
        XCTAssertNil(r.version)
        XCTAssertTrue(r.changes.isEmpty)
        XCTAssertTrue(r.extraFailures.isEmpty)
        XCTAssertNil(r.extraFixedOn)
        XCTAssertNil(r.timing)
        XCTAssertNil(r.parameters.innerValidationDays)
        XCTAssertNil(ModelBot.extraFailuresText(r))
        XCTAssertTrue(ModelBot.withCandidates(r).isEmpty)
        XCTAssertFalse(ModelBot.hasSubResults(r))
        let g = r.groups[0]
        XCTAssertNil(g.universe)
        XCTAssertNil(ModelBot.dataText(g))
        XCTAssertNil(g.buy.clustered)
        XCTAssertNil(g.sell.exit)
        XCTAssertNil(ModelBot.exitText(g.sell.exit))
        XCTAssertTrue(g.selection.isEmpty)
        XCTAssertNil(g.holdout)
        XCTAssertNil(g.extra)
        XCTAssertNil(g.model?.candidate)
        XCTAssertNil(ModelBot.liveModelText(g))
        XCTAssertTrue(n(ModelBot.buyText(g.buy)).contains("(−0,15 point, t = −0,1)"))
        XCTAssertNil(r.assets[0].outShare)
        XCTAssertNil(r.assets[0].years)
    }

    // MARK: Contract sample (v2)

    func testV2SampleAddsUp() throws {
        let r = try sample()
        XCTAssertEqual(r.version, 2)
        XCTAssertEqual(r.features.count, 24)
        XCTAssertEqual(r.changes.count, 6)
        XCTAssertEqual(r.extraFixedOn, "2026-09-29")
        XCTAssertEqual(r.timing?.threads, 4)
        XCTAssertEqual(r.parameters.innerValidationDays, 252)
        XCTAssertEqual(r.parameters.trees, 100)
        XCTAssertEqual(r.parameters.shrinkage, 0.1)
        XCTAssertEqual(r.parameters.horizonDays, 20)
        XCTAssertTrue(r.parameters.selection?.hasPrefix("À chaque réentraînement") ?? false)
        XCTAssertEqual(r.assets.count + r.failures.count, 34)
        XCTAssertEqual(r.overall.buy.signals, r.groups.reduce(0) { $0 + $1.buy.signals })
        XCTAssertEqual(r.overall.sell.signals, r.groups.reduce(0) { $0 + $1.sell.signals })
        for g in r.groups {
            XCTAssertEqual(g.stats.calibrationUp.reduce(0) { $0 + $1.rows }, g.stats.labelled)
            let m = try XCTUnwrap(g.model)
            if m.candidate == .v1 || m.candidate == .logit { XCTAssertEqual(m.up.weights.count, m.candidate == .v1 ? 16 : r.features.count) }
            XCTAssertEqual(m.scores.count, 4)
            XCTAssertNotNil(m.upModel?.logit)
            XCTAssertEqual(m.upModel?.hasTrees, false)
            XCTAssertEqual(g.candidates.map(\.id), [.v1, .logit, .trees, .trend])
            XCTAssertEqual(g.candidates.reduce(0) { $0 + $1.chosenBlocks }, g.trainedBlocks)
            XCTAssertEqual(g.selection.count, g.blocks)
            XCTAssertEqual(ModelBot.selectionRuns(g.selection).reduce(0) { $0 + $1.count }, g.blocks)
            XCTAssertEqual(g.buy.tStat, g.buy.clustered?.byDate)
            XCTAssertGreaterThan(g.universe?.extra ?? 0, 0)
            XCTAssertNotNil(g.holdout)
            XCTAssertNotNil(g.extra)
            if let m = g.buy.meanNet, let b = g.buy.baselineNet, let e = g.buy.excess { XCTAssertLessThan(abs(m - b - e), 0.02) }
        }
        XCTAssertFalse(ModelBot.anyEdge(r))
        XCTAssertTrue(r.headline.hasPrefix("Hors échantillon, le bot n'a pas fait mieux"))
        XCTAssertEqual(ModelBot.withCandidates(r).count, 2)
        XCTAssertTrue(ModelBot.hasSubResults(r))
        XCTAssertNil(ModelBot.extraFailuresText(r))
    }

    func testV2TextsOfTheRealSample() throws {
        let r = try sample()
        let s = r.groups[0], c = r.groups[1]
        XCTAssertEqual(s.buy.verdict, .negative)
        XCTAssertEqual(n(ModelBot.buyText(s.buy)),
                       "1977 achats : +0,98 % en moyenne sur 20 jours, contre +1,36 % pour une entrée au hasard sur le même actif et la même période (−0,38 point, t par jour = −2,4).")
        XCTAssertTrue(n(ModelBot.sellText(s.sell)).hasSuffix("t par jour = −4,5)."))
        XCTAssertEqual(ModelBot.clusteredText(s.buy.clustered, s.buy.tStat), "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8")
        XCTAssertEqual(n(ModelBot.exitText(s.sell.exit) ?? ""),
                       "En sortant 20 jours à chaque VENDRE : hors marché 16 % du temps ; pire baisse médiane −47,9 % contre −46,1 % en gardant ; rendement médian +289 % contre +647 % (mieux que garder : 1 actif sur 22).")
        XCTAssertEqual(ModelBot.dataText(s),
                       "22 actifs du panier et 72 de plus à l'entraînement · historique médian 20,1 ans (le plus long 20,1 ans) · marché : S&P 500 (SPY)")
        XCTAssertEqual(ModelBot.dataText(c),
                       "12 actifs du panier et 19 de plus à l'entraînement · historique médian 8,9 ans (le plus long 15,1 ans) · marché : bitcoin")
        let runs = ModelBot.selectionRuns(s.selection)
        XCTAssertEqual(runs.map(\.chosen), [.trend, .v1, .logit, .trend, .logit, .v1, .trend, .trees, .logit, .v1, .trees])
        XCTAssertEqual(runs[0].count, 4)
        XCTAssertEqual(runs[0].to, s.selection[3].start)
        XCTAssertEqual(ModelBot.runPeriod(runs[0]), "nov. 2009 → nov. 2012 (4 fois)")
        XCTAssertEqual(ModelBot.runChoice(runs[0]), "Tendance")
        let cruns = ModelBot.selectionRuns(c.selection)
        XCTAssertNil(cruns[0].chosen)
        XCTAssertEqual(ModelBot.runChoice(cruns[0]), "aucun (trop peu de données)")
        XCTAssertEqual(ModelBot.liveModelText(s), "Aujourd'hui : Logistique 24, choisi de la même façon sur la dernière année connue.")
        let v1c = s.candidates[0]
        XCTAssertEqual(v1c.label, "Régression logistique v1")
        XCTAssertEqual(ModelBot.chosenText(v1c), "retenu 3 fois sur 17")
        XCTAssertEqual(ModelBot.candidateRows(v1c).map(\.label), ["419 achats · écart au hasard", "213 ventes · baisse évitée", "Précision hausse / baisse"])
        XCTAssertEqual(ModelBot.candidateRows(v1c).map(\.value), ["+0,81 point (t 2)", "−1,64 point (t −1,6)", "−0,3 % / −0,2 %"])
        XCTAssertEqual(ModelBot.sideVerdictLabel("Achats", v1c.buy.verdictLabel), "Achats : avantage non démontré")
        let h = try XCTUnwrap(s.holdout)
        XCTAssertEqual(ModelBot.holdoutTitle(s), "Actions et ETF américains · 12 derniers mois")
        XCTAssertEqual(ModelBot.holdoutNote(h), "Du 29/09/2025 au 28/09/2026, présentés à part (même modèle choisi ; rien n'est choisi sur cette période).")
        XCTAssertEqual(h.stats.buy.signals, 102)
        XCTAssertEqual(ModelBot.extraTitle(s), "Actions et ETF américains · actifs d'entraînement hors panier")
        XCTAssertEqual(ModelBot.extraNote(s), "72 actifs fixés d'avance, hors du test principal ; eux aussi jugés hors échantillon.")
        XCTAssertEqual(s.extra?.buy.signals, 6301)
        XCTAssertEqual(ModelBot.groupSubtitle(s), "22 actifs testés · test du nov. 2009 au sept. 2026 · 17 réentraînements")
        XCTAssertEqual(ModelBot.footer(r), "Panier fixé le 29/09/2026, univers élargi le 29/09/2026. Source : \(r.source). Calcul : 13 s de téléchargement, 35 s d'entraînement et de test.")
        let a = r.assets[0]
        XCTAssertEqual(a.years, 20.1)
        XCTAssertEqual(a.dataFrom, 1157463000000)
        XCTAssertEqual(n(ModelBot.assetTestText(a)),
                       "Test : 83 achats (−0,09 point vs hasard) · 30 ventes (cours ensuite +1,79 point vs hasard) · attente 56 % · achats cumulés +359,7 %, détention +4 597,8 % · hors marché après VENDRE 14 % du temps, pire baisse −38,4 % contre −44,8 % en gardant (Yahoo Finance (20 ans max.), 20,1 ans)")
    }

    func testV2PureHelpers() {
        XCTAssertEqual(ModelBot.clusteredText(BotClustered(byDate: -2.4, dates: 1067, byAsset: -4.46, assets: 22, perSignal: -2.84), -2.4),
                       "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8")
        XCTAssertEqual(ModelBot.clusteredText(nil, 1.25), "t = 1,3")
        XCTAssertEqual(ModelBot.clusteredText(nil, nil), "t = —")
        XCTAssertEqual(ModelBot.clusteredText(BotClustered(byDate: nil, dates: 0, byAsset: nil, assets: 0, perSignal: 1), nil),
                       "t par jour — (0 jours) · par actif — · par signal 1")
        XCTAssertNil(ModelBot.exitText(BotExitStats(assets: 0, outShare: nil, medianHoldMaxDrawdown: nil, medianBotMaxDrawdown: nil,
                                                    medianDrawdownAvoided: nil, medianHoldReturn: nil, medianBotReturn: nil, beatHold: 0)))
        XCTAssertTrue(ModelBot.exitText(BotExitStats(assets: 3, outShare: 10, medianHoldMaxDrawdown: -5, medianBotMaxDrawdown: -4,
                                                     medianDrawdownAvoided: 1, medianHoldReturn: 5, medianBotReturn: 6, beatHold: 2))?
            .hasSuffix("(mieux que garder : 2 actifs sur 3).") ?? false)
        func b(_ start: Double, _ end: Double?, _ chosen: BotCandidate?) -> BotBlockOut { BotBlockOut(start: start, end: end, trainRows: 1, chosen: chosen) }
        XCTAssertEqual(ModelBot.selectionRuns([b(1, 2, .trend), b(2, 3, .trend), b(3, nil, .v1), b(4, nil, nil)]), [
            ModelBot.SelectionRun(from: 1, to: 2, chosen: .trend, count: 2),
            ModelBot.SelectionRun(from: 3, to: 3, chosen: .v1, count: 1),
            ModelBot.SelectionRun(from: 4, to: 4, chosen: nil, count: 1),
        ])
        XCTAssertEqual(ModelBot.selectionRuns([]), [])
        XCTAssertEqual(BotCandidate.trees.short, "Arbres")
        XCTAssertEqual(BotCandidate.allCases.map(\.short), ["Logistique v1", "Logistique 24", "Arbres", "Tendance"])
        XCTAssertEqual(ModelBot.day(nil), "?")
        XCTAssertEqual(ModelBot.day(0), "01/01/1970")
        XCTAssertEqual(ModelBot.sideVerdictLabel("Ventes", "Trop peu de ventes"), "Ventes : trop peu de ventes")
        XCTAssertEqual(ModelBot.sideVerdictLabel("Ventes", ""), "Ventes : ")
    }

    func testV2UniverseFailuresAndOddFields() throws {
        let r = try JSONDecoder().decode(BotReport.self, from: Data(#"""
        {"version":2,"extraFailures":[{"symbol":"ZZ","error":"x"},{"symbol":"YY","error":"y"}],"timing":{"fetchMs":"?","computeMs":1500},
         "groups":[{"id":"crypto","label":"Cryptos","universe":{"basket":1,"extra":19,"extraFailed":2,"medianYears":null,"maxYears":3},
           "selection":[{"start":1,"chosen":"magic"},"nope"],"candidates":[{"id":"trend","label":"Règle","buy":{"signals":3}}],
           "holdout":null,"extra":{"buy":{"signals":2}},"model":{"candidate":"forest","upModel":{"trees":[[0,1,2,3,4]],"logit":null}}}]}
        """#.utf8))
        XCTAssertEqual(ModelBot.extraFailuresText(r), "Univers élargi : 2 actifs indisponibles (ZZ, YY).")
        XCTAssertEqual(r.timing?.fetchMs, 0)
        XCTAssertTrue(ModelBot.footer(r).hasSuffix(" Calcul : 0 s de téléchargement, 2 s d'entraînement et de test."))
        let g = r.groups[0]
        XCTAssertEqual(ModelBot.dataText(g), "1 actif du panier et 19 de plus à l'entraînement (2 indisponibles) · historique médian — (le plus long 3 ans)")
        XCTAssertEqual(g.selection.count, 1)
        XCTAssertNil(g.selection[0].chosen)
        XCTAssertEqual(g.candidates[0].id, .trend)
        XCTAssertEqual(g.candidates[0].buy.signals, 3)
        XCTAssertNil(g.holdout)
        XCTAssertEqual(g.extra?.buy.signals, 2)
        XCTAssertNil(g.model?.candidate)
        XCTAssertEqual(g.model?.upModel?.hasTrees, true)
        XCTAssertNil(g.model?.upModel?.logit)
        XCTAssertNil(ModelBot.liveModelText(g))
    }

    // MARK: Optional-safe decoding

    func testEmptyAndOddReportsDecode() throws {
        let r = try JSONDecoder().decode(BotReport.self, from: Data("{}".utf8))
        XCTAssertTrue(r.groups.isEmpty)
        XCTAssertEqual(r.parameters.horizonDays, 20)
        XCTAssertEqual(r.overall.buy.signals, 0)
        XCTAssertFalse(ModelBot.anyEdge(r))
        let odd = try JSONDecoder().decode(BotReport.self, from: Data(#"""
        {"asOf":"x","groups":[{"id":"crypto","label":"Cryptos","buy":{"signals":31,"verdict":"edge","tStat":2.4},"sell":7},{"id":3}],
         "assets":[{"symbol":"BTC","class":"btc","now":{"action":"hodl","up":"?"}},"nope"],"failures":[{"symbol":"ZZ","error":"historique indisponible"}]}
        """#.utf8))
        XCTAssertNil(odd.asOf)
        XCTAssertEqual(odd.groups.count, 2)
        XCTAssertEqual(odd.groups[0].buy.verdict, .edge)
        XCTAssertEqual(odd.groups[0].sell.signals, 0)
        XCTAssertTrue(ModelBot.anyEdge(odd))
        XCTAssertEqual(odd.assets.count, 1)
        XCTAssertEqual(odd.assets[0].kind, .crypto)
        XCTAssertEqual(odd.assets[0].group, .crypto)
        XCTAssertNil(odd.assets[0].now.action)
        XCTAssertNil(odd.assets[0].now.up)
        XCTAssertEqual(odd.failures.map(\.symbol), ["ZZ"])
    }

    // MARK: Today's view and the decision line

    static let viewJSON = #"""
    {"available":true,"group":"crypto","groupLabel":"Cryptos","inBasket":true,"action":"wait","actionLabel":"ATTENDRE",
     "up":46.3,"down":52,"thresholdUp":49.7,"thresholdDown":58.4,"baseUp":44.7,"baseDown":53.4,"buyVerdict":"unproven","sellVerdict":"insufficient",
     "counts":false,"time":1,"contributions":[{"id":"rsi14","label":"RSI 14","value":40,"valueText":"40","weight":-0.2,"effect":"down","text":"RSI 14 : 40 (pèse contre la hausse)"}],
     "text":"ATTENDRE : …","note":"Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.","asOf":1790681774058,"link":"/app/bot"}
    """#

    func testViewActionProbabilitiesAndWhetherItCounts() throws {
        let v = try JSONDecoder().decode(BotView.self, from: Data(Self.viewJSON.utf8))
        XCTAssertEqual(v.action, .wait)
        XCTAssertEqual(v.buyVerdict, .unproven)
        XCTAssertEqual(v.sellVerdict, .insufficient)
        XCTAssertEqual(n(ModelBot.summary(v)), "ATTENDRE · hausse 46 %, baisse 52 %")
        XCTAssertEqual(v.tone, .unproven)
        XCTAssertEqual(v.countsLabel, "ne compte pas")
        XCTAssertEqual(n(ModelBot.probabilitiesText(v)), "Probabilités à 20 jours : hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)")
        XCTAssertNil(ModelBot.outOfBasketText(v))
        XCTAssertEqual(ModelBot.contributionsText(v), "RSI 14 : 40 (pèse contre la hausse)")
        XCTAssertEqual(ModelBot.trainedText(v), " · entraîné le 29/09 à 13:36")
        XCTAssertEqual(n(ModelBot.viewText(v)), "Hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)")
        XCTAssertNil(v.model)
        XCTAssertNil(ModelBot.modelSuffix(v))

        var out = v
        out.inBasket = false
        out.counts = true
        XCTAssertEqual(ModelBot.outOfBasketText(out), " · modèle des cryptos, non testé sur cet actif")
        XCTAssertTrue(ModelBot.viewText(out).hasSuffix(" · hors du panier testé"))
        XCTAssertEqual(out.tone, .edge)
        XCTAssertEqual(out.countsLabel, "compte")

        var na = v
        na.available = false
        na.action = nil
        na.contributions = []
        na.text = "Bot pas encore entraîné"
        XCTAssertEqual(ModelBot.summary(na), "Bot pas encore entraîné")
        XCTAssertEqual(ModelBot.viewText(na), "Bot pas encore entraîné")
        XCTAssertFalse(na.hasAction)
        XCTAssertEqual(na.tone, .na)
        XCTAssertNil(ModelBot.contributionsText(na))

        // Encodes back (the decision is Codable) and a bare block keeps the backend's defaults.
        let again = try JSONDecoder().decode(BotView.self, from: JSONEncoder().encode(v))
        XCTAssertEqual(again, v)
        let bare = try JSONDecoder().decode(BotView.self, from: Data(#"{"action":"short","up":"x"}"#.utf8))
        XCTAssertFalse(bare.available)
        XCTAssertNil(bare.action)
        XCTAssertNil(bare.up)
        XCTAssertEqual(bare.group, .stock)
        XCTAssertEqual(bare.tone, .na)
    }

    func testV2ViewCarriesItsModel() throws {
        let json = Self.viewJSON.replacingOccurrences(of: "\"available\":true", with: "\"available\":true,\"model\":\"logit\",\"modelLabel\":\"Régression logistique 24 mesures\"")
        var v = try JSONDecoder().decode(BotView.self, from: Data(json.utf8))
        XCTAssertEqual(v.model, .logit)
        XCTAssertEqual(v.modelLabel, "Régression logistique 24 mesures")
        XCTAssertEqual(ModelBot.modelSuffix(v), " · Régression logistique 24 mesures")
        XCTAssertEqual(n(ModelBot.viewText(v)), "Hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %) · Régression logistique 24 mesures")
        let again = try JSONDecoder().decode(BotView.self, from: JSONEncoder().encode(v))
        XCTAssertEqual(again, v)
        v.inBasket = false
        XCTAssertTrue(ModelBot.viewText(v).hasSuffix(" · Régression logistique 24 mesures · hors du panier testé"))
        let odd = try JSONDecoder().decode(BotView.self, from: Data(#"{"model":"forest","modelLabel":7}"#.utf8))
        XCTAssertNil(odd.model)
        XCTAssertNil(odd.modelLabel)
    }

    func testViewsAnswerDecodes() throws {
        let json = #"{"asOf":null,"views":[\#(Self.viewJSON.replacingOccurrences(of: "\"available\":true", with: "\"symbol\":\"BTC\",\"kind\":\"crypto\",\"available\":true")),{"symbol":"AAPL","kind":"stock","available":false,"text":"Bot pas encore entraîné","counts":false}]}"#
        let v = try JSONDecoder().decode(BotViews.self, from: Data(json.utf8))
        XCTAssertNil(v.asOf)
        XCTAssertEqual(v.views.map(\.symbol), ["BTC", "AAPL"])
        XCTAssertEqual(v.views.map(\.kind), [.crypto, .stock])
        XCTAssertEqual(v.views[1].text, "Bot pas encore entraîné")
    }

    func testDecisionCarriesTheBotOrNot() throws {
        let plain = try JSONDecoder().decode(Decision.self, from: raw("decision-evidence"))
        XCTAssertNil(plain.bot)
        var o = try XCTUnwrap(JSONSerialization.jsonObject(with: raw("decision-evidence")) as? [String: Any])
        o["bot"] = try JSONSerialization.jsonObject(with: Data(Self.viewJSON.utf8))
        let d = try JSONDecoder().decode(Decision.self, from: JSONSerialization.data(withJSONObject: o))
        let b = try XCTUnwrap(d.bot)
        XCTAssertEqual(b.action, .wait)
        XCTAssertFalse(b.counts)
        XCTAssertTrue(b.note.contains("il ne compte pas dans la décision"))
        XCTAssertEqual(b.link, "/app/bot")
    }
}
