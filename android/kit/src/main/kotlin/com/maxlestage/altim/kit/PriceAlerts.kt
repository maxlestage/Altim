package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.util.UUID

/**
 * "Préviens-moi si BTC passe sous 80 000 $": a price threshold chosen by the user, checked with the buy alerts.
 * One shot: once reached it is marked triggered (the user can re-arm it).
 */
@Serializable
data class PriceTarget(
    val id: String = UUID.randomUUID().toString(),
    val asset: Asset,
    /** true: notify when the price rises to or above [price]; false: when it falls to or below. */
    val above: Boolean,
    val price: Double,
    val created: Long = System.currentTimeMillis(),
    val triggered: Long? = null,
    /** Move alert: notify when the price moves by at least this many % (up or down) from [price], the reference. */
    val move: Double? = null,
) {
    val label: String
        get() = move?.let { "Variation de ±${Format.plain(it, 1).removeSuffix(",0")} % (depuis ${Format.price(price)})" }
            ?: "${if (above) "Au-dessus de" else "En dessous de"} ${Format.price(price)}"

    fun isReached(current: Double) = when {
        move != null -> price > 0 && kotlin.math.abs(current / price - 1) * 100 >= move
        above -> current >= price
        else -> current <= price
    }

    /** Re-armed: a move alert starts again from the current price. */
    fun rearmed(current: Double?): PriceTarget = copy(triggered = null, price = if (move != null && current != null && current > 0) current else price)

    companion object {
        /** Armed targets reached at the given prices (key "kind:SYMBOL"): the updated list and those just reached. */
        fun evaluate(targets: List<PriceTarget>, prices: Map<String, Double>, now: Long = System.currentTimeMillis()): Pair<List<PriceTarget>, List<Pair<PriceTarget, Double>>> {
            val fired = mutableListOf<Pair<PriceTarget, Double>>()
            val updated = targets.map { t ->
                val p = prices[t.asset.id]
                if (t.triggered != null || p == null || !p.isFinite() || !t.isReached(p)) t
                else t.copy(triggered = now).also { fired += it to p }
            }
            return updated to fired
        }
    }
}

/** One notification received, kept to measure what it gave afterwards (honest follow-up of the alerts). */
@Serializable
data class JournalEntry(
    val id: String = UUID.randomUUID().toString(),
    val asset: Asset,
    val source: Source,
    val title: String,
    /** Price when the alert was sent. */
    val price: Double,
    val date: Long = System.currentTimeMillis(),
) {
    @Serializable
    enum class Source { BUY, STRONG_BUY, TARGET }

    /** Change since the alert, in %. */
    fun change(current: Double?): Double? = if (current == null || !current.isFinite() || price <= 0) null else (current / price - 1) * 100
}

object AlertJournal {
    /** Most recent first, 200 entries at most. */
    const val LIMIT = 200

    fun add(entries: List<JournalEntry>, journal: List<JournalEntry>): List<JournalEntry> =
        (entries + journal).sortedByDescending { it.date }.take(LIMIT)

    data class Summary(val count: Int, val up: Int, val average: Double) {
        val upShare: Double get() = if (count > 0) up.toDouble() / count * 100 else 0.0
    }

    /**
     * Summary of the buy alerts (price targets are the user's own thresholds, not Altim's advice): how many went up
     * since, average change. Without fees nor exit rule: an indication, not a backtest. Alerts younger than an hour
     * are left out (they say nothing yet).
     */
    fun summary(journal: List<JournalEntry>, prices: Map<String, Double>, now: Long = System.currentTimeMillis(), minAgeMs: Long = 3_600_000): Summary? {
        val changes = journal
            .filter { it.source != JournalEntry.Source.TARGET && now - it.date >= minAgeMs }
            .mapNotNull { it.change(prices[it.asset.id]) }
        if (changes.isEmpty()) return null
        return Summary(changes.size, changes.count { it > 0 }, changes.average())
    }
}
