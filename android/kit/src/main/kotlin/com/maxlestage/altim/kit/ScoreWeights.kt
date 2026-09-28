package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/**
 * Weights of the decision's composite score (Réglages → Score composite), sent as `w=tech:32,mom:18,…` only when they
 * differ from the server's defaults. Same factors, defaults and cleaning as web/src/webapp/decision.ts.
 */
@Serializable
data class ScoreWeights(
    val tech: Int = 32,
    val mom: Int = 18,
    val fund: Int = 20,
    val sent: Int = 10,
    val news: Int = 10,
    val macro: Int = 10,
) {
    operator fun get(key: String): Int = when (key) {
        "tech" -> tech
        "mom" -> mom
        "fund" -> fund
        "sent" -> sent
        "news" -> news
        else -> macro
    }

    /** This weight changed, kept within 0 – 100. */
    fun with(key: String, v: Int): ScoreWeights {
        val x = v.coerceIn(0, 100)
        return when (key) {
            "tech" -> copy(tech = x)
            "mom" -> copy(mom = x)
            "fund" -> copy(fund = x)
            "sent" -> copy(sent = x)
            "news" -> copy(news = x)
            else -> copy(macro = x)
        }
    }

    val total: Int get() = FACTORS.sumOf { this[it.key] }

    /** Whole numbers 0 – 100; all at 0 → the defaults. */
    fun sanitized(): ScoreWeights {
        val w = FACTORS.fold(this) { acc, f -> acc.with(f.key, this[f.key]) }
        return if (w.total > 0) w else DEFAULT
    }

    /** "tech:40,mom:18,…", or null for the default weights (the URL stays the same). */
    fun param(): String? {
        val s = sanitized()
        if (s == DEFAULT) return null
        return FACTORS.joinToString(",") { "${it.key}:${s[it.key]}" }
    }

    class Factor(val key: String, val label: String, val hint: String, val def: Int)

    companion object {
        val DEFAULT = ScoreWeights()

        /** Same factors and default weights as the server (backend/src/engine/synthesis.rs). */
        val FACTORS = listOf(
            Factor("tech", "Technique", "tendance, volume, volatilité, structure", 32),
            Factor("mom", "Momentum", "MACD, RSI, variation sur 1 mois", 18),
            Factor("fund", "Fondamentaux", "valorisation, comptes ou réseau", 20),
            Factor("sent", "Sentiment", "Fear & Greed, financement, StockTwits", 10),
            Factor("news", "Actualités", "ton des titres sur 24 h", 10),
            Factor("macro", "Macro", "stress des marchés, VIX, taux", 10),
        )

        /** Stored JSON cleaned like the web's sanitizeScoreWeights: a bad or missing value takes its default. */
        fun parse(raw: String?): ScoreWeights {
            val o = raw?.let { runCatching { AltimJson.parseToJsonElement(it) }.getOrNull() } as? JsonObject ?: return DEFAULT
            val w = FACTORS.fold(DEFAULT) { acc, f ->
                val v = (o[f.key] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull?.takeIf { it.isFinite() }
                acc.with(f.key, v?.let { Math.round(it).toInt().coerceIn(0, 100) } ?: f.def)
            }
            return w.sanitized()
        }
    }
}
