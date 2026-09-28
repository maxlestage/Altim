package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Same cases as web/test/config-changes.test.ts, on the same decision sample (backend/tests/samples/decision-btc.json). */
class ConfigChangesTest {
    private fun btc(): Decision =
        AltimJson.decodeFromString(Decision.serializer(), requireNotNull(javaClass.getResource("/fixtures/decision-btc.json")).readText())

    @Test fun firstSightingIsABaselineThenANewVerdictIsATransition() {
        val d = btc()
        val first = ConfigChanges.apply(ConfigState(), d, false, 1000.0)
        assertNull(first.transition)
        val next = btc().copy(verdict = Verdict.BUY_ZONE, label = "ZONE D'ACHAT", level = DecisionLevel.MODERATE, levelLabel = "Signal modéré")
        val second = ConfigChanges.apply(first.state, next, false, 2000.0)
        val t = assertNotNull(second.transition)
        assertEquals(d.verdict, t.from.verdict)
        assertEquals(Verdict.BUY_ZONE, t.to.verdict)
        assertEquals(1000.0, t.since)
        assertEquals("🚨 BTC — changement de configuration : ${d.label} → ZONE D'ACHAT", t.title)
        assertEquals(ConfigChanges.snapshotOf(next, 0.0).missing, t.missing)
        assertEquals(d.setup!!.steps.count { it.state != StepState.OK }, t.missing.size)
        assertEquals(d.toBuy.size, t.triggers.size)
        // Same configuration again: nothing new.
        assertNull(ConfigChanges.apply(second.state, next, false, 3000.0).transition)
        // Personal mode has its own baseline.
        assertNull(ConfigChanges.apply(second.state, d, true, 3000.0).transition)
    }

    @Test fun aLevelChangeAloneIsToldWithTheLevels() {
        val s = ConfigChanges.apply(ConfigState(), btc(), false, 1.0).state
        val t = assertNotNull(ConfigChanges.apply(s, btc().copy(level = DecisionLevel.HIGH_RISK, levelLabel = "Risque élevé"), false, 2.0).transition)
        assertTrue(t.title.contains("→ Risque élevé"))
    }

    @Test fun atMost50TransitionsNewestFirst() {
        var s = ConfigState()
        for (i in 0 until 60) s = ConfigChanges.apply(s, btc().copy(verdict = if (i % 2 == 1) Verdict.WAIT else Verdict.BUY), false, i.toDouble()).state
        assertEquals(ConfigChanges.MAX_TRANSITIONS, s.transitions.size)
        assertEquals(59.0, s.transitions[0].at)
    }

    @Test fun damagedStorageIsDroppedBadEntriesFiltered() {
        assertEquals(ConfigState(), ConfigChanges.parse("{nope"))
        assertEquals(ConfigState(), ConfigChanges.parse("""{"version":3,"last":{},"transitions":[]}"""))
        val good = ConfigChanges.apply(ConfigChanges.apply(ConfigState(), btc(), false, 1.0).state, btc().copy(verdict = Verdict.SELL), false, 2.0).state
        val raw = ConfigChanges.encode(good)
            .replaceFirst("\"last\":{", "\"last\":{\"bad\":{\"verdict\":\"hodl\"},")
            .replace("\"transitions\":[", "\"transitions\":[{\"symbol\":1},")
        val p = ConfigChanges.parse(raw)
        assertEquals(good.last.keys, p.last.keys)
        assertEquals(1, p.transitions.size)
        // What is written reads back identical.
        assertEquals(good, ConfigChanges.parse(ConfigChanges.encode(good)))
    }

    @Test fun missingStepsAndTriggersSaidInFrench() {
        val snap = ConfigChanges.snapshotOf(btc(), 0.0)
        assertTrue(snap.missing.all { it.contains(" : pas encore") || it.contains(" : non vérifiable") }, snap.missing.toString())
        assertEquals(ConfigChanges.snapshotKey(Kind.CRYPTO, "BTC", false), "crypto:BTC:i")
    }

    // ---------- Why the signal changed (web "why the signal changed") ----------

    private fun m(
        price: Double? = 342.0, composite: Double? = 24.0,
        families: List<SnapshotFamily> = listOf(SnapshotFamily("trend", "Tendance", 50.0), SnapshotFamily("momentum", "Momentum", 40.0), SnapshotFamily("news", "Actualités", 0.0)),
        relVolume: Double? = 1.4, rsi: Double? = 62.0, support: SignalLevel? = SignalLevel(335.61, 2), resistance: SignalLevel? = SignalLevel(344.95, 2),
        newsScore: Double? = 0.0, topNews: SignalNews? = null,
    ) = SignalMetrics(price, composite, families, relVolume, rsi, support, resistance, newsScore, topNews)

    @Test fun everyMeasuredChangeIsToldMostTellingFirst() {
        val next = m(
            composite = 6.0,
            families = listOf(SnapshotFamily("trend", "Tendance", 45.0), SnapshotFamily("momentum", "Momentum", 22.0), SnapshotFamily("news", "Actualités", null)),
            relVolume = 0.7, rsi = 48.0, resistance = SignalLevel(344.9, 3),
            topNews = SignalNews("Apple cuts iPhone orders", "negative"),
        )
        assertEquals(
            listOf(
                "Score composite +24 → +6",
                "Momentum −18 pts (+40 → +22)",
                "Actualités : plus mesuré(e) (données indisponibles)",
                "Volume en baisse (1,4× → 0,7× la moyenne)",
                "RSI 62 → 48",
                "Résistance 344,90\u202F$ confirmée (touchée 3 fois)",
                "Actualité négative : « Apple cuts iPhone orders »",
            ),
            ConfigChanges.explainChange(m(), next),
        )
    }

    @Test fun levelsBrokenOrCrossedSmallMovesIgnored() {
        assertEquals(listOf("Support 335,61\u202F$ cassé (prix 330\u202F$)"), ConfigChanges.explainChange(m(), m(price = 330.0, composite = 22.0, rsi = 58.0)))
        assertEquals(listOf("Résistance 344,95\u202F$ franchie (prix 346\u202F$)"), ConfigChanges.explainChange(m(), m(price = 346.0, resistance = SignalLevel(360.0, 2))))
        assertEquals(emptyList(), ConfigChanges.explainChange(m(), m()))
    }

    @Test fun theTransitionCarriesTheExplanationARatingChangeAloneIsATransition() {
        val d = btc()
        fun score(v: Double) = Decision.CompositeScore(value = v)
        val s = ConfigChanges.apply(ConfigState(), d.copy(rating = Rating.BUY, ratingLabel = "ACHAT", score = score(24.0)), false, 1.0).state
        assertEquals(d.families.size, s.last.values.first().metrics?.families?.size)
        val t = assertNotNull(ConfigChanges.apply(s, d.copy(rating = Rating.HOLD, ratingLabel = "ATTENDRE", score = score(-30.0)), false, 2.0).transition)
        assertEquals("🚨 BTC — changement de configuration : note ACHAT → note ATTENDRE", t.title)
        // Older decision without `snapshot`: the measurements are read from the decision itself.
        assertEquals(listOf("Score composite +24 → −30"), t.changes)
    }

    @Test fun version1StorageIsMigratedDamagedMeasurementsAreDropped() {
        val v1 = """{"version":1,"last":{"crypto:BTC:i":{"verdict":"wait","level":"waiting","label":"ATTENDRE","levelLabel":"Attente","missing":[],"triggers":[],"at":1}},""" +
            """"transitions":[{"symbol":"BTC","kind":"crypto","name":"Bitcoin","personal":false,"at":2,"since":1,"from":{"verdict":"buy","level":"moderate","label":"ACHETER","levelLabel":"Signal modéré"},"to":{"verdict":"wait","level":"waiting","label":"ATTENDRE","levelLabel":"Attente"},"missing":[],"triggers":[]}]}"""
        val p = ConfigChanges.parse(v1)
        assertEquals(2, p.version)
        assertNull(p.last["crypto:BTC:i"]?.metrics)
        assertEquals(emptyList(), p.transitions[0].changes)
        val bad = v1.replace("\"version\":1", "\"version\":2").replace("\"at\":1}}", "\"at\":1,\"metrics\":{\"price\":\"x\"}}}")
        val q = ConfigChanges.parse(bad)
        assertEquals(setOf("crypto:BTC:i"), q.last.keys)
        assertNull(q.last["crypto:BTC:i"]?.metrics)
        // A migrated baseline (no measurements) gives a transition without explanation, never a made-up one.
        val t = assertNotNull(ConfigChanges.apply(p, btc().copy(verdict = Verdict.BUY, label = "ACHETER"), false, 3.0).transition)
        assertEquals(emptyList(), t.changes)
        // What is written is version 2, read back identical.
        val written = ConfigChanges.apply(p, btc(), false, 4.0).state
        assertTrue(ConfigChanges.encode(written).startsWith("{\"version\":2"))
        assertEquals(written, ConfigChanges.parse(ConfigChanges.encode(written)))
    }

    @Test fun theDecisionCardFindsTheChangeThatLedToWhatItShows() {
        val d = btc()
        val s = ConfigChanges.apply(ConfigState(), d, false, 1.0).state
        val next = btc().copy(verdict = Verdict.BUY_ZONE, level = DecisionLevel.MODERATE)
        val state = ConfigChanges.apply(s, next, false, 2.0).state
        assertEquals(1.0, ConfigChanges.latestChange(state.transitions, next)?.since)
        assertNull(ConfigChanges.latestChange(state.transitions, d))
        assertNull(ConfigChanges.latestChange(state.transitions, next.copy(mode = "personal")))
    }
}
