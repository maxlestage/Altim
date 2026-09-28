package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/** News section (/api/news): articles from ~20 sources (FR + EN), stories told by several sources merged. */
@Serializable
data class NewsItem(
    val id: String,
    val title: String,
    val link: String,
    val time: Double,
    val source: String,
    val summary: String? = null,
    val lang: String = "en",
    /** "monde", "marches", "crypto" or "actifs" (names one of the user's assets). */
    val category: String,
    val themes: List<String> = emptyList(),
    /** "negative", "positive" or "neutral" (keywords of the title, indicative). */
    val tone: String = "neutral",
    val assets: List<String> = emptyList(),
    /** Other sources that told the same story. */
    val alsoIn: List<String> = emptyList(),
    /** Serious escalation (war declared, invasion, bank run…). */
    val alert: Boolean = false,
) {
    /** Only http(s) links are opened (the server already filters, checked again here). */
    val safeUrl: String? get() = link.toHttpUrlOrNull()?.toString()

    fun age(now: Long = System.currentTimeMillis()): String = ago(time, now)

    companion object {
        /** "il y a 5 min", "il y a 3 h", "il y a 2 j". */
        fun ago(time: Double, now: Long = System.currentTimeMillis()): String {
            val m = maxOf(0L, ((now - time) / 60_000).toLong())
            if (m < 1) return "à l'instant"
            if (m < 60) return "il y a $m min"
            val h = Math.round(m / 60.0)
            return if (h < 24) "il y a $h h" else "il y a ${Math.round(h / 24.0)} j"
        }


        val themeLabels = mapOf(
            "geopolitics" to "Géopolitique / guerre", "monetary" to "Banques centrales / taux", "trade" to "Commerce / droits de douane",
            "stress" to "Crise / krach", "regulation" to "Régulation", "earnings" to "Résultats d'entreprises",
        )
    }
}

@Serializable
data class NewsReport(
    val asOf: Double,
    val items: List<NewsItem>,
    val top: List<String> = emptyList(),
    val digest: Digest,
    val sources: List<Source> = emptyList(),
) {
    @Serializable data class Digest(val total: Int, val themes: List<Theme> = emptyList(), val tone: Tone)
    @Serializable data class Theme(val theme: String, val label: String, val count: Int)
    @Serializable data class Tone(val negative: Int, val positive: Int, val neutral: Int)
    @Serializable data class Source(val name: String, val ok: Boolean, val count: Int, val error: String? = null)

    val topItems: List<NewsItem> get() = top.mapNotNull { id -> items.firstOrNull { it.id == id } }
}
