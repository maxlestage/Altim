package com.maxlestage.altim.kit

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import java.time.ZoneOffset
import java.time.ZonedDateTime
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Same cases as the contract and helper parts of web/test/guidance.test.ts, on the server's real AAPL decision
 * (backend/tests/samples/decision-guidance.json); the card itself is checked by the app's screenshot test.
 */
class GuidanceTest {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()
    private fun aapl(): Decision = AltimJson.decodeFromString(Decision.serializer(), raw("decision-guidance.json"))

    @Test fun theServersSampleDecodesWithEveryNewField() {
        val d = aapl()
        assertEquals(false, d.noTrade?.active)
        assertTrue(d.noTrade!!.unchecked.isNotEmpty())
        assertEquals(listOf("exit", "invalidation", "buy", "wait", "profit"), d.actionZones?.zones?.map { it.kind })
        assertEquals("wait", d.actionZones?.here)
        assertEquals(listOf(3, 2, 3), d.scenarios.map { it.conditions.size })
        assertEquals(listOf(d.unfolding!!.kind), d.scenarios.filter { it.unfolding }.map { it.kind })
        assertTrue(Regex("^🟢 Raisons favorables : \\d+ / 🔴 Raisons défavorables : \\d+$").matches(d.counterArgument!!.text))
        assertEquals(d.families.size, d.snapshot?.families?.size)
        assertEquals("rien à signaler", Guidance.noTradeBadge(d.noTrade!!))
    }

    @Test fun wrongGuidanceFieldsAreRefusedOlderAnswersStillPass() {
        fun with(key: String, value: kotlinx.serialization.json.JsonElement): String =
            JsonObject(AltimJson.parseToJsonElement(raw("decision-guidance.json")).jsonObject + (key to value)).toString()
        assertFailsWith<Exception> { AltimJson.decodeFromString(Decision.serializer(), with("noTrade", buildJsonObject { put("reasons", "x") })) }
        assertFailsWith<Exception> { AltimJson.decodeFromString(Decision.serializer(), with("actionZones", buildJsonObject { putJsonArray("zones") {} })) }
        assertFailsWith<Exception> { AltimJson.decodeFromString(Decision.serializer(), with("counterArgument", buildJsonObject { put("invalidators", JsonPrimitive(3)) })) }
        assertFailsWith<Exception> { AltimJson.decodeFromString(Decision.serializer(), with("snapshot", buildJsonObject { putJsonObject("families") {} })) }
        val old = AltimJson.decodeFromString(Decision.serializer(), raw("decision-btc.json"))
        assertNull(old.noTrade)
        assertNull(old.actionZones)
        assertNull(old.counterArgument)
        assertNull(old.snapshot)
        assertTrue(old.scenarios.all { it.conditions.isEmpty() && !it.unfolding })
    }

    @Test fun theLadderReadsFromTheHighestPriceDownWithThePriceMarked() {
        val z = aapl().actionZones!!
        val rows = Guidance.ladder(z)
        assertEquals(listOf("profit", "wait", "buy", "invalidation", "exit"), rows.map { (it as Guidance.LadderRow.Zone).zone.kind })
        assertEquals(listOf("wait"), rows.filterIsInstance<Guidance.LadderRow.Zone>().filter { it.here }.map { it.zone.kind })
        assertEquals("◀ vous êtes ici · 341,04 $", "◀ vous êtes ici · ${Guidance.usd(z.price)}")
        assertEquals("280,57 $ – 307,28 $", Guidance.zoneRange(z.zones.first { it.kind == "buy" }))
        // Price between two zones: a marker of its own, placed above the first zone under it.
        val between = Guidance.ladder(z.copy(price = 275.0, here = null, hereText = "Vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)"))
        val marker = between.indexOfFirst { it is Guidance.LadderRow.Marker }
        val kinds = between.map { (it as? Guidance.LadderRow.Zone)?.zone?.kind }
        assertTrue(marker > kinds.indexOf("buy"))
        assertTrue(marker < kinds.indexOf("invalidation"))
        assertEquals("◀ vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)", assertIs<Guidance.LadderRow.Marker>(between[marker]).text)
        // Price above every zone: the marker first.
        assertIs<Guidance.LadderRow.Marker>(Guidance.ladder(z.copy(price = 500.0, here = null))[0])
    }

    @Test fun scenariosShowTheirChecklistCountsAndTheOneUnfolding() {
        val d = aapl()
        val neutral = d.scenarios.first { it.kind == ScenarioKind.NEUTRAL }
        assertEquals("1/2 conditions · en cours", Guidance.scenarioCount(neutral))
        assertTrue(d.scenarios.first().conditions.any { it.text.startsWith("Clôture au-dessus de 345,34") })
        assertEquals("✓", Guidance.checkIcon("met"))
        assertEquals("non remplie", Guidance.checkLabel("unmet"))
        assertEquals("inconnue", Guidance.checkLabel("unknown"))
        assertTrue(d.counterArgument!!.invalidators.any { it.text.startsWith("Cassure de la résistance 344,95") })
    }

    @Test fun whyTheSignalChanged() {
        val d = aapl()
        val before = d.copy(
            verdict = Verdict.BUY_ZONE, label = "ZONE D'ACHAT", level = DecisionLevel.MODERATE, levelLabel = "Signal modéré",
            snapshot = d.snapshot!!.copy(composite = 30.0, relativeVolume = 1.4),
        )
        val at = { h: Int, m: Int -> ZonedDateTime.of(2026, 9, 28, h, m, 0, 0, ZoneOffset.UTC).toInstant().toEpochMilli().toDouble() }
        val s1 = ConfigChanges.apply(ConfigState(), before, false, at(12, 2)).state
        val t = assertNotNull(ConfigChanges.apply(s1, d, false, at(16, 0)).transition)
        assertEquals("Pourquoi le signal a changé depuis le 28/09 à 14:02", "Pourquoi le signal a changé depuis le ${Format.shortDateTime(t.since)}")
        assertEquals("ZONE D'ACHAT → ATTENDRE", ConfigChanges.changeSummary(t))
        assertTrue("Score composite +30 → +41" in t.changes, t.changes.toString())
        assertTrue("Volume en baisse (1,4× → 0,7× la moyenne)" in t.changes, t.changes.toString())
        assertEquals(t, ConfigChanges.latestChange(ConfigChanges.apply(s1, d, false, at(16, 0)).state.transitions, d))
    }
}
