package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

// Strategy comparator (GET /api/strategies?symbol=&kind=, backend/src/engine/strategies.rs): JSON contract and the
// pure helpers of the card (colours, chart geometry, direct labels, list view). Port of web/src/webapp/strategies.ts,
// tested on the same real answers (StrategiesTest ↔ web/test/strategies.test.ts).

@Serializable
data class StrategyRegime(
    /** "bull" | "bear" | "range" | "crisis" | "unknown". */
    val regime: String = "unknown",
    val label: String = "",
    val trades: Int = 0,
    val winRate: Double? = null,
    val avgReturn: Double? = null,
    val lowSample: Boolean = false,
)

@Serializable
data class StrategyMetrics(
    /** % over the period, costs included (DCA: gain ÷ total invested). */
    val totalReturn: Double = 0.0,
    /** % per year, null under one year (DCA: internal rate of return). */
    val cagr: Double? = null,
    /** %, negative. */
    val maxDrawdown: Double = 0.0,
    val sharpe: Double? = null,
    val sortino: Double? = null,
    val winRate: Double? = null,
    val profitFactor: Double? = null,
    /** % per trade. */
    val expectancy: Double? = null,
    val avgR: Double? = null,
    val trades: Int = 0,
    /** % of the tested days invested. */
    val exposure: Double = 0.0,
)

@Serializable
data class StrategyResult(
    /** "trend" | "momentum" | "breakout" | "meanReversion" | "swing" | "dca" | "buyHold" | "value". */
    val id: String,
    val name: String = "",
    val rule: String = "",
    val params: String = "",
    val available: Boolean = false,
    val unavailable: String? = null,
    val metrics: StrategyMetrics? = null,
    val regimes: List<StrategyRegime> = emptyList(),
    /** [ms, value of 100 invested], ≤ 200 points, same dates for every strategy. */
    val equity: List<List<Double>> = emptyList(),
    val lowSample: Boolean = false,
    val note: String? = null,
) {
    fun time(i: Int): Double = equity[i][0]
    fun value(i: Int): Double = equity[i][1]
}

@Serializable
data class StrategiesReport(
    val symbol: String,
    val kind: Kind,
    val asOf: Double = 0.0,
    val from: Double = 0.0,
    val to: Double = 0.0,
    val bars: Int = 0,
    val period: String = "",
    val source: String = "",
    val feesPct: Double = 0.0,
    val slippagePct: Double = 0.0,
    val spreadPct: Double = 0.0,
    val notes: List<String> = emptyList(),
    val strategies: List<StrategyResult> = emptyList(),
)

object Strategies {
    /** Fixed order of the chips and of the cards. */
    val ORDER = listOf("trend", "momentum", "breakout", "meanReversion", "swing", "dca", "buyHold", "value")
    val DEFAULT_SELECTION = listOf("trend", "breakout", "meanReversion", "dca", "buyHold")

    /**
     * One colour per strategy (ARGB), whatever is selected (colour follows the entity): the web palette, validated on
     * the dark card surface. Buy and hold is the neutral, dashed reference.
     */
    val COLORS: Map<String, Long> = mapOf(
        "trend" to 0xFF3987E5,
        "breakout" to 0xFFC98500,
        "meanReversion" to 0xFFD55181,
        "dca" to 0xFF008300,
        "momentum" to 0xFF9085E9,
        "swing" to 0xFFD95926,
        "value" to 0xFF199E70,
        "buyHold" to 0xFF9AA0B4,
    )
    const val REFERENCE = "buyHold"
    /** Direct labels up to 4 coloured curves (plus the reference). */
    const val MAX_LABELED = 4

    /** Short names of the chips and of the direct labels. */
    val SHORT = mapOf(
        "trend" to "Tendance",
        "momentum" to "Momentum",
        "breakout" to "Cassure",
        "meanReversion" to "Moyenne",
        "swing" to "Swing",
        "dca" to "DCA",
        "buyHold" to "Conserver",
        "value" to "PER",
    )

    fun short(id: String) = SHORT[id] ?: id

    const val NNBSP = " "

    /** "+12,3 %" / "−4,0 %" / "—". */
    fun signedPct(v: Double?, digits: Int = 1): String =
        if (v == null) "—" else "${if (v > 0) "+" else if (v < 0) "−" else ""}${JsFormat.fr(abs(v), digits, digits)}$NNBSP%"

    fun plainPct(v: Double?, digits: Int = 0): String = if (v == null) "—" else "${JsFormat.fr(v, digits, digits)}$NNBSP%"

    fun ratio(v: Double?): String = if (v == null) "—" else "${if (v < 0) "−" else ""}${JsFormat.fr(abs(v), 2, 2)}"

    /** "15 juil. 2022" (UTC calendar day). */
    fun shortDate(t: Double): String = DateTimeFormatter.ofPattern("d MMM yyyy", Locale.FRANCE).withZone(ZoneOffset.UTC).format(Instant.ofEpochMilli(t.toLong()))

    /** Value of 100 invested, whole: "127". */
    fun value100(v: Double): String = JsFormat.fr(v, 0)

    /** Strategies to draw: selected, available, with a curve, in the fixed order. */
    fun drawable(report: StrategiesReport, selected: Collection<String>): List<StrategyResult> =
        ORDER.mapNotNull { id -> report.strategies.firstOrNull { it.id == id } }
            .filter { it.id in selected && it.available && it.equity.size > 1 }

    /** Selected strategies (available or not) in the fixed order: one metric card each. */
    fun chosen(report: StrategiesReport, selected: Collection<String>): List<StrategyResult> =
        ORDER.filter { it in selected }.mapNotNull { id -> report.strategies.firstOrNull { it.id == id } }

    /** Chip toggled: removed, or added back at its place in the fixed order. */
    fun toggle(selected: List<String>, id: String): List<String> =
        if (id in selected) selected - id else ORDER.filter { it == id || it in selected }

    /** Plot box (px or dp): width, height, left, right, top, bottom margins. */
    data class Box(val w: Double, val h: Double, val l: Double, val r: Double, val t: Double, val b: Double)

    data class Geometry(val lo: Double, val hi: Double, val n: Int, private val box: Box) {
        fun x(i: Int): Double = box.l + (i.toDouble() / (n - 1)) * (box.w - box.l - box.r)
        fun y(v: Double): Double = box.t + (1 - (v - lo) / (hi - lo)) * (box.h - box.t - box.b)
    }

    /** One axis (value of 100 invested), 100 always inside the range, a little air above and below. */
    fun geometry(series: List<StrategyResult>, box: Box): Geometry? {
        val n = series.maxOfOrNull { it.equity.size } ?: 0
        if (n < 2) return null
        val values = series.flatMap { s -> s.equity.map { it[1] } }
        var lo = min(100.0, values.minOrNull() ?: 100.0)
        var hi = max(100.0, values.maxOrNull() ?: 100.0)
        val pad = ((hi - lo) * 0.06).takeIf { it != 0.0 } ?: 5.0
        lo -= pad
        hi += pad
        return Geometry(lo, hi, n, box)
    }

    /** Vertical positions of the end labels, pushed apart by [gap] and kept inside [top, bottom]; same order as [ys]. */
    fun placeLabels(ys: List<Double>, gap: Double, top: Double, bottom: Double): List<Double> {
        val order = ys.mapIndexed { i, y -> i to y }.sortedBy { it.second }
        val out = order.map { min(max(it.second, top), bottom) }.toMutableList()
        for (k in 1 until out.size) out[k] = max(out[k], out[k - 1] + gap)
        // Overflow at the bottom: shift the stack up.
        val over = if (out.isNotEmpty()) out.last() - bottom else 0.0
        if (over > 0) for (k in out.indices) out[k] = out[k] - over
        for (k in out.size - 2 downTo 0) out[k] = min(out[k], out[k + 1] - gap)
        val res = DoubleArray(ys.size)
        order.forEachIndexed { k, (i, _) -> res[i] = out[k] }
        return res.toList()
    }

    /** Point index under a pointer at [px] (same units as the box). */
    fun indexAt(px: Double, g: Geometry, box: Box): Int =
        max(0, min(g.n - 1, (((px - box.l) / (box.w - box.l - box.r)) * (g.n - 1)).roundToIntJs()))

    /** List view: value of 100 invested at [steps] evenly spaced dates (first and last included). */
    data class Checkpoint(val t: Double, val v: Double)

    fun checkpoints(s: StrategyResult, steps: Int = 5): List<Checkpoint> {
        val n = s.equity.size
        if (n == 0) return emptyList()
        val k = min(steps, n)
        val idx = (0 until k).map { ((it * (n - 1)).toDouble() / max(1, k - 1)).roundToIntJs() }.distinct()
        return idx.map { Checkpoint(s.equity[it][0], s.equity[it][1]) }
    }

    /** "échantillon trop faible" for the signal strategies with fewer than 10 trades. */
    fun lowSampleText(s: StrategyResult): String? {
        if (!s.lowSample) return null
        val t = s.metrics?.trades ?: 0
        return "échantillon trop faible ($t trade${if (t > 1) "s" else ""})"
    }

    /** Math.round of JavaScript (halves go up, −0.5 → 0). */
    private fun Double.roundToIntJs(): Int = kotlin.math.floor(this + 0.5).toInt()
}
