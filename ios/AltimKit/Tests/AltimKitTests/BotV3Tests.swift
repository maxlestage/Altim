import Foundation
import XCTest
@testable import AltimKit

/// « Bot Altim » v3: same cases as web/test/bot-v3.test.ts on the real /api/bot answer (backend/tests/samples/bot.json,
/// copied as Fixtures/bot.json), the v2 and v1 answers still read without v3, the texts, the radar card's helpers and the
/// `v3` of a view (decision line).
final class BotV3Tests: XCTestCase {
    func raw(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func report(_ name: String = "bot") throws -> BotReport { try JSONDecoder().decode(BotReport.self, from: raw(name)) }

    /// Narrow no-break spaces (before %) read as plain spaces in the expectations.
    func n(_ s: String) -> String { s.replacingOccurrences(of: "\u{202F}", with: " ") }

    // MARK: Contract sample (real v3 answer)

    func testPreRegistrationThresholdGroupsHorizonsConfigs() throws {
        let r = try report()
        let v3 = try XCTUnwrap(r.v3)
        XCTAssertEqual(r.version, 3)
        XCTAssertEqual(v3.version, 3)
        XCTAssertEqual(v3.preregDate, "2026-09-30")
        XCTAssertEqual([v3.k.v1, v3.k.v2, v3.k.v3, v3.k.total], [4, 20, 90, 114])
        XCTAssertLessThan(abs(v3.tRequired - 3.5157), 0.001)
        XCTAssertEqual(v3.alpha, 0.05)
        XCTAssertEqual(v3.forwardFrom, 1790812800000)
        XCTAssertEqual(v3.parameters.horizons, [20, 60])
        XCTAssertEqual(v3.parameters.minSignals, 30)
        XCTAssertEqual(v3.parameters.nudge, 3)
        XCTAssertEqual(v3.compute.blocks, 200)
        XCTAssertEqual(v3.compute.maxRows, 719748)
        XCTAssertEqual(v3.groups.map(\.id), [.stock, .crypto])
        XCTAssertEqual(v3.candidates.count, 8)
        XCTAssertEqual(v3.limits.count, 8)
        for g in v3.groups {
            XCTAssertEqual(g.horizons.map(\.horizon), [20, 60])
            for h in g.horizons {
                XCTAssertEqual(h.configs.map(\.id), ["absolute", "peers", "v2", "trend", "v1", "logit", "trees", "treesLong", "xsMomentum", "xsLogit", "xsTrees"])
                XCTAssertEqual(h.selection.count, h.blocks)
                XCTAssertEqual(ModelBot.choiceRuns(h.selection, family: .absolute).reduce(0) { $0 + $1.count }, h.blocks)
                for c in h.configs {
                    // Family B also judged against the group's mean (control listed as added after the pre-registration).
                    XCTAssertEqual(c.main.buyVsMean != nil && c.main.sellVsMean != nil, c.family == .peers)
                    if ModelBot.proven(c.main, .buy) { XCTAssertGreaterThanOrEqual(ModelBot.judged(c.main, .buy).t ?? 0, v3.tRequired) }
                    for s in [c.main.buy, c.main.sell] {
                        if s.verdict == .edge { XCTAssertGreaterThanOrEqual(s.t ?? 0, v3.tRequired) }
                        if s.signals < 30 { XCTAssertEqual(s.verdict, .insufficient) }
                    }
                }
            }
            XCTAssertEqual(ModelBot.headlineConfigs(g).count, 4)
        }
        XCTAssertEqual(r.headline, v3.headline)
        XCTAssertEqual(v3.afterPrereg.count, 2)
        XCTAssertTrue(v3.afterPrereg[0].contains("moyenne à parts égales"))
        XCTAssertTrue(v3.afterPrereg[1].contains("au moins 30 signaux"))
        // The one side proven on the past (stocks, peers, 20 days, buys) waits for the forward test: shown, not counted.
        let peers20 = try XCTUnwrap(v3.groups[0].horizons[0].configs.first { $0.id == "peers" })
        XCTAssertTrue(ModelBot.proven(peers20.main, .buy))
        XCTAssertFalse(ModelBot.confirmed(peers20, .buy))
        XCTAssertEqual(n(ModelBot.pendingNote(peers20, .buy, v3.tRequired) ?? ""),
                       "Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (0 à ce jour).")
        XCTAssertNil(ModelBot.pendingNote(peers20, .sell, v3.tRequired))
        XCTAssertTrue(v3.headline.contains("mais fragile"))
        XCTAssertTrue(v3.headline.contains("le bot ne pèse pas dans les décisions"))
        XCTAssertTrue(v3.headline.hasPrefix("Bot v3 : "))
        XCTAssertEqual(v3.headline.contains("aucun avantage démontré"), !ModelBot.v3AnyEdge(v3))
        XCTAssertTrue(ModelBot.v3AnyEdge(v3))
        XCTAssertEqual(ModelBot.forwardSignals(v3), 0)
        // Today's actions of the basket assets under the 4 headline configurations.
        XCTAssertTrue(r.assets.allSatisfy { $0.v3 != nil })
        let a = try XCTUnwrap(r.assets[0].v3)
        XCTAssertEqual([a.absolute20, a.absolute60, a.peers20, a.peers60], [.wait, .wait, .wait, .wait])
        XCTAssertEqual(r.timing?.peakRssMb, 300.7)
        XCTAssertEqual(r.timing?.rssBeforeMb, 87.9)
    }

    func testRealGroupFiguresAndScreenTexts() throws {
        let r = try report()
        let v3 = try XCTUnwrap(r.v3)
        let g = v3.groups[0]
        XCTAssertEqual(g.label, "Actions et ETF américains")
        XCTAssertEqual(g.market, "S&P 500 (SPY)")
        XCTAssertEqual(g.universe.basket, 22)
        XCTAssertEqual(g.universe.extra, 72)
        XCTAssertEqual(ModelBot.v3GroupSubtitle(g), "22 actifs testés, 72 de plus à l'entraînement · historique médian 36,7 ans · classement entre pairs possible depuis nov. 1993")
        let h = g.horizons[0]
        XCTAssertEqual(ModelBot.horizonTitle(h), "À 20 jours")
        XCTAssertEqual(h.blocks, 31)
        XCTAssertEqual(h.roundsUp?.fits, 30)
        XCTAssertEqual(h.roundsPeers?.median, 25)
        XCTAssertEqual(h.selection[0].v2, .trend)
        XCTAssertEqual(h.selection[0].absolute, .logit)
        XCTAssertEqual(h.selection[0].peers, .xsMomentum)
        XCTAssertEqual(h.selection[0].absoluteScores.map(\.id), [.trend, .v1, .logit, .trees, .treesLong])
        XCTAssertNil(h.selection[0].absoluteScores[4].score)
        let peers = try XCTUnwrap(h.configs.first { $0.id == "peers" })
        XCTAssertEqual(peers.label, "Classement entre pairs (choix v3)")
        XCTAssertTrue(peers.headline)
        XCTAssertTrue(peers.nested)
        XCTAssertNil(peers.candidate)
        XCTAssertNil(ModelBot.v3ChosenText(peers))
        XCTAssertEqual(peers.main.buy.signals, 2659)
        XCTAssertEqual(peers.main.buyVsMean?.t, 3.65)
        XCTAssertEqual(peers.main.sellVsMean?.verdict, .unproven)
        XCTAssertEqual(peers.main.sellVsMean?.rawVerdict, .negative)
        XCTAssertEqual(ModelBot.sideHead(peers.main, .buy), "ACHETER · face à la médiane")
        XCTAssertEqual(n(ModelBot.v3SideText(.peers, .buy, peers.main.buy, v3.tRequired)),
                       "2659 achats : +0,92 point face à la médiane du groupe (t par jour 4,9 (requis 3,52) · par actif 1,5).")
        XCTAssertEqual(ModelBot.controlText(try XCTUnwrap(peers.main.buyVsMean), v3.tRequired),
                       "Face à la moyenne du groupe (contrôle ajouté après coup) : +0,67 point (t par jour 3,7 (requis 3,52) · par actif 0,6)")
        XCTAssertEqual(ModelBot.controlVerdict(ModelBot.proven(peers.main, .buy)), "Sur le passé : avantage face aux deux références.")
        XCTAssertEqual(ModelBot.controlVerdict(false), "Retenu : non démontré (il faut les deux références) ; ne compte pas.")
        XCTAssertEqual(ModelBot.v3VerdictLabel(peers.main.buy, v3.tRequired), "Avantage au seuil corrigé (t ≥ 3,52), à confirmer")
        XCTAssertEqual(ModelBot.v3VerdictLabel(peers.main.sell, v3.tRequired), "Pire que la référence au seuil corrigé")
        XCTAssertEqual(ModelBot.configRows(peers, v3.tRequired).map(\.label), ["2659 achats", "2267 ventes", "Face à la moyenne"])
        XCTAssertEqual(ModelBot.configRows(peers, v3.tRequired)[2].value, "+0,67 point / −0,32 point (t 3,7 / −2,3)")
        let trend = try XCTUnwrap(h.configs.first { $0.id == "trend" })
        XCTAssertEqual(trend.candidate, .trend)
        XCTAssertEqual(ModelBot.v3ChosenText(trend), "retenu \(trend.chosenBlocks) fois sur \(trend.trainedBlocks)")
        XCTAssertEqual(ModelBot.configRows(trend, v3.tRequired).count, 2)
        XCTAssertEqual(ModelBot.forwardText(peers.forward, v3.tRequired), "Aucun signal jugé pour l'instant (il faut 20 à 60 jours de bourse après le signal).")
        let vol = try XCTUnwrap(g.volManaged)
        XCTAssertEqual(ModelBot.volRows(vol).map(\.label), ["Ratio de Sharpe", "Pire baisse", "Rendement annuel", "Volatilité annuelle"])
        XCTAssertEqual(n(ModelBot.volRows(vol)[1].managed), "−15,8 %")
        XCTAssertEqual(ModelBot.volRows(vol)[0].hold, "0,87")
        XCTAssertEqual(vol.forwardHold.days, 0)
        XCTAssertTrue(vol.text.hasPrefix("Portefeuille à parts égales"))

        XCTAssertEqual(ModelBot.v3Title(v3), "Bot v3 · pré-enregistré le 30/09/2026")
        XCTAssertEqual(ModelBot.forwardTitle(v3), "Depuis le 30/09/2026 (test sur l'avenir)")
        XCTAssertEqual(ModelBot.v3ProtocolText(v3),
                       "Protocole écrit avant tout calcul et figé : 4 tests en v1, 20 en v2, 90 en v3. Avec autant d'essais, un t de 2 arrive par hasard ; un avantage n'est dit démontré qu'au-delà de t = 3,52 (Bonferroni, 5 %). Horizons jugés à part : 20 et 60 jours de bourse.")
        XCTAssertTrue(ModelBot.forwardIntro(v3).contains("(après au moins 30 signaux)"))
        XCTAssertEqual(ModelBot.candidatesCount(v3), "Les 8 candidats")
        XCTAssertEqual(ModelBot.v3ComputeLine(v3, r.timing),
                       "Calcul : 9 min 43 s de calcul sur 1 cœur, pic mémoire 301 Mo (téléchargement 13 s) ; 200 réentraînements, jusqu'à 719\u{202F}748 jours × actifs par groupe et horizon.")
        XCTAssertTrue(ModelBot.referenceText(v3).hasPrefix("Le modèle de la v2 (choisi par log-loss) refait sur les nouvelles données, jugé au seuil corrigé"))
        XCTAssertTrue(ModelBot.referenceText(v3).contains("(t ≥ 3,52)"))
        XCTAssertEqual(ModelBot.resultsTitle(r), "Sélection v2 : résultats hors échantillon")
        let fr = ModelBot.headlineConfigs(g)[1]
        XCTAssertEqual(ModelBot.forwardRowTitle(g, fr), "Actions et ETF américains · Classement entre pairs · 20 jours")
        // With v3 the v2 timing moves to the v3 section.
        XCTAssertFalse(ModelBot.footer(r).contains("Calcul"))
    }

    func testV2AndV1AnswersStillReadWithoutV3() throws {
        let v2 = try report("bot-v2")
        XCTAssertEqual(v2.version, 2)
        XCTAssertNil(v2.v3)
        XCTAssertNil(v2.assets[0].v3)
        XCTAssertNil(v2.timing?.peakRssMb)
        XCTAssertEqual(ModelBot.resultsTitle(v2), "Résultats hors échantillon")
        XCTAssertNil(ModelBot.radarForward(v2))
        XCTAssertEqual(ModelBot.radarHeadline(v2), ModelBot.firstSentence(v2.headline))
        XCTAssertTrue(ModelBot.footer(v2).contains("Calcul"))
        let v1 = try report("bot-v1")
        XCTAssertNil(v1.version)
        XCTAssertNil(v1.v3)
        XCTAssertNil(v1.assets[0].v3)
    }

    // MARK: Texts (web/test/bot-v3.test.ts « v3 texts »)

    func side(signals: Int = 120, excess: Double? = 0.2, t: Double? = 1.24, tByAsset: Double? = 0.9,
              verdict: ValidationVerdict = .unproven, rawVerdict: ValidationVerdict = .unproven) -> BotV3SideStats {
        BotV3SideStats(signals: signals, mean: 1, baseline: 0.8, excess: excess, t: t, dates: 100, tByAsset: tByAsset, assets: 20, tPerSignal: 1.5,
                       beatShare: 52, verdict: verdict, rawVerdict: rawVerdict)
    }

    func testRawVsRequiredTVerdictsSides() {
        XCTAssertEqual(ModelBot.tVsRequired(1.24, 3.5157), "t = 1,2 (requis 3,52)")
        XCTAssertEqual(ModelBot.tVsRequired(nil, 3.5157), "t = — (requis 3,52)")
        XCTAssertEqual(ModelBot.v3VerdictLabel(side(), 3.52), "Non démontré")
        XCTAssertEqual(ModelBot.v3VerdictLabel(side(t: 2.4, rawVerdict: .edge), 3.52), "t ≥ 2 atteint, pas le seuil corrigé : non démontré")
        XCTAssertEqual(ModelBot.v3VerdictLabel(side(t: 4, verdict: .edge), 3.5157), "Avantage au seuil corrigé (t ≥ 3,52), à confirmer")
        XCTAssertEqual(ModelBot.v3VerdictLabel(side(signals: 3, verdict: .insufficient), 3.52), "Trop peu de signaux pour conclure")
        XCTAssertEqual(n(ModelBot.v3SideText(.peers, .buy, side(), 3.5157)),
                       "120 achats : +0,2 point face à la médiane du groupe (t par jour 1,2 (requis 3,52) · par actif 0,9).")
        XCTAssertEqual(n(ModelBot.v3SideText(.absolute, .sell, side(excess: -2.5, t: -4.51, tByAsset: nil), 3.5157)),
                       "120 ventes : baisse évitée −2,5 points (t par jour −4,5 (requis 3,52) · par actif —).")
        XCTAssertEqual(ModelBot.v3SideText(.peers, .sell, side(excess: 1), 3.52).prefix(47), "120 ventes : gain à passer sur l'actif médian +")
        XCTAssertEqual(ModelBot.tLine(side(t: 3.65, tByAsset: 0.62), 3.5157), "t par jour 3,7 (requis 3,52) · par actif 0,6")
        XCTAssertEqual(ModelBot.v3SideText(.peers, .sell, side(signals: 0), 3.5), "Aucune vente.")
        XCTAssertEqual(ModelBot.v3SideText(.absolute, .buy, side(signals: 0), 3.5), "Aucun achat.")
        XCTAssertEqual(ModelBot.v3SideText(.absolute, .buy, side(signals: 1), 3.52).prefix(9), "1 achat :")
    }

    func testForwardComputeDatesRuns() {
        let empty = BotV3ConfigStats(buy: side(signals: 0), sell: side(signals: 0))
        XCTAssertTrue(ModelBot.forwardText(empty, 3.5).hasPrefix("Aucun signal jugé pour l'instant"))
        let some = BotV3ConfigStats(buy: side(signals: 31, excess: 0.4, t: 1.26), sell: side(signals: 1, excess: -2.5))
        XCTAssertEqual(ModelBot.forwardText(some, 3.5157), "31 achats (+0,4 point, t = 1,3 (requis 3,52)) · 1 vente (−2,5 points).")
        XCTAssertEqual(ModelBot.frIso("2026-09-30"), "30/09/2026")
        XCTAssertEqual(ModelBot.computeText(BotTiming(fetchMs: 15_400, computeMs: 432_000, threads: 1, peakRssMb: 243.4)),
                       "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)")
        XCTAssertEqual(ModelBot.computeText(BotTiming(fetchMs: 2_000, computeMs: 35_000, threads: 4)), "35 s de calcul sur 4 cœurs (téléchargement 2 s)")
        XCTAssertNil(ModelBot.computeText(nil))
        let runs = ModelBot.choiceRuns([BotV3BlockOut(start: 1, absolute: .trend, peers: .xsLogit), BotV3BlockOut(start: 2, absolute: .trend, peers: .xsTrees),
                                        BotV3BlockOut(start: 3, absolute: nil, peers: .xsTrees)], family: .absolute)
        XCTAssertEqual(runs, [ModelBot.ChoiceRun(from: 1, to: 2, chosen: .trend, count: 2), ModelBot.ChoiceRun(from: 3, to: 3, chosen: nil, count: 1)])
        XCTAssertEqual(ModelBot.choiceRunsText(runs), "Tendance (2 fois) → aucun")
        XCTAssertEqual(ModelBot.choiceRunsText(ModelBot.choiceRuns([BotV3BlockOut(start: 1, peers: .xsTrees)], family: .peers)), "Arbres longs entre pairs")
        XCTAssertEqual(BotV3Candidate.allCases.map(\.short),
                       ["Tendance", "Logistique v1", "Logistique 24", "Arbres v2", "Arbres longs", "Momentum entre pairs", "Logistique entre pairs", "Arbres longs entre pairs"])
        XCTAssertEqual(BotFamily.allCases.map(\.label), ["Hausse ou baisse de l'actif", "Classement entre pairs", "Sélection v2 (log-loss)"])
        XCTAssertEqual(ModelBot.signalsCount(0), "0 signal")
        XCTAssertEqual(ModelBot.signalsCount(2), "2 signaux")
    }

    func testProvenConfirmedPending() {
        let edge = side(t: 4, verdict: .edge, rawVerdict: .edge)
        // Family A: proven on the median-free side alone; pending until 30 forward signals with a non-negative excess.
        var c = BotV3Config(id: "absolute", family: .absolute, main: BotV3ConfigStats(buy: edge, sell: side()))
        XCTAssertTrue(ModelBot.proven(c.main, .buy))
        XCTAssertFalse(ModelBot.proven(c.main, .sell))
        XCTAssertFalse(ModelBot.confirmed(c, .buy))
        XCTAssertEqual(n(ModelBot.pendingNote(c, .buy, 3.5157) ?? ""),
                       "Avantage mesuré sur le passé (t = 4 contre 3,52 exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (0 à ce jour).")
        c.forward = BotV3ConfigStats(buy: side(signals: 30, excess: 0), sell: side(signals: 0))
        XCTAssertTrue(ModelBot.confirmed(c, .buy))
        XCTAssertNil(ModelBot.pendingNote(c, .buy, 3.52))
        c.forward.buy.excess = -0.1
        XCTAssertFalse(ModelBot.confirmed(c, .buy))
        XCTAssertNotNil(ModelBot.pendingNote(c, .buy, 3.52))
        // Family B: the control against the mean must be proven too, and confirmed on the forward test.
        var b = BotV3Config(id: "peers", family: .peers, main: BotV3ConfigStats(buy: edge, sell: side(), buyVsMean: side(t: 2.2), sellVsMean: side()))
        XCTAssertFalse(ModelBot.proven(b.main, .buy))
        XCTAssertEqual(ModelBot.judged(b.main, .buy).t, 2.2)
        XCTAssertNil(ModelBot.pendingNote(b, .buy, 3.52))
        b.main.buyVsMean = side(t: 3.65, verdict: .edge)
        XCTAssertTrue(ModelBot.proven(b.main, .buy))
        b.forward = BotV3ConfigStats(buy: side(signals: 40, excess: 1), buyVsMean: side(signals: 40, excess: -0.2))
        XCTAssertFalse(ModelBot.confirmed(b, .buy))
        XCTAssertTrue(n(ModelBot.pendingNote(b, .buy, 3.5157) ?? "").hasPrefix("Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé)"))
        XCTAssertTrue(ModelBot.pendingNote(b, .buy, 3.5157)?.hasSuffix("(40 à ce jour).") ?? false)
        b.forward.buyVsMean?.excess = 0.3
        XCTAssertTrue(ModelBot.confirmed(b, .buy))
    }

    // MARK: Radar card (BotRadarCard.tsx)

    func testRadarFirstSentenceAndForwardCounter() throws {
        let r = try report()
        let v3 = try XCTUnwrap(r.v3)
        XCTAssertEqual(ModelBot.radarHeadline(r), ModelBot.firstSentence(v3.headline))
        XCTAssertTrue(ModelBot.radarHeadline(r).hasSuffix("le bot ne pèse pas dans les décisions."))
        XCTAssertTrue(ModelBot.radarHeadline(r).hasPrefix("Bot v3 : "))
        XCTAssertEqual(ModelBot.radarForward(r), "Test sur l'avenir : 0 signal sur 30")
        XCTAssertEqual(ModelBot.firstSentence("A. B. C"), "A.")
        XCTAssertEqual(ModelBot.firstSentence("Sans point"), "Sans point")
        XCTAssertEqual(ModelBot.firstSentence(""), "")
        XCTAssertEqual(ModelBot.radarTitle, "Bot Altim")
        XCTAssertEqual(ModelBot.radarError, "Résultats indisponibles pour le moment.")
        XCTAssertTrue(ModelBot.radarPending.hasPrefix("Entraînement et test en cours"))
        XCTAssertEqual(ModelBot.radarLink, "Voir le bot →")
    }

    // MARK: Odd answers and today's view

    func testOddV3Decodes() throws {
        let r = try JSONDecoder().decode(BotReport.self, from: Data(#"""
        {"version":3,"v3":{"k":{"total":"x"},"tRequired":null,"groups":[{"id":"crypto","horizons":[{"horizon":60,"configs":[{"id":"peers","family":"magic","candidate":"forest",
          "headline":true,"main":{"buy":{"signals":"n","verdict":"edge"},"buyVsMean":7},"forward":null}],"selection":[{"start":1,"peers":"xsTrees"},"nope"]}],
          "universe":null,"volManaged":{"hold":null}},"nope"],"parameters":{"horizons":[]}},
         "assets":[{"symbol":"BTC","class":"btc","v3":{"absolute20":"hodl","peers60":"buy"}}]}
        """#.utf8))
        let v3 = try XCTUnwrap(r.v3)
        XCTAssertEqual(v3.version, 3)
        XCTAssertEqual(v3.k.total, 0)
        XCTAssertEqual(v3.tRequired, 0)
        XCTAssertEqual(v3.parameters.horizons, [20, 60])
        XCTAssertEqual(v3.parameters.minSignals, 30)
        XCTAssertEqual(v3.groups.count, 1)
        let g = v3.groups[0]
        XCTAssertEqual(g.id, .crypto)
        XCTAssertEqual(g.universe.basket, 0)
        XCTAssertEqual(g.volManaged?.hold.days, 0)
        let c = g.horizons[0].configs[0]
        XCTAssertEqual(c.family, .absolute)
        XCTAssertNil(c.candidate)
        XCTAssertEqual(c.main.buy.signals, 0)
        XCTAssertNil(c.main.buyVsMean)
        XCTAssertEqual(c.forward.buy.signals, 0)
        XCTAssertEqual(g.horizons[0].selection.count, 1)
        XCTAssertEqual(ModelBot.forwardSignals(v3), 0)
        XCTAssertNil(r.assets[0].v3?.absolute20)
        XCTAssertEqual(r.assets[0].v3?.peers60, .buy)
        XCTAssertNil(ModelBot.v3ComputeLine(v3, nil))
    }

    static let v3ViewJSON = #"""
    {"available":true,"group":"stock","groupLabel":"Actions","inBasket":true,"action":"wait","up":50,"down":40,"counts":false,"text":"…",
     "note":"Ancienne note","link":"/app/bot",
     "v3":{"available":true,"counts":false,"nudge":0,"tRequired":3.5157,
       "note":"Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (0 à ce jour).",
       "pro":null,"con":null,"conHeld":null,
       "signals":[{"family":"absolute","horizon":20,"candidate":"logit","action":"wait","buyVerdict":"unproven","sellVerdict":"negative","forwardBuySignals":0,
                   "forwardSellSignals":0,"contradicted":false,"counts":false,"pending":false,"text":"Hausse ou baisse de l'actif à 20 jours : ATTENDRE"},
                  {"family":"peers","horizon":20,"candidate":"xsMomentum","action":"buy","buyVerdict":"edge","sellVerdict":"negative","forwardBuySignals":0,
                   "forwardSellSignals":0,"contradicted":false,"counts":false,"pending":true,"text":"Classement entre pairs à 20 jours : parmi les 20 % les mieux classés de son groupe"},
                  {"family":"magic","horizon":"x","action":"hold"}]}}
    """#

    func testViewCarriesV3AndTheDecisionLineUsesIt() throws {
        var v = try JSONDecoder().decode(BotView.self, from: Data(Self.v3ViewJSON.utf8))
        let x = try XCTUnwrap(v.v3)
        XCTAssertTrue(x.available)
        XCTAssertEqual(x.signals.count, 3)
        XCTAssertEqual(x.signals[1].family, .peers)
        XCTAssertEqual(x.signals[1].candidate, .xsMomentum)
        XCTAssertEqual(x.signals[1].buyVerdict, .edge)
        XCTAssertTrue(x.signals[1].pending)
        XCTAssertEqual(x.signals[2].family, .absolute)
        XCTAssertEqual(x.signals[2].horizon, 0)
        XCTAssertNil(x.signals[2].action)
        XCTAssertEqual(x.signals.map(ModelBot.v3SignalLabel), ["hausse/baisse 20 j", "entre pairs 20 j", "hausse/baisse 0 j"])
        XCTAssertTrue(v.noteNow.hasPrefix("Avantage mesuré sur le passé"))
        XCTAssertFalse(v.countsNow)
        XCTAssertEqual(v.countsLabel, "ne compte pas")
        XCTAssertEqual(v.tone, .unproven)
        // v3's counts wins over the view's own.
        v.counts = true
        XCTAssertFalse(v.countsNow)
        XCTAssertEqual(v.tone, .unproven)
        v.v3?.counts = true
        XCTAssertEqual(v.countsLabel, "compte")
        XCTAssertEqual(v.tone, .edge)
        v.v3?.note = ""
        XCTAssertEqual(v.noteNow, "Ancienne note")
        v.v3 = nil
        XCTAssertTrue(v.countsNow)
        XCTAssertEqual(v.noteNow, "Ancienne note")
        // Encodes back (the decision is cached as JSON).
        let again = try JSONDecoder().decode(BotView.self, from: JSONEncoder().encode(try JSONDecoder().decode(BotView.self, from: Data(Self.v3ViewJSON.utf8))))
        XCTAssertEqual(again.v3?.signals.count, 3)
        XCTAssertEqual(again.v3?.note, x.note)
        XCTAssertEqual(again.v3?.tRequired, 3.5157)
    }

    func testDecisionWithAV3Bot() throws {
        var o = try XCTUnwrap(JSONSerialization.jsonObject(with: raw("decision-evidence")) as? [String: Any])
        o["bot"] = try JSONSerialization.jsonObject(with: Data(Self.v3ViewJSON.utf8))
        let d = try JSONDecoder().decode(Decision.self, from: JSONSerialization.data(withJSONObject: o))
        let b = try XCTUnwrap(d.bot)
        XCTAssertEqual(b.v3?.signals.count, 3)
        XCTAssertTrue(b.noteNow.contains("mais fragile"))
        let plain = try JSONDecoder().decode(BotView.self, from: Data(BotTests.viewJSON.utf8))
        XCTAssertNil(plain.v3)
        XCTAssertEqual(plain.noteNow, plain.note)
    }
}
