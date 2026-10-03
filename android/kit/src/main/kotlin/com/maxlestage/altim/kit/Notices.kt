package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

// Local notifications of the background check for two things the app already computes but only showed inside it: the
// configuration changes of the watched assets (ConfigChanges) and the positions that became dangerous
// (PortfolioRisk.dangerousPositions). What to notify given the previous and the new state, the keys that prevent a
// second notification of the same thing, and the texts. Pure; the app posts the notifications. Port of iOS AltimKit
// ChangeNotices (same keys, limits and texts).

/** A notification to post: identifier, texts and the asset a tap opens ("crypto:BTC"; null: the app opens). */
data class LocalNotice(val id: String, val title: String, val body: String, val asset: String?)

/** What was already notified, kept on the phone. */
@Serializable
data class ChangeNoticeState(
    /** Ids of the configuration changes already notified (oldest first, at most [ChangeNotices.MAX_REMEMBERED]). */
    val configNotified: List<String> = emptyList(),
    /** Danger keys ("line id:code") present at the last measurement; null: never measured (a baseline is taken). */
    val dangerActive: List<String>? = null,
    /** When each danger key was last notified (ms): gone and back within [ChangeNotices.DANGER_COOLDOWN] is not told again. */
    val dangerNotified: Map<String, Double> = emptyMap(),
)

object ChangeNotices {
    /** SharedPreferences key of the state. */
    const val KEY = "changeNotices.v1"
    /** Transition ids remembered against a second notification. */
    const val MAX_REMEMBERED = 200
    /** Changes listed one by one in a grouped notification (the others are counted). */
    const val MAX_LISTED = 5
    /** A danger gone and back within 24 hours is not notified again (a price hovering around the stop). */
    const val DANGER_COOLDOWN = 86_400_000.0
    const val DISCLAIMER = "Conseil indicatif : Altim ne passe aucun ordre."
    const val DANGER_ADVICE = "Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque)."
    const val DANGER_DETAIL = "Montants et niveaux dans Mes avoirs."

    /** Identity of a transition, as on iOS ("crypto:BTC:false:1727000000000.0"). */
    fun id(t: ConfigTransition): String = "${t.kind.raw}:${t.symbol}:${t.personal}:${t.at}"

    // ---------- Configuration changes ----------

    /**
     * One grouped notification for the transitions just found that were never notified (the newest per asset and
     * mode); null when there is nothing new. The state remembers their ids.
     */
    fun configNotice(found: List<ConfigTransition>, state: ChangeNoticeState): Pair<LocalNotice?, ChangeNoticeState> {
        val known = state.configNotified.toSet()
        val seen = mutableSetOf<String>()
        val fresh = found.withIndex()
            .sortedWith(compareByDescending<IndexedValue<ConfigTransition>> { it.value.at }.thenBy { it.index })
            .map { it.value }
            .filter { id(it) !in known && seen.add(ConfigChanges.snapshotKey(it.kind, it.symbol, it.personal)) }
        if (fresh.isEmpty()) return null to state
        val s = state.copy(configNotified = (state.configNotified + fresh.map(::id)).takeLast(MAX_REMEMBERED))
        return configText(fresh) to s
    }

    /**
     * Texts of the notification: one change with its explanation, missing conditions and triggers (the Radar card's
     * wording), or several, one line each.
     */
    fun configText(list: List<ConfigTransition>): LocalNotice {
        val assets = list.map { "${it.kind.raw}:${it.symbol}" }.toSet()
        val asset = if (assets.size == 1) assets.first() else null
        val nid = "altim.config.${id(list[0])}"
        if (list.size == 1) {
            val t = list[0]
            val lines = mutableListOf<String>()
            if (t.changes.isNotEmpty()) lines += "Pourquoi le signal a changé : ${t.changes.joinToString(" ; ")}."
            if (t.missing.isNotEmpty()) lines += "Conditions manquantes : ${t.missing.joinToString(" ; ")}."
            if (t.triggers.isNotEmpty()) lines += "Ce qui changerait la décision : ${t.triggers.joinToString(" ; ")}."
            if (lines.isEmpty()) lines += "Niveau ${t.from.levelLabel} → ${t.to.levelLabel}."
            lines += DISCLAIMER
            return LocalNotice(nid, t.title, lines.joinToString("\n"), asset)
        }
        val lines = list.take(MAX_LISTED).map { t ->
            "${t.symbol} : ${ConfigChanges.changeSummary(t)}${if (t.missing.isEmpty()) "" else " (conditions manquantes : ${t.missing.joinToString(" ; ")})"}"
        }.toMutableList()
        if (list.size > MAX_LISTED) lines += "+ ${list.size - MAX_LISTED} autre(s) : détail sur le Radar."
        lines += DISCLAIMER
        return LocalNotice(nid, "🚨 Changements de configuration · ${list.size}", lines.joinToString("\n"), asset)
    }

    // ---------- Dangerous positions ----------

    /** "line id:code" for each reason of each dangerous line. */
    fun dangerKeys(dangers: List<Danger>): List<String> = dangers.flatMap { d -> d.reasons.map { "${d.id}:${it.code}" } }

    /**
     * One notification for the lines that newly entered a danger state (a reason absent at the last measurement, and
     * not notified in the last 24 hours), with all the reasons of those lines; null otherwise.
     * - [baseline]: danger keys already known when the state was never measured (the last measurement shown on Mes
     *   avoirs), so that an update does not notify what the user already saw.
     * - [nearStopUnknown]: line ids whose daily candles could not be read: their "near the stop" state is unknown and
     *   kept as it was (a failed download neither ends nor starts a danger).
     */
    fun dangerNotice(
        dangers: List<Danger>,
        state: ChangeNoticeState,
        now: Double,
        baseline: List<String> = emptyList(),
        nearStopUnknown: Set<String> = emptySet(),
    ): Pair<LocalNotice?, ChangeNoticeState> {
        val previous = state.dangerActive ?: baseline
        val known = previous.toSet()
        val active = dangerKeys(dangers).toMutableList()
        val kept = previous.filter { k -> k.endsWith(":near_stop") && k.removeSuffix(":near_stop") in nearStopUnknown }
        for (k in kept) if (k !in active) active += k
        val fresh = dangerKeys(dangers).filter { k -> k !in known && (state.dangerNotified[k]?.let { now - it >= DANGER_COOLDOWN } ?: true) }.toSet()
        val notified = state.dangerNotified.filterValues { now - it < DANGER_COOLDOWN } + fresh.associateWith { now }
        val s = state.copy(dangerActive = active, dangerNotified = notified)
        val lines = dangers.filter { d -> d.reasons.any { "${d.id}:${it.code}" in fresh } }
        if (lines.isEmpty()) return null to s
        return dangerText(lines, now) to s
    }

    /** Label of a danger without any figure. */
    fun reasonLabel(code: String): String = when (code) {
        "stop_broken" -> "stop cassé"
        "near_stop" -> "prix à moins d'un ATR du stop"
        else -> "perte au-delà de votre risque par idée"
    }

    /** "⚠ Position devenue dangereuse : BTC" (the Mes avoirs and Radar notices' wording), each line's reasons. */
    fun dangerText(lines: List<Danger>, now: Double): LocalNotice {
        val symbols = lines.map { it.symbol }.distinct()
        val assets = lines.map { "${it.kind.raw}:${it.symbol}" }.toSet()
        val title = "⚠ ${if (lines.size > 1) "Positions devenues dangereuses" else "Position devenue dangereuse"} : ${symbols.joinToString(", ")}"
        // Reasons as fixed labels, never the amounts (loss, share of the wealth, stop level): a notification can show on
        // the lock screen or a watch; the figures stay in Mes avoirs.
        val body = lines.map { l -> "${l.symbol} : ${l.reasons.joinToString(" ; ") { reasonLabel(it.code) }}." } + DANGER_DETAIL + DANGER_ADVICE
        return LocalNotice("altim.danger.${now.toLong()}", title, body.joinToString("\n"), if (assets.size == 1) assets.first() else null)
    }

    // ---------- Storage ----------

    /** The saved state; a damaged field starts empty instead of failing the whole state. */
    fun parse(raw: String?): ChangeNoticeState {
        val o = raw?.let { runCatching { AltimJson.parseToJsonElement(it) }.getOrNull() } as? JsonObject ?: return ChangeNoticeState()
        fun strings(key: String): List<String>? = (o[key] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
        val notified = (o["dangerNotified"] as? JsonObject)?.mapNotNull { (k, v) -> (v as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull?.let { k to it } }?.toMap()
        return ChangeNoticeState(strings("configNotified") ?: emptyList(), strings("dangerActive"), notified ?: emptyMap())
    }

    fun encode(s: ChangeNoticeState): String = AltimJson.encodeToString(ChangeNoticeState.serializer(), s)
}
