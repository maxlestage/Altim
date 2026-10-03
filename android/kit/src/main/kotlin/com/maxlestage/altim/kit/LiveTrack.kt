package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

/**
 * Live following of one asset outside the app (iOS: Live Activity on the lock screen and in the Dynamic Island;
 * Android: an ongoing notification). Same content and update rules as iOS LiveActivities: the price follows the live
 * stream while the app runs (at most every 5 s), and each alert check (also in the background) refreshes the price and
 * Altim's buy verdict. Pure; the app draws the notification.
 */
@Serializable
data class LiveTrackState(
    val symbol: String,
    val kind: Kind,
    val name: String,
    /** Dollars (the sources' currency); shown in the display currency. */
    val price: Double,
    /** Change over 24 h, in %. */
    val change: Double? = null,
    /** "ACHAT", "ATTENDRE"… (4 h signal), null when unknown. */
    val signal: String? = null,
    /** Buyable according to Altim's rule (the asset's full decision says ACHETER or ZONE D'ACHAT). */
    val buy: Boolean = false,
    /** One line: why (or why not) it can be bought. */
    val note: String,
    /** ms. */
    val updated: Long,
) {
    val asset: Asset get() = Asset(symbol, kind, name)
    val id: String get() = "${kind.raw}:$symbol"

    /** The chip: "ACHAT POSSIBLE", else the signal, else "PAS D'ACHAT". */
    val verdict: String get() = if (buy) "ACHAT POSSIBLE" else signal ?: "PAS D'ACHAT"
}

object LiveTrack {
    const val KEY = "liveTrack.v1"
    /** Live price updates at most this often (iOS limits a Live Activity's update rate). */
    const val MIN_UPDATE_MS = 5_000L
    const val NO_BUY = "Ni signal d'achat ni prix dans une zone d'achat."
    const val PENDING = "Verdict d'achat à la prochaine vérification."

    /** A new state from a price and, when known, the last alert check of this asset. */
    fun state(asset: Asset, price: Double, change: Double?, alert: BuyAlert?, signal: String?, now: Long): LiveTrackState {
        val note = when {
            alert == null -> PENDING
            alert.buy -> alert.reasons.firstOrNull() ?: alert.body
            else -> alert.blockers.firstOrNull() ?: NO_BUY
        }
        return LiveTrackState(asset.symbol, asset.kind, asset.name, price, change, signal, alert?.buy ?: false, note, now)
    }

    /** Live price tick of the followed asset: the new state, or null when it is another asset or too soon. */
    fun tick(s: LiveTrackState, tick: LiveTick, lastPush: Long, now: Long): LiveTrackState? {
        if (tick.symbol != s.symbol || tick.kind != s.kind || now - lastPush < MIN_UPDATE_MS) return null
        return s.copy(price = tick.price, change = tick.change ?: s.change, updated = now)
    }

    /** Result of an alert check: verdict and price of the followed asset (null when it is not in the check). */
    fun refresh(s: LiveTrackState, alerts: List<BuyAlert>, now: Long): LiveTrackState? {
        val alert = alerts.firstOrNull { it.symbol == s.symbol && it.kind == s.kind } ?: return null
        return state(s.asset, alert.price ?: s.price, s.change, alert, s.signal, now)
    }

    fun parse(raw: String?): LiveTrackState? = raw?.let { runCatching { AltimJson.decodeFromString(LiveTrackState.serializer(), it) }.getOrNull() }

    fun encode(s: LiveTrackState): String = AltimJson.encodeToString(LiveTrackState.serializer(), s)
}
