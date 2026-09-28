package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

private const val DAY = 86_400_000L
private fun d(iso: String) = java.time.Instant.parse("${iso}T00:00:00Z").toEpochMilli()

class DcaTest {
    @Test fun exactThreeBuys() {
        // 100 $ on days 0, 7 and 14 at 10, 20 and 5 $; last close 10 $ (same case as the site's test).
        val closes = (0..20).map { i -> d("2026-09-01") + i * DAY to if (i < 7) 10.0 else if (i < 14) 20.0 else if (i < 20) 5.0 else 10.0 }
        val r = assertNotNull(DcaResult.simulate(closes, 100.0, 7, 20, d("2026-09-22")))
        assertEquals(3, r.buys)
        assertEquals(300.0, r.invested)
        assertEquals(35.0, r.units, 1e-9)
        assertEquals(350.0, r.value, 1e-9)
        assertEquals(50.0 / 3, r.gain, 1e-9)
        assertEquals(300.0, r.lumpValue, 1e-9)
    }

    @Test fun realBitcoinCloses() {
        val real = AltimJson.decodeFromString(HistoryResponse.serializer(), requireNotNull(javaClass.getResource("/fixtures/history.json")).readText())
        val btc = real.byId.getValue("crypto:BTC")
        val r = assertNotNull(DcaResult.simulate(btc, 100.0, 7, 90))
        assertEquals(13, r.buys)
        assertEquals(84464.605, r.lastPrice, 1e-6)
        assertTrue(r.lumpGain > r.gain)
        assertNull(DcaResult.simulate(btc, 100.0, 30, 365))
        assertNull(DcaResult.simulate(btc, 0.0, 7, 90))
    }

    @Test fun decodeRealBrief() {
        val b = AltimJson.decodeFromString(Brief.serializer(), requireNotNull(javaClass.getResource("/fixtures/brief.json")).readText())
        assertTrue(b.headline.isNotBlank())
        assertEquals("calm", b.market?.level)
        assertTrue(b.movers.isNotEmpty())
        assertTrue(b.news.all { it.safeUrl != null })
    }
}
