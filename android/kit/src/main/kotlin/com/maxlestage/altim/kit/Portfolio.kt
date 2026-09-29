package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.util.UUID

/** A line of the portfolio, stored on the phone only (never sent to the server). */
@Serializable
data class Holding(
    val id: String = UUID.randomUUID().toString(),
    val asset: Asset,
    val quantity: Double,
    /** Average purchase price in [costCurrency] (optional: without it, no gain / loss). */
    val averagePrice: Double? = null,
    /** Stop set by the user in [stopCurrency] (optional): "position devenue dangereuse" alerts in Mes avoirs. */
    val stop: Double? = null,
    /** Currency the cost was typed in; absent = dollars (the only currency before the euro display). */
    val costCurrency: Currency? = null,
    /** Currency the stop was typed in; absent = dollars. */
    val stopCurrency: Currency? = null,
) {
    /** The optional stop is dropped when it is not a positive number (the line itself stays), like the web app. */
    fun cleaned(): Holding = if (stop == null || (stop.isFinite() && stop > 0)) this else copy(stop = null, stopCurrency = null)
}

/** Holdings in dollars for the engines and the server, plus the lines whose euro amounts could not be converted. */
data class UsdHoldings(
    val holdings: List<Holding>,
    /** Symbols whose euro cost could not be converted (no rate): their cost is left out and their P&L is not shown. */
    val unconverted: List<String>,
)

/** Saved amounts (in the currency they were typed in) and their dollar view, same rules as web/src/webapp/store.ts. */
object Holdings {
    /**
     * Saved amounts in dollars at the current rate: a euro cost basis becomes `cost ÷ rate`, so that the dollar P&L,
     * converted back at the same rate, is exactly the euro P&L (price in euros today − cost in euros, currency effect
     * included). Without a rate, a euro cost is dropped (no P&L) and a euro stop too: never read as dollars.
     */
    fun toUsd(list: List<Holding>): UsdHoldings {
        val unconverted = mutableListOf<String>()
        val out = list.map { h ->
            val cost = h.averagePrice?.let { Money.convert(it, Currency.stored(h.costCurrency), Currency.USD) }
            if (cost != null && !cost.isFinite()) unconverted += h.asset.symbol
            val stop = h.stop?.let { Money.convert(it, Currency.stored(h.stopCurrency), Currency.USD) }?.takeIf { it.isFinite() && it > 0 }
            h.copy(averagePrice = cost?.takeIf { it.isFinite() }, stop = stop, costCurrency = null, stopCurrency = null)
        }
        return UsdHoldings(out, unconverted)
    }

    /** A saved amount shown in the display currency (NaN when no rate allows it). */
    fun shown(v: Double, c: Currency?): Double = Money.convert(v, Currency.stored(c), Money.displayCurrency())

    /** « Prix de revient saisi en $, converti au taux du jour. » when the cost was typed in the other currency. */
    fun costNote(h: Holding?): String? {
        val avg = h?.averagePrice ?: return null
        val c = Currency.stored(h.costCurrency)
        if (c == Money.displayCurrency() || !shown(avg, h.costCurrency).isFinite()) return null
        return "Prix de revient saisi en ${c.symbol}, converti au taux du jour."
    }

    /**
     * A second purchase merged into a line: weighted average cost in the currency of the new purchase (the previous cost
     * converted at the current rate when it was typed in another currency). Null when no rate allows the conversion.
     */
    fun mergeLine(x: Holding, quantity: Double, averagePrice: Double?, costCurrency: Currency?): Holding? {
        val cur = Currency.stored(costCurrency)
        val qty = x.quantity + quantity
        if (x.averagePrice == null || averagePrice == null) return x.copy(quantity = qty, averagePrice = null, costCurrency = null)
        val prev = Money.convert(x.averagePrice, Currency.stored(x.costCurrency), cur)
        if (!prev.isFinite()) return null
        return x.copy(quantity = qty, averagePrice = (x.quantity * prev + quantity * averagePrice) / qty, costCurrency = cur)
    }
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
