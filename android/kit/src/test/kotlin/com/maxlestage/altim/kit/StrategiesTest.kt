package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/** Same cases as web/test/strategies.test.ts, on the same real answers (backend/tests/samples/strategies-*.json). */
class StrategiesTest {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()
    private val aapl = AltimJson.decodeFromString(StrategiesReport.serializer(), raw("strategies-aapl.json"))
    private val btc = AltimJson.decodeFromString(StrategiesReport.serializer(), raw("strategies-btc.json"))
    private val box = Strategies.Box(320.0, 180.0, 34.0, 62.0, 10.0, 18.0)

    @Test fun eightStrategiesInTheFixedOrderSameDates() {
        for (r in listOf(aapl, btc)) {
            assertEquals(Strategies.ORDER, r.strategies.map { it.id })
            for (s in r.strategies.filter { it.available }) {
                assertTrue(s.equity.size <= 200)
                assertEquals(r.from, s.time(0))
                assertEquals(r.to, s.time(s.equity.size - 1))
                assertNotNull(s.metrics)
                assertTrue(s.rule.length > 20)
            }
            assertTrue(r.notes.any { it.contains("aucune optimisation") })
        }
    }

    @Test fun valueComputedOnAStockNotOnACrypto() {
        assertTrue(aapl.strategies.first { it.id == "value" }.available)
        val v = btc.strategies.first { it.id == "value" }
        assertTrue(!v.available)
        assertTrue(v.unavailable!!.contains("pas de bénéfices"))
    }

    @Test fun lowSampleFollowsThe10TradeRule() {
        for (s in (aapl.strategies + btc.strategies).filter { it.metrics != null }) {
            if (s.id == "buyHold" || s.id == "dca") assertTrue(!s.lowSample) else assertEquals(s.metrics!!.trades < 10, s.lowSample)
            assertEquals(s.lowSample, Strategies.lowSampleText(s) != null)
        }
    }

    @Test fun defaultSelectionEachWithItsColour() {
        val d = Strategies.drawable(btc, Strategies.DEFAULT_SELECTION)
        assertEquals(Strategies.DEFAULT_SELECTION, d.map { it.id })
        assertEquals(Strategies.ORDER.size, Strategies.ORDER.map { Strategies.COLORS[it] }.toSet().size)
        // Unavailable strategies are never drawn.
        assertEquals(emptyList(), Strategies.drawable(btc, listOf("value")))
        // Chips: removed, then added back at its place in the fixed order.
        assertEquals(listOf("trend", "meanReversion", "dca", "buyHold"), Strategies.toggle(Strategies.DEFAULT_SELECTION, "breakout"))
        assertEquals(listOf("trend", "momentum", "breakout", "meanReversion", "dca", "buyHold"), Strategies.toggle(Strategies.DEFAULT_SELECTION, "momentum"))
    }

    @Test fun singleAxisKeeps100InsidePointerMapsBack() {
        val d = Strategies.drawable(aapl, Strategies.DEFAULT_SELECTION)
        val g = assertNotNull(Strategies.geometry(d, box))
        assertTrue(g.lo < 100)
        assertTrue(g.hi > 100)
        assertTrue(abs(g.y(g.hi) - box.t) < 1e-9)
        assertTrue(abs(g.y(g.lo) - (box.h - box.b)) < 1e-9)
        assertEquals(37, Strategies.indexAt(g.x(37), g, box))
        assertEquals(0, Strategies.indexAt(-50.0, g, box))
        assertEquals(g.n - 1, Strategies.indexAt(999.0, g, box))
    }

    @Test fun directLabelsNeverOverlap() {
        val ys = listOf(50.0, 52.0, 51.0, 170.0, 171.0)
        val out = Strategies.placeLabels(ys, 11.0, 14.0, 162.0)
        val sorted = out.sorted()
        for (k in 1 until sorted.size) assertTrue(sorted[k] - sorted[k - 1] >= 11 - 1e-9)
        assertTrue(out.max() <= 162)
        assertTrue(out.min() >= 14 - 1e-9)
        // Order preserved: the highest curve keeps the highest label.
        assertTrue(out[0] < out[3])
    }

    @Test fun listViewCheckpoints() {
        val s = aapl.strategies[0]
        val c = Strategies.checkpoints(s)
        assertEquals(5, c.size)
        assertEquals(s.time(0), c[0].t)
        assertEquals(s.time(s.equity.size - 1), c.last().t)
    }

    @Test fun formattingAndAddress() = runBlocking {
        assertEquals("+12,3 %", Strategies.signedPct(12.345))
        assertEquals("−4,0 %", Strategies.signedPct(-4.0))
        assertEquals("—", Strategies.signedPct(null))
        assertEquals("15 juil. 2022", Strategies.shortDate(aapl.from))
        val server = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse.Builder().code(200).body(raw("strategies-aapl.json")).build())
            val r = AltimClient(server.url("/"), null).strategies(Asset("BRK.B", Kind.STOCK, "Berkshire"))
            assertEquals("AAPL", r.symbol)
            val url = server.takeRequest().url
            assertEquals("/api/strategies", url.encodedPath)
            assertEquals("BRK.B", url.queryParameter("symbol"))
            assertEquals("stock", url.queryParameter("kind"))
        } finally {
            server.close()
        }
    }
}
