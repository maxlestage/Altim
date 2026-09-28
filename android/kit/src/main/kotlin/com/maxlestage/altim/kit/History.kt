package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

/** Daily closes served by /api/history: [time, close] pairs of each held asset, plus Bitcoin and SPY. */
@Serializable
data class HistoryResponse(val asOf: Double, val days: Int, val series: List<Series>) {
    @Serializable
    data class Series(val symbol: String, val kind: Kind, val closes: List<List<Double>> = emptyList(), val error: String? = null) {
        val id: String get() = "${kind.raw}:$symbol"
    }

    val byId: Map<String, List<Pair<Long, Double>>>
        get() = series.associate { s -> s.id to s.closes.filter { it.size == 2 }.map { it[0].toLong() to it[1] } }
}

/**
 * History of the portfolio: what the lines held today were worth each day of the period, compared with Bitcoin and
 * the S&P 500 held over the same days. Past purchases and sales are not known: it answers "how did what I own now
 * behave", not "how did my account do". Same computation as the site (web/src/engine/history.ts).
 */
data class PortfolioHistory(
    val points: List<Point>,
    val change: Double,
    val maxDrawdown: Double,
    val best: Day?,
    val worst: Day?,
    val benchmarks: List<Benchmark>,
    val missing: List<String>,
    val shortened: Boolean,
) {
    data class Point(val t: Long, val value: Double)
    data class Day(val t: Long, val change: Double)
    data class Benchmark(val id: String, val label: String, val change: Double, val pct: List<Double>)

    /** Change in % since the first day, one value per point. */
    val pct: List<Double> get() = points.map { (it.value / points.first().value - 1) * 100 }

    companion object {
        val BENCHMARKS = listOf("crypto:BTC" to "Bitcoin", "stock:SPY" to "S&P 500 (SPY)")
        private const val DAY = 86_400_000L
        private fun dayOf(t: Long) = Math.floorDiv(t, DAY) * DAY

        /** Last close known at the end of each day of the grid (weekends and holidays keep the Friday close). */
        private fun onGrid(closes: List<Pair<Long, Double>>, grid: List<Long>): List<Double?> {
            val sorted = closes.filter { it.second.isFinite() && it.second > 0 }.sortedBy { it.first }
            var i = 0
            var last: Double? = null
            return grid.map { d ->
                while (i < sorted.size && dayOf(sorted[i].first) <= d) last = sorted[i++].second
                last
            }
        }

        fun compute(lines: List<Pair<String, Double>>, series: Map<String, List<Pair<Long, Double>>>, days: Int, now: Long = System.currentTimeMillis()): PortfolioHistory? {
            val held = lines.filter { it.second > 0 }
            // The curve ends on the last closed day of the held assets (only closed candles are served).
            val lastTimes = held.flatMap { (id, _) -> series[id].orEmpty().map { it.first } }.filter { it <= now }
            val end = dayOf(lastTimes.maxOrNull() ?: now)
            val grid = (0..days).map { end - (days - it) * DAY }
            val missing = mutableListOf<String>()
            val valued = mutableListOf<Pair<Double, List<Double?>>>()
            for ((id, q) in held) {
                val g = onGrid(series[id].orEmpty(), grid)
                if (g.last() == null) missing += id else valued += q to g
            }
            if (valued.isEmpty()) return null
            // The curve starts on the first day every line has a price: an asset listed later would look like a gain.
            val start = grid.indices.firstOrNull { i -> valued.all { it.second[i] != null } } ?: return null
            if (grid.size - start < 2) return null
            val points = (start until grid.size).map { i -> Point(grid[i], valued.sumOf { (q, c) -> q * c[i]!! }) }

            var peak = points.first().value
            var maxDrawdown = 0.0
            var best: Day? = null
            var worst: Day? = null
            points.forEachIndexed { i, p ->
                peak = maxOf(peak, p.value)
                maxDrawdown = minOf(maxDrawdown, (p.value / peak - 1) * 100)
                if (i > 0) {
                    val c = (p.value / points[i - 1].value - 1) * 100
                    if (best == null || c > best!!.change) best = Day(p.t, c)
                    if (worst == null || c < worst!!.change) worst = Day(p.t, c)
                }
            }
            val benchmarks = BENCHMARKS.mapNotNull { (id, label) ->
                val g = onGrid(series[id].orEmpty(), grid).drop(start)
                val base = g.first() ?: return@mapNotNull null
                if (g.any { it == null }) return@mapNotNull null
                val pct = g.map { (it!! / base - 1) * 100 }
                Benchmark(id, label, pct.last(), pct)
            }
            return PortfolioHistory(
                points = points,
                change = (points.last().value / points.first().value - 1) * 100,
                maxDrawdown = maxDrawdown,
                best = best?.takeIf { it.change > 0 },
                worst = worst?.takeIf { it.change < 0 },
                benchmarks = benchmarks,
                missing = missing,
                shortened = start > 0,
            )
        }
    }
}
