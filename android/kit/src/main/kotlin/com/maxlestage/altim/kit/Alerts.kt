package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

/** "Can I buy now?" for one asset (/api/alerts): the same rule for the iPhone, the Apple Watch and Android. */
@Serializable
data class BuyAlert(
    val symbol: String,
    val kind: Kind,
    val name: String = symbol,
    val price: Double? = null,
    val asOf: Double? = null,
    // An asset whose data is unavailable comes back with only an error: never buyable.
    val buy: Boolean = false,
    val strong: Boolean = false,
    val reasons: List<String> = emptyList(),
    val blockers: List<String> = emptyList(),
    val cautions: List<String> = emptyList(),
    /** Stable identifier of the situation ("signal+zone:medium"): a new notification only when it changes. */
    val key: String = "",
    val title: String = symbol,
    val body: String = "Données indisponibles.",
    val error: String? = null,
    /** Rate the server wrote [title] and [body] with (additive). */
    val fx: FxInfo? = null,
) {
    val id: String get() = "${kind.raw}:$symbol"
    val asset: Asset get() = Asset(symbol, kind, name)
}

/**
 * Decides which alerts deserve a notification: when an asset becomes buyable, or when a new reason appears (a zone is
 * reached after a signal…). A price that hovers at the edge of a zone makes the verdict flicker: an asset is forgotten
 * only after [COOLDOWN_MS] without being buyable, and a reason already notified never notifies again meanwhile.
 * Persisted between background checks.
 */
@Serializable
data class AlertTracker(
    /** Asset → every reason already notified ("signal+zone:medium"). */
    val notified: Map<String, String> = emptyMap(),
    /** Asset → since when it is no longer buyable. */
    val lost: Map<String, Long> = emptyMap(),
) {
    companion object {
        const val COOLDOWN_MS = 6 * 3_600_000L
        private fun parts(key: String) = key.split("+").filter { it.isNotEmpty() }.toSet()
    }

    fun newAlerts(items: List<BuyAlert>, onlyStrong: Boolean, now: Long = System.currentTimeMillis()): Pair<AlertTracker, List<BuyAlert>> {
        val next = notified.toMutableMap()
        val gone = lost.toMutableMap()
        val out = mutableListOf<BuyAlert>()
        for (it in items) {
            val wanted = it.buy && (!onlyStrong || it.strong)
            if (wanted) {
                gone.remove(it.id)
                val before = next[it.id]?.let(::parts)
                val now2 = parts(it.key)
                if (before == null || !before.containsAll(now2)) out += it
                next[it.id] = ((before ?: emptySet()) + now2).sorted().joinToString("+")
            } else if (!it.buy && next.containsKey(it.id)) {
                val since = gone.getOrPut(it.id) { now }
                if (now - since >= COOLDOWN_MS) {
                    next.remove(it.id)
                    gone.remove(it.id)
                }
            }
        }
        return AlertTracker(next, gone) to out
    }
}
