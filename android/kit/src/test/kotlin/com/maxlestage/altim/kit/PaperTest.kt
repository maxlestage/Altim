package com.maxlestage.altim.kit

import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.fail

private fun paperFixture(): String = requireNotNull(PaperTest::class.java.getResource("/fixtures/paper-fixture.json")).readText()

/** Every field written, nulls included, to compare with the fixture written by JSON.stringify. */
private val Full = Json {
    encodeDefaults = true
    explicitNulls = true
}

/** Same JSON, numbers within 1e-6, object keys identical (a missing or extra field fails). */
private fun same(path: String, expected: JsonElement, actual: JsonElement) {
    when (expected) {
        is JsonNull -> if (actual !is JsonNull) fail("$path : attendu null, obtenu $actual")
        is JsonPrimitive -> {
            if (actual !is JsonPrimitive || actual is JsonNull) fail("$path : attendu $expected, obtenu $actual")
            if (expected.isString) {
                assertTrue(actual.isString, "$path : attendu une chaîne")
                assertEquals(expected.content, actual.content, path)
            } else if (expected.content == "true" || expected.content == "false") {
                assertEquals(expected.content, actual.content, path)
            } else {
                val e = expected.double
                val a = actual.double
                assertTrue(abs(e - a) <= 1e-6, "$path : attendu $e, obtenu $a")
            }
        }
        is JsonObject -> {
            if (actual !is JsonObject) fail("$path : attendu un objet, obtenu $actual")
            assertEquals(expected.keys, actual.keys, "$path : champs")
            expected.forEach { (k, v) -> same("$path.$k", v, actual.getValue(k)) }
        }
        is JsonArray -> {
            if (actual !is JsonArray) fail("$path : attendu une liste, obtenu $actual")
            assertEquals(expected.size, actual.size, "$path : longueur")
            expected.forEachIndexed { i, v -> same("$path[$i]", v, actual[i]) }
        }
    }
}

class PaperTest {
    private val t0 = 1_788_271_200_000.0 // Date.UTC(2026, 8, 1, 14, 0)
    private fun day(d: Int) = 1_788_220_800_000.0 + d * 86_400_000.0 // Date.UTC(2026, 8, 1 + d)
    private fun candle(time: Double, open: Double, high: Double, low: Double, close: Double) = Candle(time, open, high, low, close)
    private fun order(id: String, symbol: String, kind: Kind, price: Double, amount: Double, stop: Double? = null, target: Double? = null) =
        OpenOrder(id, symbol, kind, symbol, price, amount, stop, target)

    @Test fun jsRoundMatchesJavaScript() {
        // Math.round: ties toward +∞, and floor(x + 0.5) is wrong for these.
        assertEquals(0.0, Paper.jsRound(0.49999999999999994))
        assertEquals(3.0, Paper.jsRound(2.5))
        assertEquals(-2.0, Paper.jsRound(-2.5))
        assertEquals(-3.0, Paper.jsRound(-2.5000001))
        assertEquals(4503599627370497.0, Paper.jsRound(4503599627370497.0))
        // 1.005 × 100 = 100.49999999999999 in binary: Math.round(1.005 * 100) / 100 is 1 in JavaScript.
        assertEquals(1.0, Paper.round(1.005, 2))
        assertEquals(1.01, Paper.round(1.0051, 2))
    }

    /** The shared reference scenario: every step replayed and compared, field by field, with the web engine's output. */
    @Test fun referenceScenario() {
        val steps = Json.parseToJsonElement(paperFixture()).jsonArray
        assertEquals(15, steps.size)
        var s = newPaper(10_000.0, t0)
        steps.forEachIndexed { i, entry ->
            val e = entry.jsonObject
            val step = e.getValue("step").jsonObject
            var error: String? = null
            var closed: List<String>? = null
            var value: PaperValuation? = null
            val now = step["now"]?.jsonPrimitive?.double ?: 0.0
            when (val op = step.getValue("op").jsonPrimitive.content) {
                "new" -> s = newPaper(step.getValue("capital").jsonPrimitive.double, now)
                "open" -> {
                    val o = AltimJson.decodeFromJsonElement(OpenOrder.serializer(), step.getValue("order"))
                    openPosition(s, o, now).let { s = it.state; error = it.error }
                }
                "close" -> closePosition(s, step.getValue("id").jsonPrimitive.content, step.getValue("price").jsonPrimitive.double, now).let { s = it.state; error = it.error }
                "exits" -> {
                    val candles = AltimJson.decodeFromJsonElement(MapSerializer(String.serializer(), ListSerializer(Candle.serializer())), step.getValue("candles"))
                    checkExits(s, candles).let { r -> s = r.state; closed = r.closed.map { it.id } }
                }
                "value" -> value = valuation(s, AltimJson.decodeFromJsonElement(MapSerializer(String.serializer(), Double.serializer().nullable), step.getValue("prices")))
                else -> fail("étape inconnue $op")
            }
            val at = "étape $i ($step)"
            assertEquals(e["error"]?.jsonPrimitive?.content, error, "$at : erreur")
            assertEquals(e["closed"]?.jsonArray?.map { it.jsonPrimitive.content }, closed, "$at : positions fermées")
            same("$at.state", e.getValue("state"), Full.encodeToJsonElement(PaperState.serializer(), s))
            same("$at.stats", e.getValue("stats"), Full.encodeToJsonElement(PaperStats.serializer(), paperStats(s)))
            val v = e["valuation"]
            if (v == null) assertNull(value, "$at : valorisation inattendue")
            else same("$at.valuation", v, Full.encodeToJsonElement(PaperValuation.serializer(), assertNotNull(value)))
        }
        // The hand-checked totals of paper.test.ts.
        assertEquals(10_509.12, s.cash)
        val st = paperStats(s)
        assertEquals(5, st.trades)
        assertEquals(40.0, st.winRate)
        assertEquals(ReasonCounts(stop = 2, target = 2, manual = 1), st.byReason)
        assertEquals(listOf("buy" to 2, "buyZone" to 1, "none" to 1, "wait" to 1), st.byVerdict.map { it.verdict to it.trades })
    }

    @Test fun purchasePaysFeesAndSlippage() {
        val (state, error) = openPosition(newPaper(10_000.0, t0), order("a", "BTC", Kind.CRYPTO, 64_000.0, 3_000.0, 60_000.0, 72_000.0), t0)
        assertNull(error)
        val p = state.positions[0]
        assertEquals(64_000 * (1 + Paper.SLIPPAGE), p.entry, 1e-6)
        assertEquals((3_000 * (1 - Paper.FEE_RATE)) / (64_000 * 1.0005), p.quantity, 1e-9)
        assertEquals(7_000.0, state.cash)
    }

    @Test fun targetReachedNextDay() {
        var s = openPosition(newPaper(10_000.0, t0), order("a", "BTC", Kind.CRYPTO, 64_000.0, 3_000.0, 60_000.0, 72_000.0), t0).state
        // The opening day (low 59 000 < stop) is not used: its low may be before the purchase.
        s = checkExits(s, mapOf("crypto:BTC" to listOf(candle(day(0), 63_000.0, 65_000.0, 59_000.0, 64_500.0)))).state
        assertEquals(1, s.positions.size)
        val r = checkExits(s, mapOf("crypto:BTC" to listOf(candle(day(2), 70_000.0, 73_000.0, 69_000.0, 72_500.0))))
        val t = r.closed[0]
        assertEquals(PaperReason.TARGET, t.reason)
        assertEquals(72_000 * 0.9995, t.exit, 1e-6)
        val qty = (3_000 * 0.999) / (64_000 * 1.0005)
        assertEquals(qty * 71_964 * 0.999, t.proceeds, 0.005)
        assertEquals(364.89, t.pnl)
    }

    @Test fun gapBelowStopAndStopFirst() {
        var s = openPosition(newPaper(10_000.0, t0), order("e", "ETH", Kind.CRYPTO, 2_500.0, 1_000.0, 2_300.0, 3_000.0), t0).state
        var r = checkExits(s, mapOf("crypto:ETH" to listOf(candle(day(1), 2_200.0, 2_250.0, 2_150.0, 2_210.0))))
        assertEquals(2_200 * 0.9995, r.closed[0].exit, 1e-6)
        s = openPosition(newPaper(10_000.0, t0), order("b", "BTC", Kind.CRYPTO, 70_000.0, 2_000.0, 66_000.0, 76_000.0), t0).state
        r = checkExits(s, mapOf("crypto:BTC" to listOf(candle(day(1), 70_000.0, 77_000.0, 65_000.0, 71_000.0))))
        assertEquals(PaperReason.STOP, r.closed[0].reason)
    }

    @Test fun refusals() {
        val s = newPaper(1_000.0, t0)
        val o = order("x", "AAPL", Kind.STOCK, 250.0, 500.0)
        assertTrue(openPosition(s, o.copy(amount = 1_500.0), t0).error!!.contains("insuffisantes"))
        assertEquals("Liquidités simulées insuffisantes (1000 $ disponibles).", openPosition(s, o.copy(amount = 1_500.0), t0).error)
        assertEquals("Prix indisponible.", openPosition(s, o.copy(price = Double.NaN), t0).error)
        assertEquals("Montant invalide.", openPosition(s, o.copy(amount = -1.0), t0).error)
        assertNull(openPosition(s, o.copy(stop = 260.0), t0).state.positions[0].stop)
        assertEquals("Position introuvable.", closePosition(s, "nope", 10.0, t0).error)
    }

    @Test fun valuationAsIfSoldNow() {
        val s = openPosition(newPaper(10_000.0, t0), order("a", "SOL", Kind.CRYPTO, 100.0, 1_000.0), t0).state
        val v = valuation(s, mapOf("crypto:SOL" to 110.0))
        val qty = 999 / 100.05
        assertEquals(qty * 110 * 0.9995 * 0.999, v.lines[0].value!!, 0.005)
        assertEquals(9_000 + v.lines[0].value!!, v.equity, 0.005)
        val none = valuation(s, emptyMap())
        assertEquals(1, none.unpriced)
        assertEquals(10_000.0, none.equity)
    }

    @Test fun emptyStats() {
        val st = paperStats(newPaper(1_000.0, t0))
        assertEquals(0, st.trades)
        assertNull(st.profitFactor)
        assertEquals(0.0, st.maxDrawdownPct)
    }

    /** Same order as JavaScript's localeCompare: case ignored first, so "noPosition" comes after "none". */
    @Test fun verdictOrderLikeLocaleCompare() {
        var s = newPaper(10_000.0, t0)
        listOf("noPosition", "none", "Wait", "buy").forEachIndexed { i, v ->
            val o = order("p$i", "BTC", Kind.CRYPTO, 100.0, 100.0).copy(decision = if (v == "none") null else PaperDecision(v, v.uppercase(), 50.0, t0))
            s = openPosition(s, o, t0).state
            s = closePosition(s, "p$i", 100.0, t0 + i).state
        }
        assertEquals(listOf("buy", "none", "noPosition", "Wait"), paperStats(s).byVerdict.map { it.verdict })
        assertTrue(Paper.localeCompare("noPosition", "none") > 0)
        assertTrue(Paper.localeCompare("a", "B") < 0)
        assertTrue(Paper.localeCompare("buy", "buyZone") < 0)
    }

    @Test fun savedStateChecked() {
        val fresh = newPaper(5_000.0, t0)
        assertTrue(isPaperState(fresh))
        assertEquals(fresh, decodePaper(encodePaper(fresh)))
        // The final state of the scenario, written by the web engine, reads back as is.
        val last = Json.parseToJsonElement(paperFixture()).jsonArray.last().jsonObject.getValue("state").toString()
        assertEquals(10_509.12, decodePaper(last)!!.cash)
        assertFalse(isPaperState(null))
        listOf(
            null, "", "null", "{", "[]", "42",
            """{"version":1,"startCapital":1000,"cash":1000,"positions":[{"id":1}],"trades":[]}""",
            """{"version":2,"startCapital":1000,"cash":1000,"positions":[],"trades":[]}""",
            """{"startCapital":1000,"cash":1000,"positions":[],"trades":[]}""",
            """{"version":1,"startCapital":0,"cash":1000,"positions":[],"trades":[]}""",
            """{"version":1,"startCapital":1000,"cash":"x","positions":[],"trades":[]}""",
            """{"version":1,"startCapital":1000,"cash":1000,"positions":{},"trades":[]}""",
            """{"version":1,"startCapital":1000,"cash":1000,"positions":[{"id":"a","symbol":"BTC","kind":"crypto","name":"B","openedAt":0,"entry":1,"quantity":0,"invested":1}],"trades":[]}""",
            """{"version":1,"startCapital":1000,"cash":1000,"positions":[{"id":"a","symbol":"BTC","kind":"forex","name":"B","openedAt":0,"entry":1,"quantity":1,"invested":1}],"trades":[]}""",
        ).forEach { assertNull(decodePaper(it), "accepté à tort : $it") }
    }
}
