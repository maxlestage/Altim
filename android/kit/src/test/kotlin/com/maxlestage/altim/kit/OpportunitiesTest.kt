package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Same cases as web/test/opportunities.test.ts, plus the decoding of the server's contracts (flattened metrics of an
 * item, derivatives block of an anomaly report; samples shaped like backend/src/app/scan.rs) and the 202 "pending" answer.
 */
class OpportunitiesTest {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()

    private fun item(symbol: String, marketCap: Double? = 1e11, rank: Int? = null, hits: List<OppHit> = listOf(OppHit("breakout", "r", 2.0))) =
        OppItem(symbol, symbol, "Technologie", marketCap, rank, 100.0, 0.0, 1.0, 50.0, 1.0, 2.0, 5e8, 0.0, hits)

    private val d = Opportunities.DEFAULT_FILTERS

    @Test fun numericFiltersAnUnknownValueFailsASetFilter() {
        val a = item("A")
        assertTrue(Opportunities.passes(a, d))
        assertTrue(!Opportunities.passes(a, d.copy(minCap = 2e11)))
        assertTrue(!Opportunities.passes(item("B", marketCap = null), d.copy(minCap = 1e10)))
        assertTrue(Opportunities.passes(item("C", rank = 12), d.copy(maxRank = 20)))
        assertTrue(!Opportunities.passes(item("C", rank = 45), d.copy(maxRank = 20)))
        assertTrue(!Opportunities.passes(a, d.copy(maxVolatility = 1.5)))
        assertTrue(!Opportunities.passes(a, d.copy(minLiquidity = 1e9)))
    }

    @Test fun categoriesOnlyTheChosenReasonsMostReasonsFirst() {
        val items = listOf(
            item("ONE"),
            item("TWO", hits = listOf(OppHit("volume", "v", 5.0), OppHit("breakout", "b", 3.0))),
            item("OVS", hits = listOf(OppHit("oversold", "o", 9.0))),
        )
        assertEquals(listOf("TWO", "OVS", "ONE"), Opportunities.filterItems(items, d).map { it.symbol })
        val only = Opportunities.filterItems(items, d.copy(categories = listOf("breakout")))
        assertEquals(listOf("TWO", "ONE"), only.map { it.symbol })
        assertEquals(listOf("breakout"), only[0].hits.map { it.category })
        assertEquals(emptyList(), Opportunities.filterItems(items, d.copy(categories = emptyList())))
        assertEquals(mapOf("setup" to 0, "reversal" to 0, "breakout" to 2, "volume" to 1, "oversold" to 1, "fundamentals" to 0), Opportunities.countByCategory(items, d))
        assertEquals(0, Opportunities.countByCategory(items, d.copy(maxVolatility = 1.0))["breakout"])
        assertEquals(listOf("setup", "reversal", "breakout", "oversold", "fundamentals"), Opportunities.toggle(d, "volume").categories)
    }

    @Test fun liquidationsAndAmounts() {
        val l = LiquidationSummary(14_389_008.0, 5_835_591.0, 1486, 764, null, 0.0, 0.0, 24.0, true, "")
        assertEquals(71L, Math.round(Opportunities.longShare(l)!!))
        assertNull(Opportunities.longShare(l.copy(longUsd = 0.0, shortUsd = 0.0)))
        assertEquals("14,4 M$", Opportunities.compactUsd(14_389_008.0))
        assertEquals("3,1 Md$", Opportunities.compactUsd(3_060_890_723.0))
        assertEquals("843 k$", Opportunities.compactUsd(843_414.0))
        assertEquals("420 $", Opportunities.compactUsd(420.0))
    }

    @Test fun savedFiltersValidated() {
        val saved = OppSaved(Kind.CRYPTO, OppFilters(categories = listOf("volume", "setup"), maxRank = 50))
        assertEquals(saved, Opportunities.parseSaved(Opportunities.encodeSaved(saved)))
        assertEquals(OppSaved(), Opportunities.parseSaved("{oops"))
        assertEquals(OppSaved(), Opportunities.parseSaved(null))
        assertEquals(listOf("volume"), Opportunities.parseSaved("""{"market":"stock","filters":{"categories":["volume","whales"]}}""").filters.categories)
    }

    @Test fun contractsDecode() {
        val r = AltimJson.decodeFromString(OpportunityReport.serializer(), raw("opportunities-sample.json"))
        assertEquals(6, r.categories.size)
        val aapl = r.items.first { it.symbol == "AAPL" }
        // An unknown category never breaks decoding, and is never shown.
        assertEquals(listOf("fundamentals", "whales"), aapl.hits.map { it.category })
        assertEquals(1, Opportunities.filterItems(r.items, d).first { it.symbol == "AAPL" }.hits.size)
        assertEquals(listOf("RSI 14 : 68", "volatilité 3,6 %/j", "échangé 2,9 Md$/j", "capitalisation 138 Md$"), Opportunities.metrics(r.items[0]))
        val a = AltimJson.decodeFromString(AnomalyReport.serializer(), raw("anomalies-sample.json"))
        assertEquals("warning", a.anomalies.first().severity)
        assertEquals(8.0, a.derivatives?.funding?.periodHours)
        assertEquals(3, a.derivatives?.notCovered?.size)
        assertEquals("+2,5 %", Opportunities.signed(2.5))
        assertEquals("−0,002 %", Opportunities.signed(-0.002, 4))
        assertEquals("Analyse des 120 cryptos en cours (environ 30 secondes la première fois)…", Opportunities.pendingText(Kind.CRYPTO))
        // A stock: no derivatives block.
        assertNull(AltimJson.decodeFromString(AnomalyReport.serializer(), """{"symbol":"AAPL","kind":"stock","anomalies":[],"normal":[],"derivatives":null,"errors":[],"source":"s"}""").derivatives)
    }

    @Test fun pendingThenReadyAndAddresses() = runBlocking {
        val server = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse.Builder().code(202).body("""{"pending":true}""").build())
            server.enqueue(MockResponse.Builder().code(200).body(raw("opportunities-sample.json")).build())
            server.enqueue(MockResponse.Builder().code(200).body(raw("anomalies-sample.json")).build())
            val c = AltimClient(server.url("/"), null)
            assertIs<OpportunitiesResult.Pending>(c.opportunities(Kind.CRYPTO))
            assertEquals("crypto", server.takeRequest().url.queryParameter("kind"))
            val ready = assertIs<OpportunitiesResult.Ready>(c.opportunities(Kind.STOCK))
            assertEquals(150, ready.report.scanned)
            server.takeRequest()
            assertEquals("BTC", c.anomalies(Asset("BTC", Kind.CRYPTO, "Bitcoin")).symbol)
            val u = server.takeRequest().url
            assertEquals("/api/anomalies", u.encodedPath)
            assertEquals("BTC", u.queryParameter("symbol"))
        } finally {
            server.close()
        }
    }
}
