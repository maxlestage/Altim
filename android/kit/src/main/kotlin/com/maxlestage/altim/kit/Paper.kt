package com.maxlestage.altim.kit

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.text.Collator
import java.util.Locale
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/*
 * Paper trading ("Simulation"): a virtual portfolio that follows Altim's decisions with no real money, to check
 * whether "signal → exécution → résultat" holds. Everything stays on the phone (like the holdings). Exact port of
 * web/src/engine/paper.ts, checked step by step against the shared scenario (web/test/paper-fixture.json):
 * - every order pays fees (0,1 %) and slippage (0,05 %, against you) on each side;
 * - a position with a stop or a target is closed automatically when a daily candle AFTER the opening reaches it;
 *   a candle that reaches both counts as the stop (worst case); a gap below the stop is filled at the open, a
 *   target at the target (never better than planned);
 * - the candle of the opening day is not used (its low may be earlier than the purchase);
 * - results are grouped by the decision shown at the purchase.
 */

@Serializable
enum class PaperReason(val label: String) {
    @SerialName("stop") STOP("stop touché"),
    @SerialName("target") TARGET("objectif atteint"),
    @SerialName("manual") MANUAL("vente manuelle"),
}

/** The decision shown when the position was opened (null: opened without one). */
@Serializable
data class PaperDecision(val verdict: String, val label: String, val confidence: Double, val asOf: Double)

@Serializable
data class PaperPosition(
    val id: String,
    val symbol: String,
    val kind: Kind,
    val name: String,
    val openedAt: Double,
    /** Fill price (market price + slippage). */
    val entry: Double,
    val quantity: Double,
    /** Amount taken from the cash, fees included. */
    val invested: Double,
    val stop: Double? = null,
    val target: Double? = null,
    val decision: PaperDecision? = null,
) {
    val key: String get() = "${kind.raw}:$symbol"
    val asset: Asset get() = Asset(symbol, kind, name)
}

@Serializable
data class PaperTrade(
    val id: String,
    val symbol: String,
    val kind: Kind,
    val name: String,
    val openedAt: Double,
    val entry: Double,
    val quantity: Double,
    val invested: Double,
    val stop: Double? = null,
    val target: Double? = null,
    val decision: PaperDecision? = null,
    val closedAt: Double,
    /** Fill price (market price − slippage, or the stop / gap open). */
    val exit: Double,
    val reason: PaperReason,
    /** Cash received, fees deducted. */
    val proceeds: Double,
    val pnl: Double,
    val pnlPct: Double,
)

@Serializable
data class PaperState(
    val version: Int,
    val startCapital: Double,
    val cash: Double,
    val startedAt: Double = 0.0,
    val positions: List<PaperPosition> = emptyList(),
    val trades: List<PaperTrade> = emptyList(),
)

@Serializable
data class OpenOrder(
    val id: String,
    val symbol: String,
    val kind: Kind,
    val name: String,
    /** Market price now. */
    val price: Double,
    /** Amount to invest, fees included. */
    val amount: Double,
    val stop: Double? = null,
    val target: Double? = null,
    val decision: PaperDecision? = null,
)

/** New state, or the same state and the French reason of the refusal. */
data class PaperResult(val state: PaperState, val error: String? = null)

data class PaperExits(val state: PaperState, val closed: List<PaperTrade>)

@Serializable
data class OpenLine(
    val id: String,
    val symbol: String,
    val kind: Kind,
    val name: String,
    val openedAt: Double,
    val entry: Double,
    val quantity: Double,
    val invested: Double,
    val stop: Double? = null,
    val target: Double? = null,
    val decision: PaperDecision? = null,
    val price: Double? = null,
    val value: Double? = null,
    val pnl: Double? = null,
    val pnlPct: Double? = null,
)

@Serializable
data class PaperValuation(
    val cash: Double,
    /** Value of the open positions at the given prices (sale fees and slippage deducted, as if sold now). */
    val positionsValue: Double,
    val equity: Double,
    val pnl: Double,
    val pnlPct: Double,
    val lines: List<OpenLine>,
    /** Positions without a price: valued at their cost. */
    val unpriced: Int,
)

@Serializable
data class VerdictStats(val verdict: String, val label: String, val trades: Int, val wins: Int, val winRate: Double, val avgPnlPct: Double)

@Serializable
data class ReasonCounts(val stop: Int, val target: Int, val manual: Int)

@Serializable
data class PaperStats(
    val trades: Int,
    val wins: Int,
    val winRate: Double,
    val avgWinPct: Double? = null,
    val avgLossPct: Double? = null,
    /** Sum of gains ÷ sum of losses (null without a losing trade). */
    val profitFactor: Double? = null,
    val realizedPnl: Double,
    val best: PaperTrade? = null,
    val worst: PaperTrade? = null,
    /** Worst fall of the realized capital (start + closed trades in order), %. */
    val maxDrawdownPct: Double,
    val byReason: ReasonCounts,
    /** Results grouped by the decision shown at the purchase ("Sans décision" when opened without one). */
    val byVerdict: List<VerdictStats>,
)

object Paper {
    const val FEE_RATE = 0.001
    const val SLIPPAGE = 0.0005
    const val DEFAULT_CAPITAL = 10_000.0

    /**
     * JavaScript's Math.round: the nearest integer, ties toward +∞ (−2,5 → −2). x − floor(x) is exact for every
     * double, unlike floor(x + 0.5), which is wrong for 0.49999999999999994 and for large odd values.
     */
    fun jsRound(x: Double): Double {
        if (!x.isFinite()) return x
        val f = floor(x)
        val r = if (x - f >= 0.5) f + 1 else f
        // Math.round(-0.4) is −0 in JavaScript.
        return if (r == 0.0 && (x < 0 || (x == 0.0 && 1 / x < 0))) -0.0 else r
    }

    fun round(v: Double, d: Int = 8): Double {
        val p = 10.0.pow(d)
        return jsRound(v * p) / p
    }

    /** A number written like JavaScript's String(n): 10000, 7000.5, 0.25. */
    fun jsString(v: Double): String = when {
        v.isNaN() -> "NaN"
        v.isInfinite() -> if (v > 0) "Infinity" else "-Infinity"
        v == floor(v) && kotlin.math.abs(v) < 1e21 -> v.toLong().toString()
        else -> v.toString()
    }

    private val collator: Collator = Collator.getInstance(Locale.ROOT)

    internal fun localeCompare(a: String, b: String): Int = collator.compare(a, b)
}

private fun r(v: Double, d: Int = 8) = Paper.round(v, d)

fun newPaper(capital: Double, now: Double): PaperState {
    val c = if (capital.isFinite() && capital > 0) r(capital, 2) else Paper.DEFAULT_CAPITAL
    return PaperState(version = 1, startCapital = c, cash = c, startedAt = now)
}

/** Buys `amount` $ of the asset at `price` + slippage, fees deducted. Error text when refused. */
fun openPosition(s: PaperState, o: OpenOrder, now: Double): PaperResult {
    if (!(o.price > 0) || !o.price.isFinite()) return PaperResult(s, "Prix indisponible.")
    if (!(o.amount > 0) || !o.amount.isFinite()) return PaperResult(s, "Montant invalide.")
    if (o.amount > s.cash + 1e-9) return PaperResult(s, "Liquidités simulées insuffisantes (${Paper.jsString(r(s.cash, 2))} $ disponibles).")
    val entry = o.price * (1 + Paper.SLIPPAGE)
    val stop = o.stop?.takeIf { it > 0 && it < entry }
    val target = o.target?.takeIf { it > entry }
    val fee = o.amount * Paper.FEE_RATE
    val position = PaperPosition(
        id = o.id, symbol = o.symbol, kind = o.kind, name = o.name, openedAt = now,
        entry = r(entry), quantity = r((o.amount - fee) / entry, 10), invested = r(o.amount, 2),
        stop = stop, target = target, decision = o.decision,
    )
    return PaperResult(s.copy(cash = r(s.cash - o.amount, 2), positions = s.positions + position))
}

private fun close(s: PaperState, p: PaperPosition, fill: Double, at: Double, reason: PaperReason): PaperState {
    val gross = p.quantity * fill
    val proceeds = r(gross - gross * Paper.FEE_RATE, 2)
    val pnl = r(proceeds - p.invested, 2)
    val trade = PaperTrade(
        id = p.id, symbol = p.symbol, kind = p.kind, name = p.name, openedAt = p.openedAt, entry = p.entry,
        quantity = p.quantity, invested = p.invested, stop = p.stop, target = p.target, decision = p.decision,
        closedAt = at, exit = r(fill), reason = reason, proceeds = proceeds, pnl = pnl, pnlPct = r((pnl / p.invested) * 100, 4),
    )
    return s.copy(cash = r(s.cash + proceeds, 2), positions = s.positions.filter { it.id != p.id }, trades = s.trades + trade)
}

/** Sells the whole position at the market price − slippage, fees deducted. */
fun closePosition(s: PaperState, id: String, price: Double, now: Double): PaperResult {
    val p = s.positions.firstOrNull { it.id == id } ?: return PaperResult(s, "Position introuvable.")
    if (!(price > 0) || !price.isFinite()) return PaperResult(s, "Prix indisponible.")
    return PaperResult(close(s, p, price * (1 - Paper.SLIPPAGE), now, PaperReason.MANUAL))
}

/**
 * Closes the positions whose stop or target was reached by a daily candle that started after the opening
 * (candles of key "kind:symbol", any order). Returns the new state and the trades just closed.
 */
fun checkExits(s: PaperState, candles: Map<String, List<Candle>>): PaperExits {
    var state = s
    val closed = mutableListOf<PaperTrade>()
    for (p in s.positions) {
        if (p.stop == null && p.target == null) continue
        val list = (candles[p.key] ?: emptyList()).filter { it.time > p.openedAt && it.low > 0 && it.high >= it.low }.sortedBy { it.time }
        for (c in list) {
            var fill: Double? = null
            var reason: PaperReason? = null
            if (p.stop != null && c.low <= p.stop) {
                fill = min(p.stop, c.open) * (1 - Paper.SLIPPAGE)
                reason = PaperReason.STOP
            } else if (p.target != null && c.high >= p.target) {
                fill = p.target * (1 - Paper.SLIPPAGE)
                reason = PaperReason.TARGET
            }
            if (fill != null && reason != null) {
                state = close(state, p, fill, c.time, reason)
                closed += state.trades.last()
                break
            }
        }
    }
    return PaperExits(state, closed)
}

fun valuation(s: PaperState, prices: Map<String, Double?>): PaperValuation {
    var positionsValue = 0.0
    var unpriced = 0
    val lines = s.positions.map { p ->
        val base = OpenLine(p.id, p.symbol, p.kind, p.name, p.openedAt, p.entry, p.quantity, p.invested, p.stop, p.target, p.decision)
        val price = prices[p.key]
        if (!(price != null && price > 0)) {
            unpriced++
            positionsValue += p.invested
            return@map base
        }
        val gross = p.quantity * price * (1 - Paper.SLIPPAGE)
        val value = r(gross - gross * Paper.FEE_RATE, 2)
        positionsValue += value
        val pnl = r(value - p.invested, 2)
        base.copy(price = price, value = value, pnl = pnl, pnlPct = r((pnl / p.invested) * 100, 4))
    }
    val equity = r(s.cash + positionsValue, 2)
    val pnl = r(equity - s.startCapital, 2)
    return PaperValuation(s.cash, r(positionsValue, 2), equity, pnl, r((pnl / s.startCapital) * 100, 4), lines, unpriced)
}

fun paperStats(s: PaperState): PaperStats {
    val t = s.trades.sortedBy { it.closedAt }
    val wins = t.filter { it.pnl > 0 }
    val losses = t.filter { it.pnl <= 0 }
    fun mean(v: List<Double>): Double? = if (v.isNotEmpty()) r(v.fold(0.0) { a, b -> a + b } / v.size, 4) else null
    val gains = wins.fold(0.0) { a, x -> a + x.pnl }
    val lost = -losses.fold(0.0) { a, x -> a + x.pnl }
    var capital = s.startCapital
    var peak = capital
    var maxDd = 0.0
    for (x in t) {
        capital += x.pnl
        peak = max(peak, capital)
        maxDd = min(maxDd, (capital / peak - 1) * 100)
    }
    val groups = LinkedHashMap<String, MutableList<PaperTrade>>()
    for (x in t) groups.getOrPut(x.decision?.verdict ?: "none") { mutableListOf() } += x
    val byVerdict = groups.entries.map { (verdict, list) ->
        val w = list.count { it.pnl > 0 }
        VerdictStats(
            verdict = verdict,
            label = list[0].decision?.label ?: "Sans décision",
            trades = list.size,
            wins = w,
            winRate = r((w.toDouble() / list.size) * 100, 4),
            avgPnlPct = mean(list.map { it.pnlPct })!!,
        )
    }.sortedWith { a, b -> if (b.trades != a.trades) b.trades - a.trades else Paper.localeCompare(a.verdict, b.verdict) }
    val byPct = t.sortedByDescending { it.pnlPct }
    return PaperStats(
        trades = t.size,
        wins = wins.size,
        winRate = if (t.isNotEmpty()) r((wins.size.toDouble() / t.size) * 100, 4) else 0.0,
        avgWinPct = mean(wins.map { it.pnlPct }),
        avgLossPct = mean(losses.map { it.pnlPct }),
        profitFactor = if (lost > 0) r(gains / lost, 4) else null,
        realizedPnl = r(t.fold(0.0) { a, x -> a + x.pnl }, 2),
        best = byPct.firstOrNull(),
        worst = byPct.lastOrNull(),
        maxDrawdownPct = r(maxDd, 4),
        byReason = ReasonCounts(
            stop = t.count { it.reason == PaperReason.STOP },
            target = t.count { it.reason == PaperReason.TARGET },
            manual = t.count { it.reason == PaperReason.MANUAL },
        ),
        byVerdict = byVerdict,
    )
}

/** Accepts only a well-formed saved state (the stored text can be damaged or edited). Same rule as isPaperState. */
fun isPaperState(s: PaperState?): Boolean =
    s != null && s.version == 1 && s.startCapital.isFinite() && s.startCapital > 0 && s.cash.isFinite() &&
        s.positions.all { it.quantity > 0 && it.entry > 0 && it.invested > 0 }

/** Reads a saved state: null for anything malformed (never an exception). */
fun decodePaper(text: String?): PaperState? {
    if (text.isNullOrBlank()) return null
    val s = runCatching { PaperJson.decodeFromString(PaperState.serializer(), text) }.getOrNull()
    return s?.takeIf { isPaperState(it) }
}

fun encodePaper(s: PaperState): String = PaperJson.encodeToString(PaperState.serializer(), s)

/** Strict (no value coerced to a default): a damaged field makes the whole saved state rejected. */
private val PaperJson = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
}
