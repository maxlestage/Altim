package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.math.RoundingMode
import java.text.Collator
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs

// « Validation du modèle » (GET /api/validation, 202 {pending:true} while the first computation runs): JSON contract
// (every field optional) and the pure helpers of the screen (sorting, texts). Port of web/src/webapp/model-validation.ts,
// tested with the same cases (ModelValidationTest ↔ web/test/validation.test.ts). Percentages are in %, times in ms. The
// server computes everything; nothing here recomputes a statistic.

/** Trade statistics pooled over every trade of a group (each trade weighs the same). */
interface ValPooled {
    val trades: Int
    val winRate: Double?
    val profitFactor: Double?
    /** Mean net return per trade (%), costs included. */
    val expectancy: Double?
    val avgR: Double?
    /** Mean ÷ standard error of the net trade returns. */
    val tStat: Double?
}

@Serializable
data class ValRegimeGroup(
    /** "bull" | "bear" | "range" | "crisis" | "unknown". */
    val regime: String = "unknown",
    val label: String = "",
    override val trades: Int = 0,
    override val winRate: Double? = null,
    override val profitFactor: Double? = null,
    override val expectancy: Double? = null,
    override val avgR: Double? = null,
    override val tStat: Double? = null,
    /** Asset-days in this regime (regime read the day before). */
    val days: Int = 0,
    /** Assets with ≥ 20 days in it; those where the signal made more than buy-and-hold over these days. */
    val assets: Int = 0,
    val beatHold: Int = 0,
    val beatShare: Double? = null,
    /** Medians over those assets, compounded over the regime's days (%). */
    val medianSignal: Double? = null,
    val medianHold: Double? = null,
    val lowSample: Boolean = false,
    /** "insufficient" | "edge" | "negative" | "unproven". */
    val verdict: String = "unproven",
    val verdictLabel: String = "",
) : ValPooled

@Serializable
data class ValNamed(val symbol: String = "", val value: Double = 0.0)

@Serializable
data class ValGroup(
    /** "all" or the asset class ("stock" | "btc" | "eth" | "altcoin"). */
    val id: String = "all",
    val label: String = "",
    val assets: Int = 0,
    val years: Double? = null,
    override val trades: Int = 0,
    override val winRate: Double? = null,
    override val profitFactor: Double? = null,
    override val expectancy: Double? = null,
    override val avgR: Double? = null,
    override val tStat: Double? = null,
    val medianReturn: Double? = null,
    val worstReturn: ValNamed? = null,
    val medianHold: Double? = null,
    /** ≤ 0. */
    val medianDrawdown: Double? = null,
    val worstDrawdown: ValNamed? = null,
    val medianHoldDrawdown: Double? = null,
    val medianSharpe: Double? = null,
    val medianSortino: Double? = null,
    val medianExposure: Double? = null,
    val beatHold: Int = 0,
    val beatShare: Double? = null,
    val medianAfterTax: Double? = null,
    val medianHoldAfterTax: Double? = null,
    val lowSample: Boolean = false,
    val verdict: String = "unproven",
    val verdictLabel: String = "",
    val regimes: List<ValRegimeGroup> = emptyList(),
) : ValPooled

@Serializable
data class ValRegimeDays(val regime: String = "unknown", val days: Int = 0, val signal: Double = 0.0, val hold: Double = 0.0)

@Serializable
data class ValAsset(
    val symbol: String,
    val name: String = "",
    val kind: Kind = Kind.STOCK,
    val `class`: String = "stock",
    val group: String = "",
    val from: Double = 0.0,
    val to: Double = 0.0,
    val bars: Int = 0,
    val trades: Int = 0,
    val winRate: Double? = null,
    val profitFactor: Double? = null,
    val expectancy: Double? = null,
    val avgR: Double? = null,
    val maxDrawdown: Double = 0.0,
    val holdMaxDrawdown: Double = 0.0,
    val sharpe: Double? = null,
    val sortino: Double? = null,
    val totalReturn: Double = 0.0,
    val buyAndHold: Double = 0.0,
    val exposure: Double = 0.0,
    val beatHold: Boolean = false,
    val afterTax: Double = 0.0,
    val holdAfterTax: Double = 0.0,
    val lowSample: Boolean = false,
    val regimeDays: List<ValRegimeDays> = emptyList(),
    val source: String = "",
) {
    /** Signal − buy-and-hold (points of %). */
    val gap: Double get() = totalReturn - buyAndHold
    val asset: Asset get() = Asset(symbol, kind, name.ifBlank { symbol })
}

@Serializable
data class ValFailure(val symbol: String = "", val name: String = "", val kind: Kind = Kind.STOCK, val `class`: String = "stock", val error: String = "")

@Serializable
data class ValBasketEntry(val symbol: String = "", val name: String = "", val kind: Kind = Kind.STOCK, val `class`: String = "stock", val group: String = "")

@Serializable
data class ValParameters(
    val feesPct: Double = 0.0,
    val slippagePct: Double = 0.0,
    val spreadStockPct: Double = 0.0,
    val spreadCryptoPct: Double = 0.0,
    val lookback: Int = 0,
    val warmup: Int = 0,
    val rewardRisk: Double = 0.0,
    val taxRatePct: Double = 30.0,
    val minTrades: Int = 0,
    val tEdge: Double = 0.0,
    val regimeRule: String = "",
)

@Serializable
data class ValOutOfSample(
    /** "verified" | "notVerifiable". */
    val status: String = "notVerifiable",
    val note: String = "",
)

@Serializable
data class ValidationReport(
    val asOf: Double = 0.0,
    val basketFixedOn: String = "",
    val headline: String = "",
    val from: Double? = null,
    val to: Double? = null,
    val years: Double? = null,
    val minYears: Double? = null,
    val maxYears: Double? = null,
    val overall: ValGroup = ValGroup(),
    val classes: List<ValGroup> = emptyList(),
    /** Basket order (by class), never ranked by performance. */
    val assets: List<ValAsset> = emptyList(),
    val failures: List<ValFailure> = emptyList(),
    val basket: List<ValBasketEntry> = emptyList(),
    val parameters: ValParameters = ValParameters(),
    val protections: List<String> = emptyList(),
    val outOfSample: ValOutOfSample = ValOutOfSample(),
    val limits: List<String> = emptyList(),
    val source: String = "",
)

/** The report, or "come back in a few seconds" (202 while the first computation runs). */
sealed interface ValidationResult {
    data object Pending : ValidationResult
    data class Ready(val report: ValidationReport) : ValidationResult
}

object ModelValidation {
    const val PATH = "/api/validation"

    val CLASS_ORDER = listOf("stock", "btc", "eth", "altcoin")
    val CLASS_SHORT = mapOf("stock" to "Actions", "btc" to "Bitcoin", "eth" to "Ethereum", "altcoin" to "Altcoins")

    fun classShort(c: String): String = CLASS_SHORT[c] ?: c

    enum class Sort(val label: String) { CLASS("Par classe"), NAME("Par nom"), GAP("Écart avec la détention") }

    /**
     * Rows in the chosen order: by class (the basket's own order, the default), by name, or by the gap signal −
     * buy-and-hold (largest first). Stable sorts; never changes the report.
     */
    fun sortAssets(assets: List<ValAsset>, key: Sort): List<ValAsset> = when (key) {
        Sort.NAME -> {
            val collator = Collator.getInstance(Locale.FRENCH)
            assets.sortedWith { a, b -> collator.compare(a.symbol, b.symbol) }
        }
        Sort.GAP -> assets.sortedByDescending { it.gap }
        Sort.CLASS -> assets.sortedBy { CLASS_ORDER.indexOf(it.`class`) }
    }

    /** Narrow no-break space, as Intl in the browser. */
    const val NNBSP = " "

    /** French number with at most [max] decimals, rounded half away from zero like Intl in the browser. */
    fun fr(v: Double, max: Int): String = JsFormat.fr(v, max)

    /** "+12,3 %", "−4 %", "—". */
    fun signedPct(v: Double?, digits: Int = 1): String {
        if (v == null || !v.isFinite()) return "—"
        val sign = if (v < 0) "−" else if (v > 0) "+" else ""
        return "$sign${fr(abs(v), digits)}$NNBSP%"
    }

    fun plain(v: Double?, digits: Int = 2): String {
        if (v == null || !v.isFinite()) return "—"
        return "${if (v < 0) "−" else ""}${fr(abs(v), digits)}"
    }

    fun years(v: Double?): String {
        if (v == null || !v.isFinite()) return "—"
        return "${fr(v, 1)}${NNBSP}an${if (v >= 2) "s" else ""}"
    }

    /** Tone of a verdict for the chips: good only for a measured positive gain, warning for a measured loss. */
    fun verdictTone(v: String): Tone = when (v) {
        "edge" -> Tone.GOOD
        "negative" -> Tone.WARN
        else -> Tone.NEUTRAL
    }

    /** "6 sur 34 (18 %)". */
    fun beatText(beat: Int, n: Int, share: Double?): String {
        if (n == 0) return "aucun actif comparable"
        return "$beat sur $n${if (share == null) "" else " (${fr(share, 0)}$NNBSP%)"}"
    }

    /** What the regime days say, in one sentence. */
    fun regimeDaysText(g: ValRegimeGroup): String {
        if (g.assets == 0) return "Aucun actif n'a passé 20 jours dans ce régime : pas de comparaison avec la détention."
        return "Pendant ces jours, le signal a fait mieux que la détention sur ${beatText(g.beatHold, g.assets, g.beatShare)} actifs " +
            "(médianes : signal ${signedPct(g.medianSignal)}, détention ${signedPct(g.medianHold)})."
    }

    /** Trades line of a pooled group: "650 trades · réussite 38 % · espérance +0,2 % · t = 2,2". */
    fun pooledText(p: ValPooled): String {
        if (p.trades == 0) return "Aucun trade."
        val parts = mutableListOf(
            "${p.trades} trade${if (p.trades > 1) "s" else ""}",
            "réussite ${fr(p.winRate ?: 0.0, 0)}$NNBSP%",
            "espérance ${signedPct(p.expectancy, 2)}",
        )
        p.profitFactor?.let { parts += "facteur de profit ${plain(it)}" }
        p.tStat?.let { parts += "t = ${plain(it, 1)}" }
        return parts.joinToString(" · ")
    }

    private val MONTHS = listOf("janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.")

    /** "déc. 2021" (UTC), "?" when unknown. */
    fun monthYear(ms: Double?): String {
        if (ms == null) return "?"
        val d = Instant.ofEpochMilli(ms.toLong()).atZone(ZoneOffset.UTC)
        return "${MONTHS[d.monthValue - 1]} ${d.year}"
    }

    /** Regimes of a group, the "unknown" one (history too short to classify) last. */
    fun regimesOf(g: ValGroup): List<ValRegimeGroup> = g.regimes.sortedBy { if (it.regime == "unknown") 1 else 0 }

    /** "Tous" or the short class name, for the regime filter chips. */
    fun groupChip(g: ValGroup): String = if (g.id == "all") "Tous" else classShort(g.id)

    /** History tile: "0,5 à 4,8 ans" when the spans differ by 0.2 year or more, else the median span. */
    fun historyTile(r: ValidationReport): String {
        val lo = r.minYears
        val hi = r.maxYears
        return if (lo != null && hi != null && hi - lo >= 0.2) "${plain(lo, 1)} à ${years(hi)}" else years(r.years)
    }

    /** "Période : déc. 2021 – sept. 2026 (bougies journalières). Calculé le 29/09/2026 08:22, mis à jour toutes les 12 h." */
    fun periodText(r: ValidationReport, zone: ZoneId = ZoneId.systemDefault()): String {
        val at = DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm", Locale.FRANCE).withZone(zone).format(Instant.ofEpochMilli(r.asOf.toLong()))
        return "Période : ${monthYear(r.from)} – ${monthYear(r.to)} (bougies journalières). Calculé le $at, mis à jour toutes les 12 h."
    }

    /** "2 actifs non testés :" */
    fun failuresTitle(n: Int): String = "$n actif${if (n > 1) "s" else ""} non testé${if (n > 1) "s" else ""} :"

    /** Group card subtitle: "22 actifs · historique médian 4,8 ans". */
    fun groupSubtitle(g: ValGroup): String = "${g.assets} actif${if (g.assets > 1) "s" else ""} · historique médian ${years(g.years)}"

    /** "14 469 jours-actifs dans ce régime." */
    fun regimeDays(g: ValRegimeGroup): String = "${fr(g.days.toDouble(), 0)} jours-actifs dans ce régime."

    /** "Signal +9 % · détention +95,6 % (moins bien, écart −86,6 %)". */
    fun assetLine(a: ValAsset): String =
        "Signal ${signedPct(a.totalReturn)} · détention ${signedPct(a.buyAndHold)} (${if (a.beatHold) "mieux" else "moins bien"}, écart ${signedPct(a.gap)})"

    /** Detail line of an asset row (trades, win rate, drawdown, Sharpe, span, source). */
    fun assetDetails(a: ValAsset): String =
        "${a.trades} trade${if (a.trades > 1) "s" else ""} · réussite ${if (a.winRate == null) "—" else "${plain(a.winRate, 0)}$NNBSP%"} · " +
            "facteur de profit ${plain(a.profitFactor)} · espérance ${signedPct(a.expectancy, 2)} · recul max ${signedPct(a.maxDrawdown)} " +
            "(détention ${signedPct(a.holdMaxDrawdown)}) · Sharpe ${plain(a.sharpe)} · Sortino ${plain(a.sortino)} · " +
            "${years((a.to - a.from) / (365.25 * 86_400_000))} (${a.source})"

    /** Costs line of « Biais et limites ». */
    fun costsText(r: ValidationReport): String {
        val p = r.parameters
        val fixed = r.basketFixedOn.split("-").reversed().joinToString("/")
        return "Coûts par ordre : frais ${plain(p.feesPct, 3)}$NNBSP%, glissement ${plain(p.slippagePct, 3)}$NNBSP%, écart achat/vente supposé ${plain(p.spreadStockPct, 3)}$NNBSP% (actions) ou " +
            "${plain(p.spreadCryptoPct, 3)}$NNBSP% (cryptos), moitié payée à chaque ordre. Panier fixé le $fixed. Source : ${r.source}."
    }

    fun isPending(body: String): Boolean = Opportunities.isPending(body)
}
