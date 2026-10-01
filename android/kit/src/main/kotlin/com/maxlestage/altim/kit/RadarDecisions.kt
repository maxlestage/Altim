package com.maxlestage.altim.kit

/**
 * The Radar's verdict is the full decision (the same as the asset's Décision card), read from the decisions seen on this
 * phone (ConfigChanges' snapshots, less than 12 h old; the Radar re-reads them every 15 minutes). The 4 h technical
 * signal is only one of its inputs: it is shown as a direction, never as an order (web DecisionBadge, technicalText,
 * ratingTone, RATING_RANK).
 */
object RadarDecisions {
    /** A decision older than this is not shown (the badge reads "Décision…"). */
    const val MAX_AGE = 12 * 3_600_000.0

    /** The technical signal as a direction, not an order. */
    fun technicalText(action: Action): String = when (action) {
        Action.STRONG_BUY -> "nettement haussier"
        Action.BUY -> "haussier"
        Action.SELL -> "baissier"
        Action.STRONG_SELL -> "nettement baissier"
        Action.HOLD -> "neutre"
    }

    /** Colour of that direction (up, down, flat). */
    fun technicalTone(action: Action): Tone = when (action) {
        Action.STRONG_BUY, Action.BUY -> Tone.GOOD
        Action.SELL, Action.STRONG_SELL -> Tone.BAD
        Action.HOLD -> Tone.NEUTRAL
    }

    /** Rank of a rating for sorting (buy side first); no rating last. */
    fun rank(r: Rating?): Int = when (r) {
        Rating.STRONG_BUY -> 0
        Rating.BUY -> 1
        Rating.HOLD -> 2
        Rating.REDUCE -> 3
        Rating.SELL -> 4
        Rating.STRONG_SELL -> 5
        null -> 9
    }

    /** Colour of a decision on the Radar: the rating's, else the verdict's (older servers). */
    fun tone(rating: Rating?, verdict: Verdict): Tone = when {
        rating == Rating.STRONG_BUY || rating == Rating.BUY -> Tone.GOOD
        rating == Rating.SELL || rating == Rating.STRONG_SELL || rating == Rating.REDUCE -> Tone.BAD
        rating == null && (verdict == Verdict.BUY || verdict == Verdict.BUY_ZONE) -> Tone.GOOD
        rating == null && (verdict == Verdict.SELL || verdict == Verdict.TRIM) -> Tone.BAD
        else -> Tone.NEUTRAL
    }

    fun tone(s: ConfigSnapshot): Tone = tone(s.rating, s.verdict)

    /** The badge's words: the rating's label, else the verdict's. */
    fun label(s: ConfigSnapshot): String = s.rating?.label ?: s.label.ifBlank { s.verdict.label }

    /** The line under the badge (why it says ATTENDRE or AUCUNE POSITION), when the decision gave one (web DecisionNote). */
    fun note(s: ConfigSnapshot): String? = s.chipNote?.takeIf { it.isNotBlank() }

    /** The freshest decision seen for this asset (personal or informational), when less than 12 h old; null otherwise. */
    fun cached(state: ConfigState, asset: Asset, now: Double): ConfigSnapshot? =
        listOf(true, false)
            .mapNotNull { state.last[ConfigChanges.snapshotKey(asset.kind, asset.symbol, it)] }
            .filter { now - it.at in 0.0..MAX_AGE }
            .maxByOrNull { it.at }

    /** "Décision" order: the rating (buy side first), then the confidence; stable (ties keep the user's order). */
    fun sorted(assets: List<Asset>, state: ConfigState, now: Double): List<Asset> {
        val seen = assets.associate { it.id to cached(state, it, now) }
        return assets.sortedWith(compareBy<Asset> { rank(seen[it.id]?.rating) }.thenByDescending { seen[it.id]?.confidence ?: 0.0 })
    }

    /** Opportunities: the assets whose full decision says ACHETER or ZONE D'ACHAT, most confident first, 3 at most. */
    fun opportunities(assets: List<Asset>, state: ConfigState, now: Double): List<Pair<Asset, ConfigSnapshot>> =
        assets.mapNotNull { a -> cached(state, a, now)?.takeIf { it.verdict == Verdict.BUY || it.verdict == Verdict.BUY_ZONE }?.let { a to it } }
            .sortedByDescending { it.second.confidence ?: 0.0 }
            .take(3)
}
