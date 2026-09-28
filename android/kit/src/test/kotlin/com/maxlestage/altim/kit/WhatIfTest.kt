package com.maxlestage.altim.kit

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

// Same cases, same generated data and same expected values as web/test/whatif.test.ts.

private const val WDAY = 86_400_000.0
private val W0 = LocalDate.of(2026, 1, 5).atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli().toDouble() // a Monday

private fun wseries(closes: List<Double>, times: List<Double>) = closes.mapIndexed { i, c -> Candle(times[i], c, c, c, c, 1.0) }

private fun wreturns(n: Int, seed: Long): List<Double> {
    var x = seed
    return List(n) {
        x = (x * 16807) % 2147483647
        (x.toDouble() / 2147483647 - 0.5) * 0.04
    }
}

private fun wpath(r: List<Double>): List<Double> {
    val out = mutableListOf(100.0)
    r.forEach { out += out.last() * (1 + it) }
    return out
}

private fun everyDay(n: Int) = List(n) { W0 + it * WDAY }
private fun weekdays(n: Int) = everyDay(ceil(n * 1.5).toInt()).filter {
    val d = Instant.ofEpochMilli(it.toLong()).atOffset(ZoneOffset.UTC).dayOfWeek.value
    d != 6 && d != 7
}.take(n)

private fun wclose(expected: Double, actual: Double?, eps: Double = 1e-6) =
    assertTrue(actual != null && abs(expected - actual) < eps, "attendu $expected, obtenu $actual")

class WhatIfTest {
    @Test fun measuredOnlyOnTheDaysBothHaveAClose() {
        val days = weekdays(121)
        val q = wreturns(120, 11)
        val factor = wseries(wpath(q), days)
        // The crypto moves 1.5 × QQQ on weekdays and has extra weekend candles with big moves.
        val cryptoTimes = everyDay(ceil(121 * 1.5).toInt())
        val scaled = wpath(q.map { 1.5 * it })
        val byDay = days.mapIndexed { i, t -> t to scaled[i] }.toMap()
        var last = 100.0
        val crypto = wseries(cryptoTimes.map { t -> byDay[t]?.also { last = it } ?: (last * 1.03) }, cryptoTimes)
        val b = WhatIf.factorBeta(crypto, factor)
        assertTrue(b.estimated)
        assertEquals(90, b.days)
        wclose(1.5, b.beta)
        wclose(1.0, b.correlation)
        // One year of shared sessions for « Et si… ? ».
        assertEquals(250, WhatIf.BETA_DAYS)
        assertEquals(120, WhatIf.factorBeta(crypto, factor, WhatIf.BETA_DAYS).days)
    }

    @Test fun under30SharedDaysNotEstimated() {
        val t = everyDay(20)
        val b = WhatIf.factorBeta(wseries(wpath(wreturns(19, 3)), t), wseries(wpath(wreturns(19, 5)), t))
        assertTrue(!b.estimated)
        assertEquals(19, b.days)
        assertEquals(30, PortfolioRisk.MIN_BETA_DAYS)
    }

    private fun h(id: String, symbol: String, kind: Kind, quantity: Double) = Holding(id, Asset(symbol, kind, symbol), quantity, 100.0)
    private val a = RiskPortfolio.of(
        listOf(h("1", "NVDA", Kind.STOCK, 30.0), h("2", "NVDA", Kind.STOCK, 10.0), h("3", "QQQ", Kind.STOCK, 20.0), h("4", "DOGE", Kind.CRYPTO, 20.0)),
        2_000.0,
        mapOf("stock:NVDA" to 100.0, "stock:QQQ" to 100.0, "crypto:DOGE" to 100.0),
        emptyMap(),
    )

    @Test fun eachLineMovesByItsBetaTheFactorIsBeta1ALineWithoutBetaIsNotCovered() {
        val r = WhatIf.whatIf(a, "qqq", -10.0, mapOf("stock:NVDA" to FactorBeta(2.0, 90, true, 0.8), "crypto:DOGE" to FactorBeta(1.0, 12, false, null)))
        assertEquals(10_000.0, r.base)
        val nvda = r.lines.first { it.symbol == "NVDA" }
        assertEquals(4_000.0, nvda.value)
        assertEquals(-20.0, nvda.movePercent)
        assertEquals(800.0, nvda.loss)
        assertEquals(2.0, nvda.beta)
        val qqq = r.lines.first { it.symbol == "QQQ" }
        assertTrue(qqq.reference)
        assertEquals(200.0, qqq.loss)
        val doge = r.lines.first { it.symbol == "DOGE" }
        assertNull(doge.loss)
        assertNull(doge.movePercent)
        assertEquals(1_000.0, r.loss)
        assertEquals(10.0, r.lossPercent)
        assertEquals(listOf("DOGE"), r.uncovered)
        assertEquals(2_000.0, r.uncoveredValue)
        assertEquals("NVDA", r.worst!!.symbol)
        assertEquals(90 to 90, r.minDays to r.maxDays)
    }

    @Test fun anAmountIsSpreadOnTheCurrentWeightsAMoveNeverGoesBelowMinus100() {
        val r = WhatIf.whatIf(a, "qqq", -50.0, mapOf("stock:NVDA" to FactorBeta(3.0, 60, true, 0.9)), 1_000.0)
        assertTrue(r.scaled)
        assertEquals(200.0, r.cash)
        val nvda = r.lines.first { it.symbol == "NVDA" }
        assertEquals(400.0, nvda.value)
        assertEquals(-100.0, nvda.movePercent)
        assertEquals(400.0, nvda.loss)
        assertEquals(500.0, r.loss)
        assertEquals(50.0, r.lossPercent)
    }

    @Test fun aNegativeBetaGainsInTheFallTheWorstLineIsTheBiggestLossOnly() {
        val r = WhatIf.whatIf(a, "spy", -10.0, mapOf("stock:NVDA" to FactorBeta(-0.5, 90, true, -0.3), "stock:QQQ" to FactorBeta(1.2, 90, true, 0.95)))
        assertEquals(-200.0, r.lines.first { it.symbol == "NVDA" }.loss)
        assertEquals("QQQ", r.worst!!.symbol)
    }
}
