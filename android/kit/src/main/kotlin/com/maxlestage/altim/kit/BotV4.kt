package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

// « Bots sélectifs » (v4, additive, defaulted: `v4` of /api/bot, of an asset row and of a view): 16 bots that speak
// rarely — only when two models agree on an extreme score — each with its measured precision (Wilson 95 % interval),
// signals per year and status (prouvé, en attente, contredit, non prouvé). Port of the Yew front's `app/bot/v4.rs` and
// of `altim_core::engine::bot_v4`'s texts. Codes (side, status, action) stay strings so an unknown value never breaks
// decoding. Rates in %, excesses in points of %, times in ms. Nothing here recomputes a statistic.

/** A bot's figures on one period; `reference` = the higher of the base rate and the random entry's share. */
@Serializable
data class BotV4Stats(
    val signals: Int = 0,
    val hits: Int = 0,
    val precision: Double? = null,
    val wilsonLow: Double? = null,
    val wilsonHigh: Double? = null,
    val baseRate: Double? = null,
    val randomRate: Double? = null,
    val reference: Double? = null,
    val t: Double? = null,
    val excess: Double? = null,
    val excessMedian: Double? = null,
    val perYear: Double? = null,
    val coverage: Double? = null,
    val meanProb: Double? = null,
    val from: Double? = null,
    val to: Double? = null,
    /** "precise" | "notProven" | "insufficient" | "contrary". */
    val verdict: String? = null,
)

@Serializable
data class BotV4Bot(
    /** "stock-20-rise". */
    val id: String = "",
    val group: String = "stock",
    val horizon: Int = 0,
    /** "rise" | "fall" | "top" | "bottom". */
    val side: String = "rise",
    val label: String = "",
    val hit: String = "",
    val models: String = "",
    val blocks: Int = 0,
    val openBlocks: Int = 0,
    val main: BotV4Stats = BotV4Stats(),
    val extra: BotV4Stats = BotV4Stats(),
    val forward: BotV4Stats = BotV4Stats(),
    /** "proven" | "pending" | "contradicted" | "notProven". */
    val status: String = "notProven",
    /** Today: the gate's level (%; null: closed) and the basket assets it speaks on. */
    val todayLevel: Double? = null,
    val today: List<String> = emptyList(),
    val text: String = "",
)

@Serializable
data class BotV4K(val v1: Int = 0, val v2: Int = 0, val v3: Int = 0, val v4: Int = 0, val total: Int = 0)

@Serializable
data class BotV4Parameters(val minSignals: Int = 30)

@Serializable
data class BotV4Report(
    val preregDate: String = "",
    val afterPrereg: List<String> = emptyList(),
    val k: BotV4K = BotV4K(),
    val tRequired: Double = 0.0,
    val headline: String = "",
    val forwardHeadline: String = "",
    val bots: List<BotV4Bot> = emptyList(),
    val method: List<String> = emptyList(),
    val limits: List<String> = emptyList(),
    val parameters: BotV4Parameters = BotV4Parameters(),
)

/** The bots speaking today on a basket asset (ids). */
@Serializable
data class BotV4AssetNow(val time: Double? = null, val bots: List<String> = emptyList())

/** One bot today for one asset; `action` null: « pas d'avis ». */
@Serializable
data class BotV4Signal(
    val id: String = "",
    val label: String = "",
    val horizon: Int = 0,
    val side: String = "rise",
    val action: String? = null,
    val probability: Double? = null,
    val precision: Double? = null,
    val wilsonLow: Double? = null,
    val wilsonHigh: Double? = null,
    val reference: Double? = null,
    val perYear: Double? = null,
    val status: String = "notProven",
    val counts: Boolean = false,
    val text: String = "",
)

@Serializable
data class BotV4View(
    val available: Boolean = false,
    val signals: List<BotV4Signal> = emptyList(),
    val counts: Boolean = false,
    val nudge: Double = 0.0,
    val note: String = "",
) {
    val speaking: List<BotV4Signal> get() = signals.filter { it.action != null }
}

/** Texts of the « Bots sélectifs » section and line (same as the Yew front). */
object BotV4Texts {
    const val SECTION_TITLE = "Bots sélectifs"
    const val INTRO = "Chaque bot parle rarement : seulement quand deux modèles sont d'accord sur un score extrême, sinon « pas d'avis ». Précision = part des signaux qui ont réussi, avec son intervalle à 95 % ; « précis » seulement si le bas de l'intervalle dépasse le hasard et que le t dépasse le seuil corrigé."
    const val FORWARD_TITLE = "Depuis le 02/10/2026 (test sur l'avenir)"
    const val METHOD_TITLE = "Comment les bots sélectifs sont jugés"

    fun title(r: BotV4Report): String = "Bots sélectifs · pré-enregistrés le ${r.preregDate.split("-").reversed().joinToString("/")}"

    fun sideLabel(side: String): String = when (side) {
        "rise" -> "Hausse"
        "fall" -> "Baisse"
        "top" -> "Haut du classement"
        "bottom" -> "Bas du classement"
        else -> side
    }

    fun avis(side: String): String = when (side) {
        "rise" -> "ACHETER (hausse attendue)"
        "fall" -> "VENDRE (baisse attendue)"
        "top" -> "ACHETER (dans le haut de son groupe)"
        "bottom" -> "ALLÉGER (dans le bas de son groupe)"
        else -> side
    }

    fun statusLabel(status: String): String = when (status) {
        "proven" -> "prouvé"
        "pending" -> "en attente"
        "contradicted" -> "contredit"
        else -> "non prouvé"
    }

    private fun num(v: Double, digits: Int): String {
        val raw = String.format(java.util.Locale.ROOT, "%.${digits}f", v)
        val s = if ('.' in raw) raw.trimEnd('0').trimEnd('.') else raw
        return (if (s == "-0") "0" else s).replace('.', ',').replace("-", "−")
    }

    /** "62 %" (null: "—"). */
    fun pct(v: Double?): String = if (v == null) "—" else "${num(v, 0)} %"

    /** "62 % (57–67 %)" or "aucun signal". */
    fun precisionShort(s: BotV4Stats): String =
        if (s.signals == 0) "aucun signal" else "${pct(s.precision)} (${pct(s.wilsonLow)}–${pct(s.wilsonHigh)})"

    fun todayText(b: BotV4Bot): String = when {
        b.todayLevel == null -> "pas d'avis (seuil fermé sur la dernière année)"
        b.today.isEmpty() -> "pas d'avis"
        else -> "${avis(b.side)} : ${b.today.joinToString(", ")}"
    }

    fun rhythm(s: BotV4Stats): String = if (s.perYear == null) "${s.signals} · rythme —" else "${s.signals} · ${num(s.perYear, 1)} signaux par an"

    fun tText(s: BotV4Stats, required: Double): String = "${s.t?.let { num(it, 1) } ?: "—"} (requis ${num(required, 2)})"

    fun preciseCount(r: BotV4Report): String = "${r.bots.count { it.status != "notProven" }} / ${r.bots.size}"

    /** "Actions et ETF américains · 20 jours": cards of four bots, in the report's order. */
    fun cards(r: BotV4Report): List<Pair<String, List<BotV4Bot>>> {
        val out = mutableListOf<Pair<String, MutableList<BotV4Bot>>>()
        for (b in r.bots) {
            val t = "${if (b.group == "crypto") "Cryptos (bitcoin, ether, altcoins)" else "Actions et ETF américains"} · ${b.horizon} jours"
            if (out.isNotEmpty() && out.last().first == t) out.last().second.add(b) else out.add(t to mutableListOf(b))
        }
        return out
    }

    fun viewHead(v: BotV4View): String {
        val n = v.speaking.size
        return if (n == 0) "pas d'avis aujourd'hui (${v.signals.size} bots)" else "$n avis sur ${v.signals.size} bots"
    }

    fun signalText(s: BotV4Signal): String =
        "${avis(s.side)} · précision mesurée ${pct(s.precision)} (${pct(s.wilsonLow)}–${pct(s.wilsonHigh)}) contre ${pct(s.reference)} au hasard · ${s.perYear?.let { num(it, 1) } ?: "—"} signaux par an"

    /** « Hausse 20 j » from a bot's id. */
    fun shortLabel(id: String): String {
        val p = id.split("-", limit = 3)
        return if (p.size == 3 && p[2] in listOf("rise", "fall", "top", "bottom")) "${sideLabel(p[2])} ${p[1]} j" else id
    }
}
