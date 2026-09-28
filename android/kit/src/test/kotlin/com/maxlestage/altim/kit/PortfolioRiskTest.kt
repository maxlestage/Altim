package com.maxlestage.altim.kit

import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

// Same cases, same generated data and same expected numbers as web/test/portfolio-risk.test.ts: the pseudo-random
// series come from the same generator (Park–Miller, exact in both languages), so both engines see identical inputs.

private const val DAY = 86_400_000.0
private const val T0 = 1_767_225_600_000.0 // Date.UTC(2026, 0, 1)

/** Daily candles from closes (high/low ± 1 %). */
private fun series(closes: List<Double>, start: Double = T0): List<Candle> =
    closes.mapIndexed { i, c -> Candle(start + i * DAY, c, c * 1.01, c * 0.99, c, 1.0) }

/** Deterministic pseudo-random daily returns. */
private fun returns(n: Int, seed: Long): List<Double> {
    var x = seed
    return List(n) {
        x = (x * 16807) % 2147483647
        (x.toDouble() / 2147483647 - 0.5) * 0.06
    }
}

private fun path(r: List<Double>, start: Double = 100.0): List<Double> {
    val out = mutableListOf(start)
    r.forEach { out += out.last() * (1 + it) }
    return out
}

private fun h(id: String, symbol: String, kind: Kind, quantity: Double, averagePrice: Double, stop: Double? = null) =
    Holding(id, Asset(symbol, kind, symbol), quantity, averagePrice, stop)

private fun close(expected: Double, actual: Double, digits: Int = 9) =
    assertTrue(abs(expected - actual) < 0.5 * Math.pow(10.0, -digits.toDouble()), "attendu $expected, obtenu $actual")

class PortfolioRiskTest {
    private val bench = returns(120, 7)

    @Test fun betaOfTwiceTheBenchmark() {
        val b = PortfolioRisk.estimateBeta(series(path(bench.map { 2 * it })), series(path(bench)))
        assertTrue(b.estimated)
        assertEquals(90, b.days)
        close(2.0, b.beta, 6)
    }

    @Test fun tooShortAHistoryFallsBackToOne() {
        // A recent listing: 20 days shared with the benchmark.
        val b = PortfolioRisk.estimateBeta(series(path(bench.take(20)), T0 + 100 * DAY), series(path(bench)))
        assertEquals(BetaEstimate(1.0, 0, false), b)
    }

    private val stressHoldings = listOf(h("1", "ETH", Kind.CRYPTO, 1.0, 1000.0), h("2", "AAPL", Kind.STOCK, 10.0, 100.0))
    private val stressPortfolio = RiskPortfolio.of(stressHoldings, 1000.0, mapOf("crypto:ETH" to 1000.0, "stock:AAPL" to 100.0), emptyMap())
    private val stress = PortfolioRisk.stressTest(stressPortfolio, mapOf("crypto:ETH" to BetaEstimate(1.5, 90, true)))

    @Test fun eachLineMovesByBetaTimesItsBenchmarkShock() {
        val all10 = stress.first { it.scenario.key == "all10" }
        // ETH 1000 × 1.5 × 10 % + AAPL 1000 × 1 × 10 % (no beta: 1)
        close(250.0, all10.loss)
        close(250.0 / 3000 * 100, all10.lossPercent)
        close(250.0 / 2000 * 100, all10.investedLossPercent)
        assertEquals("ETH", all10.worst?.symbol)
    }

    @Test fun mixedScenarios() {
        close(300.0, stress.first { it.scenario.key == "crypto20" }.loss)
        close(450.0 + 100, stress.first { it.scenario.key == "stock10crypto30" }.loss)
        assertEquals(PortfolioRisk.STRESS_SCENARIOS.map { it.key }, stress.map { it.scenario.key })
    }

    @Test fun aLineNeverLosesMoreThanItsValue() {
        val big = PortfolioRisk.stressTest(stressPortfolio, mapOf("crypto:ETH" to BetaEstimate(5.0, 90, true))).first { it.scenario.key == "all30" }
        close(1000.0 + 300, big.loss)
    }

    private val limitBench = returns(120, 11)
    private val eth = series(path(limitBench))
    private val sol = series(path(limitBench.mapIndexed { i, x -> x * 1.2 + (if (i % 2 == 1) 0.001 else -0.001) }))
    private val aapl = series(path(returns(120, 99)))

    @Test fun aCorrelatedClusterIsFoundAndWeighed() {
        val c = PortfolioRisk.correlatedClusters(listOf(ClusterInput("ETH", 30.0, eth), ClusterInput("SOL", 25.0, sol), ClusterInput("AAPL", 20.0, aapl)))
        assertEquals(1, c.size)
        assertEquals(listOf("ETH", "SOL"), c[0].symbols.sorted())
        assertEquals(55.0, c[0].weight)
        assertTrue(c[0].averageCorrelation > 0.7)
    }

    @Test fun limitsAgainstTheSettings() {
        val now = T0 + 120 * DAY + 3_600_000
        val holdings = listOf(
            h("1", "ETH", Kind.CRYPTO, 1.0, eth.last().close), h("2", "SOL", Kind.CRYPTO, 1.0, sol.last().close), h("3", "AAPL", Kind.STOCK, 1.0, aapl.last().close),
        )
        // Prices 10 % under the previous close (the last candle is today's): a big daily loss.
        val prices = mapOf("crypto:ETH" to eth[eth.size - 2].close * 0.9, "crypto:SOL" to sol[sol.size - 2].close * 0.9, "stock:AAPL" to aapl[aapl.size - 2].close * 0.9)
        val daily = mapOf("crypto:ETH" to eth, "crypto:SOL" to sol, "stock:AAPL" to aapl)
        val a = RiskPortfolio.of(holdings, 0.0, prices, daily)
        val d = assertNotNull(PortfolioRisk.dailyChange(a, daily, now))
        close(-10.0, d.percent, 6)
        assertEquals(3, d.covered)
        val clusters = PortfolioRisk.correlatedClusters(a.lines.map { ClusterInput(it.symbol, it.weight, daily.getValue(it.key)) })
        val checks = PortfolioRisk.checkLimits(a, RiskSettings(maxPositionPercent = 20.0, maxCryptoPercent = 30.0), clusters, d)
        val by = checks.associateBy { it.code }
        assertEquals(LimitLevel.DANGER, by.getValue("max_weight").level)
        assertEquals(LimitLevel.WARNING, by.getValue("crypto_cap").level)
        assertEquals(LimitLevel.DANGER, by.getValue("daily_loss").level)
        assertTrue(by.getValue("daily_loss").detail.contains(PortfolioRisk.DAILY_LOSS_REACHED))
        if (a.crypto > 40 && clusters.any { it.weight > 40 }) assertEquals(LimitLevel.WARNING, by.getValue("cluster").level)
        val calm = PortfolioRisk.checkLimits(
            a, RiskSettings(maxPositionPercent = 100.0, maxCryptoPercent = 100.0, dailyLossLimitPercent = 50.0, riskPerTradePercent = 100.0), emptyList(), d,
        )
        assertTrue(calm.all { it.level == LimitLevel.OK }, calm.toString())
        assertEquals(LimitLevel.NA, PortfolioRisk.checkLimits(a, RiskSettings.DEFAULT, emptyList(), null).first { it.code == "daily_loss" }.level)
    }

    @Test fun previousClose() {
        val c = series(listOf(10.0, 11.0, 12.0))
        assertEquals(11.0, PortfolioRisk.previousClose(c, T0 + 2 * DAY + 5_000))
        assertEquals(12.0, PortfolioRisk.previousClose(c, T0 + 3 * DAY + 5_000))
        assertNull(PortfolioRisk.previousClose(emptyList(), T0))
    }

    @Test fun positionsThatBecameDangerous() {
        val c = series(List(30) { 100.0 + it % 2 })
        val holdings = listOf(
            h("a", "BTC", Kind.CRYPTO, 1.0, 100.0, 101.5), h("b", "ETH", Kind.CRYPTO, 1.0, 100.0, 99.5), h("c", "AAPL", Kind.STOCK, 1.0, 200.0), h("d", "MSFT", Kind.STOCK, 1.0, 50.0, 10.0),
        )
        val daily = mapOf("crypto:BTC" to c, "crypto:ETH" to c, "stock:AAPL" to c, "stock:MSFT" to c)
        val a = RiskPortfolio.of(holdings, 10_000.0, daily.mapValues { 101.0 }, daily)
        val stops = holdings.associate { it.id to it.stop }
        val by = PortfolioRisk.dangerousPositions(a, RiskSettings.DEFAULT, daily, stops).associate { it.symbol to it.reasons.map { r -> r.code } }
        assertEquals(listOf("stop_broken"), by["BTC"])
        assertEquals(listOf("near_stop"), by["ETH"])
        // AAPL: −99 $ latent on a 10 404 $ portfolio (1 % = 104 $): not yet; with 0.5 % it is.
        assertNull(by["AAPL"])
        assertNull(by["MSFT"])
        val strict = PortfolioRisk.dangerousPositions(a, RiskSettings(riskPerTradePercent = 0.5), emptyMap(), emptyMap())
        assertEquals(listOf("AAPL"), strict.map { it.symbol })
    }

    // ---------- Android specifics ----------

    @Test fun betasOfTheHoldingsWithTheBenchmarkItself() {
        val b = PortfolioRisk.betas(listOf(h("1", "BTC", Kind.CRYPTO, 1.0, 1.0), h("2", "ETH", Kind.CRYPTO, 1.0, 1.0)), mapOf("crypto:BTC" to series(path(bench)), "crypto:ETH" to series(path(bench.map { 2 * it }))))
        assertEquals(true, b.getValue("crypto:BTC").reference)
        close(2.0, b.getValue("crypto:ETH").beta, 6)
    }

    @Test fun withoutAverageCostNoLatentLoss() {
        val a = RiskPortfolio.of(listOf(Holding("x", Asset("AAPL", Kind.STOCK, "Apple"), 1.0, null)), 0.0, mapOf("stock:AAPL" to 50.0), emptyMap())
        assertNull(a.lines[0].invested)
        assertTrue(PortfolioRisk.dangerousPositions(a, RiskSettings(riskPerTradePercent = 0.25), emptyMap(), emptyMap()).isEmpty())
    }

    @Test fun dangersSavedReadBackValidated() {
        val raw = PortfolioRisk.encodeDangers(listOf(Danger("1", "BTC", Kind.CRYPTO, "Bitcoin", listOf(DangerReason("stop_broken", "Stop cassé")))), 42.0)
        val s = assertNotNull(PortfolioRisk.parseDangers(raw))
        assertEquals("BTC", s.items[0].symbol)
        assertEquals(42.0, s.at)
        assertEquals(emptyList(), PortfolioRisk.parseDangers("""{"version":1,"at":1,"items":[{"id":"x","reasons":[{"code":"boom"}]}]}""")?.items)
        assertNull(PortfolioRisk.parseDangers("[]"))
        assertNull(PortfolioRisk.parseDangers("{nope"))
    }

    @Test fun holdingStopIsCleaned() {
        val base = Holding("1", Asset("BTC", Kind.CRYPTO, "Bitcoin"), 1.0, 10.0)
        assertNull(base.copy(stop = -3.0).cleaned().stop)
        assertNull(base.copy(stop = Double.NaN).cleaned().stop)
        assertEquals(5.0, base.copy(stop = 5.0).cleaned().stop)
    }

    @Test fun settingsOutOfRangeFallBack() {
        assertEquals(RiskSettings.DEFAULT, RiskSettings(riskPerTradePercent = 99.0, maxCryptoPercent = -1.0).sanitized())
    }
}
