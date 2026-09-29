package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/** Same rules as web technicalText, ratingTone, RATING_RANK and the Radar's DecisionBadge / opportunities. */
class RadarDecisionsTest {
    private val h = 3_600_000.0
    private val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
    private val eth = Asset("ETH", Kind.CRYPTO, "Ethereum")
    private val aapl = Asset("AAPL", Kind.STOCK, "Apple")
    private val nvda = Asset("NVDA", Kind.STOCK, "NVIDIA")

    private fun snap(verdict: Verdict, at: Double, rating: Rating? = null, confidence: Double? = null) =
        ConfigSnapshot(verdict, DecisionLevel.MODERATE, verdict.label, "Signal modéré", emptyList(), emptyList(), at, rating, rating?.label, null, confidence)

    private fun state(vararg e: Pair<String, ConfigSnapshot>) = ConfigState(last = e.toMap())

    private fun key(a: Asset, personal: Boolean = false) = ConfigChanges.snapshotKey(a.kind, a.symbol, personal)

    @Test fun theTechnicalSignalIsADirectionNotAnOrder() {
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
        assertEquals(listOf(0, 1, 2, 3, 4, 5, 9), (Rating.entries + listOf<Rating?>(null)).map { RadarDecisions.rank(it) })
        assertEquals(Tone.GOOD, RadarDecisions.tone(Rating.STRONG_BUY, Verdict.WAIT))
        assertEquals(Tone.GOOD, RadarDecisions.tone(Rating.BUY, Verdict.BUY_ZONE))
        assertEquals(Tone.NEUTRAL, RadarDecisions.tone(Rating.HOLD, Verdict.BUY))
        assertEquals(Tone.BAD, RadarDecisions.tone(Rating.REDUCE, Verdict.WAIT))
        assertEquals(Tone.BAD, RadarDecisions.tone(Rating.STRONG_SELL, Verdict.SELL))
        // Older servers without a rating: the verdict decides.
        assertEquals(Tone.GOOD, RadarDecisions.tone(null, Verdict.BUY_ZONE))
        assertEquals(Tone.BAD, RadarDecisions.tone(null, Verdict.TRIM))
        assertEquals(Tone.NEUTRAL, RadarDecisions.tone(null, Verdict.NO_POSITION))
    }

    @Test fun theBadgeReadsTheRatingElseTheVerdict() {
        assertEquals("ATTENDRE", RadarDecisions.label(snap(Verdict.WAIT, 0.0, Rating.HOLD)))
        assertEquals("ACHAT FORT", RadarDecisions.label(snap(Verdict.BUY, 0.0, Rating.STRONG_BUY)))
        assertEquals("ZONE D'ACHAT", RadarDecisions.label(snap(Verdict.BUY_ZONE, 0.0)))
    }

    @Test fun theFreshestDecisionUnder12Hours() {
        val now = 100 * h
        val info = snap(Verdict.WAIT, now - 2 * h, Rating.HOLD)
        val personal = snap(Verdict.BUY_ZONE, now - h, Rating.BUY)
        assertEquals(personal, RadarDecisions.cached(state(key(btc) to info, key(btc, true) to personal), btc, now))
        assertEquals(info, RadarDecisions.cached(state(key(btc) to info, key(btc, true) to snap(Verdict.BUY, now - 13 * h)), btc, now))
        assertNull(RadarDecisions.cached(state(key(btc) to snap(Verdict.BUY, now - 12.5 * h)), btc, now))
        assertNull(RadarDecisions.cached(state(key(btc) to info), eth, now))
    }

    @Test fun decisionOrderThenConfidenceStable() {
        val now = 10 * h
        val s = state(
            key(btc) to snap(Verdict.WAIT, now, Rating.HOLD, 70.0),
            key(eth) to snap(Verdict.BUY_ZONE, now, Rating.BUY, 55.0),
            key(aapl) to snap(Verdict.BUY, now, Rating.BUY, 80.0),
        )
        // NVDA has no decision yet: last.
        assertEquals(listOf(aapl, eth, btc, nvda), RadarDecisions.sorted(listOf(nvda, btc, eth, aapl), s, now))
    }

    @Test fun opportunitiesAreFullDecisionsToBuy() {
        val now = 10 * h
        val s = state(
            key(btc) to snap(Verdict.WAIT, now, Rating.HOLD, 90.0),
            key(eth) to snap(Verdict.BUY_ZONE, now, Rating.BUY, 55.0),
            key(aapl) to snap(Verdict.BUY, now, Rating.STRONG_BUY, 80.0),
            key(nvda) to snap(Verdict.BUY, now - 13 * h, Rating.BUY, 99.0),
        )
        assertEquals(listOf(aapl, eth), RadarDecisions.opportunities(listOf(btc, eth, aapl, nvda), s, now).map { it.first })
    }

    @Test fun theSnapshotKeepsTheConfidence() {
        val d = Decision(symbol = "BTC", kind = Kind.CRYPTO, confidence = 64.0)
        assertEquals(64.0, ConfigChanges.snapshotOf(d, 1.0).confidence)
        val round = ConfigChanges.parse(ConfigChanges.encode(ConfigChanges.apply(ConfigState(), d, false, 1.0).state))
        assertEquals(64.0, round.last[key(btc)]?.confidence)
    }
}
