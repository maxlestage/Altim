package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.put

/** Radar verdict of an asset pushed by the live socket when it changes (`{"type":"verdict",…}`). */
@Serializable
data class VerdictPush(
    val symbol: String,
    val kind: Kind,
    /** "buy" | "buyZone" | "wait" | "noPosition" | "trim" | "sell" (the decision's verdict). */
    val verdict: String,
    val label: String? = null,
    val rating: String? = null,
    val ratingLabel: String? = null,
    val chipNote: String? = null,
    val asOf: Double? = null,
) {
    val key: String get() = "${kind.raw}:$symbol"
}

/** One message of the real-time WebSocket `/api/ws` (protocol v1, README « Temps réel (WebSocket) »), as iOS reads it. */
sealed interface LiveMessage {
    data class Hello(val version: Int) : LiveMessage
    data class Tick(val tick: LiveTick) : LiveMessage
    /** The `/api/alerts` items of the subscription (after `subscribe`, then only when one changed). */
    data class Alerts(val items: List<BuyAlert>, val checkedAt: Double) : LiveMessage
    /** The alerts were checked again and did not change. */
    data class Checked(val checkedAt: Double) : LiveMessage
    data class Verdict(val verdict: VerdictPush) : LiveMessage
    data class Fx(val rate: Double, val asOf: Double?) : LiveMessage
    data object Pong : LiveMessage
    data class Error(val code: String, val message: String) : LiveMessage
    /** A type this version does not know (a newer server): ignored. */
    data object Other : LiveMessage
}

object LiveSocket {
    const val PROTOCOL = 1
    /** Application ping from the client: the server answers, so a silent socket is a dead one. */
    const val PING_MS = 20_000L
    /** Without any message for this long the socket is closed and reopened (and the badge stops saying « en direct »). */
    const val SILENCE_MS = 45_000L
    /** Openings that fail in a row before falling back to the SSE stream (`/api/live`). */
    const val FALLBACK_AFTER = 2
    /** Same cap as the server. */
    const val MAX_ASSETS = 20

    /** Decodes one text frame; null when it is not a JSON object with a `type`. */
    fun decode(text: String): LiveMessage? {
        val obj = runCatching { AltimJson.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return null
        val type = (obj["type"] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return null
        fun num(k: String): Double? = (obj[k] as? JsonPrimitive)?.doubleOrNull
        fun str(k: String): String = (obj[k] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: ""
        return runCatching {
            when (type) {
                "hello" -> LiveMessage.Hello(num("v")?.toInt() ?: 0)
                "tick" -> LiveMessage.Tick(AltimJson.decodeFromJsonElement(LiveTick.serializer(), obj))
                "alerts" -> LiveMessage.Alerts(
                    AltimJson.decodeFromJsonElement(ListSerializer(BuyAlert.serializer()), obj["items"] as? JsonArray ?: return null),
                    num("checkedAt") ?: 0.0,
                )
                "checked" -> LiveMessage.Checked(num("checkedAt") ?: return null)
                "verdict" -> LiveMessage.Verdict(AltimJson.decodeFromJsonElement(VerdictPush.serializer(), obj))
                "fx" -> {
                    val rate = num("rate")?.takeIf { it.isFinite() && it > 0 } ?: return null
                    LiveMessage.Fx(rate, num("asOf"))
                }
                "pong" -> LiveMessage.Pong
                "error" -> LiveMessage.Error(str("code"), str("message"))
                else -> LiveMessage.Other
            }
        }.getOrNull()
    }

    /** The `subscribe` message (replaces the previous subscription), the first [MAX_ASSETS] assets. */
    fun subscribe(assets: List<Asset>, interval: String = "4h", usd: Boolean): String = buildJsonObject {
        put("type", "subscribe")
        put("assets", buildJsonArray { assets.take(MAX_ASSETS).forEach { a -> add(buildJsonObject { put("kind", a.kind.raw); put("symbol", a.symbol) }) } })
        put("interval", interval)
        put("currency", if (usd) "USD" else "EUR")
    }.toString()

    /** Pause before reconnection [attempt] (0-based): 1 s, 2 s, 4 s, 8 s, 16 s, then 30 s. */
    fun backoffMs(attempt: Int): Long = minOf(30_000L, 1_000L shl minOf(attempt, 5))

    /** « En direct » only while messages arrive. */
    fun isLive(lastMessage: Long?, now: Long): Boolean = lastMessage != null && now - lastMessage < SILENCE_MS
}
