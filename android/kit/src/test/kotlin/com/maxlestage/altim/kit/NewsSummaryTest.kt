package com.maxlestage.altim.kit

import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.jsonObject
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Same cases as web/test/news-summary.test.ts, on the same real sample (web/test/news-summary-sample.json, copied in the fixtures). */
class NewsSummaryTest {
    // A real /api/news `summary` (28/09/2026, radar AAPL, BTC, NVDA; guard and hourly candles cached for them).
    private val list: List<StorySummary> = run {
        val raw = requireNotNull(javaClass.getResource("/fixtures/news-summary-sample.json")).readText()
        AltimJson.decodeFromJsonElement(ListSerializer(StorySummary.serializer()), AltimJson.parseToJsonElement(raw).jsonObject.getValue("summary"))
    }

    @Test fun realSampleAtMostFiveEventsEachComplete() {
        assertTrue(list.isNotEmpty())
        assertTrue(list.size <= 5)
        for (s in list) {
            assertTrue(s.impact in setOf("low", "medium", "high"))
            assertEquals(s.link, s.links[0].link)
            assertTrue(s.independentSources <= s.sources)
            // Measured only with a move, and every move's asset is one of the story's.
            assertEquals(s.moves.isNotEmpty(), s.impactBasis == "measured")
            for (m in s.moves) assertTrue(m.asset in s.assets)
            for (t in s.technical) assertTrue(t.asset in s.assets)
        }
    }

    @Test fun labels() {
        fun at(impact: String) = list[0].copy(impact = impact)
        assertEquals(NewsSummary.Heading("Aucun événement important aujourd'hui", null), NewsSummary.heading(emptyList()))
        assertEquals(
            NewsSummary.Heading("Aucun événement important aujourd'hui", "2 sujets repris par plusieurs sources, à impact faible."),
            NewsSummary.heading(listOf(at("low"), at("low"))),
        )
        assertEquals(
            NewsSummary.Heading("1 événement important aujourd'hui", "Et 1 sujet repris par plusieurs sources, à impact faible."),
            NewsSummary.heading(listOf(at("high"), at("low"))),
        )
        assertEquals("2 événements importants aujourd'hui", NewsSummary.heading(listOf(at("high"), at("medium"))).title)
        assertEquals("important", NewsSummary.IMPACT_LABEL["high"])
        val nvda = list.first { "stock:NVDA" in it.assets }
        assertEquals("impact mesuré", NewsSummary.basisLabel(nvda))
        assertEquals("Convergent · 2 sources · ton des titres : 2 positifs", NewsSummary.consensusText(nvda))
        assertEquals("NVDA −0,4 % depuis la publication", NewsSummary.moveText(nvda.moves[0].copy(changePct = -0.4220148770699983)))
        assertEquals(AssetLink("stock", "BRK-B"), NewsSummary.assetLink("stock:BRK-B"))
        assertEquals(Asset("BRK-B", Kind.STOCK, "BRK-B"), NewsSummary.asset("stock:BRK-B"))
        assertNull(NewsSummary.asset("bond:X"))
    }

    @Test fun consensusDivergentSingleRepeated() {
        val base = list[0]
        val divergent = base.copy(sources = 3, consensus = StorySummary.Consensus("divergent", "negative", 2, 1, 0))
        assertEquals("Divergent · 3 sources · ton des titres : 2 négatifs, 1 positif", NewsSummary.consensusText(divergent))
        val single = base.copy(sources = 1, consensus = base.consensus.copy(agreement = "single"))
        assertEquals("Une seule source : pas de consensus mesurable", NewsSummary.consensusText(single))
        assertEquals("Même titre repris par 4 sources : pas de consensus mesurable", NewsSummary.consensusText(single.copy(sources = 4)))
        assertEquals("Divergent · 4 sources (3 titres distincts) · ton des titres : 2 négatifs, 1 positif", NewsSummary.consensusText(divergent.copy(sources = 4)))
    }

    @Test fun ruleDetailAndOlderServer() {
        val nvda = list.first { "stock:NVDA" in it.assets }
        val text = NewsSummary.ruleText(nvda)
        assertTrue(text.startsWith("Règle : "), text)
        assertTrue(text.contains("par règle ; retenu : "), text)
        // An older server has no `summary`: the report still decodes.
        val old = AltimJson.decodeFromString(NewsReport.serializer(), requireNotNull(javaClass.getResource("/fixtures/news.json")).readText())
        assertNull(old.summary)
    }
}
