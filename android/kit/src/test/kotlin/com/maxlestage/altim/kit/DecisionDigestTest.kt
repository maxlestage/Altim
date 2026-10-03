package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/** The Radar follows the full decision; the 4 h technical signal is only a direction (same cases as iOS DecisionDigestTests). */
class DecisionDigestTest {
    private fun btc(): Decision = AltimJson.decodeFromString(
        Decision.serializer(),
        requireNotNull(DecisionDigestTest::class.java.getResource("/fixtures/decision-btc.json")).readText(),
    )

    private val hour = 3_600_000.0

    private fun digest(verdict: Verdict, rating: Rating?, confidence: Double = 50.0, at: Double = 0.0, personal: Boolean = false) =
        DecisionDigest(verdict, rating, verdict.label, DecisionLevel.WAITING, confidence, personal, at)

    @Test fun technicalText() {
        assertEquals("nettement haussier", RadarDecisions.technicalText(Action.STRONG_BUY))
        assertEquals("haussier", RadarDecisions.technicalText(Action.BUY))
        assertEquals("neutre", RadarDecisions.technicalText(Action.HOLD))
        assertEquals("baissier", RadarDecisions.technicalText(Action.SELL))
        assertEquals("nettement baissier", RadarDecisions.technicalText(Action.STRONG_SELL))
        assertEquals(Tone.GOOD, RadarDecisions.technicalTone(Action.BUY))
        assertEquals(Tone.BAD, RadarDecisions.technicalTone(Action.STRONG_SELL))
        assertEquals(Tone.NEUTRAL, RadarDecisions.technicalTone(Action.HOLD))
    }

    @Test fun ratingRankAndTone() {
        val order = listOf(Rating.STRONG_BUY, Rating.BUY, Rating.HOLD, Rating.REDUCE, Rating.SELL, Rating.STRONG_SELL, null)
        assertEquals(listOf(0, 1, 2, 3, 4, 5, 9), order.map(DecisionDigests::ratingRank))
        assertEquals(Tone.GOOD, DecisionDigests.ratingTone(Rating.BUY, Verdict.WAIT))
        assertEquals(Tone.BAD, DecisionDigests.ratingTone(Rating.REDUCE, Verdict.BUY))
        assertEquals(Tone.NEUTRAL, DecisionDigests.ratingTone(Rating.HOLD, Verdict.BUY_ZONE))
        assertEquals(Tone.GOOD, DecisionDigests.ratingTone(null, Verdict.BUY_ZONE))
        assertEquals(Tone.BAD, DecisionDigests.ratingTone(null, Verdict.TRIM))
        assertEquals(Tone.NEUTRAL, DecisionDigests.ratingTone(null, Verdict.WAIT))
    }

    @Test fun badgeLabel() {
        assertEquals("ACHAT FORT", digest(Verdict.WAIT, Rating.STRONG_BUY).badgeLabel)
        assertEquals("ZONE D'ACHAT", digest(Verdict.BUY_ZONE, null).badgeLabel)
        // An unknown server rating decodes as none: the decision's label.
        val unknown = AltimJson.decodeFromString(
            DecisionDigest.serializer(),
            """{"verdict":"wait","rating":"moon","label":"ATTENDRE","level":"waiting","confidence":55,"personal":false,"at":1}""",
        )
        assertEquals("ATTENDRE", unknown.badgeLabel)
    }

    @Test fun recordAndFreshness() {
        val d = btc()
        val now = d.asOf + 1000
        val all = DecisionDigests.record(emptyMap(), d, personal = false, now = now)
        val asset = Asset("BTC", Kind.CRYPTO, "Bitcoin")
        val got = assertNotNull(DecisionDigests.fresh(all, asset, now))
        assertEquals(Verdict.WAIT, got.verdict)
        assertEquals("ATTENDRE", got.badgeLabel)
        assertEquals(67.0, got.confidence)
        assertEquals(d.asOf, got.at)
        assertEquals(Tone.NEUTRAL, got.tone)
        assertFalse(got.isBuy)
        // More than 12 h later: "Décision…" rather than a stale verdict.
        assertNull(DecisionDigests.fresh(all, asset, d.asOf + 12 * hour + 1))
        assertNotNull(DecisionDigests.fresh(all, asset, d.asOf + 12 * hour))
    }

    @Test fun personalKeptWhileRecent() {
        val personal = btc().copy(verdict = Verdict.BUY_ZONE)
        val t0 = personal.asOf
        val all = DecisionDigests.record(emptyMap(), personal, personal = true, now = t0)
        var info = btc().copy(asOf = t0 + hour)
        // The Radar's informational decision does not replace the personal one seen on the page.
        val kept = DecisionDigests.record(all, info, personal = false, now = t0 + hour)
        assertEquals(Verdict.BUY_ZONE, kept["crypto:BTC"]?.verdict)
        assertEquals(true, kept["crypto:BTC"]?.personal)
        // Once the personal one is stale, it is replaced.
        info = info.copy(asOf = t0 + 13 * hour)
        val replaced = DecisionDigests.record(all, info, personal = false, now = t0 + 13 * hour)
        assertEquals(Verdict.WAIT, replaced["crypto:BTC"]?.verdict)
        // An older answer (offline cache) never replaces a newer one.
        val old = btc().copy(asOf = t0 - hour)
        assertEquals(replaced, DecisionDigests.record(replaced, old, personal = true, now = t0 + 14 * hour))
    }

    @Test fun sortsAndOpportunities() {
        val a = Asset("A", Kind.CRYPTO, "A")
        val b = Asset("B", Kind.STOCK, "B")
        val c = Asset("C", Kind.CRYPTO, "C")
        val e = Asset("E", Kind.CRYPTO, "E")
        val now = 100 * hour
        val all = mapOf(
            a.id to digest(Verdict.WAIT, Rating.HOLD, confidence = 80.0, at = now),
            b.id to digest(Verdict.BUY_ZONE, Rating.BUY, confidence = 40.0, at = now),
            c.id to digest(Verdict.BUY, Rating.BUY, confidence = 70.0, at = now),
            e.id to digest(Verdict.BUY, Rating.STRONG_BUY, confidence = 90.0, at = now - 13 * hour),
        )
        val d = Asset("D", Kind.CRYPTO, "D")
        // Stale (E) and unknown (D) last, in the user's order.
        assertEquals(listOf("C", "B", "A", "D", "E"), DecisionDigests.sortByDecision(listOf(d, a, b, e, c), all, now).map { it.symbol })
        val changes = mapOf("A" to 1.0, "B" to -3.0, "C" to 2.0)
        assertEquals(listOf("B", "C", "A", "D"), DecisionDigests.sortByChange(listOf(d, a, b, c)) { changes[it.symbol] }.map { it.symbol })
        val opp = DecisionDigests.opportunities(listOf(a, b, c, d, e), all, now)
        assertEquals(listOf("C", "B"), opp.map { it.asset.symbol })
        assertEquals("ACHAT", opp.first().decision.badgeLabel)
        assertEquals(listOf("C"), DecisionDigests.opportunities(listOf(a, b, c), all, now, limit = 1).map { it.asset.symbol })
    }

    @Test fun storedRoundTrip() {
        val x = digest(Verdict.BUY_ZONE, Rating.BUY, confidence = 61.0, at = 5.0)
        val y = digest(Verdict.WAIT, Rating.HOLD).copy(note = "zone d'achat 67 653,51 € (−10,1 %)")
        val all = mapOf("crypto:BTC" to x, "stock:AAPL" to y)
        assertEquals(all, DecisionDigests.parse(DecisionDigests.encode(all)))
        assertEquals(emptyMap(), DecisionDigests.parse("{damaged"))
        assertEquals(emptyMap(), DecisionDigests.parse(null))
    }

    /** The short reason under the chip (the decision's `chipNote`): kept in the digest, absent from an older server. */
    @Test fun chipNote() {
        val d = btc()
        assertNull(d.chipNote)
        assertNull(DecisionDigest.of(d, personal = false, now = d.asOf).noteLine)
        val noted = d.copy(chipNote = "zone d'achat 67 653,51 € (−10,1 %)")
        val got = DecisionDigest.of(noted, personal = false, now = noted.asOf)
        assertEquals("ATTENDRE", got.badgeLabel)
        assertEquals("zone d'achat 67 653,51 € (−10,1 %)", got.noteLine)
        assertNull(digest(Verdict.WAIT, Rating.HOLD).noteLine)
        assertNull(digest(Verdict.WAIT, Rating.HOLD).copy(note = " ").noteLine)
        val older = """{"crypto:BTC":{"verdict":"wait","rating":"hold","label":"ATTENDRE","level":"waiting","confidence":55,"personal":false,"at":1}}"""
        assertNull(DecisionDigests.parse(older)["crypto:BTC"]?.note)
    }
}
