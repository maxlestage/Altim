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
        assertEquals(ConfigState(), ConfigChanges.parse("""{"version":2,"last":{},"transitions":[]}"""))
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
}
