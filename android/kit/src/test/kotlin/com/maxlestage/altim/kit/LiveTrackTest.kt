package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Live following of an asset (iOS LiveActivities): note, verdict chip, 5-second throttle, refresh by an alert check. */
class LiveTrackTest {
    private val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")

    @Test fun noteAndVerdict() {
        val pending = LiveTrack.state(btc, 88_000.0, 1.2, null, null, 0)
        assertEquals(LiveTrack.PENDING, pending.note)
        assertEquals("PAS D'ACHAT", pending.verdict)
        val buy = BuyAlert("BTC", Kind.CRYPTO, buy = true, reasons = listOf("Zone d'achat moyen terme atteinte."), body = "corps")
        val s = LiveTrack.state(btc, 88_000.0, 1.2, buy, null, 0)
        assertTrue(s.buy)
        assertEquals("Zone d'achat moyen terme atteinte.", s.note)
        assertEquals("ACHAT POSSIBLE", s.verdict)
        assertEquals("corps", LiveTrack.state(btc, 1.0, null, buy.copy(reasons = emptyList()), null, 0).note)
        val no = BuyAlert("BTC", Kind.CRYPTO, buy = false, blockers = listOf("Choc de marché en cours."))
        assertEquals("Choc de marché en cours.", LiveTrack.state(btc, 1.0, null, no, null, 0).note)
        assertEquals(LiveTrack.NO_BUY, LiveTrack.state(btc, 1.0, null, no.copy(blockers = emptyList()), null, 0).note)
        assertEquals("ATTENDRE", LiveTrack.state(btc, 1.0, null, no, "ATTENDRE", 0).verdict)
    }

    @Test fun ticksAreThrottledAndOnlyForTheFollowedAsset() {
        val s = LiveTrack.state(btc, 88_000.0, 1.2, null, null, 0)
        val tick = LiveTick("BTC", Kind.CRYPTO, 88_500.0, null, time = 0.0)
        assertNull(LiveTrack.tick(s, tick, lastPush = 1_000, now = 4_000))
        val next = LiveTrack.tick(s, tick, lastPush = 1_000, now = 6_000)
        assertEquals(88_500.0, next?.price)
        assertEquals(1.2, next?.change)
        assertEquals(6_000L, next?.updated)
        assertNull(LiveTrack.tick(s, tick.copy(symbol = "ETH"), 0, 10_000))
    }

    @Test fun refreshedByAnAlertCheck() {
        val s = LiveTrack.state(btc, 88_000.0, 1.2, null, "ACHAT", 0)
        assertNull(LiveTrack.refresh(s, listOf(BuyAlert("ETH", Kind.CRYPTO, buy = true)), 10))
        val r = LiveTrack.refresh(s, listOf(BuyAlert("BTC", Kind.CRYPTO, price = 87_000.0, buy = false)), 10)
        assertEquals(87_000.0, r?.price)
        assertEquals(1.2, r?.change)
        assertFalse(r!!.buy)
        assertEquals("ACHAT", r.verdict)
        assertEquals(r, LiveTrack.parse(LiveTrack.encode(r)))
        assertNull(LiveTrack.parse("{broken"))
    }
}
