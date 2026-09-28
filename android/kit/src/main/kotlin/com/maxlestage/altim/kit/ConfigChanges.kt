package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

// Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT"): the
// last decision seen per asset (verdict, level, unmet setup steps, buy conditions) is kept on this phone and compared
// with each new one (Radar refresh, asset page). The server is stateless: the diff is done here. Port of
// web/src/webapp/config-changes.ts (same texts, same rules), tested with the same cases (ConfigChangesTest).

/** What the decision says is missing and what would change it. */
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
)

@Serializable
data class ConfigLabel(val verdict: Verdict, val level: DecisionLevel, val label: String, val levelLabel: String)

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
) {
    /** "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the level when only it changed). */
    val title: String
        get() {
            val (a, b) = if (from.verdict == to.verdict) from.levelLabel to to.levelLabel else from.label to to.label
            return "🚨 $symbol — changement de configuration : $a → $b"
        }
}

@Serializable
data class ConfigState(val version: Int = 1, val last: Map<String, ConfigSnapshot> = emptyMap(), val transitions: List<ConfigTransition> = emptyList())

data class ConfigUpdate(val state: ConfigState, val transition: ConfigTransition?)

object ConfigChanges {
    /** Transitions kept (newest first). */
    const val MAX_TRANSITIONS = 50
    /** Storage key on this phone (versioned). */
    const val KEY = "configChanges.v1"

    /** Every field written (the version too), read back strictly: no default fills a damaged entry. */
    private val Strict = Json { ignoreUnknownKeys = true; encodeDefaults = true }

    private fun stepState(s: StepState) = when (s) {
        StepState.NO -> "pas encore"
        StepState.UNKNOWN -> "non vérifiable"
        StepState.OK -> "validée"
    }

    private fun usd(v: Double) = "${Format.plain(v, if (v >= 1) 2 else 6)} $"

    fun snapshotOf(d: Decision, at: Double) = ConfigSnapshot(
        verdict = d.verdict,
        level = d.level,
        label = d.verdictLabel,
        levelLabel = d.levelText,
        missing = d.setup?.steps.orEmpty().filter { it.state != StepState.OK }
            .map { "${it.label} : ${stepState(it.state)}${if (it.detail.isNotEmpty()) " (${it.detail})" else ""}" },
        triggers = d.toBuy.map { c -> if (c.level != null && !c.text.contains("$")) "${c.text} (${usd(c.level)})" else c.text },
        at = at,
    )

    fun snapshotKey(kind: Kind, symbol: String, personal: Boolean) = "${kind.raw}:$symbol:${if (personal) "p" else "i"}"

    /** A transition when the verdict or the level changed; null for the first sighting or the same configuration. */
    fun diff(prev: ConfigSnapshot?, next: ConfigSnapshot, symbol: String, kind: Kind, name: String, personal: Boolean): ConfigTransition? {
        if (prev == null || (prev.verdict == next.verdict && prev.level == next.level)) return null
        fun pick(s: ConfigSnapshot) = ConfigLabel(s.verdict, s.level, s.label, s.levelLabel)
        return ConfigTransition(symbol, kind, name, personal, next.at, prev.at, pick(prev), pick(next), next.missing, next.triggers)
    }

    /** New state after seeing a decision (pure): baseline replaced, transition prepended when the configuration changed. */
    fun apply(state: ConfigState, d: Decision, personal: Boolean, now: Double): ConfigUpdate {
        val kind = d.kind ?: return ConfigUpdate(state, null)
        val key = snapshotKey(kind, d.symbol, personal)
        val next = snapshotOf(d, now)
        val t = diff(state.last[key], next, d.symbol, kind, d.name.ifEmpty { d.symbol }, personal)
        return ConfigUpdate(
            ConfigState(1, state.last + (key to next), if (t != null) (listOf(t) + state.transitions).take(MAX_TRANSITIONS) else state.transitions),
            t,
        )
    }

    /** Last time this asset's informational decision was compared (the Radar re-reads it at most every 15 minutes). */
    fun lastSeen(state: ConfigState, asset: Asset): Double? = state.last[snapshotKey(asset.kind, asset.symbol, false)]?.at

    // ---------- Validation on read (a damaged or older entry is dropped, never trusted) ----------

    private fun <T> strict(serializer: kotlinx.serialization.KSerializer<T>, e: JsonElement): T? = runCatching { Strict.decodeFromJsonElement(serializer, e) }.getOrNull()

    fun parse(raw: String?): ConfigState {
        val o = raw?.let { runCatching { Json.parseToJsonElement(it) }.getOrNull() } as? JsonObject ?: return ConfigState()
        val last = o["last"] as? JsonObject
        val transitions = o["transitions"] as? JsonArray
        if ((o["version"] as? JsonPrimitive)?.content != "1" || last == null || transitions == null) return ConfigState()
        return ConfigState(
            1,
            last.mapNotNull { (k, v) -> strict(ConfigSnapshot.serializer(), v)?.let { k to it } }.toMap(),
            transitions.mapNotNull { strict(ConfigTransition.serializer(), it) }.take(MAX_TRANSITIONS),
        )
    }

    fun encode(state: ConfigState): String = Strict.encodeToString(ConfigState.serializer(), state)
}
