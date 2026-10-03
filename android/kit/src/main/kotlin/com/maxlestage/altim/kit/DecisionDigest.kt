package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer

// The Radar's verdict is the full decision (the same as the asset's Décision card), never the 4 h technical signal,
// which is only one of its inputs (web DecisionBadge / RATING_RANK / ratingTone, iOS AltimKit DecisionDigest). The
// decisions seen on this phone (asset page, the Radar's 15-minute re-reading, the background check) are kept as small
// digests, separately from the configuration diff.

/** What the Radar keeps of a decision seen on this phone. */
@Serializable
data class DecisionDigest(
    val verdict: Verdict,
    val rating: Rating? = null,
    /** The decision's own label ("ATTENDRE", "ZONE D'ACHAT"…), shown when it has no rating. */
    val label: String = "",
    val level: DecisionLevel = DecisionLevel.WAITING,
    /** 0–100. */
    val confidence: Double = 0.0,
    val personal: Boolean = false,
    /** ms: when the decision was computed (never later than when it was received). */
    val at: Double = 0.0,
    /** The decision's `chipNote`: the short reason under the chip; null from an older server or an older digest. */
    val note: String? = null,
) {
    /** Badge text: the rating's label (web RATING_UI), else the decision's label. */
    val badgeLabel: String get() = rating?.label ?: label.ifBlank { verdict.label }

    /** The line under the chip, when there is one (web DecisionNote). */
    val noteLine: String? get() = note?.takeIf { it.isNotBlank() }

    val tone: Tone get() = DecisionDigests.ratingTone(rating, verdict)

    /** ACHETER or ZONE D'ACHAT. */
    val isBuy: Boolean get() = verdict == Verdict.BUY || verdict == Verdict.BUY_ZONE

    companion object {
        /** [now] in ms; an answer served from the offline cache keeps its own (older) time. */
        fun of(d: Decision, personal: Boolean, now: Double): DecisionDigest {
            val at = if (d.asOf.isFinite() && d.asOf > 0) minOf(now, d.asOf) else now
            return DecisionDigest(d.verdict, d.rating, d.label, d.level, d.confidence, personal, at, d.chipNote)
        }
    }
}

/** A watched asset whose recent full decision is ACHETER or ZONE D'ACHAT. */
data class DecisionOpportunity(val asset: Asset, val decision: DecisionDigest)

object DecisionDigests {
    /** Stored on the phone (SharedPreferences), cleared at logout (another server would show them as its own). */
    const val KEY = "decisionDigests.v1"
    /** Older than this, the Radar shows "Décision…" rather than a stale verdict. */
    const val MAX_AGE = 12 * 3_600_000.0
    /** Entries kept at most (the newest). */
    const val MAX_ENTRIES = 200
    /** Shown while no recent decision is known. */
    const val PENDING_LABEL = "Décision…"

    fun key(kind: Kind, symbol: String) = "${kind.raw}:$symbol"

    /** Adds a decision; an informational one does not replace a personal one still recent (seen on the asset's page). */
    fun record(all: Map<String, DecisionDigest>, d: Decision, personal: Boolean, now: Double): Map<String, DecisionDigest> {
        val kind = d.kind ?: return all
        val k = key(kind, d.symbol)
        val next = DecisionDigest.of(d, personal, now)
        val old = all[k]
        if (!personal && old != null && old.personal && now - old.at <= MAX_AGE) return all
        if (old != null && old.at > next.at) return all
        val out = all + (k to next)
        if (out.size <= MAX_ENTRIES) return out
        return out.entries.sortedByDescending { it.value.at }.take(MAX_ENTRIES).associate { it.key to it.value }
    }

    /** The decision of this asset when less than 12 h old. */
    fun fresh(all: Map<String, DecisionDigest>, asset: Asset, now: Double): DecisionDigest? =
        all[key(asset.kind, asset.symbol)]?.takeIf { now - it.at <= MAX_AGE }

    /** Rank of a rating for sorting (buy side first); none last (web RATING_RANK). */
    fun ratingRank(r: Rating?): Int = when (r) {
        Rating.STRONG_BUY -> 0
        Rating.BUY -> 1
        Rating.HOLD -> 2
        Rating.REDUCE -> 3
        Rating.SELL -> 4
        Rating.STRONG_SELL -> 5
        null -> 9
    }

    /** Colour of a rating on the Radar; the verdict's when there is no rating (web `ratingTone`). */
    fun ratingTone(r: Rating?, verdict: Verdict): Tone = when {
        r == Rating.STRONG_BUY || r == Rating.BUY || (r == null && (verdict == Verdict.BUY || verdict == Verdict.BUY_ZONE)) -> Tone.GOOD
        r == Rating.SELL || r == Rating.STRONG_SELL || r == Rating.REDUCE || (r == null && (verdict == Verdict.SELL || verdict == Verdict.TRIM)) -> Tone.BAD
        else -> Tone.NEUTRAL
    }

    /** "Décision" sort of the Radar: rating (buy side first), then confidence; the user's order breaks ties. */
    fun sortByDecision(assets: List<Asset>, all: Map<String, DecisionDigest>, now: Double): List<Asset> =
        assets.withIndex().sortedWith(
            compareBy<IndexedValue<Asset>> { ratingRank(fresh(all, it.value, now)?.rating) }
                .thenByDescending { fresh(all, it.value, now)?.confidence ?: 0.0 }
                .thenBy { it.index },
        ).map { it.value }

    /** "Variation" sort: the largest move first, either way; unknown last, the user's order breaks ties. */
    fun sortByChange(assets: List<Asset>, change: (Asset) -> Double?): List<Asset> =
        assets.withIndex().sortedWith(
            compareByDescending<IndexedValue<Asset>> { change(it.value)?.let { c -> kotlin.math.abs(c) } ?: -1.0 }.thenBy { it.index },
        ).map { it.value }

    /**
     * "Opportunités détectées": watched assets whose recent full decision is ACHETER or ZONE D'ACHAT (not the 4 h
     * technical signal alone), the most confident first, 3 at most.
     */
    fun opportunities(assets: List<Asset>, all: Map<String, DecisionDigest>, now: Double, limit: Int = 3): List<DecisionOpportunity> =
        assets.withIndex().mapNotNull { (i, a) -> fresh(all, a, now)?.takeIf { it.isBuy }?.let { i to DecisionOpportunity(a, it) } }
            .sortedWith(compareByDescending<Pair<Int, DecisionOpportunity>> { it.second.decision.confidence }.thenBy { it.first })
            .take(limit)
            .map { it.second }

    private val SERIALIZER = MapSerializer(String.serializer(), DecisionDigest.serializer())

    /** The saved digests (damaged or older data: empty). */
    fun parse(raw: String?): Map<String, DecisionDigest> =
        raw?.let { runCatching { AltimJson.decodeFromString(SERIALIZER, it) }.getOrNull() } ?: emptyMap()

    fun encode(all: Map<String, DecisionDigest>): String = AltimJson.encodeToString(SERIALIZER, all)
}
