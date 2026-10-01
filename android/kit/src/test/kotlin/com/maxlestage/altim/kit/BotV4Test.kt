package com.maxlestage.altim.kit

import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

// « Bots sélectifs » (v4), same cases as the iOS BotV4Tests: the real /api/bot answer of 01/10/2026
// (backend/tests/samples/bot-v4.json reduced to `v4` and the basket rows), the v3 answer still read without v4, the texts and
// the `v4` of a view (decision line).

class BotV4Test {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()
    private fun decode(name: String) = AltimJson.decodeFromString(BotReport.serializer(), raw(name))

    @Test fun realReportSixteenBotsAndCorrectedThreshold() {
        val r = decode("bot-v4.json")
        val v4 = assertNotNull(r.v4)
        assertEquals("2026-10-01", v4.preregDate)
        assertEquals(130, v4.k.total)
        assertEquals(16, v4.k.v4)
        assertTrue(abs(v4.tRequired - 3.5504) < 0.001)
        assertEquals(30, v4.parameters.minSignals)
        assertEquals(16, v4.bots.size)
        assertEquals(listOf("rise", "fall", "top", "bottom"), v4.bots.take(4).map { it.side })
        assertEquals("stock-20-rise", v4.bots.first().id)
        assertEquals("crypto-60-bottom", v4.bots.last().id)
        assertTrue(v4.headline.startsWith("Bots sélectifs : "))
        assertEquals(listOf(4, 4, 4, 4), BotV4Texts.cards(v4).map { it.second.size })
        assertEquals("Actions et ETF américains · 20 jours", BotV4Texts.cards(v4).first().first)
        for (b in v4.bots) {
            if (b.forward.signals < 30) assertNotEquals("proven", b.status)
            if (b.status != "notProven") assertTrue((b.main.t ?: 0.0) >= v4.tRequired)
            if (b.main.signals > 0) assertTrue((b.main.wilsonLow ?: 0.0) <= (b.main.precision ?: 0.0) && (b.main.precision ?: 0.0) <= (b.main.wilsonHigh ?: 0.0))
            assertTrue(BotV4Texts.todayText(b).isNotBlank())
        }
        assertTrue(r.assets.all { it.v4 != null })
        val v3 = decode("bot.json")
        assertNull(v3.v4)
        assertTrue(v3.assets.all { it.v4 == null })
    }

    @Test fun texts() {
        val s = BotV4Stats(signals = 120, hits = 80, precision = 66.7, wilsonLow = 57.8, wilsonHigh = 74.5, reference = 54.2, perYear = 4.3)
        assertEquals("67 % (58 %–75 %)", BotV4Texts.precisionShort(s))
        assertEquals("aucun signal", BotV4Texts.precisionShort(BotV4Stats()))
        assertEquals("50 % (44 %–56 %)", BotV4Texts.precisionShort(BotV4Stats(signals = 2, precision = 49.6, wilsonLow = 43.5, wilsonHigh = 55.7)))
        assertEquals("100 %", BotV4Texts.pct(100.0))
        assertEquals("120 · 4,3 signaux par an", BotV4Texts.rhythm(s))
        assertEquals("Hausse 20 j", BotV4Texts.shortLabel("stock-20-rise"))
        assertEquals("Bas du classement 60 j", BotV4Texts.shortLabel("crypto-60-bottom"))
        assertEquals("pas d'avis (seuil fermé sur la dernière année)", BotV4Texts.todayText(BotV4Bot(id = "a")))
        assertEquals("pas d'avis", BotV4Texts.todayText(BotV4Bot(id = "a", todayLevel = 5.0)))
        assertEquals("ACHETER (dans le haut de son groupe) : AAPL, NVDA", BotV4Texts.todayText(BotV4Bot(id = "a", side = "top", todayLevel = 5.0, today = listOf("AAPL", "NVDA"))))
        assertEquals("en attente", BotV4Texts.statusLabel("pending"))
        assertEquals("−1,2 (requis 3,55)", BotV4Texts.tText(BotV4Stats(t = -1.24), 3.5504))
    }

    @Test fun viewInADecision() {
        val json = """
            {"available": true, "action": "wait", "counts": false, "note": "Bots sélectifs : pas d'avis aujourd'hui (aucun score assez extrême) ; ils ne comptent pas.",
             "v3": {"available": true, "signals": [], "counts": false, "nudge": 0, "tRequired": 3.5157, "note": "Aucun avantage démontré"},
             "v4": {"available": true, "counts": false, "nudge": 0, "note": "n", "signals": [
               {"id": "stock-20-rise", "label": "Hausse à 20 jours (actions)", "horizon": 20, "side": "rise", "action": "buy", "probability": 71.2,
                "precision": 61.0, "wilsonLow": 55.0, "wilsonHigh": 66.6, "reference": 56.1, "perYear": 6.2, "status": "notProven", "counts": false, "text": "t"},
               {"id": "stock-20-fall", "label": "Baisse à 20 jours (actions)", "horizon": 20, "side": "fall", "status": "weird", "counts": false, "text": "t"}]}}
        """.trimIndent()
        val v = AltimJson.decodeFromString(BotView.serializer(), json)
        val w = assertNotNull(v.v4)
        assertEquals(listOf("stock-20-rise"), w.speaking.map { it.id })
        assertEquals("non prouvé", BotV4Texts.statusLabel(w.signals[1].status))
        assertEquals("1 avis sur 2 bots", BotV4Texts.viewHead(w))
        assertEquals(
            "ACHETER (hausse attendue) · précision mesurée 61 % (55 %–67 %) contre 56 % au hasard · 6,2 signaux par an",
            BotV4Texts.signalText(w.signals[0]),
        )
        assertFalse(v.effectiveCounts)
        assertEquals("Bots sélectifs : pas d'avis aujourd'hui (aucun score assez extrême) ; ils ne comptent pas.", v.effectiveNote)
    }
}
