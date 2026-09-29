package com.maxlestage.altim.kit

import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer

// Background notifications of what the app already shows inside: the configuration changes of the Radar
// ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT") and the positions that became dangerous (Mes
// avoirs). Pure rules: what to notify given what was already notified, the dedupe keys, and the texts (the same as
// the web cards: Radar.tsx « Changements de configuration » and « Position devenue dangereuse »).

/** One notification: its title, its text, and the asset a tap opens ("crypto:BTC"; null = the app). */
data class Notice(val title: String, val body: String, val asset: String?)

object ConfigNotices {
    /** Keys of the transitions already notified (or already there when the notifications were turned on). */
    const val KEY = "configNotices.v1"
    /** Transitions listed in a grouped notification; the others are counted. */
    const val MAX_LINES = 5

    /** One key per transition: asset, mode and time (a transition is recorded once, at its own time). */
    fun key(t: ConfigTransition) = "${t.kind.raw}:${t.symbol}:${if (t.personal) "p" else "i"}:${t.at.toLong()}"

    /** First check after the notifications were turned on: the transitions already listed are not news. */
    fun baseline(transitions: List<ConfigTransition>): Set<String> = transitions.map(::key).toSet()

    /**
     * Transitions not notified yet (newest first, as stored) and the keys to keep: those of the transitions still
     * listed, so the set stays bounded by [ConfigChanges.MAX_TRANSITIONS] and a notified one never comes back.
     */
    fun fresh(notified: Set<String>, transitions: List<ConfigTransition>): Pair<Set<String>, List<ConfigTransition>> =
        transitions.map(::key).toSet() to transitions.filter { key(it) !in notified }

    /** The card's lines of one transition: why it changed, missing conditions, what would change the decision. */
    fun details(t: ConfigTransition): List<String> = buildList {
        if (t.changes.isNotEmpty()) add("Pourquoi le signal a changé : ${t.changes.joinToString(" ; ")}.")
        if (t.missing.isNotEmpty()) add("Conditions manquantes : ${t.missing.joinToString(" ; ")}.")
        if (t.triggers.isNotEmpty()) add("Ce qui changerait la décision : ${t.triggers.joinToString(" ; ")}.")
    }

    /**
     * ONE notification for all the new transitions: one transition = the card's title and lines (a tap opens the
     * asset); several = one line each (a tap opens the app on the Radar). Null when nothing is new.
     */
    fun notice(fresh: List<ConfigTransition>): Notice? {
        val t = fresh.firstOrNull() ?: return null
        if (fresh.size == 1) {
            val lines = details(t).ifEmpty { listOf("Niveau ${t.from.levelLabel} → ${t.to.levelLabel}.") }
            return Notice(t.title, (lines + listOfNotNull(if (t.personal) "Mode personnel." else null)).joinToString("\n"), "${t.kind.raw}:${t.symbol}")
        }
        val lines = fresh.take(MAX_LINES).map { x ->
            val missing = if (x.missing.isNotEmpty()) " — Conditions manquantes : ${x.missing.joinToString(" ; ")}." else ""
            "${x.symbol} : ${ConfigChanges.changeSummary(x)}${if (x.personal) " (mode personnel)" else ""}$missing"
        }
        val more = if (fresh.size > MAX_LINES) listOf("… et ${fresh.size - MAX_LINES} autre(s) : détail dans le Radar.") else emptyList()
        return Notice("🚨 ${fresh.size} changements de configuration", (lines + more).joinToString("\n"), null)
    }

    fun parse(raw: String?): Set<String>? =
        raw?.let { runCatching { AltimJson.decodeFromString(ListSerializer(String.serializer()), it) }.getOrNull() }?.toSet()

    fun encode(keys: Set<String>): String = AltimJson.encodeToString(ListSerializer(String.serializer()), keys.sorted())
}

object DangerNotices {
    /** Danger codes already notified, per line (holding id → codes). */
    const val KEY = "dangerNotices.v1"

    /**
     * Lines that newly entered a danger state, or with a new reason (near the stop → stop broken): notified once.
     * The state kept is the dangers of now, so a line that left the danger zone is notified again if it comes back.
     * No previous state: every current danger is new. `unmeasured`: lines whose price or daily candles were missing
     * this time; their previous codes are kept (a failed fetch is not "out of danger", so no repeat when it works again).
     */
    fun fresh(previous: Map<String, List<String>>?, now: List<Danger>, unmeasured: Set<String> = emptySet()): Pair<Map<String, List<String>>, List<Danger>> {
        val prev = previous.orEmpty()
        val measured = now.associate { d -> d.id to d.reasons.map { it.code } }
        val state = (measured.keys + prev.keys.filter { it in unmeasured }).associateWith { id ->
            ((if (id in unmeasured) prev[id].orEmpty() else emptyList()) + measured[id].orEmpty()).distinct().sorted()
        }
        return state to now.filter { d -> d.reasons.any { it.code !in prev[d.id].orEmpty() } }
    }

    /** Web wording: "⚠ Position(s) devenue(s) dangereuse(s) dans vos avoirs", then "BTC : <reasons>" per line. */
    fun notice(fresh: List<Danger>): Notice? {
        val d = fresh.firstOrNull() ?: return null
        val title = "⚠ ${if (fresh.size > 1) "Positions devenues dangereuses" else "Position devenue dangereuse"} dans vos avoirs"
        val body = fresh.joinToString("\n") { x -> "${x.symbol} : ${x.reasons.joinToString(" ") { it.text }}" }
        return Notice(title, body, if (fresh.size == 1) "${d.kind.raw}:${d.symbol}" else null)
    }

    fun parse(raw: String?): Map<String, List<String>>? =
        raw?.let { runCatching { AltimJson.decodeFromString(MapSerializer(String.serializer(), ListSerializer(String.serializer())), it) }.getOrNull() }

    fun encode(state: Map<String, List<String>>): String =
        AltimJson.encodeToString(MapSerializer(String.serializer(), ListSerializer(String.serializer())), state)
}
