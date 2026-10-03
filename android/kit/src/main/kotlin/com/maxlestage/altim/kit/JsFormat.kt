package com.maxlestage.altim.kit

import java.math.BigDecimal
import java.math.RoundingMode

/**
 * Numbers written like `toLocaleString("fr-FR")` in the browser (iOS AltimKit `JSFormat`): narrow no-break space
 * between thousands, comma, halves away from zero, no trailing zeros beyond [min]. Intl rounds the shortest decimal
 * writing of the double (9.01 − 95.56 = −86.55 → "86,6", although the exact binary value is 86.5499…), so the same is
 * done here, by hand on the decimal digits rather than with DecimalFormat (which rounds the binary value).
 */
object JsFormat {
    private const val NNBSP = ' '

    /** At most [max] decimals. */
    fun fr(v: Double, max: Int): String = fr(v, 0, max)

    /** Between [min] and [max] decimals. */
    fun fr(v: Double, min: Int, max: Int): String {
        if (!v.isFinite()) return v.toString()
        val plain = shortestDecimal(kotlin.math.abs(v))
        val rounded = BigDecimal(plain).setScale(max, RoundingMode.HALF_UP).toPlainString()
        val parts = rounded.split('.')
        val intPart = parts[0]
        var frac = if (parts.size > 1) parts[1] else ""
        while (frac.length > min && frac.endsWith('0')) frac = frac.dropLast(1)
        val grouped = StringBuilder()
        intPart.forEachIndexed { k, c ->
            if (k > 0 && (intPart.length - k) % 3 == 0) grouped.append(NNBSP)
            grouped.append(c)
        }
        val body = if (frac.isEmpty()) grouped.toString() else "$grouped,$frac"
        // A tiny negative number rounded to zero is written without its sign, as the browser does.
        val zero = intPart.all { it == '0' } && frac.all { it == '0' }
        return if (v < 0 && !zero) "-$body" else body
    }

    /** Shortest decimal that reads back as the same double ("86.55", "0.00001"), without exponent. */
    fun shortestDecimal(v: Double): String {
        if (v == 0.0) return "0"
        // Java's Double.toString is the shortest round-trip writing (JDK 19+, and for every value the tests use).
        return BigDecimal(v.toString()).stripTrailingZeros().toPlainString()
    }
}
