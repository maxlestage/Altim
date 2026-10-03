package com.maxlestage.altim.kit

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs

/** Currency of an amount: shown (Réglages → Devise d'affichage) or saved with a value typed by the user. */
@Serializable
enum class Currency {
    @SerialName("EUR") EUR,
    @SerialName("USD") USD;

    val symbol: String get() = if (this == EUR) "€" else "$"

    companion object {
        /** Saved tag of an amount: anything but "EUR" (absent in older data) is dollars. */
        fun of(raw: String?): Currency = if (raw == "EUR") EUR else USD
        fun stored(c: Currency?): Currency = c ?: USD
    }
}

/** `GET /api/fx` with a rate (euros for 1 dollar). */
data class FxRate(
    val rate: Double,
    val usdPerEur: Double,
    /** Time of the quote (ms). */
    val time: Long,
    /** "Yahoo Finance", "BCE" or "Frankfurter (BCE)". */
    val source: String,
    val fetchedAt: Long,
    val stale: Boolean,
)

/** `/api/fx` body (rate null with `error` when no source answered). */
@Serializable
data class FxResponse(
    val base: String = "USD",
    val quote: String = "EUR",
    val rate: Double? = null,
    val usdPerEur: Double? = null,
    val time: Double? = null,
    val source: String? = null,
    val fetchedAt: Double? = null,
    val stale: Boolean = false,
    val error: String? = null,
)

/** Rate the server wrote its French texts with (additive `fx` field of decision, alerts, brief, zones, anomalies). */
@Serializable
data class FxInfo(
    val currency: String? = null,
    val rate: Double? = null,
    val usdPerEur: Double? = null,
    val time: Double? = null,
    val source: String? = null,
)

/**
 * EUR/USD rate of the app (/api/fx: Yahoo Finance, else ECB, else Frankfurter), same rules as web/src/webapp/fx.ts:
 * refreshed every 10 minutes, the last valid rate kept on the phone for 7 days (shown with its source and time), never a
 * made-up default.
 */
object Fx {
    const val KEY = "altim.fx.v1"
    const val REFRESH_MS = 10 * 60_000L
    const val KEEP_MS = 7 * 86_400_000L

    /** A valid rate from the response, else null (never a default). */
    fun parse(x: FxResponse?, now: Long = System.currentTimeMillis()): FxRate? {
        val rate = x?.rate ?: return null
        if (!rate.isFinite() || rate <= 0 || rate > 5) return null
        val time = x.time ?: return null
        val source = x.source ?: return null
        return FxRate(rate, x.usdPerEur ?: (1 / rate), time.toLong(), source, x.fetchedAt?.toLong() ?: now, x.stale)
    }

    fun parse(raw: String?, now: Long = System.currentTimeMillis()): FxRate? =
        raw?.let { runCatching { AltimJson.decodeFromString(FxResponse.serializer(), it) }.getOrNull() }?.let { parse(it, now) }

    /** The saved rate when younger than 7 days (read by Altim, not quoted: a weekend's Friday close stays valid). */
    fun saved(raw: String?, now: Long = System.currentTimeMillis()): FxRate? {
        val r = parse(raw, now) ?: return null
        return if (now - r.fetchedAt < KEEP_MS) r.copy(stale = true) else null
    }

    fun encode(r: FxResponse): String = AltimJson.encodeToString(FxResponse.serializer(), r)
}

/**
 * Display currency of every amount: engines and the server compute in dollars (the quote currency of the sources), the
 * display converts to euros with the EUR/USD rate of /api/fx. Same as web/src/money.ts: process-wide state set by the
 * app ([set]); without a rate the amounts stay in dollars with "$" (a rate is never made up).
 */
object Money {
    /** No-break space, the default separator before the symbol (same as the web formatters). */
    const val NBSP = " "
    const val NNBSP = " "

    @Volatile private var wanted: Currency = Currency.USD
    @Volatile private var fx: FxRate? = null

    /** Sets what the formatters use: the currency chosen in Réglages and the last known rate. */
    fun set(want: Currency, rate: FxRate?) {
        wanted = want
        fx = rate?.takeIf { it.rate.isFinite() && it.rate > 0 }
    }

    val wantedCurrency: Currency get() = wanted
    fun currentFx(): FxRate? = fx

    /** Currency actually shown: euros only when chosen and a rate is known. */
    fun displayCurrency(): Currency = if (wanted == Currency.EUR && fx != null) Currency.EUR else Currency.USD

    fun symbol(c: Currency = displayCurrency()): String = c.symbol

    /** [v] from one currency to another at the current rate; NaN when no rate allows it. */
    fun convert(v: Double, from: Currency, to: Currency): Double {
        if (from == to) return v
        val r = fx ?: return Double.NaN
        return if (from == Currency.USD) v * r.rate else v / r.rate
    }

    /** Dollars → display currency. */
    fun toDisplay(usd: Double): Double = convert(usd, Currency.USD, displayCurrency())

    /** Display currency → dollars (what the user typed, for the engines and the server). */
    fun fromDisplay(v: Double): Double = convert(v, displayCurrency(), Currency.USD)

    /** Numbers written like the browser (iOS `Money.fr` = `JSFormat.fr`). */
    private fun fr(v: Double, min: Int, max: Int): String = JsFormat.fr(v, min, max)

    /** A dollar amount in the display currency with [min]–[max] decimals: "212,40 €" (or "$" without a rate). */
    fun money(usd: Double, min: Int = 2, max: Int = min, sep: String = NBSP): String = "${fr(toDisplay(usd), min, max)}$sep${symbol()}"

    /** A dollar amount in the display currency with the caller's number format (applied to the converted value). */
    fun moneyFmt(usd: Double, sep: String = NBSP, fmt: (Double) -> String): String = "${fmt(toDisplay(usd))}$sep${symbol()}"

    /** A price with the digits of a price (2 from 1, 4 from 0.01, 8 below), chosen on the converted value. */
    fun price(usd: Double, sep: String = NBSP): String {
        val v = toDisplay(usd)
        val digits = if (v >= 1) 2 else if (v >= 0.01) 4 else 8
        return "${fr(v, digits, digits)}$sep${symbol()}"
    }

    /** Large amounts: "421 Md€", "3,16 Md€", "850 M€", "12,5 k€". */
    fun compact(usd: Double, sep: String = NBSP): String {
        val v = toDisplay(usd)
        val a = abs(v)
        val sym = symbol()
        val (div, unit) = when {
            a >= 1e9 -> 1e9 to "Md$sym"
            a >= 1e6 -> 1e6 to "M$sym"
            a >= 1e4 -> 1e3 to "k$sym"
            else -> 1.0 to sym
        }
        val x = v / div
        val digits = if (abs(x) >= 100) 0 else if (abs(x) >= 10) 1 else 2
        return "${fr(x, 0, digits)}$sep$unit"
    }

    /** A threshold shown in its own currency ("80 000,00 €"), like thresholdText of the web notifications. */
    fun threshold(v: Double, c: Currency): String =
        "${fr(v, if (v >= 1) 2 else 4, if (v >= 1) 2 else 8)} ${c.symbol}"

    /** An amount typed back into a field, in the display currency ("" when no rate allows it). */
    fun inputText(v: Double): String = if (v.isFinite()) fr(Math.round(v * 1e8) / 1e8, 0, 8).replace(" ", "") else ""

    /** An amount typed by the user and the currency it was typed in (Sélection budget). */
    @Serializable
    data class Typed(val amount: Double, val currency: Currency)

    /** Saved budget: `{"amount":…,"currency":…}`, or a bare number (older versions: dollars); null when unusable. */
    fun parseBudget(raw: String?): Typed? {
        if (raw == null) return null
        val e = runCatching { AltimJson.parseToJsonElement(raw) }.getOrNull() ?: return null
        (e as? kotlinx.serialization.json.JsonPrimitive)?.let { p ->
            if (p.isString) return null
            val v = p.content.toDoubleOrNull() ?: return null
            return if (v.isFinite() && v > 0) Typed(v, Currency.USD) else null
        }
        val o = e as? kotlinx.serialization.json.JsonObject ?: return null
        val a = (o["amount"] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { !it.isString }?.content?.toDoubleOrNull() ?: return null
        if (!a.isFinite() || a <= 0) return null
        return Typed(a, Currency.of((o["currency"] as? kotlinx.serialization.json.JsonPrimitive)?.content))
    }

    fun encodeBudget(t: Typed): String = AltimJson.encodeToString(Typed.serializer(), t)

    private val PARIS: ZoneId = ZoneId.of("Europe/Paris")

    /** "1 $ = 0,8819 € · Yahoo Finance, 17:15" (or the date when not today); null without a rate. */
    fun fxLine(r: FxRate? = fx, now: Long = System.currentTimeMillis()): String? {
        if (r == null) return null
        val t = Instant.ofEpochMilli(r.time).atZone(PARIS)
        val sameDay = Instant.ofEpochMilli(now).atZone(PARIS).toLocalDate() == t.toLocalDate()
        val hm = DateTimeFormatter.ofPattern("HH:mm", Locale.FRANCE).format(t)
        val whenText = if (sameDay) hm else DateTimeFormatter.ofPattern("d MMM", Locale.FRANCE).format(t) + " " + hm
        return "1 $ = ${fr(r.rate, 3, 4)} € · ${r.source}, $whenText"
    }

    /** The line under amounts (FxNote of the web): the rate and its source, or why the amounts stay in dollars. Null in dollars. */
    fun note(now: Long = System.currentTimeMillis()): String? {
        if (wanted == Currency.USD) return null
        val r = fx ?: return "Taux EUR/USD indisponible : montants affichés en $."
        return fxLine(r, now) + if (r.stale) " (dernier taux connu)" else ""
    }
}
