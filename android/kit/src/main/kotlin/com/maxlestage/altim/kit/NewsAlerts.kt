package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

/**
 * News worth a notification: a serious escalation (war declared, invasion, bank run…) told by at least 2 sources (an
 * opinion piece that "could" trigger a bank run is told by one), or a story about one of the user's assets told by at
 * least 3 sources. Only stories of the last 6 hours; a story already notified (same
 * article, or a title with the same words told by another source) never notifies again for 48 hours.
 * Persisted between background checks. Same rule on the iPhone.
 */
@Serializable
data class NewsAlertTracker(val seen: List<Seen> = emptyList()) {
    @Serializable data class Seen(val id: String, val words: List<String>, val time: Long)

    companion object {
        const val FRESH_MS = 6 * 3_600_000L
        const val KEEP_MS = 48 * 3_600_000L
        /** Sources telling a story about one of the user's assets before it is notified (the first one + 2). */
        const val MIN_OTHER_SOURCES = 2

        fun words(title: String): Set<String> =
            title.lowercase().split(Regex("[^\\p{L}\\p{N}]+")).filter { it.length >= 4 }.toSet()

        private fun similar(a: Set<String>, b: Set<String>): Boolean {
            if (a.isEmpty() || b.isEmpty()) return false
            return a.intersect(b).size.toDouble() / a.union(b).size >= 0.5
        }

        /** Why this story is worth a notification, or null. */
        fun reason(item: NewsItem, owned: Set<String>): String? = when {
            item.alert && item.alsoIn.isNotEmpty() -> "alert"
            item.alsoIn.size >= MIN_OTHER_SOURCES && item.assets.any { it in owned } -> "asset"
            else -> null
        }
    }

    fun newAlerts(items: List<NewsItem>, owned: Set<String>, now: Long = System.currentTimeMillis()): Pair<NewsAlertTracker, List<NewsItem>> {
        val kept = seen.filter { now - it.time < KEEP_MS }.toMutableList()
        val out = mutableListOf<NewsItem>()
        for (item in items) {
            if (now - item.time > FRESH_MS || reason(item, owned) == null) continue
            val w = words(item.title)
            if (kept.any { it.id == item.id || similar(it.words.toSet(), w) }) continue
            kept += Seen(item.id, w.sorted(), now)
            out += item
        }
        return NewsAlertTracker(kept) to out
    }
}
