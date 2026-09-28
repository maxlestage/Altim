package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// « Pourquoi ça bouge ? » (GET /api/why?symbol=&kind=, backend/src/engine/why.rs): what was observed with today's move,
// from the server's cached sources only — co-occurring observations, never presented as causes. POST /api/ask sends a
// question with only that data to an AI model, when the server says `askEnabled`. Same contract as web WhyCard.tsx.

@Serializable
data class WhyFactor(
    val key: String = "",
    val label: String = "",
    /** "up" | "down" | "neutral". */
    val direction: String = "neutral",
    /** "low" | "medium" | "high". */
    val magnitude: String = "low",
    val detail: String = "",
    val source: String = "",
    /** "observed" | "possibleCorrelation" | "unverifiable". */
    val certainty: String = "unverifiable",
    val certaintyLabel: String = "",
    val brief: String? = null,
)

@Serializable
data class WhyReport(
    val symbol: String,
    val kind: Kind? = null,
    val name: String = "",
    val asOf: Double = 0.0,
    val change: Change? = null,
    val volume: Volume? = null,
    val factors: List<WhyFactor> = emptyList(),
    val notCovered: List<NotCovered> = emptyList(),
    val summary: String = "",
    val disclaimer: String = "",
    val askEnabled: Boolean = false,
    val sources: List<Source> = emptyList(),
) {
    @Serializable
    data class Change(val pct: Double, val price: Double = 0.0, val previousClose: Double = 0.0, val previousCloseTime: Double = 0.0, val atrPct: Double? = null)

    @Serializable
    data class Volume(val ratio: Double, val day: Double = 0.0, val volume: Double = 0.0, val average: Double = 0.0)

    @Serializable
    data class NotCovered(val key: String = "", val label: String = "", val reason: String = "")

    @Serializable
    data class Source(val name: String = "", val ok: Boolean = false, val detail: String = "")
}

@Serializable
internal data class AskRequest(val symbol: String, val kind: String, val question: String)

@Serializable
data class AskAnswer(val answer: String, val model: String = "", val question: String = "", val data: JsonElement? = null, val disclaimer: String = "") {
    /** The last decision computed on the server was sent with the observations. */
    val withDecision: Boolean get() = (data as? JsonObject)?.get("derniereDecision")?.let { it !is kotlinx.serialization.json.JsonNull } == true
}

object Why {
    fun directionIcon(d: String) = when (d) { "up" -> "↗"; "down" -> "↘"; else -> "→" }
    fun directionWord(d: String) = when (d) { "up" -> "haussier"; "down" -> "baissier"; else -> "neutre" }
    fun magnitudeWord(m: String) = when (m) { "low" -> "faible"; "medium" -> "moyen"; "high" -> "fort"; else -> m }

    /** "Non couvert : financement (…) ; …." or null. */
    fun notCoveredText(r: WhyReport): String? =
        if (r.notCovered.isEmpty()) null
        else "Non couvert : " + r.notCovered.joinToString(" ; ") { "${it.label.lowercase()} (${it.reason.removeSuffix(".")})" } + "."

    /** A question is sent from 3 characters, 500 at most. */
    fun canAsk(question: String) = question.trim().length >= 3

    const val ASK_NOTE = "Réponse d'un modèle d'IA (Claude, Anthropic) à partir des seules données ci-dessus, envoyées à Anthropic avec votre question (jamais vos avoirs). " +
        "Elle peut se tromper ; ce n'est pas un conseil. Quelques questions par minute au plus."

    fun answerDataText(a: AskAnswer) = "Données utilisées : les observations ci-dessus${if (a.withDecision) " et la dernière décision calculée" else ""}."
}
