package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.util.UUID

/** A line of the portfolio, stored on the phone only (never sent to the server). */
@Serializable
data class Holding(
    val id: String = UUID.randomUUID().toString(),
    val asset: Asset,
    val quantity: Double,
    /** Average purchase price in dollars (optional: without it, no gain / loss). */
    val averagePrice: Double? = null,
    /** Stop set by the user in dollars (optional): "position devenue dangereuse" alerts in Mes avoirs. */
    val stop: Double? = null,
) {
    /** The optional stop is dropped when it is not a positive number (the line itself stays), like the web app. */
    fun cleaned(): Holding = if (stop == null || (stop.isFinite() && stop > 0)) this else copy(stop = null)
}

data class PortfolioLine(
    val holding: Holding,
    val price: Double?,
    val value: Double?,
    val cost: Double?,
    /** Share of the portfolio value (0–100). */
    val weight: Double? = null,
) {
    val gain: Double? get() = if (value != null && cost != null) value - cost else null
    val gainPercent: Double? get() = gain?.let { g -> cost?.takeIf { it > 0 }?.let { g / it * 100 } }
}

class Portfolio(holdings: List<Holding>, prices: Map<String, Double>) {
    val lines: List<PortfolioLine>
    val total: Double
    val cost: Double?
    /** Share in cryptos (0–100). */
    val cryptoShare: Double
    /** Largest line (0–100): above 40 %, the portfolio depends on one asset. */
    val largest: Double
    val warnings: List<String>

    val gain: Double? get() = cost?.let { total - it }
    val gainPercent: Double? get() = cost?.takeIf { it > 0 }?.let { (total - it) / it * 100 }

    /** Values the holdings at the given prices (key "kind:SYMBOL"). Lines without a price are left out of the total. */
    init {
        val raw = holdings.map { h ->
            val p = prices[h.asset.id]
            PortfolioLine(h, p, p?.let { it * h.quantity }, h.averagePrice?.let { it * h.quantity })
        }
        total = raw.mapNotNull { it.value }.sum()
        val t = total
        lines = raw.map { l -> l.copy(weight = l.value?.let { if (t > 0) it / t * 100 else 0.0 }) }
            .sortedByDescending { it.value ?: -1.0 }
        val priced = lines.filter { it.value != null }
        cost = if (priced.isNotEmpty() && priced.all { it.cost != null }) priced.sumOf { it.cost!! } else null
        cryptoShare = if (t > 0) priced.filter { it.holding.asset.kind == Kind.CRYPTO }.sumOf { it.value!! } / t * 100 else 0.0
        largest = lines.mapNotNull { it.weight }.maxOrNull() ?: 0.0
        val w = mutableListOf<String>()
        if (priced.size >= 2 && largest > 40) {
            w += "${lines.first().holding.asset.symbol} pèse ${Format.plain(largest, 0)} % du portefeuille : une seule ligne décide de presque tout."
        }
        if (t > 0 && cryptoShare > 50) {
            w += "Plus de la moitié en cryptos (${Format.plain(cryptoShare, 0)} %) : des baisses de 50 % ou plus arrivent régulièrement."
        }
        val missing = lines.filter { it.price == null }.map { it.holding.asset.symbol }
        if (missing.isNotEmpty()) w += "Prix indisponible pour ${missing.joinToString(", ")} : ligne hors du total."
        warnings = w
    }
}
