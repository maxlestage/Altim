package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable

// « Bot Altim » v3 (additive, defaulted: `v3` of /api/bot, of an asset row and of a view): horizons 20 / 60, the peers
// ranking, the corrected threshold (K tests since v1), the forward test after the pre-registration. Port of the v3 part
// of web/src/webapp/model-bot.ts (and of BotV3.tsx / BotRadarCard.tsx's pure bits), tested with the same cases as
// web/test/bot-v3.test.ts. Codes (family, candidate, verdict, action) stay strings so an unknown value never breaks
// decoding. Nothing here recomputes a statistic.

/**
 * One side of a configuration: excess in points of % (buy: mean − baseline; sell: baseline − mean), `t` by date (the
 * verdict's), verdict at the corrected threshold and at the raw t ≥ 2 (for comparison only).
 */
@Serializable
data class BotV3SideStats(
    val signals: Int = 0,
    val mean: Double? = null,
    val baseline: Double? = null,
    val excess: Double? = null,
    val t: Double? = null,
    val dates: Int = 0,
    val tByAsset: Double? = null,
    val assets: Int = 0,
    val tPerSignal: Double? = null,
    val beatShare: Double? = null,
    /** "insufficient" | "edge" | "negative" | "unproven"; null when unknown. */
    val verdict: String? = null,
    val rawVerdict: String? = null,
)

@Serializable
data class BotV3ConfigStats(
    val testRows: Int = 0,
    val labelled: Int = 0,
    val from: Double? = null,
    val to: Double? = null,
    val buy: BotV3SideStats = BotV3SideStats(),
    val sell: BotV3SideStats = BotV3SideStats(),
    val waitShare: Double? = null,
    val medianBotReturn: Double? = null,
    val medianHoldReturn: Double? = null,
    val beatHold: Int = 0,
    val holdAssets: Int = 0,
    val exit: BotExitStats? = null,
    val brierSkillUp: Double? = null,
    val brierSkillDown: Double? = null,
    /** Family « peers »: the same sides against the group's equal-weight mean (control added after the first real run). */
    val buyVsMean: BotV3SideStats? = null,
    val sellVsMean: BotV3SideStats? = null,
)

/** `main`: basket, signals dated up to the pre-registration; `extra`: training assets (nested only); `forward`: after it. */
@Serializable
data class BotV3Config(
    val id: String = "",
    /** "absolute" | "peers" | "v2". */
    val family: String = "absolute",
    val candidate: String? = null,
    val nested: Boolean = false,
    val headline: Boolean = false,
    val label: String = "",
    val trainedBlocks: Int = 0,
    val chosenBlocks: Int = 0,
    val main: BotV3ConfigStats = BotV3ConfigStats(),
    val extra: BotV3ConfigStats? = null,
    val forward: BotV3ConfigStats = BotV3ConfigStats(),
)

@Serializable
data class BotV3EconScore(val id: String = "", val score: Double? = null, val signals: Int = 0)

@Serializable
data class BotV3BlockOut(
    val start: Double = 0.0,
    val end: Double? = null,
    val trainRows: Int = 0,
    val v2: String? = null,
    val absolute: String? = null,
    val peers: String? = null,
    val absoluteScores: List<BotV3EconScore> = emptyList(),
    val peersScores: List<BotV3EconScore> = emptyList(),
    val roundsUp: Int? = null,
    val roundsDown: Int? = null,
    val roundsPeers: Int? = null,
)

@Serializable
data class BotV3Rounds(val fits: Int = 0, val min: Int? = null, val median: Double? = null, val max: Int? = null)

@Serializable
data class BotV3Horizon(
    val horizon: Int = 20,
    val blocks: Int = 0,
    val testFrom: Double? = null,
    val testTo: Double? = null,
    val configs: List<BotV3Config> = emptyList(),
    val selection: List<BotV3BlockOut> = emptyList(),
    val roundsUp: BotV3Rounds = BotV3Rounds(),
    val roundsDown: BotV3Rounds = BotV3Rounds(),
    val roundsPeers: BotV3Rounds = BotV3Rounds(),
)

/** Annualised (%, Sharpe without risk-free rate), max drawdown ≤ 0 (%), mean exposure 0-1. */
@Serializable
data class BotV3SeriesStats(
    val days: Int = 0,
    val annualReturn: Double? = null,
    val annualVol: Double? = null,
    val sharpe: Double? = null,
    val maxDrawdown: Double? = null,
    val totalReturn: Double? = null,
    val meanExposure: Double? = null,
)

@Serializable
data class BotV3VolManaged(
    val assets: Int = 0,
    val from: Double? = null,
    val to: Double? = null,
    val periods: Double = 0.0,
    val hold: BotV3SeriesStats = BotV3SeriesStats(),
    val managed: BotV3SeriesStats = BotV3SeriesStats(),
    val medianSharpeHold: Double? = null,
    val medianSharpeManaged: Double? = null,
    val medianDrawdownHold: Double? = null,
    val medianDrawdownManaged: Double? = null,
    val betterSharpe: Int = 0,
    val shallowerDrawdown: Int = 0,
    val forwardHold: BotV3SeriesStats = BotV3SeriesStats(),
    val forwardManaged: BotV3SeriesStats = BotV3SeriesStats(),
    val text: String = "",
)

@Serializable
data class BotV3Group(
    /** "stock" | "crypto". */
    val id: String = "stock",
    val label: String = "",
    val market: String = "",
    val universe: BotUniverse = BotUniverse(),
    val peersFrom: Double? = null,
    val horizons: List<BotV3Horizon> = emptyList(),
    val volManaged: BotV3VolManaged? = null,
    val text: String = "",
)

@Serializable
data class BotV3K(val v1: Int = 0, val v2: Int = 0, val v3: Int = 0, val total: Int = 0)

@Serializable
data class BotV3CandidateInfo(val id: String = "", val family: String = "absolute", val label: String = "", val description: String = "")

@Serializable
data class BotV3Parameters(
    val horizons: List<Int> = emptyList(),
    val stockFrom: String = "",
    val maxTreeRows: Int = 0,
    val longMaxRounds: Int = 0,
    val longDepth: Int = 0,
    val longShrinkage: Double = 0.0,
    val longMinLeaf: Int = 0,
    val patience: Int = 0,
    val minPeers: Int = 0,
    val minValSignals: Int = 0,
    val minSignals: Int = 30,
    val nudge: Double = 0.0,
)

@Serializable
data class BotV3Compute(val maxRows: Int = 0, val blocks: Int = 0, val rowBytes: Int = 0)

/** Since v3: the pre-registered v3; the v2-shaped fields of the report then hold v2's selection at 20 days. */
@Serializable
data class BotV3Report(
    val version: Int = 3,
    val preregDate: String = "",
    val forwardFrom: Double = 0.0,
    val afterPrereg: List<String> = emptyList(),
    val k: BotV3K = BotV3K(),
    val alpha: Double = 0.05,
    val tRequired: Double = 0.0,
    val headline: String = "",
    val forwardHeadline: String = "",
    val groups: List<BotV3Group> = emptyList(),
    val candidates: List<BotV3CandidateInfo> = emptyList(),
    val changes: List<String> = emptyList(),
    val method: List<String> = emptyList(),
    val limits: List<String> = emptyList(),
    val parameters: BotV3Parameters = BotV3Parameters(),
    val compute: BotV3Compute = BotV3Compute(),
)

/** Today's actions of an asset of the basket under the 4 headline configurations ("buy" | "wait" | "sell" | null). */
@Serializable
data class BotV3AssetNow(
    val time: Double? = null,
    val absolute20: String? = null,
    val absolute60: String? = null,
    val peers20: String? = null,
    val peers60: String? = null,
)

/** One headline configuration today for one asset. */
@Serializable
data class BotV3Signal(
    val family: String = "absolute",
    val horizon: Int = 20,
    val candidate: String? = null,
    val action: String? = null,
    val buyVerdict: String? = null,
    val sellVerdict: String? = null,
    val forwardBuySignals: Int = 0,
    val forwardSellSignals: Int = 0,
    val contradicted: Boolean = false,
    val counts: Boolean = false,
    /** Proven on the past but awaiting 30 confirming forward signals: shown only (since the rule of 30/09/2026). */
    val pending: Boolean = false,
    val text: String = "",
)

/** v3 in a decision: the 4 headline configurations today; `counts` when one of them is on a confirmed side. */
@Serializable
data class BotV3View(
    val available: Boolean = false,
    val signals: List<BotV3Signal> = emptyList(),
    val counts: Boolean = false,
    val nudge: Double = 0.0,
    val tRequired: Double = 0.0,
    val note: String = "",
    val pro: String? = null,
    val con: String? = null,
    val conHeld: String? = null,
)

object BotV3 {
    /** Forward signals a proven side needs before it counts. */
    const val FORWARD_NEEDED = 30

    private fun plain(v: Double?, digits: Int = 2) = ModelValidation.plain(v, digits)
    private fun signedPct(v: Double?, digits: Int = 1) = ModelValidation.signedPct(v, digits)
    private fun points(v: Double?) = Bot.points(v)
    private fun s(n: Int) = if (n > 1) "s" else ""

    val V3_SHORT: Map<String, String> = linkedMapOf(
        "trend" to "Tendance", "v1" to "Logistique v1", "logit" to "Logistique 24", "trees" to "Arbres v2", "treesLong" to "Arbres longs",
        "xsMomentum" to "Momentum entre pairs", "xsLogit" to "Logistique entre pairs", "xsTrees" to "Arbres longs entre pairs",
    )
    val FAMILY_LABEL: Map<String, String> = linkedMapOf(
        "absolute" to "Hausse ou baisse de l'actif", "peers" to "Classement entre pairs", "v2" to "Sélection v2 (log-loss)",
    )

    fun short(id: String?): String = if (id == null) "aucun" else V3_SHORT[id] ?: id
    fun familyLabel(f: String): String = FAMILY_LABEL[f] ?: f

    /** "12/10/2026" from "2026-10-12". */
    fun frIso(s: String): String = s.split("-").reversed().joinToString("/")

    /** "t = −1,2 (requis 3,52)". */
    fun tVsRequired(t: Double?, required: Double): String = "t = ${if (t == null) "—" else plain(t, 1)} (requis ${plain(required, 2)})"

    /** "t par jour 3,7 (requis 3,52) · par actif 0,6": the verdict's t, the one it needs, and the t by asset. */
    fun tLine(x: BotV3SideStats, required: Double): String =
        "t par jour ${if (x.t == null) "—" else plain(x.t, 1)} (requis ${plain(required, 2)}) · par actif ${if (x.tByAsset == null) "—" else plain(x.tByAsset, 1)}"

    /** Verdict at the corrected threshold, in French; says when only the raw t ≥ 2 was reached. */
    fun verdictLabel(x: BotV3SideStats, required: Double): String = when (x.verdict) {
        "edge" -> "Avantage au seuil corrigé (t ≥ ${plain(required, 2)}), à confirmer"
        "negative" -> "Pire que la référence au seuil corrigé"
        "insufficient" -> "Trop peu de signaux pour conclure"
        else -> if (x.rawVerdict == "edge") "t ≥ 2 atteint, pas le seuil corrigé : non démontré" else "Non démontré"
    }

    /** A side of a configuration in one sentence: "412 achats : +0,31 point face à la médiane du groupe (t par jour 1,1 (requis 3,52) · par actif 0,4)." */
    fun sideText(family: String, buy: Boolean, x: BotV3SideStats, required: Double): String {
        if (x.signals == 0) return if (buy) "Aucun achat." else "Aucune vente."
        val ref = if (family == "peers") "la médiane du groupe" else "une entrée au hasard"
        if (buy) return "${x.signals} achat${s(x.signals)} : ${points(x.excess)} face à $ref (${tLine(x, required)})."
        val verb = if (family == "peers") "gain à passer sur l'actif médian" else "baisse évitée"
        return "${x.signals} vente${s(x.signals)} : $verb ${points(x.excess)} (${tLine(x, required)})."
    }

    /** Forward test of a configuration so far. */
    fun forwardText(c: BotV3ConfigStats, required: Double): String {
        val n = c.buy.signals + c.sell.signals
        if (n == 0) return "Aucun signal jugé pour l'instant (il faut 20 à 60 jours de bourse après le signal)."
        return "${c.buy.signals} achat${s(c.buy.signals)} (${points(c.buy.excess)}, ${tVsRequired(c.buy.t, required)}) · " +
            "${c.sell.signals} vente${s(c.sell.signals)} (${points(c.sell.excess)})."
    }

    data class VolRow(val label: String, val managed: String, val hold: String)

    /** Sharpe, max drawdown, yearly return and volatility of the managed trend vs holding (equal-weight portfolio). */
    fun volRows(v: BotV3VolManaged): List<VolRow> = listOf(
        VolRow("Ratio de Sharpe", plain(v.managed.sharpe, 2), plain(v.hold.sharpe, 2)),
        VolRow("Pire baisse", signedPct(v.managed.maxDrawdown), signedPct(v.hold.maxDrawdown)),
        VolRow("Rendement annuel", signedPct(v.managed.annualReturn), signedPct(v.hold.annualReturn)),
        VolRow("Volatilité annuelle", Bot.pct0(v.managed.annualVol), Bot.pct0(v.hold.annualVol)),
    )

    /** "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)". */
    fun computeText(t: BotTiming?): String? {
        if (t == null) return null
        val sec = Math.round(t.computeMs / 1000).toInt()
        val dur = if (sec >= 60) "${sec / 60} min ${sec % 60} s" else "$sec s"
        val mem = t.peakRssMb?.let { ", pic mémoire ${Math.round(it)} Mo" } ?: ""
        return "$dur de calcul sur ${t.threads} cœur${s(t.threads)}$mem (téléchargement ${Math.round(t.fetchMs / 1000)} s)"
    }

    data class Headline(val horizon: Int, val c: BotV3Config)

    /** The 4 headline configurations of a group: (horizon, config). */
    fun headlineConfigs(g: BotV3Group): List<Headline> = g.horizons.flatMap { h -> h.configs.filter { it.headline }.map { Headline(h.horizon, it) } }

    /** The figures that judge a side: family « peers »'s control against the group's mean when measured, else the side itself. */
    fun judged(c: BotV3ConfigStats, buy: Boolean): BotV3SideStats = (if (buy) c.buyVsMean else c.sellVsMean) ?: if (buy) c.buy else c.sell

    /** A side proven at the corrected threshold (family « peers »: against the median and the mean). */
    fun proven(c: BotV3ConfigStats, buy: Boolean): Boolean = (if (buy) c.buy else c.sell).verdict == "edge" && judged(c, buy).verdict == "edge"

    /** A side proven on the past and confirmed by the forward test (≥ 30 signals, excess ≥ 0 against each reference): only then does it count. */
    fun confirmed(c: BotV3Config, buy: Boolean): Boolean {
        val refs = listOfNotNull(if (buy) c.forward.buy else c.forward.sell, if (buy) c.forward.buyVsMean else c.forward.sellVsMean)
        return proven(c.main, buy) && refs.all { it.signals >= FORWARD_NEEDED && it.excess != null && it.excess >= 0 }
    }

    /** « Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : … » for a proven side awaiting its forward test; null otherwise. */
    fun pendingText(c: BotV3Config, buy: Boolean, required: Double): String? {
        if (!proven(c.main, buy) || confirmed(c, buy)) return null
        val t = judged(c.main, buy).t
        return "Avantage mesuré sur le passé (t = ${if (t == null) "—" else plain(t, 2)} contre ${plain(required, 2)} exigé) mais fragile : il ne comptera qu'après " +
            "30 signaux sur l'avenir qui le confirment (${(if (buy) c.forward.buy else c.forward.sell).signals} à ce jour)."
    }

    /** Whether a v3 headline configuration has an edge on either side (at the corrected threshold, both references). */
    fun anyEdge(r: BotV3Report): Boolean = r.groups.any { g -> headlineConfigs(g).any { proven(it.c.main, true) || proven(it.c.main, false) } }

    /** Signals judged so far in the forward test (headline configurations). */
    fun forwardSignals(r: BotV3Report): Int = r.groups.sumOf { g -> headlineConfigs(g).sumOf { it.c.forward.buy.signals + it.c.forward.sell.signals } }

    /** Consecutive retrainings with the same choice of one family (runs of identical choices). */
    data class ChoiceRun(val from: Double, val to: Double, val chosen: String?, val count: Int)

    fun choiceRuns(blocks: List<BotV3BlockOut>, peers: Boolean): List<ChoiceRun> {
        val out = mutableListOf<ChoiceRun>()
        for (b in blocks) {
            val c = if (peers) b.peers else b.absolute
            val last = out.lastOrNull()
            if (last != null && last.chosen == c) out[out.size - 1] = last.copy(to = b.start, count = last.count + 1) else out += ChoiceRun(b.start, b.start, c, 1)
        }
        return out
    }

    /** "Tendance (4 fois) → Logistique 24 → aucun". */
    fun runsText(runs: List<ChoiceRun>): String = runs.joinToString(" → ") { "${short(it.chosen)}${if (it.count > 1) " (${it.count} fois)" else ""}" }

    // ---------- Screen texts (web BotV3.tsx) ----------

    /** "Bot v3 · pré-enregistré le 30/09/2026". */
    fun title(v: BotV3Report): String = "Bot v3 · pré-enregistré le ${frIso(v.preregDate)}"

    /** "0 signal", "1 signal", "12 signaux" (the web's tile and Radar card write "signal" + "s": "signals" for 2 or more). */
    fun signalsWord(n: Int): String = "$n ${if (n > 1) "signaux" else "signal"}"

    fun protocolText(v: BotV3Report): String =
        "Protocole écrit avant tout calcul et figé : ${v.k.v1} tests en v1, ${v.k.v2} en v2, ${v.k.v3} en v3. Avec autant d'essais, un t de 2 arrive par hasard ; " +
            "un avantage n'est dit démontré qu'au-delà de t = ${plain(v.tRequired, 2)} (Bonferroni, 5 %). Horizons jugés à part : ${v.parameters.horizons.joinToString(" et ")} jours de bourse."

    fun resultsNote(v: BotV3Report): String =
        "Panier fixe, signaux datés jusqu'au ${frIso(v.preregDate)} ; modèle choisi à chaque réentraînement sur une validation interne, jamais sur le test."

    fun forwardTitle(v: BotV3Report): String = "Depuis le ${frIso(v.preregDate)} (test sur l'avenir)"

    fun forwardNote(v: BotV3Report): String =
        "Le seul test vraiment neuf : ces signaux n'existaient pas quand le protocole a été figé. Même calcul, même seuil ; un avantage passé qu'il contredit (après au " +
            "moins ${v.parameters.minSignals} signaux) cesse de compter."

    /** "Actions et ETF américains · Classement entre pairs · 20 jours". */
    fun forwardRowTitle(g: BotV3Group, h: Headline): String = "${g.label} · ${familyLabel(h.c.family)} · ${h.horizon} jours"

    /** "22 actifs testés, 72 de plus à l'entraînement · historique médian 36,7 ans · classement entre pairs possible depuis nov. 1993". */
    fun groupSubtitle(g: BotV3Group): String =
        "${g.universe.basket} actifs testés, ${g.universe.extra} de plus à l'entraînement · historique médian ${plain(g.universe.medianYears, 1)} ans" +
            (g.peersFrom?.let { " · classement entre pairs possible depuis ${ModelValidation.monthYear(it)}" } ?: "")

    /** "À 20 jours" and " · test de janv. 1996 à sept. 2026 · 31 réentraînements". */
    fun horizonTitle(h: BotV3Horizon): String = "À ${h.horizon} jours"

    fun horizonSubtitle(h: BotV3Horizon): String =
        "test de ${ModelValidation.monthYear(h.testFrom)} à ${ModelValidation.monthYear(h.testTo)} · ${h.blocks} réentraînements"

    /** "ACHETER · face à la médiane" (family « peers »), "ACHETER" otherwise. */
    fun sideHead(buy: Boolean, c: BotV3ConfigStats): String =
        "${if (buy) "ACHETER" else "VENDRE"}${if ((if (buy) c.buyVsMean else c.sellVsMean) != null) " · face à la médiane" else ""}"

    /** Family « peers » against the group's equal-weight mean (control added after the first real run). */
    fun controlText(x: BotV3SideStats, required: Double): String =
        "Face à la moyenne du groupe (contrôle ajouté après coup) : ${points(x.excess)} (${tLine(x, required)})"

    fun controlVerdict(proven: Boolean): String =
        if (proven) "Sur le passé : avantage face aux deux références." else "Retenu : non démontré (il faut les deux références) ; ne compte pas."

    /** "+75 % / +1 396 %" (medians: bot's buys compounded / holding). */
    fun holdText(c: BotV3ConfigStats): String = "${signedPct(c.medianBotReturn, 0)} / ${signedPct(c.medianHoldReturn, 0)}"

    /** Compact rows of a configuration: buys, sells and (family « peers ») against the mean. */
    fun configBuyRow(c: BotV3Config, required: Double): Pair<String, String> =
        "${c.main.buy.signals} achats" to "${points(c.main.buy.excess)} (${tVsRequired(c.main.buy.t, required)})"

    fun configSellRow(c: BotV3Config, required: Double): Pair<String, String> =
        "${c.main.sell.signals} ventes" to "${points(c.main.sell.excess)} (${tVsRequired(c.main.sell.t, required)})"

    fun configMeanRow(c: BotV3Config): Pair<String, String>? {
        if (c.main.buyVsMean == null) return null
        val b = judged(c.main, true)
        val v = judged(c.main, false)
        return "Face à la moyenne" to "${points(b.excess)} / ${points(v.excess)} (t ${plain(b.t, 1)} / ${plain(v.t, 1)})"
    }

    fun chosenText(c: BotV3Config): String = "retenu ${c.chosenBlocks} fois sur ${c.trainedBlocks}"

    /** "22 actifs du panier, de janv. 1996 à sept. 2026 · portefeuille à parts égales". */
    fun volSubtitle(v: BotV3VolManaged): String =
        "${v.assets} actifs du panier, de ${ModelValidation.monthYear(v.from)} à ${ModelValidation.monthYear(v.to)} · portefeuille à parts égales"

    fun candidateLine(c: BotV3CandidateInfo): String = "${c.label} (${familyLabel(c.family)}) — ${c.description}"

    /** "Calcul : 9 min 43 s de calcul… ; 200 réentraînements, jusqu'à 719 748 jours × actifs par groupe et horizon." */
    fun computeLine(v: BotV3Report, t: BotTiming?): String? = computeText(t)?.let {
        "Calcul : $it ; ${v.compute.blocks} réentraînements, jusqu'à ${ModelValidation.fr(v.compute.maxRows.toDouble(), 0)} jours × actifs par groupe et horizon."
    }

    /** "v2 (choisi par log-loss) … t ≥ 3,52 …": the v2 reference header's note. */
    fun v2ReferenceNote(v: BotV3Report): String =
        "Le modèle de la v2 (choisi par log-loss) refait sur les nouvelles données, jugé au seuil corrigé (t ≥ ${plain(v.tRequired, 2)}). C'est lui qui donne les " +
            "probabilités de hausse et de baisse ci-dessous ; il ne compte dans aucune décision."

    /** Label of a view's signal: "hausse/baisse 20 j", "entre pairs 60 j". */
    fun signalLabel(x: BotV3Signal): String = "${if (x.family == "peers") "entre pairs" else "hausse/baisse"} ${x.horizon} j"

    // ---------- Radar card (web BotRadarCard.tsx) ----------

    /** The headline's first sentence (the verdict; the details are on the Bot screen). */
    fun firstSentence(s: String): String {
        val i = s.indexOf(". ")
        return if (i < 0) s else s.substring(0, i + 1)
    }

    /** First sentence of v3's headline (else the report's). */
    fun radarHeadline(r: BotReport): String = firstSentence(r.v3?.headline ?: r.headline)

    /** "Test sur l'avenir : 0 signal sur 30", null before v3. */
    fun radarCounter(r: BotReport): String? = r.v3?.let { "Test sur l'avenir : ${signalsWord(forwardSignals(it))} sur 30" }
}

/** Whether the bot counts in this decision: v3's rule when present (the server copies it too), else v1/v2's. */
val BotView.effectiveCounts: Boolean get() = v3?.counts ?: counts

/** The decision's note: v3's when present and non-empty, else the view's. */
val BotView.effectiveNote: String get() = v3?.note?.takeIf { it.isNotBlank() } ?: note
