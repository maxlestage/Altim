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

    /** Exactly [digits] decimals: 12,30 (French, narrow no-break space between thousands). */
    fun fixed(v: Double, digits: Int): String = number(v, digits, digits)

    /** Quantity of units: up to 8 decimals for cryptos, no trailing zeros. */
    fun quantity(v: Double): String = number(v, 0, 8)

    fun date(ms: Double, time: Boolean = false): String {
        val pattern = if (time) "d MMM yyyy 'à' HH:mm" else "d MMMM yyyy"
        return DateTimeFormatter.ofPattern(pattern, FR).withZone(PARIS).format(Instant.ofEpochMilli(ms.toLong()))
    }

    /** "28/09 à 14:02" (Paris time). */
    fun shortDateTime(ms: Double): String = DateTimeFormatter.ofPattern("dd/MM 'à' HH:mm", FR).withZone(PARIS).format(Instant.ofEpochMilli(ms.toLong()))

    /** Filing and price dates: New York calendar day (a timestamp at 00:00 UTC is a bare date, read as such). */
    fun nyDate(ms: Double): String {
        val zone = if (ms % 86_400_000.0 == 0.0) ZoneId.of("UTC") else ZoneId.of("America/New_York")
        return DateTimeFormatter.ofPattern("d MMMM yyyy", FR).withZone(zone).format(Instant.ofEpochMilli(ms.toLong()))
    }

    /** Large amounts: 421 Md$, 3,16 Md$, 850 M$, 12,5 k$ (same as the web card). */
    fun compactUsd(v: Double?): String {
        if (v == null || !v.isFinite()) return "—"
        val a = abs(v)
        val (div, unit) = when {
            a >= 1e9 -> 1e9 to "Md$"
            a >= 1e6 -> 1e6 to "M$"
            a >= 1e4 -> 1e3 to "k$"
            else -> 1.0 to "$"
        }
        val x = v / div
        return "${number(x, 0, if (abs(x) >= 100) 0 else if (abs(x) >= 10) 1 else 2)} $unit"
    }

    /** Large counts: 19,93 millions; below a million, whole. */
    fun count(v: Double?): String {
        if (v == null || !v.isFinite()) return "—"
        if (v >= 1e9) return "${number(v / 1e9, 0, 2)} milliards"
        if (v >= 1e6) return "${number(v / 1e6, 0, 2)} millions"
        return number(v, 0, 0)
    }

    /** "10 000", "0,25", "1 234,5" → number (spaces, narrow spaces and French comma accepted). */
    fun parse(text: String): Double? =
        text.replace(" ", "").replace(" ", "").replace(" ", "").replace(",", ".").toDoubleOrNull()?.takeIf { it.isFinite() }
}
