package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

private const val DAY = 86_400_000L
private fun d(iso: String) = java.time.Instant.parse("${iso}T00:00:00Z").toEpochMilli()

/** Same cases as the site's test (web/test/tools.test.ts). */
class ToolsTest {
    private val real = AltimJson.decodeFromString(HistoryResponse.serializer(), requireNotNull(javaClass.getResource("/fixtures/history.json")).readText())

    @Test fun compareRealCloses() {
        val c = assertNotNull(Tools.compare(real.byId, listOf("crypto:BTC", "crypto:ETH", "stock:AAPL"), 90, real.asOf.toLong()))
        val (btc, eth, aapl) = c.stats
        c.stats.forEach { s ->
            assertEquals(c.days.size, s.pct.size)
            assertEquals(0.0, s.pct.first())
            assertTrue(s.maxDrawdown <= 0)
        }
        assertTrue(eth.volatility > btc.volatility && btc.volatility > aapl.volatility)
        assertTrue(c.correlation[0][1]!! > 0.5)
        assertEquals(c.correlation[0][1], c.correlation[1][0])
    }

    @Test fun compareExact() {
        val a = listOf(100.0, 110.0, 99.0, 120.0).mapIndexed { i, v -> d("2026-09-01") + i * DAY to v }
        val b = listOf(50.0, 55.0).mapIndexed { i, v -> d("2026-09-03") + i * DAY to v }
        val c = assertNotNull(Tools.compare(mapOf("crypto:A" to a, "crypto:B" to b), listOf("crypto:A", "crypto:B", "crypto:GONE"), 30, d("2026-09-05")))
        assertEquals(listOf("crypto:GONE"), c.missing)
        assertEquals(d("2026-09-03"), c.days.first())
        assertEquals(21.21, c.stats[0].pct.last(), 0.005)
        assertEquals(10.0, c.stats[1].change, 1e-9)
        assertNull(c.correlation[0][1])
    }

    @Test fun positionSize() {
        val p = assertNotNull(Tools.positionSize(10_000.0, 1.0, 100.0, 95.0, 110.0))
        assertEquals(20.0, p.quantity, 1e-9)
        assertEquals(2000.0, p.amount, 1e-9)
        assertEquals(100.0, p.risk, 1e-9)
        assertEquals(2.0, p.ratio!!, 1e-9)
        assertFalse(p.capped)
        val capped = assertNotNull(Tools.positionSize(1000.0, 2.0, 100.0, 99.5))
        assertTrue(capped.capped)
        assertEquals(1000.0, capped.amount, 1e-9)
        assertNull(Tools.positionSize(1000.0, 1.0, 100.0, 120.0))
    }

    @Test fun rebalance() {
        val r = assertNotNull(Tools.rebalance(listOf(Triple("crypto:BTC", Kind.CRYPTO, 6000.0), Triple("crypto:ETH", Kind.CRYPTO, 2000.0), Triple("stock:AAPL", Kind.STOCK, 2000.0)), 50.0))
        assertEquals(10_000.0, r.total)
        assertEquals(80.0, r.current.getValue(Kind.CRYPTO), 1e-9)
        assertEquals(-3000.0, r.moves.getValue(Kind.CRYPTO), 1e-9)
        assertEquals(3000.0, r.moves.getValue(Kind.STOCK), 1e-9)
        assertEquals(listOf("crypto:BTC" to -2250.0, "crypto:ETH" to -750.0, "stock:AAPL" to 3000.0), r.lines)
    }

    @Test fun saleAfterTax() {
        val t = Tools.saleTotal(listOf(Triple("a", 10_000.0, 6000.0), Triple("b", 5000.0, 7000.0), Triple("c", 1000.0, null)))
        assertEquals(10.0, t.lines[0].fees, 1e-9)
        assertEquals(3990.0, t.lines[0].gain!!, 1e-9)
        assertEquals(1197.0, t.lines[0].tax, 1e-9)
        assertEquals(8793.0, t.lines[0].net, 1e-9)
        assertEquals(0.0, t.lines[1].tax)
        assertNull(t.lines[2].gain)
        assertEquals((3990.0 - 2005.0) * 0.3, t.tax, 1e-9)
        assertEquals(16_000.0 - 16.0 - (3990.0 - 2005.0) * 0.3, t.net, 1e-9)
        assertEquals(1, t.unknownCost)
    }

    @Test fun projection() {
        assertEquals(1000.0 + 100 * 120, Tools.projection(1000.0, 100.0, 10, 0.0).last().value, 1e-6)
        assertEquals(10_000 * Math.pow(1.08, 5.0), Tools.projection(10_000.0, 0.0, 5, 8.0).last().value, 1e-6)
        val r = Math.pow(1.04, 1.0 / 12) - 1
        assertEquals(200 * (Math.pow(1 + r, 240.0) - 1) / r, Tools.projection(0.0, 200.0, 20, 4.0).last().value, 1e-6)
    }

    @Test fun moveAlert() {
        val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
        val t = PriceTarget(asset = btc, above = false, price = 100.0, move = 5.0)
        assertFalse(t.isReached(104.9))
        assertTrue(t.isReached(105.0))
        assertTrue(t.isReached(94.0))
        val (updated, fired) = PriceTarget.evaluate(listOf(t), mapOf(btc.id to 106.0))
        assertEquals(1, fired.size)
        // Re-armed: starts again from the current price.
        val again = updated.single().rearmed(106.0)
        assertNull(again.triggered)
        assertEquals(106.0, again.price)
        assertFalse(again.isReached(110.0))
        // Stored alerts from before keep working (no "move" field).
        val old = AltimJson.decodeFromString(PriceTarget.serializer(), """{"id":"x","asset":{"symbol":"BTC","kind":"crypto","name":"Bitcoin"},"above":true,"price":90000.0,"created":1}""")
        assertNull(old.move)
        assertTrue(old.isReached(90001.0))
    }
}
