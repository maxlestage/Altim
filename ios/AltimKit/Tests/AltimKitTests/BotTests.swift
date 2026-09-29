import Foundation
import XCTest
@testable import AltimKit

/// « Bot Altim »: same cases as web/test/bot.test.ts on the real /api/bot answer (backend/tests/samples/bot.json, copied
/// as Fixtures/bot.json), plus the optional-safe decoding, /api/bot/views and the `bot` field of a decision.
final class BotTests: XCTestCase {
    func raw(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func sample() throws -> BotReport { try JSONDecoder().decode(BotReport.self, from: raw("bot")) }

    /// Narrow no-break spaces (before %) read as plain spaces in the expectations.
    func n(_ s: String) -> String { s.replacingOccurrences(of: "\u{202F}", with: " ") }

    // MARK: Contract sample

    func testGroupsAssetsAndSignalsAddUp() throws {
        let r = try sample()
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
        let r = try sample()
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
        let r = try sample()
        let s = r.groups[0], c = r.groups[1]
        XCTAssertEqual(n(ModelBot.buyText(s.buy)),
                       "108 achats : +1,7 % en moyenne sur 20 jours, contre +1,85 % pour une entrée au hasard sur le même actif et la même période (−0,15 point, t = −0,1).")
        XCTAssertEqual(n(ModelBot.sellText(c.sell)),
                       "7 ventes : le cours a fait −1,01 % dans les 20 jours suivants, contre −3,45 % après un jour au hasard (cours ensuite supérieur de 2,44 points, t = −0,7).")
        XCTAssertEqual(n(ModelBot.waitText(s.wait)), "ATTENDRE 85 % des jours testés, suivis en moyenne de +0,98 % (tous les jours : +1,39 %).")
        XCTAssertEqual(ModelBot.groupSubtitle(s), "22 actifs · test du oct. 2024 au sept. 2026 · 4 réentraînements")
        XCTAssertEqual(ModelBot.calibrationTitle(s, up: false), "Actions et ETF américains · baisse")
        XCTAssertEqual(ModelBot.calibrationRows(s.stats.calibrationUp).map(\.label), ["40 à 50 %", "50 à 60 %", "60 à 70 %", "70 % et plus"])
        XCTAssertEqual(n(ModelBot.precisionText(s.stats.brierSkillUp)), "Précision : 1,1 % moins bien que la fréquence de base.")
        XCTAssertEqual(n(ModelBot.buyRows(s.buy)[0].value), "46 % / 55 %")
        XCTAssertEqual(ModelBot.assetsTile(r), "34 / 34")
        XCTAssertEqual(ModelBot.signalsTile(r), "207 / 54")
        XCTAssertEqual(ModelBot.parametersText(r), "Horizon 20 jours · seuil : fréquence d'entraînement + 5 points · coûts d'un aller-retour 0,31 % (actions), 0,32 % (cryptos).")
        XCTAssertTrue(ModelBot.footer(r).hasPrefix("Panier fixé le 29/09/2026. Source : "))
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
        var b = try sample().groups[0].buy
        b.signals = 0
        XCTAssertEqual(ModelBot.buyText(b), "Aucun achat pendant les périodes de test.")
        XCTAssertEqual(ModelBot.waitText(BotWaitStats(days: 0, share: nil, meanNet: nil, baselineNet: nil)), "Jamais sur ATTENDRE pendant les tests.")
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
