package com.maxlestage.altim.kit

import java.time.LocalDate
import java.time.ZoneOffset
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

// Same cases, same data and same expected values as web/test/journal.test.ts.

private val T0 = LocalDate.of(2026, 6, 1).atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli().toDouble()
private val AT = T0 + 10 * 3_600_000 // entry at 10:00 UTC on day 0
private const val JDAY = TradeJournal.DAY

private fun near(expected: Double, actual: Double?, eps: Double = 1e-6) =
    assertTrue(actual != null && abs(expected - actual) < eps, "attendu $expected, obtenu $actual")

/** A decision with only what the journal reads. */
private fun decision(
    verdict: Verdict = Verdict.BUY,
    label: String = "Acheter",
    vetoes: List<Decision.Veto> = listOf(Decision.Veto("x", "Volatilité extrême", false, true, "")),
    plan: Decision.Plan? = Decision.Plan(98.0, 102.0, 100.0, 95.0, 110.0, 120.0, 5.0, 10.0, 20.0, 2.0, 2.0, true, "swing"),
    rating: Rating? = Rating.BUY,
    ratingLabel: String = "Achat",
    degraded: Decision.Degraded? = Decision.Degraded(false, "", emptyList()),
    regime: MarketRegime? = MarketRegime(RegimeKind.RISK_ON, "Risk-on"),
) = Decision(
    symbol = "BTC", kind = Kind.CRYPTO, name = "Bitcoin", asOf = AT - 3_600_000, price = 100.0,
    verdict = verdict, label = label, level = DecisionLevel.STRONG, levelLabel = "Signal fort", confidence = 72.0, headline = "Rebond sur support",
    vetoes = vetoes,
    setup = Decision.Setup("Rebond", listOf(Decision.Step("Support", StepState.OK, ""), Decision.Step("Volume", StepState.NO, "")), 1, 2),
    plan = plan, pros = listOf("a", "b", "c", "d"), cons = listOf("x"),
    liquidity = Decision.Liquidity(relativeVolume = 1.4),
    rating = rating, ratingLabel = ratingLabel, score = Decision.CompositeScore(value = 64.0),
    degraded = degraded, marketRegime = regime,
    horizon = Decision.HorizonClass("swing", "Swing", 1.0, ""), events = emptyList(), eventsKnown = true,
)

private fun entry(
    id: String = "e1", side: String = "buy", price: Double = 100.0, stop: Double? = 95.0, targets: List<Double?> = listOf(110.0),
    note: String = "", decision: Decision? = decision(), source: String = "paper",
) = TradeJournal.createEntry(NewTradeEntry(id, AT, source, side, "BTC", Kind.CRYPTO, "Bitcoin", price, stop = stop, targets = targets, note = note, decision = decision))

/** Daily candles from day 0 (the entry day) with (low, high, close) per day. */
private fun days(vararg rows: Triple<Double, Double, Double>): List<Candle> =
    rows.mapIndexed { i, (low, high, close) -> Candle(T0 + i * JDAY, if (i > 0) rows[i - 1].third else 100.0, high, low, close, 10.0) }

private fun d(low: Number, high: Number, close: Number) = Triple(low.toDouble(), high.toDouble(), close.toDouble())

class TradeJournalTest {
    // ---------- Writing entries ----------

    @Test fun snapshotKeepsTheReasonsVetoesSetupAndPlanMarketFromTheDecision() {
        val e = entry(note = "  rebond  ")
        val snap = e.decision!!
        assertEquals(listOf("a", "b", "c"), snap.pros)
        assertEquals(emptyList(), snap.vetoes)
        assertEquals(listOf(JournalDecision.Step("Support", "ok"), JournalDecision.Step("Volume", "no")), snap.setup.steps)
        assertEquals(64.0, snap.score)
        assertEquals("riskOn", e.market.regime)
        assertEquals(1.4, e.market.relativeVolume)
        assertEquals(0, e.market.events)
        assertNull(e.market.macroScore)
        assertNull(e.market.atrPct)
        assertEquals("Achat (Signal fort) · configuration « Rebond » 1/2 · horizon Swing", e.signal)
        assertEquals("rebond", e.note)
    }

    @Test fun aStopAboveThePriceAndTargetsBelowItAreDroppedASaleKeepsNeither() {
        val e = entry(stop = 105.0, targets = listOf(90.0, 130.0, null))
        assertNull(e.stop)
        assertEquals(listOf(130.0), e.targets)
        val s = entry(side = "sell")
        assertNull(s.stop)
        assertEquals(emptyList(), s.targets)
    }

    @Test fun olderDecisionsWithoutTheAddedFieldsStillGiveAnEntry() {
        val old = decision().copy(rating = null, ratingLabel = "", score = null, degraded = null, marketRegime = null, horizon = null, events = null, eventsKnown = false)
        val e = TradeJournal.createEntry(NewTradeEntry("x", AT, "real", "buy", "BTC", Kind.CRYPTO, "Bitcoin", 100.0, decision = old))
        assertNull(e.decision!!.rating)
        assertNull(e.decision!!.score)
        assertEquals(false, e.decision!!.degraded)
        assertEquals("swing", e.decision!!.horizon)
        assertNull(e.market.regime)
        assertNull(e.market.events)
    }

    @Test fun stateHelpersAddPatchRemoveValidate() {
        var s = TradeJournal.addEntry(TradeJournal.empty(), entry())
        s = TradeJournal.addEntry(s, entry())
        assertEquals(1, s.entries.size)
        s = TradeJournal.patchEntry(s, "e1", note = "x", market = JournalMarket(macroScore = 31.0, macroLevel = "tense"))
        assertEquals(31.0, s.entries[0].market.macroScore)
        assertEquals("tense", s.entries[0].market.macroLevel)
        assertEquals("riskOn", s.entries[0].market.regime)
        assertEquals("x", s.entries[0].note)
        val saved = TradeJournal.parseSaved(TradeJournal.encode(s))
        assertNull(saved.error)
        assertEquals(s, saved.state)
        assertEquals(0, TradeJournal.removeEntry(s, "e1").entries.size)
    }

    @Test fun atrAndRelativeVolumeUseOnlyTheDaysClosedBeforeTheEntry() {
        val c = (0 until 30).map { i -> Candle(T0 - (30 - i) * JDAY, 100.0, 102.0, 98.0, 100.0, if (i == 29) 30.0 else 10.0) }.toMutableList()
        // The entry day's own candle (huge range, huge volume) must not count.
        c += Candle(T0, 100.0, 200.0, 50.0, 150.0, 1000.0)
        val (atrPct, relVol) = TradeJournal.marketFromCandles(c, AT)
        near(4.0, atrPct, 1e-5)
        near(3.0, relVol, 1e-5)
    }

    @Test fun aHoldingEditBecomesAPurchaseOrASale() {
        assertEquals(HoldingChange("buy", 1.0, 120.0, true), TradeJournal.holdingChange(1.0, 100.0, 2.0, 110.0, 130.0))
        assertEquals(HoldingChange("buy", 2.0, 130.0, false), TradeJournal.holdingChange(1.0, 100.0, 3.0, 100.0, 130.0))
        assertEquals(HoldingChange("sell", 1.5, 90.0, false), TradeJournal.holdingChange(2.0, 100.0, 0.5, 100.0, 90.0))
        assertNull(TradeJournal.holdingChange(2.0, 100.0, 1.0, 100.0, null))
        assertNull(TradeJournal.holdingChange(2.0, 100.0, 2.0, 90.0, 90.0))
        // No average cost (optional on the phone): the live price.
        assertEquals(HoldingChange("buy", 1.0, 130.0, false), TradeJournal.holdingChange(1.0, null, 2.0, null, 130.0))
    }

    // ---------- Review ----------

    @Test fun target1ReachedOnDay6() {
        // Day 0 (entry day) touches the stop before the purchase: ignored.
        val c = days(d(90, 101, 100), d(99, 103, 102), d(98, 104, 103), d(99, 105, 104), d(100, 106, 105), d(101, 107, 106), d(104, 111, 109), d(105, 108, 107), d(104, 108, 106), d(104, 107, 106), d(103, 106, 105))
        val r = TradeJournal.reviewEntry(entry(), c, AT + 40 * JDAY)
        val h10 = r.horizons.first { it.days == 10 }
        assertEquals("ready", h10.status)
        assertEquals("target1", h10.first)
        assertEquals(6, h10.firstDays)
        assertEquals(2.0, h10.resultR)
        near(11.0, h10.mfePct)
        near(-0.4, h10.maeR)
        assertEquals(2.0, r.planR)
        assertTrue("L'objectif 1 a été atteint en 6 jours (+2 R)." in r.worked, r.worked.toString())
        assertEquals("ready", r.horizons.first { it.days == 30 }.status)
    }

    @Test fun stopTouchedWhileTheSignalWasDegraded() {
        val e = entry(decision = decision(degraded = Decision.Degraded(true, "Données incomplètes", emptyList())))
        val c = days(d(99, 101, 100), d(99, 102, 101), d(94, 112, 96), d(95, 97, 96))
        val r = TradeJournal.reviewEntry(e, c, AT + 5 * JDAY)
        val h3 = r.horizons[0]
        assertEquals("stop", h3.first)
        assertEquals(2, h3.firstDays)
        assertEquals(-1.0, h3.resultR)
        assertEquals("Le stop a été touché en 2 jours (−1 R) alors que le signal était dégradé à l'entrée.", r.failed[0])
        assertEquals(false, r.coherent)
        assertEquals("pending", r.horizons[1].status)
    }

    @Test fun aGapBelowTheStopIsCountedAtTheOpen() {
        val c = days(d(99, 101, 100), d(88, 92, 90)).toMutableList()
        c[1] = c[1].copy(open = 90.0)
        assertEquals(-2.0, TradeJournal.reviewEntry(entry(), c, AT + 4 * JDAY).horizons[0].resultR)
    }

    @Test fun coherenceVetoesRiskRewardZoneRegimeVerdict() {
        val dd = decision(
            verdict = Verdict.WAIT, label = "Attendre",
            vetoes = listOf(Decision.Veto("v", "Annonce macro dans les 48 h", true, true, "")),
            plan = decision().plan!!.copy(riskReward = 1.2),
            regime = MarketRegime(RegimeKind.RISK_OFF, "Risk-off"),
        )
        val checks = TradeJournal.coherenceChecks(entry(price = 104.08, decision = dd)).associateBy { it.code }
        assertEquals(false, checks.getValue("vetoes").ok)
        assertTrue(checks.getValue("vetoes").detail.contains("Annonce macro dans les 48 h"))
        assertEquals(false, checks.getValue("rr").ok)
        assertEquals("Entrée hors zone d'achat : +2 % au-dessus du haut de la zone.", checks.getValue("zone").detail)
        assertEquals(false, checks.getValue("regime").ok)
        assertEquals(false, checks.getValue("verdict").ok)
        assertEquals(true, checks.getValue("degraded").ok)
    }

    @Test fun withoutADecisionNothingIsVerifiableWithoutAStopNoR() {
        val bare = entry(decision = null, stop = null, targets = emptyList())
        val r = TradeJournal.reviewEntry(bare, days(d(99, 101, 100), d(100, 104, 103), d(101, 105, 104), d(102, 106, 105)), AT + 4 * JDAY)
        assertNull(r.coherent)
        assertNull(r.risk)
        assertNull(r.horizons[0].resultR)
        assertEquals("+5 % en 3 jours (sans stop, résultat en R non mesurable).", r.worked[0])
        val fromPlan = TradeJournal.reviewEntry(entry(stop = null, targets = emptyList()), emptyList(), AT)
        assertEquals(JournalLevels(95.0, "plan", listOf(110.0, 120.0), "plan"), fromPlan.levels)
    }

    @Test fun aHistoryThatStartsAfterTheEntryGivesNoReview() {
        val c = days(d(99, 101, 100), d(99, 101, 100), d(99, 101, 100), d(99, 101, 100)).drop(2)
        assertEquals("noData", TradeJournal.reviewEntry(entry(), c, AT + 5 * JDAY).horizons[0].status)
    }

    @Test fun aClosedPaperTradeAddsItsRealizedR() {
        assertEquals(1.0, TradeJournal.reviewEntry(entry(), emptyList(), AT + JDAY, ClosedInfo(AT + JDAY, 105.0, "vente manuelle")).realizedR)
    }

    @Test fun aSaleThePriceAfterTheSaleCheckedAgainstTheDecision() {
        val e = entry(side = "sell", decision = decision(verdict = Verdict.TRIM, label = "Alléger"))
        val r = TradeJournal.reviewEntry(e, days(d(99, 101, 100), d(95, 99, 96), d(94, 97, 95), d(93, 96, 94)), AT + 4 * JDAY)
        assertTrue(r.worked[0].contains("3 jours après la vente, le cours est à −6 %"), r.worked[0])
        assertNull(r.planRespected)
        assertEquals(true, r.coherent)
    }

    // ---------- Profile ----------

    @Test fun groupsByRatingPlanAndRegimeWithSampleSizes() {
        val win = days(d(99, 101, 100), d(100, 111, 110), d(104, 108, 107), d(104, 108, 107))
        val loss = days(d(99, 101, 100), d(94, 100, 95), d(95, 97, 96), d(95, 97, 96))
        val reviews = (0 until 5).map { TradeJournal.reviewEntry(entry(id = "w$it"), win, AT + 5 * JDAY) } +
            TradeJournal.reviewEntry(entry(id = "l", decision = decision(regime = MarketRegime(RegimeKind.RISK_OFF, "Risk-off"), rating = Rating.HOLD, ratingLabel = "Conserver")), loss, AT + 5 * JDAY)
        val p = TradeJournal.profile(reviews, 3)
        assertEquals(6, p.reviewed)
        val all = assertNotNull(p.all)
        assertEquals(6, all.n)
        assertEquals(1.5, all.avgR)
        assertEquals(false, all.lowSample)
        assertEquals(listOf(Triple("buy", 5, false), Triple("hold", 1, true)), p.byRating.map { Triple(it.key, it.n, it.lowSample) })
        assertEquals(listOf("yes" to 5, "no" to 1), p.byPlan.map { it.key to it.n })
        assertEquals(listOf("riskOn" to 2.0, "riskOff" to -1.0), p.byRegime.map { it.key to it.avgR })
        assertEquals(-1.2, p.byRegime[1].worstMaeR)
        assertEquals(0, TradeJournal.profile(reviews, 10).reviewed)
    }

    @Test fun savedJournalEmptyValidUnreadable() {
        assertEquals(SavedJournal(TradeJournal.empty(), null), TradeJournal.parseSaved(null))
        val s = TradeJournal.addEntry(TradeJournal.empty(), entry())
        assertEquals(1, TradeJournal.parseSaved(TradeJournal.encode(s)).state.entries.size)
        assertTrue(TradeJournal.parseSaved("{oops").error!!.contains("illisible"))
        assertTrue(TradeJournal.parseSaved("""{"version":2,"entries":[]}""").error!!.contains("illisible"))
        assertTrue(TradeJournal.parseSaved("""{"version":1,"entries":[{"id":1}]}""").error!!.contains("illisible"))
    }

    @Test fun snapshotDecisionWithoutAPlan() {
        assertNull(TradeJournal.snapshotDecision(decision(plan = null)).plan)
    }
}
