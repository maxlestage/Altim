package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlin.math.abs

// Guidance of the decision (backend/src/engine/guidance.rs, fields added to /api/decision, absent from older answers):
// when not to trade, action zones, watched scenarios, counter-argument and the compact snapshot kept to explain a later
// change of the signal. Never changes the verdict. Same contract as web/src/webapp/decision.ts, tested on the server's
// real sample (GuidanceTest ↔ web/test/guidance.test.ts).

@Serializable
data class NoTradeReason(
    /** "volatility" | "liquidity" | "spread" | "earnings" | "announcement" | "event" | "trendless" | "weakSignal" | "degraded" | "marketClosed"… */
    val code: String = "",
    val label: String = "",
    val detail: String = "",
)

/** "Quand ne PAS trader": `headline` empty when not active; `unchecked`: checks without data. */
@Serializable
data class NoTrade(
    val active: Boolean = false,
    val headline: String = "",
    val reasons: List<NoTradeReason> = emptyList(),
    val unchecked: List<String> = emptyList(),
)

/** A band of the ladder, USD (from = to for a single level). kind: "invalidation" | "exit" | "buy" | "wait" | "profit". */
@Serializable
data class ActionZone(val kind: String = "", val label: String = "", val from: Double = 0.0, val to: Double = 0.0, val note: String = "")

/** `zones` ascending by price; `here`: kind of the zone holding the price (null between zones or outside). */
@Serializable
data class ActionZones(val price: Double, val zones: List<ActionZone> = emptyList(), val here: String? = null, val hereText: String = "")

/** "met" | "unmet" | "unknown". */
@Serializable
data class ScenarioCheck(val text: String = "", val state: String = "unknown", val detail: String = "")

@Serializable
data class Unfolding(val kind: ScenarioKind = ScenarioKind.NEUTRAL, val title: String = "", val met: Int = 0, val total: Int = 0, val text: String = "")

/** kind "level": value is a price (USD); "volume": a daily volume in units of the asset; else value null. */
@Serializable
data class Invalidator(val kind: String = "condition", val text: String = "", val value: Double? = null)

@Serializable
data class CounterArgument(
    val favourable: Int = 0,
    val unfavourable: Int = 0,
    val text: String = "",
    val familiesText: String = "",
    val invalidators: List<Invalidator> = emptyList(),
)

@Serializable
data class SnapshotLevel(val price: Double, val touches: Int = 0)

@Serializable
data class SnapshotFamily(val key: String = "", val label: String = "", val score: Double? = null)

@Serializable
data class SnapshotNews(val title: String = "", val tone: String = "neutral", val time: Double = 0.0)

/** Compact numbers kept to explain a later change of the signal. */
@Serializable
data class DecisionSnapshot(
    val price: Double? = null,
    val composite: Double? = null,
    val families: List<SnapshotFamily> = emptyList(),
    val momentum: Double? = null,
    val relativeVolume: Double? = null,
    val rsi: Double? = null,
    val adx: Double? = null,
    val nearestSupport: SnapshotLevel? = null,
    val nearestResistance: SnapshotLevel? = null,
    val newsScore: Double? = null,
    val topNews: SnapshotNews? = null,
)

object Guidance {
    /** A dollar price in the display currency like the web card (`usd` of decision.ts): no decimals when whole, else 2 (4 or 8 below 1). */
    fun usd(v: Double?): String {
        if (v == null || !v.isFinite()) return "—"
        val x = Money.toDisplay(v)
        val a = abs(x)
        val digits = if (a >= 1) (if (x == Math.rint(x)) 0 else 2) else if (a >= 0.01) 4 else 8
        return "${Format.fixed(x, digits)}${Money.NNBSP}${Money.symbol()}"
    }

    /** Check state: icon and word, never colour alone. */
    fun checkIcon(state: String) = when (state) { "met" -> "✓"; "unmet" -> "✕"; else -> "?" }
    fun checkLabel(state: String) = when (state) { "met" -> "remplie"; "unmet" -> "non remplie"; else -> "inconnue" }

    /** "Quand ne pas trader" section badge: "2 raisons" / "rien à signaler". */
    fun noTradeBadge(n: NoTrade): String =
        if (n.active) "${n.reasons.size} raison${if (n.reasons.size > 1) "s" else ""}" else "rien à signaler"

    /** One row of the ladder: a zone, or the price marker when it sits between / outside the zones. */
    sealed interface LadderRow {
        data class Zone(val zone: ActionZone, val here: Boolean) : LadderRow
        data class Marker(val text: String) : LadderRow
    }

    /**
     * Vertical ladder, highest price at the top. Price outside every zone: its marker goes above the first zone lying
     * entirely under it ("◀ vous êtes ici : entre … ").
     */
    fun ladder(z: ActionZones): List<LadderRow> {
        val rows = z.zones.reversed()
        val markerAt = if (z.here != null) -1 else rows.indexOfFirst { it.to < z.price }.let { if (it < 0) rows.size else it }
        val marker = LadderRow.Marker("◀ " + z.hereText.replaceFirst(Regex("^Vous êtes ici : "), "vous êtes ici : "))
        val out = mutableListOf<LadderRow>()
        rows.forEachIndexed { i, r ->
            if (i == markerAt) out += marker
            out += LadderRow.Zone(r, z.here == r.kind)
        }
        if (markerAt == rows.size) out += marker
        return out
    }

    /** "280,57 € – 307,28 €", or the single level. */
    fun zoneRange(z: ActionZone): String = if (z.from == z.to) usd(z.from) else "${usd(z.from)} – ${usd(z.to)}"

    /** "1/2 conditions · en cours". */
    fun scenarioCount(s: Decision.Scenario): String? =
        if (s.conditions.isEmpty()) null
        else "${s.met}/${s.conditions.size} condition${if (s.conditions.size > 1) "s" else ""}${if (s.unfolding) " · en cours" else ""}"
}
