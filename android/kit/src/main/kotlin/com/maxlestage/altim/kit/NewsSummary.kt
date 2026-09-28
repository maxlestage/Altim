package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.util.Locale
import kotlin.math.abs

// "Résumé intelligent" of the news (`summary` of /api/news, computed by the server in engine/news_summary.rs): types
// and the pure helpers of the card, port of web/src/webapp/news-summary.ts tested on the same real sample
// (NewsSummaryTest ↔ web/test/news-summary.test.ts). Every field has a default: an older server has no `summary`.

@Serializable
data class StorySummary(
    /** Id of the merged story in `items`. */
    val id: String,
    val title: String,
    val link: String = "",
    val source: String = "",
    val time: Double = 0.0,
    val category: String = "",
    val themes: List<String> = emptyList(),
    val alert: Boolean = false,
    /** Distinct sources that told the story. */
    val sources: Int = 1,
    /** Distinct headlines among them (one article syndicated word for word counts once): what the rule counts. */
    val independentSources: Int = 1,
    /** Each source's own headline (8 at most), the first one first. */
    val links: List<Link> = emptyList(),
    /** The user's assets named in the headlines: "stock:AAPL"… */
    val assets: List<String> = emptyList(),
    /** "low" | "medium" | "high". */
    val impact: String = "low",
    /** "measured": from the move of the named assets since publication; "rule": sources, theme, the user's assets. */
    val impactBasis: String = "rule",
    val ruleImpact: String = "low",
    val impactPoints: Int = 0,
    /** French, one line per point of the rule, and the measured moves. */
    val impactReasons: List<String> = emptyList(),
    val consensus: Consensus = Consensus(),
    /** Hourly closes of the server's cache: from the close preceding publication to the last one. */
    val moves: List<Move> = emptyList(),
    /** The story against the asset's technical trend (the guard's, when cached). */
    val technical: List<Technical> = emptyList(),
) {
    @Serializable
    data class Link(val source: String = "", val title: String = "", val link: String = "", val time: Double = 0.0, val tone: String = "neutral")

    /** agreement: "convergent" | "divergent" | "single". */
    @Serializable
    data class Consensus(val agreement: String = "single", val tone: String = "neutral", val negative: Int = 0, val positive: Int = 0, val neutral: Int = 0)

    @Serializable
    data class Move(
        val asset: String,
        val fromTime: Double = 0.0,
        val fromPrice: Double = 0.0,
        val toTime: Double = 0.0,
        val toPrice: Double = 0.0,
        val changePct: Double,
        val source: String = "",
    )

    /** trend: "up" | "down" | "range". */
    @Serializable
    data class Technical(val asset: String, val trend: String = "range", val text: String = "")
}

/** "stock:AAPL" → kind, symbol. */
data class AssetLink(val kind: String, val symbol: String)

object NewsSummary {
    val IMPACT_LABEL = mapOf("low" to "faible", "medium" to "moyen", "high" to "important")

    fun impactLabel(impact: String): String = IMPACT_LABEL[impact] ?: impact

    fun basisLabel(s: StorySummary): String = if (s.impactBasis == "measured") "impact mesuré" else "impact estimé par règle"

    data class Heading(val title: String, val others: String?)

    /** Counts the events of moyen or important impact; the faible ones are "autres sujets". */
    fun heading(list: List<StorySummary>): Heading {
        val n = list.count { it.impact != "low" }
        val rest = list.size - n
        val title = when (n) {
            0 -> "Aucun événement important aujourd'hui"
            1 -> "1 événement important aujourd'hui"
            else -> "$n événements importants aujourd'hui"
        }
        val others = if (rest == 0) null else "${if (n == 0) "" else "Et "}$rest sujet${if (rest > 1) "s" else ""} repris par plusieurs sources, à impact faible."
        return Heading(title, others)
    }

    private fun plural(n: Int, one: String, many: String) = "$n ${if (n > 1) many else one}"

    /** "Convergent · 3 sources · ton des titres : 2 négatifs, 1 neutre" (tones counted over the distinct headlines). */
    fun consensusText(s: StorySummary): String {
        val c = s.consensus
        if (c.agreement == "single") {
            return if (s.sources > 1) "Même titre repris par ${s.sources} sources : pas de consensus mesurable" else "Une seule source : pas de consensus mesurable"
        }
        val parts = listOfNotNull(
            c.negative.takeIf { it != 0 }?.let { plural(it, "négatif", "négatifs") },
            c.positive.takeIf { it != 0 }?.let { plural(it, "positif", "positifs") },
            c.neutral.takeIf { it != 0 }?.let { plural(it, "neutre", "neutres") },
        )
        val titles = c.negative + c.positive + c.neutral
        val head = "${if (c.agreement == "convergent") "Convergent" else "Divergent"} · ${plural(s.sources, "source", "sources")}"
        return "$head${if (titles < s.sources) " ($titles titres distincts)" else ""} · ton des titres : ${parts.joinToString(", ")}"
    }

    /** "stock:AAPL" → kind "stock", symbol "AAPL" (the asset screen to open). */
    fun assetLink(id: String): AssetLink {
        if (!id.contains(":")) return AssetLink("", id)
        val parts = id.split(":")
        return AssetLink(parts[0], parts.getOrElse(1) { id })
    }

    /** The asset to open from "stock:AAPL" (null for an unknown kind). */
    fun asset(id: String): Asset? {
        val a = assetLink(id)
        return Kind.of(a.kind)?.let { Asset(a.symbol, it, a.symbol) }
    }

    /** "AAPL +4,0 % depuis la publication". */
    fun moveText(m: StorySummary.Move): String {
        val pct = "${if (m.changePct >= 0) "+" else "−"}${String.format(Locale.ROOT, "%.1f", abs(m.changePct)).replace('.', ',')} %"
        return "${assetLink(m.asset).symbol} $pct depuis la publication"
    }

    /** "Règle : … → 3 points, impact moyen par règle ; retenu : …" (the "Sources et calcul" detail). */
    fun ruleText(s: StorySummary): String {
        val rule = s.impactReasons.filter { it.contains("(+") }.joinToString(" ; ")
        val measured = s.impactReasons.filter { !it.contains("(+") }.joinToString(" ; ")
        return "Règle : $rule → ${s.impactPoints} point${if (s.impactPoints > 1) "s" else ""}, impact ${impactLabel(s.ruleImpact)} par règle" +
            if (s.impactBasis == "measured") " ; retenu : ${impactLabel(s.impact)}, d'après la variation mesurée ($measured)." else "."
    }
}
