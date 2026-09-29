package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlin.math.abs
import kotlin.math.max

// "Opportunités du moment" (GET /api/opportunities?kind=, 202 {pending:true} while the first scan runs) and the
// anomalies of one asset (GET /api/anomalies?symbol=&kind=, with the crypto derivatives block): JSON contracts and the
// pure filtering of the scan (category chips, market cap or rank, liquidity, volatility). Port of
// web/src/engine/opportunities.ts, tested with the same cases (OpportunitiesTest ↔ web/test/opportunities.test.ts).

@Serializable
data class OppHit(
    /** "setup" | "reversal" | "breakout" | "volume" | "oversold" | "fundamentals". */
    val category: String,
    val reason: String = "",
    val strength: Double = 0.0,
)

@Serializable
data class OppItem(
    val symbol: String,
    val name: String = "",
    val sector: String = "",
    /** Stocks: Nasdaq market cap, USD. Cryptos: null (rank instead). */
    val marketCap: Double? = null,
    /** Cryptos: CoinGecko market-cap rank. */
    val rank: Int? = null,
    val price: Double = 0.0,
    val time: Double = 0.0,
    val change1d: Double? = null,
    val rsi14: Double? = null,
    val volumeRatio: Double? = null,
    /** ATR(14) ÷ price, % per day. */
    val volatility: Double? = null,
    /** Average daily traded value over 20 sessions, USD. */
    val liquidity: Double? = null,
    val distanceAtr: Double? = null,
    val hits: List<OppHit> = emptyList(),
)

@Serializable
data class OppCategoryInfo(val id: String, val label: String = "", val rule: String = "", val analyzed: Int = 0, val note: String? = null, val error: String? = null)

@Serializable
data class OppNotCovered(val label: String = "", val reason: String = "")

@Serializable
data class OpportunityReport(
    val kind: Kind = Kind.STOCK,
    val asOf: Double = 0.0,
    val scanned: Int = 0,
    val universe: String = "",
    val topN: Int = 0,
    val categories: List<OppCategoryInfo> = emptyList(),
    val items: List<OppItem> = emptyList(),
    val notCovered: List<OppNotCovered> = emptyList(),
    val source: String = "",
)

/** The scan, or "come back in a few seconds" (202 while the first scan of the day runs). */
sealed interface OpportunitiesResult {
    data object Pending : OpportunitiesResult
    data class Ready(val report: OpportunityReport) : OpportunitiesResult
}

/** Filters of the page, stored on the phone. */
@Serializable
data class OppFilters(
    val categories: List<String> = Opportunities.CATEGORIES,
    /** Stocks: minimum market cap (USD). */
    val minCap: Double? = null,
    /** Cryptos: best rank allowed (top N by market cap). */
    val maxRank: Int? = null,
    /** Minimum average daily traded value (USD). */
    val minLiquidity: Double? = null,
    /** Maximum daily volatility (ATR %). */
    val maxVolatility: Double? = null,
)

/** Market and filters of the page, saved on this phone. */
@Serializable
data class OppSaved(val market: Kind = Kind.STOCK, val filters: OppFilters = OppFilters())

// ---------- Anomalies of one asset ----------

@Serializable
data class Anomaly(
    /** "volume" | "priceVolume" | "zScore" | "openInterest" | "funding" | "longShort". */
    val code: String = "",
    /** "normal" | "warning" | "high". */
    val severity: String = "normal",
    val triggered: Boolean = false,
    val title: String = "",
    val value: Double = 0.0,
    val threshold: Double = 0.0,
    val unit: String = "",
    val measured: String = "",
    val meaning: String = "",
    val source: String = "",
)

@Serializable
data class LiquidationSummary(
    val longUsd: Double = 0.0,
    val shortUsd: Double = 0.0,
    val longCount: Int = 0,
    val shortCount: Int = 0,
    val largest: Largest? = null,
    val from: Double = 0.0,
    val to: Double = 0.0,
    val hours: Double = 0.0,
    val complete: Boolean = false,
    val scope: String = "",
) {
    @Serializable
    data class Largest(val usd: Double, val long: Boolean = false, val price: Double = 0.0, val time: Double = 0.0)
}

@Serializable
data class Derivatives(
    val source: String = "",
    val liquidations: LiquidationSummary? = null,
    val openInterest: OpenInterest? = null,
    /** Rates in % per settlement period. */
    val funding: Funding? = null,
    val longShort: LongShort? = null,
    val errors: List<String> = emptyList(),
    val notCovered: List<OppNotCovered> = emptyList(),
) {
    @Serializable
    data class OpenInterest(val usd: Double, val time: Double = 0.0, val change24h: Double? = null, val change7d: Double? = null)

    @Serializable
    data class Funding(val rate: Double, val p5: Double = 0.0, val p95: Double = 0.0, val samples: Int = 0, val periodHours: Double? = null, val time: Double = 0.0)

    @Serializable
    data class LongShort(val ratio: Double, val p5: Double = 0.0, val p95: Double = 0.0, val samples: Int = 0)
}

@Serializable
data class AnomalyReport(
    val symbol: String,
    val kind: Kind? = null,
    val asOf: Double = 0.0,
    val price: Double? = null,
    val session: Double? = null,
    val anomalies: List<Anomaly> = emptyList(),
    val normal: List<Anomaly> = emptyList(),
    val derivatives: Derivatives? = null,
    val errors: List<String> = emptyList(),
    val source: String = "",
    /** Rate the server wrote its texts with (additive). */
    val fx: FxInfo? = null,
)

object Opportunities {
    val CATEGORIES = listOf("setup", "reversal", "breakout", "volume", "oversold", "fundamentals")
    val SHORT = mapOf(
        "setup" to "Configurations",
        "reversal" to "Retournements",
        "breakout" to "Cassures",
        "volume" to "Volume anormal",
        "oversold" to "Survendus",
        "fundamentals" to "Fondamentaux",
    )

    fun short(c: String) = SHORT[c] ?: c

    val DEFAULT_FILTERS = OppFilters()

    /** Choices of the filters (label, value), same as the web page: thresholds in dollars, labelled in the display currency. */
    val CAPS: List<Pair<String, Double?>>
        get() = listOf<Pair<String, Double?>>("Toutes" to null) + listOf(1e10, 5e10, 2e11).map { "≥ ${Format.compactUsd(it)}" to it }
    val RANKS: List<Pair<String, Int?>> = listOf("Tous" to null, "Top 20" to 20, "Top 50" to 50, "Top 100" to 100)
    val LIQ: List<Pair<String, Double?>>
        get() = listOf<Pair<String, Double?>>("Toutes" to null) + listOf(1e6, 1e7, 1e8).map { "≥ ${Format.compactUsd(it)} / jour" to it }
    val VOL: List<Pair<String, Double?>> = listOf("Toutes" to null, "≤ 2 % / jour" to 2.0, "≤ 4 % / jour" to 4.0, "≤ 8 % / jour" to 8.0)

    /** Whether an asset passes the numeric filters (an unknown value fails a filter that is set). */
    fun passes(item: OppItem, f: OppFilters): Boolean {
        if (f.minCap != null && !(item.marketCap != null && item.marketCap >= f.minCap)) return false
        if (f.maxRank != null && !(item.rank != null && item.rank <= f.maxRank)) return false
        if (f.minLiquidity != null && !(item.liquidity != null && item.liquidity >= f.minLiquidity)) return false
        if (f.maxVolatility != null && !(item.volatility != null && item.volatility <= f.maxVolatility)) return false
        return true
    }

    /** Assets that pass the filters, with only the hits of the chosen categories; the most hits (then strongest) first. */
    fun filterItems(items: List<OppItem>, f: OppFilters): List<OppItem> {
        val wanted = f.categories.toSet()
        fun best(i: OppItem) = max(0.0, i.hits.maxOfOrNull { it.strength } ?: 0.0)
        return items.filter { passes(it, f) }
            .map { i -> i.copy(hits = i.hits.filter { it.category in wanted }) }
            .filter { it.hits.isNotEmpty() }
            .sortedWith(compareByDescending<OppItem> { it.hits.size }.thenByDescending { best(it) })
    }

    /** Number of assets per category once the numeric filters are applied (for the chips). */
    fun countByCategory(items: List<OppItem>, f: OppFilters): Map<String, Int> {
        val out = CATEGORIES.associateWith { 0 }.toMutableMap()
        for (i in items) {
            if (!passes(i, f)) continue
            for (c in i.hits.map { it.category }.toSet()) if (c in out) out[c] = out.getValue(c) + 1
        }
        return out
    }

    /** A chip toggled (the order of the choices is kept). */
    fun toggle(f: OppFilters, c: String): OppFilters =
        f.copy(categories = if (c in f.categories) f.categories - c else f.categories + c)

    private val Lenient = Json { ignoreUnknownKeys = true; coerceInputValues = true }

    /** Saved market and filters; unknown categories are dropped, a damaged value gives the defaults. */
    fun parseSaved(raw: String?): OppSaved {
        val s = raw?.let { runCatching { Lenient.decodeFromString(OppSaved.serializer(), it) }.getOrNull() } ?: return OppSaved()
        return s.copy(filters = s.filters.copy(categories = s.filters.categories.filter { it in CATEGORIES }))
    }

    fun encodeSaved(s: OppSaved): String = Lenient.encodeToString(OppSaved.serializer(), s)

    /** "The body of a 202": {pending: true}. */
    fun isPending(body: String): Boolean =
        runCatching { (AltimJson.parseToJsonElement(body).jsonObject["pending"] as? JsonPrimitive)?.content == "true" }.getOrDefault(false)

    // ---------- Anomalies ----------

    /** Share of the liquidated value that was long positions (%), null when nothing was liquidated. */
    fun longShare(l: LiquidationSummary): Double? {
        val total = l.longUsd + l.shortUsd
        return if (total > 0) l.longUsd / total * 100 else null
    }

    /** A dollar amount in the display currency: "12,3 M€", "850 k€", "420 €" ("$" without a rate). */
    fun compactUsd(usd: Double): String {
        val v = Money.toDisplay(usd)
        val s = Money.symbol()
        val a = abs(v)
        return when {
            a >= 1e9 -> "${Format.plain(v / 1e9, 1)} Md$s"
            a >= 1e6 -> "${Format.plain(v / 1e6, 1)} M$s"
            a >= 1e3 -> "${Format.plain(v / 1e3, 0)} k$s"
            else -> "${Format.plain(v, 0)} $s"
        }
    }

    /** "+12,3 %" / "−0,0100 %" (maximum [digits] decimals). */
    fun signed(v: Double, digits: Int = 1) = "${if (v >= 0) "+" else "−"}${Format.plain(abs(v), digits)} %"

    /** "RSI 14 : 61", "volatilité 2,3 %/j", "échangé 32 Md€/j", "capitalisation 4 100 Md€" (the known ones). */
    fun metrics(i: OppItem): List<String> = listOfNotNull(
        i.rsi14?.let { "RSI 14 : ${Math.round(it)}" },
        i.volatility?.let { "volatilité ${Format.plain(it, 1)} %/j" },
        i.liquidity?.let { "échangé ${compactUsd(it)}/j" },
        i.marketCap?.let { "capitalisation ${compactUsd(it)}" },
    )

    /** Shown while the first scan of the day runs. */
    fun pendingText(kind: Kind) = "Analyse des ${if (kind == Kind.CRYPTO) "120 cryptos" else "150 actions"} en cours (environ 30 secondes la première fois)…"

    /** "+1,2 %" of the last session (one decimal at most). */
    fun change(v: Double) = signed(v, 1)
}
