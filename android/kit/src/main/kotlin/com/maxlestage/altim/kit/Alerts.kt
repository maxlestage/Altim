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
) {
    val id: String get() = "${kind.raw}:$symbol"
    val asset: Asset get() = Asset(symbol, kind, name)
}

/**
 * Decides which alerts deserve a notification: only when an asset becomes buyable, or when the reason changes
 * (a zone is reached after a signal…). An asset that stops being buyable is forgotten, so its next buy opportunity
 * notifies again. Persisted between background checks.
 */
@Serializable
data class AlertTracker(val notified: Map<String, String> = emptyMap()) {
    fun newAlerts(items: List<BuyAlert>, onlyStrong: Boolean): Pair<AlertTracker, List<BuyAlert>> {
        val next = notified.toMutableMap()
        val out = mutableListOf<BuyAlert>()
        for (it in items) {
            val wanted = it.buy && (!onlyStrong || it.strong)
            if (wanted) {
                if (next[it.id] != it.key) {
                    out += it
                    next[it.id] = it.key
                }
            } else if (!it.buy) {
                next.remove(it.id)
            }
        }
        return AlertTracker(next) to out
    }
}
