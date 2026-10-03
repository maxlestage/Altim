package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlin.math.abs
import kotlin.math.max

// Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT"): the
// last decision seen per asset (verdict, level, rating, unmet setup steps, buy conditions) is kept on this phone and
// compared with each new one (Radar refresh, asset page). The server is stateless: the diff is done here.
// Each snapshot also keeps the decision's measurements (family scores, composite score, nearest support and
// resistance, relative volume, RSI, news tone) so that a change of verdict or rating is explained ("Momentum
// −18 pts", "Volume en baisse (1,4× → 0,7× la moyenne)", …): `explainChange`.
// Port of web/src/webapp/config-changes.ts version 2 (same texts, same rules; version 1 entries are migrated on read),
// tested with the same cases (ConfigChangesTest).

@Serializable
data class SignalLevel(val price: Double, val touches: Int)

@Serializable
data class SignalNews(val title: String, val tone: String)

/** Measurements of a decision kept to explain a later change (the server's `snapshot`, else read from the decision). */
@Serializable
data class SignalMetrics(
    val price: Double?,
    val composite: Double?,
    val families: List<SnapshotFamily>,
    /** Last daily volume ÷ 20-day average. */
    val relVolume: Double?,
    val rsi: Double?,
    val support: SignalLevel?,
    val resistance: SignalLevel?,
    val newsScore: Double?,
    val topNews: SignalNews?,
)

/** What the decision says is missing and what would change it, with its measurements (version 2). */
@Serializable
data class ConfigSnapshot(
    val verdict: Verdict,
    val level: DecisionLevel,
    val label: String,
    val levelLabel: String,
    /** Setup steps not met yet ("Cassure de la résistance : pas encore"). */
    val missing: List<String>,
    /** Conditions that would change the decision (the decision's `toBuy`). */
    val triggers: List<String>,
    val at: Double,
    /** Added in version 2 (absent from migrated entries). */
    val rating: Rating? = null,
    val ratingLabel: String? = null,
    val metrics: SignalMetrics? = null,
    /** The decision's confidence (0–100), for the Radar's order and its opportunities; absent from older entries. */
    val confidence: Double? = null,
    /** The decision's `chipNote` (the line under the Radar's chip); absent from older entries and older servers. */
    val chipNote: String? = null,
)

@Serializable
data class ConfigLabel(
    val verdict: Verdict,
    val level: DecisionLevel,
    val label: String,
    val levelLabel: String,
    val rating: Rating? = null,
    val ratingLabel: String? = null,
)

@Serializable
data class ConfigTransition(
    val symbol: String,
    val kind: Kind,
    val name: String,
    /** Personal mode (with the user's average cost and weights) or market data only: compared separately. */
    val personal: Boolean,
    val at: Double,
    /** When the previous configuration was seen. */
    val since: Double,
    val from: ConfigLabel,
    val to: ConfigLabel,
    val missing: List<String>,
    val triggers: List<String>,
    /** What changed in the measurements ("Momentum −18 pts (+40 → +22)"); empty when unknown (older entries). */
    val changes: List<String> = emptyList(),
) {
    /** "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the level, then the rating, when only it changed). */
    val title: String
        get() {
            val (a, b) = when {
                from.verdict != to.verdict -> from.label to to.label
                from.level != to.level || from.ratingLabel.isNullOrEmpty() || to.ratingLabel.isNullOrEmpty() -> from.levelLabel to to.levelLabel
                else -> "note ${from.ratingLabel}" to "note ${to.ratingLabel}"
            }
            return "🚨 $symbol — changement de configuration : $a → $b"
        }
}

@Serializable
data class ConfigState(
    val version: Int = ConfigChanges.STATE_VERSION,
    val last: Map<String, ConfigSnapshot> = emptyMap(),
    val transitions: List<ConfigTransition> = emptyList(),
)

data class ConfigUpdate(val state: ConfigState, val transition: ConfigTransition?)

object ConfigChanges {
    /** Transitions kept (newest first). */
    const val MAX_TRANSITIONS = 50
    /** Storage key on this phone (the version is inside the value). */
    const val KEY = "configChanges.v1"
    /** Version of the stored state (1: without measurements nor explanations, migrated on read). */
    const val STATE_VERSION = 2

    /** Thresholds under which a change is not worth telling. */
    object Limits {
        const val COMPOSITE = 5.0
        const val FAMILY = 10.0
        const val VOLUME = 0.3
        const val RSI = 10.0
        const val NEWS = 15.0
        const val LEVEL = 0.005
    }

    /** Every field written (the version too), read back strictly: no default fills a damaged entry. */
    private val Strict = Json { ignoreUnknownKeys = true; encodeDefaults = true }

    private fun stepState(s: StepState) = when (s) {
        StepState.NO -> "pas encore"
        StepState.UNKNOWN -> "non vérifiable"
        StepState.OK -> "validée"
    }

    private fun usd(v: Double) = Money.moneyFmt(v, " ") { x -> JsFormat.fr(x, if (x >= 1) 2 else 6) }

    private fun fin(v: Double?): Double? = v?.takeIf { it.isFinite() }

    /** The server's compact snapshot when given, else the same numbers read from the decision (older servers). */
    fun metricsOf(d: Decision): SignalMetrics {
        val s = d.snapshot
        if (s != null) {
            val labels = d.families.associate { it.key to it.label }
            return SignalMetrics(
                price = fin(s.price), composite = fin(s.composite),
                families = s.families.map { f -> SnapshotFamily(f.key, f.label.ifEmpty { labels[f.key] ?: f.key }, fin(f.score)) },
                relVolume = fin(s.relativeVolume), rsi = fin(s.rsi),
                support = s.nearestSupport?.takeIf { it.price.isFinite() }?.let { SignalLevel(it.price, it.touches) },
                resistance = s.nearestResistance?.takeIf { it.price.isFinite() }?.let { SignalLevel(it.price, it.touches) },
                newsScore = fin(s.newsScore), topNews = s.topNews?.let { SignalNews(it.title, it.tone) },
            )
        }
        val fam = d.families.map { SnapshotFamily(it.key, it.label, fin(it.score)) }
        return SignalMetrics(
            price = fin(d.price), composite = fin(d.score?.value), families = fam, relVolume = fin(d.liquidity?.relativeVolume), rsi = null,
            support = d.structure?.nearestSupport?.takeIf { it.price.isFinite() }?.let { SignalLevel(it.price, it.touches) },
            resistance = d.structure?.nearestResistance?.takeIf { it.price.isFinite() }?.let { SignalLevel(it.price, it.touches) },
            newsScore = fam.firstOrNull { it.key == "news" }?.score, topNews = null,
        )
    }

    /** "1,4×" (web `num(v, 1)`). */
    private fun x1(v: Double) = "${if (v < 0) "−" else ""}${JsFormat.fr(abs(v), 1)}×"
    private fun num0(v: Double) = "${if (v < 0) "−" else ""}${JsFormat.fr(abs(v), 0)}"
    private fun sameLevel(a: Double, b: Double) = abs(a - b) / max(abs(a), 1e-9) <= Limits.LEVEL

    /**
     * What changed between two snapshots, most telling first: composite score, families (largest moves first, 4 at
     * most), relative volume, RSI, support / resistance (broken, crossed, confirmed or replaced) and news. Pure.
     */
    fun explainChange(prev: SignalMetrics, next: SignalMetrics): List<String> {
        val out = mutableListOf<String>()
        if (prev.composite != null && next.composite != null && abs(next.composite - prev.composite) >= Limits.COMPOSITE) {
            out += "Score composite ${signedScore(prev.composite)} → ${signedScore(next.composite)}"
        }
        val before = prev.families.associateBy { it.key }
        val moves = mutableListOf<Pair<Double, String>>()
        for (f in next.families) {
            val p = before[f.key] ?: continue
            val ps = p.score
            val fs = f.score
            if (ps != null && fs != null) {
                val d = fs - ps
                if (abs(d) >= Limits.FAMILY) moves += abs(d) to "${f.label} ${signedScore(d)} pts (${signedScore(ps)} → ${signedScore(fs)})"
            } else if (ps != null && fs == null) moves += 0.0 to "${f.label} : plus mesuré(e) (données indisponibles)"
            else if (ps == null && fs != null) moves += 0.0 to "${f.label} : de nouveau mesuré(e) (${signedScore(fs)})"
        }
        // Stable sort: equal moves keep the families' order.
        out += moves.sortedByDescending { it.first }.take(4).map { it.second }
        if (prev.relVolume != null && next.relVolume != null && abs(next.relVolume - prev.relVolume) >= Limits.VOLUME) {
            out += "Volume en ${if (next.relVolume > prev.relVolume) "hausse" else "baisse"} (${x1(prev.relVolume)} → ${x1(next.relVolume)} la moyenne)"
        }
        if (prev.rsi != null && next.rsi != null && abs(next.rsi - prev.rsi) >= Limits.RSI) {
            out += "RSI ${num0(prev.rsi)} → ${num0(next.rsi)}"
        }
        val r0 = prev.resistance
        val r1 = next.resistance
        if (r0 != null && next.price != null && next.price > r0.price && (prev.price == null || prev.price <= r0.price)) {
            out += "Résistance ${Guidance.usd(r0.price)} franchie (prix ${Guidance.usd(next.price)})"
        } else if (r0 != null && r1 != null && sameLevel(r0.price, r1.price) && r1.touches > r0.touches) {
            out += "Résistance ${Guidance.usd(r1.price)} confirmée (touchée ${r1.touches} fois)"
        } else if (r0 != null && r1 != null && !sameLevel(r0.price, r1.price)) out += "Résistance la plus proche : ${Guidance.usd(r0.price)} → ${Guidance.usd(r1.price)}"
        val s0 = prev.support
        val s1 = next.support
        if (s0 != null && next.price != null && next.price < s0.price && (prev.price == null || prev.price >= s0.price)) {
            out += "Support ${Guidance.usd(s0.price)} cassé (prix ${Guidance.usd(next.price)})"
        } else if (s0 != null && s1 != null && sameLevel(s0.price, s1.price) && s1.touches > s0.touches) {
            out += "Support ${Guidance.usd(s1.price)} confirmé (touché ${s1.touches} fois)"
        } else if (s0 != null && s1 != null && !sameLevel(s0.price, s1.price)) out += "Support le plus proche : ${Guidance.usd(s0.price)} → ${Guidance.usd(s1.price)}"
        val n = next.topNews
        if (n != null && n.tone != "neutral" && n.title != prev.topNews?.title) {
            out += "Actualité ${if (n.tone == "negative") "négative" else "positive"} : « ${n.title} »"
        } else if (prev.newsScore != null && next.newsScore != null && abs(next.newsScore - prev.newsScore) >= Limits.NEWS) {
            out += "Ton des actualités ${signedScore(prev.newsScore)} → ${signedScore(next.newsScore)}"
        }
        return out
    }

    fun snapshotOf(d: Decision, at: Double) = ConfigSnapshot(
        verdict = d.verdict,
        level = d.level,
        label = d.verdictLabel,
        levelLabel = d.levelText,
        missing = d.setup?.steps.orEmpty().filter { it.state != StepState.OK }
            .map { "${it.label} : ${stepState(it.state)}${if (it.detail.isNotEmpty()) " (${it.detail})" else ""}" },
        triggers = d.toBuy.map { c -> if (c.level != null && !c.text.contains(Regex("[$€]"))) "${c.text} (${usd(c.level)})" else c.text },
        at = at,
        rating = d.rating,
        ratingLabel = d.rating?.let { r -> d.ratingLabel.ifBlank { r.label } },
        metrics = metricsOf(d),
        confidence = fin(d.confidence),
        chipNote = d.chipNote?.takeIf { it.isNotBlank() },
    )

    fun snapshotKey(kind: Kind, symbol: String, personal: Boolean) = "${kind.raw}:$symbol:${if (personal) "p" else "i"}"

    /**
     * A transition when the verdict, the level or the rating (when both snapshots have one) changed; null for the first
     * sighting or the same configuration. `changes` explains it when both snapshots kept their measurements.
     */
    fun diff(prev: ConfigSnapshot?, next: ConfigSnapshot, symbol: String, kind: Kind, name: String, personal: Boolean): ConfigTransition? {
        val ratingChanged = prev?.rating != null && next.rating != null && prev.rating != next.rating
        if (prev == null || (prev.verdict == next.verdict && prev.level == next.level && !ratingChanged)) return null
        fun pick(s: ConfigSnapshot) = ConfigLabel(s.verdict, s.level, s.label, s.levelLabel, s.rating, s.rating?.let { s.ratingLabel })
        val changes = if (prev.metrics != null && next.metrics != null) explainChange(prev.metrics, next.metrics) else emptyList()
        return ConfigTransition(symbol, kind, name, personal, next.at, prev.at, pick(prev), pick(next), next.missing, next.triggers, changes)
    }

    /** New state after seeing a decision (pure): baseline replaced, transition prepended when the configuration changed. */
    fun apply(state: ConfigState, d: Decision, personal: Boolean, now: Double): ConfigUpdate {
        val kind = d.kind ?: return ConfigUpdate(state, null)
        val key = snapshotKey(kind, d.symbol, personal)
        val next = snapshotOf(d, now)
        val t = diff(state.last[key], next, d.symbol, kind, d.name.ifEmpty { d.symbol }, personal)
        return ConfigUpdate(
            ConfigState(STATE_VERSION, state.last + (key to next), if (t != null) (listOf(t) + state.transitions).take(MAX_TRANSITIONS) else state.transitions),
            t,
        )
    }

    /** Last time this asset's informational decision was compared (the Radar re-reads it at most every 15 minutes). */
    fun lastSeen(state: ConfigState, asset: Asset): Double? = state.last[snapshotKey(asset.kind, asset.symbol, false)]?.at

    /**
     * The latest transition of this asset if it led to the configuration shown now (same verdict and level), for
     * "Pourquoi le signal a changé depuis …"; null otherwise.
     */
    fun latestChange(transitions: List<ConfigTransition>, d: Decision): ConfigTransition? {
        val personal = d.isPersonal
        val t = transitions.firstOrNull { it.symbol == d.symbol && it.kind == d.kind && it.personal == personal } ?: return null
        return t.takeIf { it.to.verdict == d.verdict && it.to.level == d.level }
    }

    /** The block's text under "Pourquoi le signal a changé depuis le …": the title without its "🚨 BTC — … : " start. */
    fun changeSummary(t: ConfigTransition): String = t.title.replaceFirst(Regex("^🚨 \\S+ — changement de configuration : "), "")

    // ---------- Validation on read (a damaged or older entry is dropped, never trusted) ----------

    private fun <T> strict(serializer: kotlinx.serialization.KSerializer<T>, e: JsonElement): T? = runCatching { Strict.decodeFromJsonElement(serializer, e) }.getOrNull()

    /** A snapshot; damaged measurements alone are dropped from it (the rest is kept). */
    private fun snapshot(e: JsonElement): ConfigSnapshot? {
        val o = e as? JsonObject ?: return null
        val base = strict(ConfigSnapshot.serializer(), JsonObject(o - "metrics")) ?: return null
        val m = o["metrics"]?.takeIf { it !is JsonNull } ?: return base
        return base.copy(metrics = strict(SignalMetrics.serializer(), m)?.takeIf { x -> listOfNotNull(x.price, x.composite, x.relVolume, x.rsi, x.newsScore).all { it.isFinite() } })
    }

    /**
     * Stored state, validated: version 2, or version 1 migrated (its snapshots have no measurements, its transitions
     * no explanation). A damaged entry is dropped; an unknown version gives an empty state.
     */
    fun parse(raw: String?): ConfigState {
        val o = raw?.let { runCatching { Json.parseToJsonElement(it) }.getOrNull() } as? JsonObject ?: return ConfigState()
        val last = o["last"] as? JsonObject
        val transitions = o["transitions"] as? JsonArray
        val version = (o["version"] as? JsonPrimitive)?.content
        if ((version != "1" && version != "$STATE_VERSION") || last == null || transitions == null) return ConfigState()
        return ConfigState(
            STATE_VERSION,
            last.mapNotNull { (k, v) -> snapshot(v)?.let { k to it } }.toMap(),
            transitions.mapNotNull { strict(ConfigTransition.serializer(), it) }.take(MAX_TRANSITIONS),
        )
    }

    fun encode(state: ConfigState): String = Strict.encodeToString(ConfigState.serializer(), state)
}
