package com.maxlestage.altim.kit

import kotlin.math.sqrt

/**
 * Decision tools that only compute (no advice added): comparison of assets over the same days, position size for a
 * chosen risk, and rebalancing towards a target split. Same computations as the site (web/src/engine/tools.ts).
 */
object Tools {
    private const val DAY = 86_400_000L
    private fun dayOf(t: Long) = Math.floorDiv(t, DAY) * DAY

    // ---------- Comparison ----------

    data class Stat(val id: String, val change: Double, val volatility: Double, val maxDrawdown: Double, val pct: List<Double>)
    data class Comparison(val stats: List<Stat>, val correlation: List<List<Double?>>, val days: List<Long>, val missing: List<String>)

    private fun returns(closes: List<Pair<Long, Double>>): Map<Long, Double> =
        (1 until closes.size).associate { dayOf(closes[it].first) to closes[it].second / closes[it - 1].second - 1 }

    private fun correlation(a: Map<Long, Double>, b: Map<Long, Double>): Double? {
        val common = a.keys.filter { it in b }
        if (common.size < 20) return null
        val xs = common.map { a.getValue(it) }
        val ys = common.map { b.getValue(it) }
        val mx = xs.average()
        val my = ys.average()
        var sxy = 0.0
        var sxx = 0.0
        var syy = 0.0
        for (i in xs.indices) {
            sxy += (xs[i] - mx) * (ys[i] - my)
            sxx += (xs[i] - mx) * (xs[i] - mx)
            syy += (ys[i] - my) * (ys[i] - my)
        }
        return if (sxx > 0 && syy > 0) sxy / sqrt(sxx * syy) else null
    }

    fun compare(series: Map<String, List<Pair<Long, Double>>>, ids: List<String>, days: Int, now: Long = System.currentTimeMillis()): Comparison? {
        val clean = mutableMapOf<String, List<Pair<Long, Double>>>()
        val missing = mutableListOf<String>()
        for (id in ids) {
            val c = series[id].orEmpty().filter { it.second > 0 && it.first <= now }.sortedBy { it.first }
            if (c.size < 2) missing += id else clean[id] = c
        }
        val kept = ids.filter { it in clean }
        if (kept.isEmpty()) return null
        val end = dayOf(kept.maxOf { clean.getValue(it).last().first })
        // Common period: from the latest first day among the assets (a younger asset shortens it) to the last close.
        val start = maxOf(end - days * DAY, kept.maxOf { dayOf(clean.getValue(it).first().first) })
        if (end - start < DAY) return null
        val grid = generateSequence(start) { it + DAY }.takeWhile { it <= end }.toList()
        fun inPeriod(id: String) = clean.getValue(id).filter { dayOf(it.first) in start..end }
        val stats = kept.map { id ->
            val c = inPeriod(id)
            val all = clean.getValue(id)
            var i = 0
            var last = 0.0
            val onGrid = grid.map { d ->
                while (i < all.size && dayOf(all[i].first) <= d) last = all[i++].second
                last
            }
            var peak = c.first().second
            var dd = 0.0
            for ((_, v) in c) {
                peak = maxOf(peak, v)
                dd = minOf(dd, (v / peak - 1) * 100)
            }
            val r = returns(c).values.toList()
            val mean = if (r.isEmpty()) 0.0 else r.average()
            val sd = sqrt(r.sumOf { (it - mean) * (it - mean) } / maxOf(1, r.size - 1))
            val perYear = if (id.startsWith("crypto:")) 365.0 else 252.0
            Stat(id, (onGrid.last() / onGrid.first() - 1) * 100, sd * sqrt(perYear) * 100, dd, onGrid.map { (it / onGrid.first() - 1) * 100 })
        }
        val rets = kept.map { returns(inPeriod(it)) }
        val corr = kept.indices.map { a -> kept.indices.map { b -> if (a == b) 1.0 else correlation(rets[a], rets[b]) } }
        return Comparison(stats, corr, grid, missing)
    }

    // ---------- Position size ----------

    data class Position(
        val quantity: Double,
        val amount: Double,
        val capitalShare: Double,
        val risk: Double,
        val stopDistance: Double,
        val reward: Double?,
        val ratio: Double?,
        val capped: Boolean,
    )

    /** Quantity such that hitting the stop costs [riskPct] % of the capital, never more than the capital itself. */
    fun positionSize(capital: Double, riskPct: Double, entry: Double, stop: Double, target: Double? = null): Position? {
        if (capital <= 0 || riskPct <= 0 || entry <= 0 || stop <= 0 || stop >= entry) return null
        val perUnit = entry - stop
        var quantity = capital * riskPct / 100 / perUnit
        var capped = false
        if (quantity * entry > capital) {
            quantity = capital / entry
            capped = true
        }
        val risk = quantity * perUnit
        val reward = if (target != null && target > entry) quantity * (target - entry) else null
        return Position(quantity, quantity * entry, quantity * entry / capital * 100, risk, perUnit / entry * 100, reward, reward?.let { it / risk }, capped)
    }

    // ---------- Sale after fees and tax ----------

    data class Sale(val id: String, val gross: Double, val fees: Double, val gain: Double?, val tax: Double, val net: Double)
    data class SaleTotal(val gross: Double, val fees: Double, val gain: Double?, val tax: Double, val net: Double, val lines: List<Sale>, val unknownCost: Int)

    /**
     * What a sale leaves once the fees and the tax on the gain are paid (France: flat tax "PFU" of 30 %). A loss pays
     * no tax; selling everything, the losses of the year offset the gains. Line by line for cryptos is an estimate
     * (France computes their gain on the whole portfolio at each sale).
     */
    fun saleTotal(lines: List<Triple<String, Double, Double?>>, taxPct: Double = 30.0, feePct: Double = 0.1): SaleTotal {
        val each = lines.map { (id, value, cost) ->
            val fees = maxOf(0.0, value) * maxOf(0.0, feePct) / 100
            val gain = cost?.let { value - fees - it }
            val tax = if (gain != null && gain > 0) gain * maxOf(0.0, taxPct) / 100 else 0.0
            Sale(id, value, fees, gain, tax, value - fees - tax)
        }
        val known = each.filter { it.gain != null }
        val gain = if (known.isEmpty()) null else known.sumOf { it.gain!! }
        val gross = each.sumOf { it.gross }
        val fees = each.sumOf { it.fees }
        val tax = if (gain != null && gain > 0) gain * maxOf(0.0, taxPct) / 100 else 0.0
        return SaleTotal(gross, fees, gain, tax, gross - fees - tax, each, each.size - known.size)
    }

    // ---------- Projection ----------

    /** Yearly returns shown side by side: hypotheses to compare, not forecasts. */
    val PROJECTION_RATES = listOf(0.0, 4.0, 8.0)

    data class YearPoint(val year: Int, val value: Double, val paid: Double)

    /** Value after [years] of [start] plus [monthly] at the end of each month, compounded monthly at [ratePct] a year. */
    fun projection(start: Double, monthly: Double, years: Int, ratePct: Double): List<YearPoint> {
        val r = Math.pow(1 + ratePct / 100, 1.0 / 12) - 1
        var v = maxOf(0.0, start)
        var paid = v
        val out = mutableListOf(YearPoint(0, v, paid))
        for (m in 1..years * 12) {
            v = v * (1 + r) + maxOf(0.0, monthly)
            paid += maxOf(0.0, monthly)
            if (m % 12 == 0) out += YearPoint(m / 12, v, paid)
        }
        return out
    }

    // ---------- Rebalancing ----------

    data class Rebalance(val total: Double, val current: Map<Kind, Double>, val moves: Map<Kind, Double>, val lines: List<Pair<String, Double>>)

    /** Buys and sells that bring the lines to the target split (the phone keeps no cash: crypto + stocks = 100 %). */
    fun rebalance(lines: List<Triple<String, Kind, Double>>, targetCrypto: Double): Rebalance? {
        if (targetCrypto < 0 || targetCrypto > 100) return null
        val by = Kind.entries.associateWith { k -> lines.filter { it.second == k && it.third > 0 }.sumOf { it.third } }
        val total = by.values.sum()
        if (total <= 0) return null
        val target = mapOf(Kind.CRYPTO to targetCrypto, Kind.STOCK to 100 - targetCrypto)
        val moves = Kind.entries.associateWith { total * target.getValue(it) / 100 - by.getValue(it) }
        val current = Kind.entries.associateWith { by.getValue(it) / total * 100 }
        val split = lines.filter { it.third > 0 && by.getValue(it.second) > 0 }.map { it.first to moves.getValue(it.second) * it.third / by.getValue(it.second) }
        return Rebalance(total, current, moves, split)
    }
}
