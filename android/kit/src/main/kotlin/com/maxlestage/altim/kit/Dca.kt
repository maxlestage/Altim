package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

/**
 * "What if I had invested X $ every week / month": regular purchases at the daily close, over real past prices,
 * compared with the same total invested at once on the first day. No fees, no taxes; the past does not predict the
 * future. Same computation as the site (web/src/engine/dca.ts) and the iPhone.
 */
data class DcaResult(
    val buys: Int,
    val invested: Double,
    val units: Double,
    val value: Double,
    val gain: Double,
    val averagePrice: Double,
    val lastPrice: Double,
    val lumpValue: Double,
    val lumpGain: Double,
    /** Value of the plan and amount invested at each close, for a small chart. */
    val path: List<Triple<Long, Double, Double>>,
    val first: Long,
    val last: Long,
) {
    companion object {
        private const val DAY = 86_400_000L

        fun simulate(closes: List<Pair<Long, Double>>, amount: Double, everyDays: Int, days: Int, now: Long = System.currentTimeMillis()): DcaResult? {
            if (amount <= 0 || everyDays <= 0) return null
            val sorted = closes.filter { it.second > 0 && it.first <= now }.sortedBy { it.first }
            if (sorted.size < 2) return null
            val end = sorted.last().first
            val start = end - days * DAY
            val inRange = sorted.filter { it.first >= start }
            // The asset must have existed for the whole period, otherwise the comparison is meaningless.
            if (inRange.size < 2 || sorted.first().first > start + 7 * DAY) return null
            var units = 0.0
            var invested = 0.0
            var next = inRange.first().first
            val path = mutableListOf<Triple<Long, Double, Double>>()
            for ((t, c) in inRange) {
                if (t >= next) {
                    units += amount / c
                    invested += amount
                    next += everyDays * DAY
                    // Weekends and holidays: the next purchase happens at the next close available.
                    while (next <= t) next += everyDays * DAY
                }
                path += Triple(t, invested, units * c)
            }
            val lastPrice = inRange.last().second
            val value = units * lastPrice
            val lump = invested / inRange.first().second * lastPrice
            return DcaResult(
                buys = Math.round(invested / amount).toInt(),
                invested = invested,
                units = units,
                value = value,
                gain = (value / invested - 1) * 100,
                averagePrice = invested / units,
                lastPrice = lastPrice,
                lumpValue = lump,
                lumpGain = (lump / invested - 1) * 100,
                path = path,
                first = inRange.first().first,
                last = end,
            )
        }
    }
}

/** "Point du jour" (/api/brief): market climate, what can be bought now, moves since the last daily close, top stories. */
@Serializable
data class Brief(
    val asOf: Double,
    val headline: String,
    val market: Market? = null,
    val buyable: List<Buy> = emptyList(),
    val movers: List<Mover> = emptyList(),
    val news: List<NewsItem> = emptyList(),
) {
    @Serializable data class Market(val level: String, val label: String, val score: Double, val themes: List<String> = emptyList())
    @Serializable data class Buy(val symbol: String, val kind: Kind, val strong: Boolean = false, val title: String = "")
    @Serializable data class Mover(val symbol: String, val kind: Kind, val price: Double, val change: Double) {
        val asset: Asset get() = Asset(symbol, kind, symbol)
    }
}
