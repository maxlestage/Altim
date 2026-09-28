package com.maxlestage.altim.kit

import java.time.Instant
import java.time.ZoneOffset
import kotlin.math.max
import kotlin.math.sqrt

// « Et si… ? » (Mes avoirs): a shock on one market factor (Nasdaq-100 via QQQ, S&P 500 via SPY, Bitcoin), each line
// moving by its beta to that factor measured on the days where both have a daily close. Exact port of the "Et si… ?"
// part of web/src/engine/portfolio-risk.ts (`FACTORS`, `factorBeta`, `whatIf`), tested with the same cases
// (WhatIfTest ↔ web/test/whatif.test.ts). Pure, deterministic functions.

data class Factor(val key: String, val symbol: String, val kind: Kind, val label: String, val name: String) {
    val id: String get() = "${kind.raw}:$symbol"
    val asset: Asset get() = Asset(symbol, kind, label)
}

data class FactorBeta(
    val beta: Double,
    /** Shared daily returns used. */
    val days: Int,
    /** false: under [PortfolioRisk.MIN_BETA_DAYS] shared returns (beta 1 only as a placeholder). */
    val estimated: Boolean,
    /** Correlation of the daily returns with the factor (null: too few days). */
    val correlation: Double?,
)

data class WhatIfLine(
    val key: String,
    val symbol: String,
    val name: String,
    val kind: Kind,
    /** Value of the line in the simulated amount (USD). */
    val value: Double,
    /** % of the simulated amount. */
    val weight: Double,
    val beta: Double?,
    val days: Int,
    val correlation: Double?,
    /** The line is the factor itself (beta 1 by definition). */
    val reference: Boolean,
    /** Move of the line (%), null when its beta could not be measured ("non couvert"). */
    val movePercent: Double?,
    /** USD, positive = loss; null when not covered. */
    val loss: Double?,
)

data class WhatIfResult(
    val factor: String,
    val shock: Double,
    /** Simulated amount: the portfolio's value, or the amount entered spread on the current weights. */
    val base: Double,
    val scaled: Boolean,
    val cash: Double,
    val lines: List<WhatIfLine>,
    /** Total loss of the covered lines (USD, positive = loss) and its share of `base`. */
    val loss: Double,
    val lossPercent: Double,
    /** Value of the lines whose beta could not be measured (left out of the total, never guessed). */
    val uncoveredValue: Double,
    val uncovered: List<String>,
    val worst: WhatIfLine?,
    /** Shortest and longest window of the betas used (days). */
    val minDays: Int?,
    val maxDays: Int?,
)

object WhatIf {
    /** One year of shared sessions, steadier than 90 days for a shock on one market. */
    const val BETA_DAYS = 250
    /** Below this |correlation| the beta explains little of the line's moves: said next to the line. */
    const val WEAK_CORRELATION = 0.3

    /** Factors of the simulator, in the order of the chips. */
    val FACTORS = listOf(
        Factor("qqq", "QQQ", Kind.STOCK, "Nasdaq-100 (QQQ)", "le Nasdaq-100"),
        Factor("spy", "SPY", Kind.STOCK, "S&P 500 (SPY)", "le S&P 500"),
        Factor("btc", "BTC", Kind.CRYPTO, "Bitcoin", "le Bitcoin"),
    )
    val SHOCKS = listOf(-5.0, -10.0, -20.0, -30.0, -50.0)

    fun factor(key: String): Factor = FACTORS.firstOrNull { it.key == key } ?: FACTORS[0]

    private fun dayKey(ms: Double): String = Instant.ofEpochMilli(ms.toLong()).atOffset(ZoneOffset.UTC).toLocalDate().toString()

    /**
     * Beta of an asset to a factor on the days where BOTH have a daily close (no forward fill: a crypto's weekends do
     * not become days where a stock "did not move"), returns between consecutive shared days, last [days] of them.
     * Not estimated under [PortfolioRisk.MIN_BETA_DAYS] shared returns.
     */
    fun factorBeta(asset: List<Candle>, factor: List<Candle>, days: Int = PortfolioRisk.BETA_DAYS): FactorBeta {
        val fa = LinkedHashMap<String, Double>()
        factor.filter { it.close > 0 }.forEach { fa[dayKey(it.time)] = it.close }
        val own = LinkedHashMap<String, Double>()
        asset.filter { it.close > 0 }.forEach { own[dayKey(it.time)] = it.close }
        val shared = own.entries.filter { fa.containsKey(it.key) }.sortedBy { it.key }.map { it.key to it.value }.takeLast(days + 1)
        val x = mutableListOf<Double>()
        val y = mutableListOf<Double>()
        for (i in 1 until shared.size) {
            x += shared[i].second / shared[i - 1].second - 1
            y += fa.getValue(shared[i].first) / fa.getValue(shared[i - 1].first) - 1
        }
        val n = x.size
        if (n < PortfolioRisk.MIN_BETA_DAYS) return FactorBeta(1.0, n, false, null)
        val mx = x.sum() / n
        val my = y.sum() / n
        var cov = 0.0
        var vx = 0.0
        var vy = 0.0
        for (i in 0 until n) {
            cov += (x[i] - mx) * (y[i] - my)
            vx += (x[i] - mx) * (x[i] - mx)
            vy += (y[i] - my) * (y[i] - my)
        }
        if (!(vy > 0)) return FactorBeta(1.0, n, false, null)
        return FactorBeta(cov / vy, n, true, if (vx > 0) cov / sqrt(vx * vy) else null)
    }

    /**
     * Each line moves by its beta to the factor × the shock (never below −100 %); cash does not move. Lines of the same
     * asset are merged. With [amount] (> 0), the lines are rescaled to that amount on the current weights (cash included).
     */
    fun whatIf(a: RiskPortfolio, factorKey: String, shock: Double, betas: Map<String, FactorBeta>, amount: Double? = null): WhatIfResult {
        val f = factor(factorKey)
        val scaled = amount != null && amount.isFinite() && amount > 0 && a.total > 0
        val k = if (scaled) amount!! / a.total else 1.0
        val base = if (scaled) amount!! else a.total
        val merged = LinkedHashMap<String, Pair<RiskLine, Double>>()
        for (l in a.lines) {
            val prev = merged[l.key]
            merged[l.key] = (prev?.first ?: l) to (prev?.second ?: 0.0) + l.value * k
        }
        val lines = merged.map { (key, m) ->
            val l = m.first
            val value = m.second
            val reference = l.symbol == f.symbol && l.kind == f.kind
            val b = betas[key]
            val beta = if (reference) 1.0 else b?.takeIf { it.estimated }?.beta
            val move = beta?.let { max(-100.0, it * shock) }
            WhatIfLine(
                key, l.symbol, l.name, l.kind, value, if (base > 0) value / base * 100 else 0.0,
                beta, if (reference) 0 else b?.days ?: 0, if (reference) 1.0 else b?.correlation, reference,
                move, move?.let { -value * it / 100 },
            )
        }.sortedWith(compareByDescending<WhatIfLine> { it.loss ?: Double.NEGATIVE_INFINITY }.thenByDescending { it.value })
        val covered = lines.filter { it.loss != null }
        val loss = covered.sumOf { it.loss!! }
        var worst: WhatIfLine? = null
        for (l in covered) if (l.loss!! > 0 && (worst == null || l.loss > worst.loss!!)) worst = l
        val measured = covered.filter { !it.reference }.map { it.days }
        return WhatIfResult(
            factorKey, shock, base, scaled, a.cash * k, lines, loss, if (base > 0) loss / base * 100 else 0.0,
            lines.filter { it.loss == null }.sumOf { it.value },
            lines.filter { it.loss == null }.map { it.symbol },
            worst,
            measured.minOrNull(),
            measured.maxOrNull(),
        )
    }
}
