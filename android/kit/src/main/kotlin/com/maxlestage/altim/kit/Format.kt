package com.maxlestage.altim.kit

import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs

/** French formatting, identical to the web app (web/src/market.ts) and the iPhone app: prices in dollars, signed percentages. */
object Format {
    private val FR = Locale.FRANCE
    private val PARIS = ZoneId.of("Europe/Paris")

    private fun number(v: Double, min: Int, max: Int): String {
        val symbols = DecimalFormatSymbols(FR).apply {
            // Narrow no-break space as the thousands separator, like Intl in the browser.
            groupingSeparator = ' '
            decimalSeparator = ','
        }
        val f = DecimalFormat("#,##0", symbols).apply {
            minimumFractionDigits = min
            maximumFractionDigits = max
            roundingMode = RoundingMode.HALF_EVEN
        }
        return f.format(v)
    }

    /** 2 decimals from 1 $, 4 from 0.01 $, 8 below (small cryptos). */
    fun price(v: Double?): String {
        if (v == null || !v.isFinite()) return "—"
        val digits = if (abs(v) >= 1) 2 else if (abs(v) >= 0.01) 4 else 8
        return "${number(v, digits, digits)} $"
    }

    /** Whole dollars from 100 $ (amounts, budgets). */
    fun money(v: Double): String = "${number(v, 0, if (abs(v) >= 100) 0 else 2)} $"

    /** +1,25 % / −0,40 % (true minus sign). */
    fun percent(v: Double?, digits: Int = 2): String {
        if (v == null || !v.isFinite()) return "—"
        return "${if (v >= 0) "+" else "−"}${number(abs(v), digits, digits)} %"
    }

    fun plain(v: Double, digits: Int = 2): String = number(v, 0, digits)

    /** Quantity of units: up to 8 decimals for cryptos, no trailing zeros. */
    fun quantity(v: Double): String = number(v, 0, 8)

    fun date(ms: Double, time: Boolean = false): String {
        val pattern = if (time) "d MMM yyyy 'à' HH:mm" else "d MMMM yyyy"
        return DateTimeFormatter.ofPattern(pattern, FR).withZone(PARIS).format(Instant.ofEpochMilli(ms.toLong()))
    }

    /** "10 000", "0,25", "1 234,5" → number (spaces, narrow spaces and French comma accepted). */
    fun parse(text: String): Double? =
        text.replace(" ", "").replace(" ", "").replace(" ", "").replace(",", ".").toDoubleOrNull()?.takeIf { it.isFinite() }
}
