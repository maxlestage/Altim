package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

private const val DAY = 86_400_000L
private fun d(iso: String) = java.time.Instant.parse("${iso}T00:00:00Z").toEpochMilli()

class HistoryTest {
    // Real answer of /api/history (ETH, AAPL + Bitcoin and SPY), 28 September 2026: the same file as the site's test.
    private val real = AltimJson.decodeFromString(HistoryResponse.serializer(), requireNotNull(javaClass.getResource("/fixtures/history.json")).readText())

    @Test fun realCloses() {
        val h = assertNotNull(PortfolioHistory.compute(listOf("crypto:ETH" to 2.0, "stock:AAPL" to 10.0), real.byId, 90, real.asOf.toLong()))
        assertEquals(91, h.points.size)
        assertFalse(h.shortened)
        assertEquals(d("2026-09-27"), h.points.last().t)
        assertEquals(2 * 2688.32 + 10 * 341.07, h.points.last().value, 0.1)
        h.points.zipWithNext().forEach { (a, b) -> assertEquals(DAY, b.t - a.t) }
        assertEquals(listOf("Bitcoin", "S&P 500 (SPY)"), h.benchmarks.map { it.label })
        val btcStart = real.byId.getValue("crypto:BTC").first { it.first == h.points.first().t }.second
        assertEquals((84464.605 / btcStart - 1) * 100, h.benchmarks[0].change, 1e-6)
        assertEquals(0.0, h.pct.first())
        // Same figures as the site on the same file (web/test/history.test.ts, card checked in the browser): +45,5 %.
        assertEquals(45.5, h.change, 0.05)
        assertEquals(40.3, h.benchmarks[0].change, 0.05)
        assertEquals(-6.8, h.maxDrawdown, 0.05)
    }

    @Test fun weekendAndLateListing() {
        val stock = mapOf("stock:X" to listOf(d("2026-09-18") to 100.0, d("2026-09-21") to 110.0))
        val h = assertNotNull(PortfolioHistory.compute(listOf("stock:X" to 1.0), stock, 3, d("2026-09-21") + 3_600_000))
        assertEquals(listOf(100.0, 100.0, 100.0, 110.0), h.points.map { it.value })
        assertEquals(10.0, h.best!!.change, 1e-9)
        assertNull(h.worst)

        val late = mapOf(
            "crypto:A" to (0..10).map { d("2026-09-01") + it * DAY to 10.0 },
            "crypto:NEW" to (0..3).map { d("2026-09-08") + it * DAY to 5.0 },
        )
        val h2 = assertNotNull(PortfolioHistory.compute(listOf("crypto:A" to 1.0, "crypto:NEW" to 2.0), late, 10, d("2026-09-11") + 1))
        assertTrue(h2.shortened)
        assertEquals(d("2026-09-08"), h2.points.first().t)
        assertEquals(0.0, h2.change)

        val closes = listOf(100.0, 120.0, 90.0, 108.0, 60.0, 80.0).mapIndexed { i, c -> d("2026-09-01") + i * DAY to c }
        val h3 = assertNotNull(PortfolioHistory.compute(listOf("crypto:A" to 1.0, "crypto:GONE" to 3.0), mapOf("crypto:A" to closes), 5, d("2026-09-06") + 1))
        assertEquals(-50.0, h3.maxDrawdown, 1e-9)
        assertEquals(listOf("crypto:GONE"), h3.missing)
        assertNull(PortfolioHistory.compute(listOf("crypto:GONE" to 1.0), emptyMap(), 30))
    }
}

class NewsAlertTest {
    private val now = d("2026-09-28") + 8 * 3_600_000
    private fun item(id: String, title: String, hoursAgo: Double = 1.0, alert: Boolean = false, assets: List<String> = emptyList(), alsoIn: Int = 0) =
        NewsItem(id = id, title = title, link = "https://ex.com/$id", time = now - hoursAgo * 3_600_000, source = "S", category = "monde",
            assets = assets, alsoIn = List(alsoIn) { "O$it" }, alert = alert)

    @Test fun rule() {
        val owned = setOf("crypto:BTC")
        val items = listOf(
            item("1", "Russia declares war on neighbour, markets slide", alert = true, alsoIn = 1),
            item("6", "AI agents could trigger a bank run, says economist", alert = true), // one source: an opinion
            item("2", "Bitcoin ETF inflows reach record as price jumps", assets = listOf("crypto:BTC"), alsoIn = 2),
            item("3", "Bitcoin miners expand in Texas", assets = listOf("crypto:BTC"), alsoIn = 1), // only 2 sources
            item("4", "Ethereum upgrade goes live", assets = listOf("crypto:ETH"), alsoIn = 5), // not owned
            item("5", "Bank collapse sparks contagion fears", hoursAgo = 7.0, alert = true), // too old
        )
        val (t1, first) = NewsAlertTracker().newAlerts(items, owned, now)
        assertEquals(listOf("1", "2"), first.map { it.id })

        // Next check: the same stories, one told again by another source under a close title → nothing new.
        val again = items + item("9", "Russia declares war on its neighbour, markets slide", alert = true, alsoIn = 3)
        val (t2, second) = t1.newAlerts(again, owned, now + 15 * 60_000)
        assertEquals(emptyList(), second)

        // Two days later the memory is released (the stories are too old anyway).
        val (t3, _) = t2.newAlerts(emptyList(), owned, now + 49 * 3_600_000)
        assertTrue(t3.seen.isEmpty())
    }
}
