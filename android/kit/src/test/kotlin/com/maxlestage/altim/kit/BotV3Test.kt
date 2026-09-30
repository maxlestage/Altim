package com.maxlestage.altim.kit

import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

// Same cases and expected values as web/test/bot-v3.test.ts, on the real /api/bot answers: v3 (backend/tests/samples/bot.json),
// v2 (bot-v2.json) and v1 (bot-v1.json), all three decoded by the same contract.

private const val NB = " "

class BotV3Test {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()
    private fun decode(name: String) = AltimJson.decodeFromString(BotReport.serializer(), raw(name))
    private val report = decode("bot.json")
    private val v3 = report.v3!!

    @Test fun allThreeSamplesDecode() {
        val v2 = decode("bot-v2.json")
        val v1 = decode("bot-v1.json")
        assertEquals(3, report.version)
        assertEquals(2, v2.version)
        assertNull(v1.version)
        assertNull(v2.v3)
        assertNull(v1.v3)
        assertTrue(v2.assets.all { it.v3 == null })
        assertEquals(34, report.assets.size + report.failures.size)
        assertEquals(34, v2.assets.size + v2.failures.size)
        assertEquals(34, v1.assets.size + v1.failures.size)
        // v3 timing: memory before and at its peak.
        assertEquals(1, report.timing?.threads)
        assertEquals(87.9, report.timing?.rssBeforeMb)
        assertEquals(300.7, report.timing?.peakRssMb)
        assertNull(v2.timing?.peakRssMb)
    }

    @Test fun preRegistrationThresholdGroupsHorizonsConfigurations() {
        assertEquals("2026-09-30", v3.preregDate)
        assertEquals(BotV3K(4, 20, 90, 114), v3.k)
        assertTrue(abs(v3.tRequired - 3.5157) < 0.001)
        assertEquals(listOf(20, 60), v3.parameters.horizons)
        assertEquals(listOf("stock", "crypto"), v3.groups.map { it.id })
        for (g in v3.groups) {
            assertEquals(listOf(20, 60), g.horizons.map { it.horizon })
            for (h in g.horizons) {
                assertEquals(listOf("absolute", "peers", "v2", "trend", "v1", "logit", "trees", "treesLong", "xsMomentum", "xsLogit", "xsTrees"), h.configs.map { it.id })
                assertEquals(h.blocks, h.selection.size)
                for (c in h.configs) {
                    // Family « peers » also judged against the group's mean (control listed as added after the pre-registration).
                    assertEquals(c.family == "peers", c.main.buyVsMean != null && c.main.sellVsMean != null)
                    if (BotV3.proven(c.main, true)) assertTrue(BotV3.judged(c.main, true).t!! >= v3.tRequired)
                    for (x in listOf(c.main.buy, c.main.sell)) {
                        if (x.verdict == "edge") assertTrue(x.t!! >= v3.tRequired)
                        if (x.signals < 30) assertEquals("insufficient", x.verdict)
                    }
                }
            }
            assertEquals(4, BotV3.headlineConfigs(g).size)
        }
        assertEquals(v3.headline, report.headline)
        assertEquals(2, v3.afterPrereg.size)
        assertTrue(v3.afterPrereg[0].contains("moyenne à parts égales"))
        assertTrue(v3.afterPrereg[1].contains("au moins 30 signaux"))
        // The one side proven on the past (stocks, peers, 20 days, buys) waits for the forward test: shown, not counted.
        val peers20 = v3.groups[0].horizons[0].configs.first { it.id == "peers" }
        assertTrue(BotV3.proven(peers20.main, true))
        assertFalse(BotV3.confirmed(peers20, true))
        assertNull(BotV3.pendingText(peers20, false, v3.tRequired))
        assertEquals(
            "Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (0 à ce jour).",
            BotV3.pendingText(peers20, true, v3.tRequired),
        )
        // Confirmed once 30 forward signals agree against both references.
        val ok = BotV3SideStats(signals = 30, excess = 0.1)
        val confirmedCfg = peers20.copy(forward = peers20.forward.copy(buy = ok, buyVsMean = ok))
        assertTrue(BotV3.confirmed(confirmedCfg, true))
        assertNull(BotV3.pendingText(confirmedCfg, true, v3.tRequired))
        assertFalse(BotV3.confirmed(peers20.copy(forward = peers20.forward.copy(buy = ok, buyVsMean = ok.copy(excess = -0.1))), true))
        assertTrue(v3.headline.contains("mais fragile"))
        assertTrue(v3.headline.contains("le bot ne pèse pas dans les décisions"))
        assertTrue(v3.headline.startsWith("Bot v3 : "))
        assertEquals(!BotV3.anyEdge(v3), v3.headline.contains("aucun avantage démontré"))
        assertEquals(0, BotV3.forwardSignals(v3))
        // Today's actions of the basket assets under the 4 headline configurations.
        assertTrue(report.assets.all { it.v3 != null })
        assertEquals("wait", report.assets[0].v3?.absolute20)
    }

    private fun side(
        signals: Int = 120, excess: Double? = 0.2, t: Double? = 1.24, tByAsset: Double? = 0.9, verdict: String? = "unproven", rawVerdict: String? = "unproven",
    ) = BotV3SideStats(signals, 1.0, 0.8, excess, t, 100, tByAsset, 20, 1.5, 52.0, verdict, rawVerdict)

    @Test fun rawVsRequiredTVerdictsSides() {
        assertEquals("t = 1,2 (requis 3,52)", BotV3.tVsRequired(1.24, 3.5157))
        assertEquals("t = — (requis 3,52)", BotV3.tVsRequired(null, 3.5157))
        assertEquals("Non démontré", BotV3.verdictLabel(side(), 3.52))
        assertEquals("t ≥ 2 atteint, pas le seuil corrigé : non démontré", BotV3.verdictLabel(side(t = 2.4, rawVerdict = "edge"), 3.52))
        assertEquals("Avantage au seuil corrigé (t ≥ 3,52), à confirmer", BotV3.verdictLabel(side(verdict = "edge", t = 4.0), 3.5157))
        assertEquals("Trop peu de signaux pour conclure", BotV3.verdictLabel(side(verdict = "insufficient", signals = 3), 3.52))
        assertEquals("Pire que la référence au seuil corrigé", BotV3.verdictLabel(side(verdict = "negative"), 3.52))
        assertEquals("120 achats : +0,2 point face à la médiane du groupe (t par jour 1,2 (requis 3,52) · par actif 0,9).", BotV3.sideText("peers", true, side(), 3.5157))
        assertEquals(
            "120 ventes : baisse évitée −2,5 points (t par jour −4,5 (requis 3,52) · par actif —).",
            BotV3.sideText("absolute", false, side(excess = -2.5, t = -4.51, tByAsset = null), 3.5157),
        )
        assertEquals("t par jour 3,7 (requis 3,52) · par actif 0,6", BotV3.tLine(side(t = 3.65, tByAsset = 0.62), 3.5157))
        assertEquals("Aucune vente.", BotV3.sideText("peers", false, side(signals = 0), 3.5))
        assertEquals("Aucun achat.", BotV3.sideText("absolute", true, side(signals = 0), 3.5))
        assertTrue(BotV3.sideText("peers", false, side(), 3.5).startsWith("120 ventes : gain à passer sur l'actif médian +0,2 point"))
    }

    @Test fun forwardManagedTrendComputeDates() {
        val empty = BotV3ConfigStats(buy = side(signals = 0), sell = side(signals = 0))
        assertTrue(BotV3.forwardText(empty, 3.5).startsWith("Aucun signal jugé pour l'instant"))
        assertEquals("1 achat (+0,2 point, t = 1,2 (requis 3,52)) · 2 ventes (+0,2 point).", BotV3.forwardText(BotV3ConfigStats(buy = side(signals = 1), sell = side(signals = 2)), 3.5157))
        assertEquals("30/09/2026", BotV3.frIso("2026-09-30"))
        val g = v3.groups[0]
        assertEquals(listOf("Ratio de Sharpe", "Pire baisse", "Rendement annuel", "Volatilité annuelle"), BotV3.volRows(g.volManaged!!).map { it.label })
        assertEquals(BotV3.VolRow("Pire baisse", "−15,8${NB}%", "−52,4${NB}%"), BotV3.volRows(g.volManaged!!)[1])
        assertEquals(
            "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)",
            BotV3.computeText(BotTiming(fetchMs = 15_400.0, computeMs = 432_000.0, threads = 1, peakRssMb = 243.4)),
        )
        assertNull(BotV3.computeText(null))
        assertEquals("9 min 43 s de calcul sur 1 cœur, pic mémoire 301 Mo (téléchargement 13 s)", BotV3.computeText(report.timing))
        assertEquals("45 s de calcul sur 4 cœurs (téléchargement 2 s)", BotV3.computeText(BotTiming(2000.0, 45_000.0, 4)))
    }

    @Test fun screenTexts() {
        assertEquals("Bot v3 · pré-enregistré le 30/09/2026", BotV3.title(v3))
        assertEquals("Depuis le 30/09/2026 (test sur l'avenir)", BotV3.forwardTitle(v3))
        assertTrue(BotV3.protocolText(v3).startsWith("Protocole écrit avant tout calcul et figé : 4 tests en v1, 20 en v2, 90 en v3."))
        assertTrue(BotV3.protocolText(v3).endsWith("au-delà de t = 3,52 (Bonferroni, 5 %). Horizons jugés à part : 20 et 60 jours de bourse."))
        assertTrue(BotV3.forwardNote(v3).endsWith("(après au moins 30 signaux) cesse de compter."))
        val g = v3.groups[0]
        assertTrue(BotV3.groupSubtitle(g).startsWith("22 actifs testés, 72 de plus à l'entraînement · historique médian 36,7 ans · classement entre pairs possible depuis "))
        val h = g.horizons[0]
        assertEquals("À 20 jours", BotV3.horizonTitle(h))
        assertTrue(BotV3.horizonSubtitle(h).endsWith(" · 31 réentraînements"))
        val peers = h.configs.first { it.id == "peers" }
        assertEquals("ACHETER · face à la médiane", BotV3.sideHead(true, peers.main))
        assertEquals("VENDRE", BotV3.sideHead(false, h.configs[0].main))
        assertEquals("Face à la moyenne du groupe (contrôle ajouté après coup) : +0,67 point (t par jour 3,7 (requis 3,52) · par actif 0,6)", BotV3.controlText(peers.main.buyVsMean!!, v3.tRequired))
        assertEquals("Sur le passé : avantage face aux deux références.", BotV3.controlVerdict(true))
        assertEquals("Retenu : non démontré (il faut les deux références) ; ne compte pas.", BotV3.controlVerdict(false))
        assertEquals("Face à la moyenne" to "+0,67 point / −0,32 point (t 3,7 / −2,3)", BotV3.configMeanRow(peers))
        assertNull(BotV3.configMeanRow(h.configs[0]))
        assertEquals("2659 achats" to "+0,92 point (t = 4,9 (requis 3,52))", BotV3.configBuyRow(peers, v3.tRequired))
        assertEquals("retenu 0 fois sur 31", BotV3.chosenText(peers))
        val runs = BotV3.choiceRuns(h.selection, peers = true)
        assertEquals(h.blocks, runs.sumOf { it.count })
        assertEquals("xsMomentum", runs[0].chosen)
        assertTrue(BotV3.runsText(runs).startsWith("Momentum entre pairs"))
        assertEquals(
            "Tendance (2 fois) → Arbres longs → aucun",
            BotV3.runsText(listOf(BotV3.ChoiceRun(1.0, 2.0, "trend", 2), BotV3.ChoiceRun(3.0, 3.0, "treesLong", 1), BotV3.ChoiceRun(4.0, 4.0, null, 1))),
        )
        assertEquals("Classement entre pairs", BotV3.familyLabel("peers"))
        assertEquals("Hausse ou baisse de l'actif", BotV3.FAMILY_LABEL["absolute"])
        assertEquals("Arbres longs entre pairs", BotV3.V3_SHORT["xsTrees"])
        assertEquals(
            "Calcul : 9 min 43 s de calcul sur 1 cœur, pic mémoire 301 Mo (téléchargement 13 s) ; 200 réentraînements, jusqu'à 719${NB}748 jours × actifs par groupe et horizon.",
            BotV3.computeLine(v3, report.timing),
        )
        assertEquals("entre pairs 60 j", BotV3.signalLabel(BotV3Signal(family = "peers", horizon = 60)))
        assertEquals("hausse/baisse 20 j", BotV3.signalLabel(BotV3Signal()))
        assertTrue(BotV3.v2ReferenceNote(v3).contains("(t ≥ 3,52)"))
    }

    @Test fun radarCard() {
        assertTrue(BotV3.radarHeadline(report).endsWith("le bot ne pèse pas dans les décisions."))
        assertEquals(BotV3.firstSentence(v3.headline), BotV3.radarHeadline(report))
        assertEquals("Test sur l'avenir : 0 signal sur 30", BotV3.radarCounter(report))
        assertEquals("A.", BotV3.firstSentence("A. B. C"))
        assertEquals("sans point", BotV3.firstSentence("sans point"))
        val v2 = decode("bot-v2.json")
        assertNull(BotV3.radarCounter(v2))
        assertEquals(BotV3.firstSentence(v2.headline), BotV3.radarHeadline(v2))
        assertEquals("Test sur l'avenir : 12 signaux sur 30", "Test sur l'avenir : ${BotV3.signalsWord(12)} sur 30")
    }

    @Test fun viewV3CountsAndNote() {
        val json = """{"available":true,"action":"wait","counts":false,"note":"v2","v3":{"available":true,"counts":true,"nudge":3,"tRequired":3.5157,
            "note":"Avantage démontré au seuil corrigé (t ≥ 3,52) pour ce signal : compte un peu dans la décision (au plus 3 points), jamais contre un veto.",
            "signals":[{"family":"peers","horizon":20,"candidate":"xsMomentum","action":"buy","counts":true,"pending":false,"text":"x"}],"pro":"p","future":1}}"""
        val v = AltimJson.decodeFromString(BotView.serializer(), json)
        val x = assertNotNull(v.v3)
        assertTrue(v.effectiveCounts)
        assertEquals("edge", v.tone)
        assertTrue(v.effectiveNote.startsWith("Avantage démontré au seuil corrigé"))
        assertEquals("buy", x.signals[0].action)
        assertEquals("p", x.pro)
        // Without v3 (older server), and with an empty v3 note.
        val old = v.copy(v3 = null)
        assertFalse(old.effectiveCounts)
        assertEquals("v2", old.effectiveNote)
        assertEquals("unproven", old.tone)
        assertEquals("v2", v.copy(v3 = x.copy(note = "")).effectiveNote)
        // A v3 view that does not count overrides a v2 one that did.
        assertFalse(v.copy(counts = true, v3 = x.copy(counts = false)).effectiveCounts)
    }
}
