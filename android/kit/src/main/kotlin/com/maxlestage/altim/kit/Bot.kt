package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs

// « Bot Altim » (GET /api/bot, 202 {pending:true} while the first training runs; GET /api/bot/views?symbols=; `bot` of
// /api/decision): JSON contract (every field optional, so an older or partial answer decodes) and the pure helpers of
// the screen and of the decision line. Port of web/src/webapp/model-bot.ts, tested with the same cases (BotTest ↔
// web/test/bot.test.ts). Returns and probabilities are in %, "points" are differences of % (signal − random day),
// times in ms. The server computes everything; nothing here recomputes a statistic. Codes (group, action, verdict)
// stay strings so an unknown value never breaks decoding. v2 fields are additive and defaulted (a v1 answer still
// decodes): `version`, `changes`, per group `universe`, `dataYears`, `selection`, `candidates`, `holdout`, `extra`,
// `market`, `clustered` t in each side, `exit` of the sell side, the live model's `candidate`.

/** t of a side's excesses: by date (the verdict's since v2), by asset, per signal (v1's). */
@Serializable
data class BotClustered(
    val byDate: Double? = null,
    val dates: Int = 0,
    val byAsset: Double? = null,
    val assets: Int = 0,
    val perSignal: Double? = null,
)

/** Holding vs leaving for 20 days at each VENDRE signal (medians over the assets; drawdowns ≤ 0 in %). */
@Serializable
data class BotExitStats(
    val assets: Int = 0,
    val outShare: Double? = null,
    val medianHoldMaxDrawdown: Double? = null,
    val medianBotMaxDrawdown: Double? = null,
    val medianDrawdownAvoided: Double? = null,
    val medianHoldReturn: Double? = null,
    val medianBotReturn: Double? = null,
    val beatHold: Int = 0,
)

/** Probability range [from, to) in %, rows = labelled test days, predicted = mean probability, realised = share. */
@Serializable
data class BotBucket(
    val from: Double = 0.0,
    val to: Double = 0.0,
    val label: String = "",
    val rows: Int = 0,
    val predicted: Double? = null,
    val realised: Double? = null,
)

/** ACHETER signals out of sample (non-overlapping per asset). excess = meanNet − baselineNet (same asset and period). */
@Serializable
data class BotBuyStats(
    val signals: Int = 0,
    val meanNet: Double? = null,
    val baselineNet: Double? = null,
    val allDaysNet: Double? = null,
    val excess: Double? = null,
    val tStat: Double? = null,
    val rawTStat: Double? = null,
    /** Since v2 (then [tStat] is the by-date t). */
    val clustered: BotClustered? = null,
    val hitRate: Double? = null,
    val baselineHitRate: Double? = null,
    val medianBotReturn: Double? = null,
    val medianHoldReturn: Double? = null,
    val beatHold: Int = 0,
    val assets: Int = 0,
    /** "insufficient" | "edge" | "negative" | "unproven"; null when unknown. */
    val verdict: String? = null,
    val verdictLabel: String = "",
)

/** VENDRE signals: what holding made over the next 20 days after them vs a random day; avoided = baseline − after. */
@Serializable
data class BotSellStats(
    val signals: Int = 0,
    val meanAfter: Double? = null,
    val baselineAfter: Double? = null,
    val allDaysAfter: Double? = null,
    val avoided: Double? = null,
    val tStat: Double? = null,
    /** Since v2 (then [tStat] is the by-date t). */
    val clustered: BotClustered? = null,
    val fallRate: Double? = null,
    val baselineFallRate: Double? = null,
    val meanDrawdown: Double? = null,
    val baselineDrawdown: Double? = null,
    val exit: BotExitStats? = null,
    val verdict: String? = null,
    val verdictLabel: String = "",
)

@Serializable
data class BotWaitStats(val days: Int = 0, val share: Double? = null, val meanNet: Double? = null, val baselineNet: Double? = null)

/** Statistics of a set of test days (the whole basket, or one group). */
interface BotStatsLike {
    val testRows: Int
    val labelled: Int
    val buy: BotBuyStats
    val sell: BotSellStats
    val wait: BotWaitStats
    val brierSkillUp: Double?
    val brierSkillDown: Double?
    val calibrationUp: List<BotBucket>
    val calibrationDown: List<BotBucket>
}

@Serializable
data class BotStats(
    override val testRows: Int = 0,
    override val labelled: Int = 0,
    override val buy: BotBuyStats = BotBuyStats(),
    override val sell: BotSellStats = BotSellStats(),
    override val wait: BotWaitStats = BotWaitStats(),
    override val brierSkillUp: Double? = null,
    override val brierSkillDown: Double? = null,
    override val calibrationUp: List<BotBucket> = emptyList(),
    override val calibrationDown: List<BotBucket> = emptyList(),
) : BotStatsLike

@Serializable
data class BotWeight(val id: String = "", val coef: Double = 0.0, val mean: Double = 0.0, val sd: Double = 0.0)

@Serializable
data class BotModelOut(val baseRate: Double = 0.0, val threshold: Double = 0.0, val intercept: Double = 0.0, val weights: List<BotWeight> = emptyList())

/** Inner-validation log-loss of a candidate ("v1" | "logit" | "trees" | "trend"); null when it could not be scored. */
@Serializable
data class BotCandidateScore(val id: String = "", val logLoss: Double? = null)

/** Probabilities of the trend rule per state. */
@Serializable
data class BotTrend(val probs: List<Double> = emptyList(), val baseRate: Double = 0.0, val rows: Int = 0)

/** A live side model: `logit` weights, `trees` (compact nodes [feature, threshold, left, right, value], kept raw) or `trend`. */
@Serializable
data class BotLiveSide(
    val baseRate: Double = 0.0,
    val threshold: Double = 0.0,
    val logit: BotModelOut? = null,
    val trees: JsonElement? = null,
    val trend: BotTrend? = null,
)

@Serializable
data class BotLiveModel(
    val trainedRows: Int = 0,
    val trainedFrom: Double? = null,
    val trainedTo: Double? = null,
    val up: BotModelOut = BotModelOut(),
    val down: BotModelOut = BotModelOut(),
    /** Since v2: the chosen candidate ("v1" | "logit" | "trees" | "trend"). */
    val candidate: String? = null,
    val scores: List<BotCandidateScore> = emptyList(),
    val upModel: BotLiveSide? = null,
    val downModel: BotLiveSide? = null,
)

/** One candidate's own walk-forward result: for information only, never used to choose. */
@Serializable
data class BotCandidateStat(
    val id: String = "",
    val label: String = "",
    val description: String = "",
    val trainedBlocks: Int = 0,
    val chosenBlocks: Int = 0,
    override val testRows: Int = 0,
    override val labelled: Int = 0,
    override val buy: BotBuyStats = BotBuyStats(),
    override val sell: BotSellStats = BotSellStats(),
    override val wait: BotWaitStats = BotWaitStats(),
    override val brierSkillUp: Double? = null,
    override val brierSkillDown: Double? = null,
    override val calibrationUp: List<BotBucket> = emptyList(),
    override val calibrationDown: List<BotBucket> = emptyList(),
) : BotStatsLike

/** One retraining: test period [start, end), training rows, inner-validation log-loss of each candidate, choice. */
@Serializable
data class BotBlockOut(
    val start: Double = 0.0,
    val end: Double? = null,
    val trainRows: Int = 0,
    val chosen: String? = null,
    val scores: List<BotCandidateScore> = emptyList(),
)

@Serializable
data class BotUniverse(
    val basket: Int = 0,
    val extra: Int = 0,
    val extraFailed: Int = 0,
    val rows: Int = 0,
    val dataFrom: Double? = null,
    val medianYears: Double? = null,
    val maxYears: Double? = null,
)

/** The last 12 months, shown apart. */
@Serializable
data class BotHoldout(
    val from: Double? = null,
    val to: Double? = null,
    override val testRows: Int = 0,
    override val labelled: Int = 0,
    override val buy: BotBuyStats = BotBuyStats(),
    override val sell: BotSellStats = BotSellStats(),
    override val wait: BotWaitStats = BotWaitStats(),
    override val brierSkillUp: Double? = null,
    override val brierSkillDown: Double? = null,
    override val calibrationUp: List<BotBucket> = emptyList(),
    override val calibrationDown: List<BotBucket> = emptyList(),
) : BotStatsLike

/** One group (stocks or cryptos): its statistics are flattened in the object, as served. */
@Serializable
data class BotGroupStat(
    /** "stock" | "crypto". */
    val id: String = "stock",
    val label: String = "",
    val assets: Int = 0,
    val testFrom: Double? = null,
    val testTo: Double? = null,
    val blocks: Int = 0,
    val trainedBlocks: Int = 0,
    override val testRows: Int = 0,
    override val labelled: Int = 0,
    override val buy: BotBuyStats = BotBuyStats(),
    override val sell: BotSellStats = BotSellStats(),
    override val wait: BotWaitStats = BotWaitStats(),
    override val brierSkillUp: Double? = null,
    override val brierSkillDown: Double? = null,
    override val calibrationUp: List<BotBucket> = emptyList(),
    override val calibrationDown: List<BotBucket> = emptyList(),
    val model: BotLiveModel? = null,
    val text: String = "",
    // Since v2.
    val universe: BotUniverse? = null,
    val dataYears: Double? = null,
    val selection: List<BotBlockOut> = emptyList(),
    val candidates: List<BotCandidateStat> = emptyList(),
    val holdout: BotHoldout? = null,
    /** The nested result on the extra training assets (out of sample too, not the headline). */
    val extra: BotStats? = null,
    val market: String? = null,
) : BotStatsLike

/** Today's view of one asset: action ("buy" | "wait" | "sell") and probabilities (%), null without enough history. */
@Serializable
data class BotNowView(val time: Double? = null, val action: String? = null, val up: Double? = null, val down: Double? = null)

@Serializable
data class BotAssetRow(
    val symbol: String = "",
    val name: String = "",
    val kind: Kind = Kind.STOCK,
    val `class`: String = "stock",
    val group: String = "stock",
    val testRows: Int = 0,
    val testFrom: Double? = null,
    val testTo: Double? = null,
    val buys: Int = 0,
    val buyMean: Double? = null,
    val buyExcess: Double? = null,
    val sells: Int = 0,
    val sellAvoided: Double? = null,
    val waitShare: Double? = null,
    val botReturn: Double? = null,
    val holdReturn: Double? = null,
    val now: BotNowView = BotNowView(),
    val source: String = "",
    // Since v2.
    val years: Double? = null,
    val dataFrom: Double? = null,
    val outShare: Double? = null,
    val holdMaxDrawdown: Double? = null,
    val botMaxDrawdown: Double? = null,
) {
    val asset: Asset get() = Asset(symbol, kind, name.ifBlank { symbol })
}

@Serializable
data class BotFeature(val id: String = "", val label: String = "", val help: String = "")

@Serializable
data class BotParameters(
    val horizonDays: Int = 20,
    val retrainEvery: Int = 0,
    val purgeDays: Int = 0,
    val minTrainDays: Int = 0,
    val warmup: Int = 0,
    val l2: Double = 0.0,
    val thresholdMargin: Double = 0.0,
    val costStockPct: Double = 0.0,
    val costCryptoPct: Double = 0.0,
    val minSignals: Int = 0,
    val tEdge: Double = 0.0,
    val nudge: Double = 0.0,
    // Since v2.
    val innerValidationDays: Int? = null,
    val trainStride: Int? = null,
    val holdoutDays: Int? = null,
    val stockYears: Int? = null,
    val trees: Int? = null,
    val treeDepth: Int? = null,
    val shrinkage: Double? = null,
    val minLeaf: Int? = null,
    val selection: String? = null,
)

@Serializable
data class BotTiming(val fetchMs: Double = 0.0, val computeMs: Double = 0.0, val threads: Int = 0)

@Serializable
data class BotReport(
    val asOf: Double = 0.0,
    val basketFixedOn: String = "",
    val headline: String = "",
    val overall: BotStats = BotStats(),
    /** Stocks then cryptos (those with at least one asset). */
    val groups: List<BotGroupStat> = emptyList(),
    /** Basket order, never ranked by performance. */
    val assets: List<BotAssetRow> = emptyList(),
    val failures: List<ValFailure> = emptyList(),
    val features: List<BotFeature> = emptyList(),
    val parameters: BotParameters = BotParameters(),
    val method: List<String> = emptyList(),
    val limits: List<String> = emptyList(),
    val source: String = "",
    // Since v2.
    val version: Int? = null,
    val changes: List<String> = emptyList(),
    val extraFailures: List<ValFailure> = emptyList(),
    val extraFixedOn: String? = null,
    val timing: BotTiming? = null,
)

/** A feature's weight in today's probability ("up": pushes the probability up, "down": down). */
@Serializable
data class BotContribution(
    val id: String = "",
    val label: String = "",
    val value: Double = 0.0,
    val valueText: String = "",
    val weight: Double = 0.0,
    val effect: String = "up",
    val text: String = "",
)

/** The bot's view of one asset now (`bot` of /api/decision, items of /api/bot/views with symbol and kind added). */
@Serializable
data class BotView(
    val available: Boolean = false,
    /** "stock" | "crypto". */
    val group: String = "stock",
    val groupLabel: String = "",
    val inBasket: Boolean = false,
    /** "buy" | "wait" | "sell"; null without enough history. */
    val action: String? = null,
    val actionLabel: String? = null,
    val up: Double? = null,
    val down: Double? = null,
    val thresholdUp: Double? = null,
    val thresholdDown: Double? = null,
    val baseUp: Double? = null,
    val baseDown: Double? = null,
    val buyVerdict: String? = null,
    val sellVerdict: String? = null,
    /** Today's action is on a side with an out-of-sample edge: it counts (a little) in the decision. */
    val counts: Boolean = false,
    val time: Double? = null,
    val contributions: List<BotContribution> = emptyList(),
    val text: String = "",
    val note: String = "",
    val asOf: Double? = null,
    val link: String = "/app/bot",
    /** Since v2: the candidate model behind the view ("v1" | "logit" | "trees" | "trend") and its label. */
    val model: String? = null,
    val modelLabel: String? = null,
    // Items of /api/bot/views only.
    val symbol: String = "",
    val kind: Kind? = null,
) {
    /** Tone of the decision's block (web `dec-proof` classes): "na", "edge" (counts) or "unproven". */
    val tone: String get() = if (!available || action == null) "na" else if (counts) "edge" else "unproven"
}

@Serializable
data class BotViews(val asOf: Double? = null, val views: List<BotView> = emptyList())

/** The report, or "come back in a few seconds" (202 while the first training runs). */
sealed interface BotResult {
    data object Pending : BotResult
    data class Ready(val report: BotReport) : BotResult
}

object Bot {
    const val PATH = "/api/bot"
    const val VIEWS_PATH = "/api/bot/views"
    const val MAX_VIEWS = 20

    /** `symbols` of /api/bot/views: "BTC:crypto,AAPL:stock" (20 at most). */
    fun viewsQuery(assets: List<Asset>): Map<String, String> =
        mapOf("symbols" to assets.take(MAX_VIEWS).joinToString(",") { "${it.symbol}:${it.kind.raw}" })

    /** Label and tone of an action ("buy" | "wait" | "sell"); null for an unknown code. */
    data class ActionUi(val label: String, val tone: Tone)

    fun actionUi(action: String?): ActionUi? = when (action) {
        "buy" -> ActionUi("ACHETER", Tone.GOOD)
        "wait" -> ActionUi("ATTENDRE", Tone.WARN)
        "sell" -> ActionUi("VENDRE", Tone.BAD)
        else -> null
    }

    private const val NNBSP = ModelValidation.NNBSP
    private fun fr(v: Double, max: Int) = ModelValidation.fr(v, max)
    private fun signedPct(v: Double?, digits: Int = 1) = ModelValidation.signedPct(v, digits)
    private fun plain(v: Double?, digits: Int = 2) = ModelValidation.plain(v, digits)
    private fun s(n: Int) = if (n > 1) "s" else ""

    /** "+0,52 point", "−2,44 points", "—". */
    fun points(v: Double?): String {
        if (v == null || !v.isFinite()) return "—"
        return "${if (v < 0) "−" else if (v > 0) "+" else ""}${fr(abs(v), 2)} point${if (abs(v) >= 2) "s" else ""}"
    }

    /** "54 %", "—". */
    fun pct0(v: Double?): String = if (v == null || !v.isFinite()) "—" else "${fr(v, 0)}$NNBSP%"

    private fun tText(t: Double?) = if (t == null) "t non calculable" else "t = ${plain(t, 1)}"

    /** v2 (clustered present): the t is by date. */
    private fun sideT(t: Double?, c: BotClustered?) = if (c != null && t != null) "t par jour = ${plain(t, 1)}" else tText(t)

    /** Short names of the fixed candidate models of v2. */
    val CANDIDATE_SHORT: Map<String, String> = linkedMapOf("v1" to "Logistique v1", "logit" to "Logistique 24", "trees" to "Arbres", "trend" to "Tendance")

    fun candidateShort(id: String): String = CANDIDATE_SHORT[id] ?: id

    /** "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8" (the verdict reads the first). */
    fun clusteredText(c: BotClustered?, t: Double?): String {
        if (c == null) return "t = ${if (t == null) "—" else plain(t, 1)}"
        fun f(v: Double?) = if (v == null) "—" else plain(v, 1)
        return "t par jour ${f(c.byDate)} (${c.dates} jours) · par actif ${f(c.byAsset)} · par signal ${f(c.perSignal)}"
    }

    /** What leaving at each VENDRE did to the holding: time out, worst fall, return (medians over the assets). */
    fun exitText(e: BotExitStats?): String? {
        if (e == null || e.assets == 0) return null
        return "En sortant 20 jours à chaque VENDRE : hors marché ${pct0(e.outShare)} du temps ; pire baisse médiane ${signedPct(e.medianBotMaxDrawdown)} contre " +
            "${signedPct(e.medianHoldMaxDrawdown)} en gardant ; rendement médian ${signedPct(e.medianBotReturn, 0)} contre ${signedPct(e.medianHoldReturn, 0)} " +
            "(mieux que garder : ${e.beatHold} actif${s(e.beatHold)} sur ${e.assets})."
    }

    /** "22 actifs du panier et 72 de plus à l'entraînement · historique médian 20,1 ans (le plus long 20,1 ans) · marché : S&P 500 (SPY)". */
    fun dataText(g: BotGroupStat): String? {
        val u = g.universe ?: return null
        fun y(v: Double?) = if (v == null) "—" else "${plain(v, 1)} ans"
        val failed = if (u.extraFailed > 0) " (${u.extraFailed} indisponible${s(u.extraFailed)})" else ""
        return "${u.basket} actif${s(u.basket)} du panier et ${u.extra} de plus à l'entraînement$failed · historique médian ${y(u.medianYears)} " +
            "(le plus long ${y(u.maxYears)})${if (!g.market.isNullOrEmpty()) " · marché : ${g.market}" else ""}"
    }

    /** Consecutive retrainings with the same choice: [from] / [to] = start of the first / last retraining of the run. */
    data class SelectionRun(val from: Double, val to: Double, val chosen: String?, val count: Int)

    /** Retrainings grouped by consecutive identical choices. */
    fun selectionRuns(blocks: List<BotBlockOut>): List<SelectionRun> {
        val out = mutableListOf<SelectionRun>()
        for (b in blocks) {
            val last = out.lastOrNull()
            if (last != null && last.chosen == b.chosen) {
                out[out.size - 1] = last.copy(to = b.start, count = last.count + 1)
            } else {
                out += SelectionRun(b.start, b.start, b.chosen, 1)
            }
        }
        return out
    }

    /** "108 achats : +1,7 % en moyenne sur 20 jours, contre +1,85 % pour une entrée au hasard… (−0,15 point, t = −0,1)." */
    fun buyText(b: BotBuyStats): String {
        if (b.signals == 0) return "Aucun achat pendant les périodes de test."
        return "${b.signals} achat${s(b.signals)} : ${signedPct(b.meanNet, 2)} en moyenne sur 20 jours, contre ${signedPct(b.baselineNet, 2)} pour une entrée au hasard " +
            "sur le même actif et la même période (${points(b.excess)}, ${sideT(b.tStat, b.clustered)})."
    }

    /** What followed the VENDRE signals, said without a sign to decode. */
    fun sellText(v: BotSellStats): String {
        if (v.signals == 0) return "Aucune vente pendant les périodes de test."
        val a = v.avoided
        val diff = when {
            a == null -> ""
            a >= 0 -> "cours ensuite inférieur de ${points(a).replace("+", "")}"
            else -> "cours ensuite supérieur de ${points(-a).replace("+", "")}"
        }
        return "${v.signals} vente${s(v.signals)} : le cours a fait ${signedPct(v.meanAfter, 2)} dans les 20 jours suivants, contre ${signedPct(v.baselineAfter, 2)} " +
            "après un jour au hasard ($diff, ${sideT(v.tStat, v.clustered)})."
    }

    fun waitText(w: BotWaitStats): String {
        if (w.days == 0) return "Jamais sur ATTENDRE pendant les tests."
        return "ATTENDRE ${pct0(w.share)} des jours testés, suivis en moyenne de ${signedPct(w.meanNet, 2)} (tous les jours : ${signedPct(w.baselineNet, 2)})."
    }

    /** Calibration buckets that have days in them. */
    fun calibrationRows(b: List<BotBucket>): List<BotBucket> = b.filter { it.rows > 0 }

    /** "meilleur que la fréquence de base" / "moins bon…" from a Brier skill (%). */
    fun skillText(skill: Double?): String {
        if (skill == null) return "non calculable"
        if (abs(skill) < 0.5) return "pas mieux que la fréquence de base"
        return if (skill > 0) "${fr(skill, 1)}$NNBSP% mieux que la fréquence de base" else "${fr(-skill, 1)}$NNBSP% moins bien que la fréquence de base"
    }

    /** Decision summary: "ATTENDRE · hausse 54 %, baisse 44 %" (the view's text when it has no action). */
    fun summary(v: BotView): String {
        val ui = actionUi(v.action)
        if (!v.available || ui == null) return v.text
        return "${ui.label} · hausse ${pct0(v.up)}, baisse ${pct0(v.down)}"
    }

    /** Whether any group has an edge on either side (then the headline says which one counts). */
    fun anyEdge(r: BotReport): Boolean = r.groups.any { it.buy.verdict == "edge" || it.sell.verdict == "edge" }

    // ---------- Screen texts (web Bot.tsx) ----------

    /** "34 / 34". */
    fun assetsTile(r: BotReport): String = "${r.assets.size} / ${r.assets.size + r.failures.size}"

    /** "207 / 54". */
    fun signalsTile(r: BotReport): String = "${r.overall.buy.signals} / ${r.overall.sell.signals}"

    /** "Horizon 20 jours · seuil : fréquence d'entraînement + 5 points · coûts… Calculé le 29/09/2026 11:36, réentraîné toutes les 12 h." */
    fun parametersText(r: BotReport, zone: ZoneId = ZoneId.systemDefault()): String {
        val p = r.parameters
        val at = DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm", Locale.FRANCE).withZone(zone).format(Instant.ofEpochMilli(r.asOf.toLong()))
        return "Horizon ${p.horizonDays} jours · seuil : fréquence d'entraînement + ${plain(p.thresholdMargin, 0)} points · coûts d'un aller-retour " +
            "${plain(p.costStockPct, 2)} % (actions), ${plain(p.costCryptoPct, 2)} % (cryptos). Calculé le $at, réentraîné toutes les 12 h."
    }

    /** "2 actifs non utilisés :". */
    fun failuresTitle(n: Int): String = "$n actif${s(n)} non utilisé${s(n)} :"

    /** "22 actifs testés · test du oct. 2024 au sept. 2026 · 4 réentraînements". */
    fun groupSubtitle(g: BotGroupStat): String =
        "${g.assets} actif${s(g.assets)} testé${s(g.assets)} · test du ${ModelValidation.monthYear(g.testFrom)} au ${ModelValidation.monthYear(g.testTo)} · " +
            "${g.trainedBlocks} réentraînement${s(g.trainedBlocks)}"

    /** "Aujourd'hui : hausse 56 %, baisse 40 %". */
    fun todayText(a: BotAssetRow): String = "Aujourd'hui : hausse ${pct0(a.now.up)}, baisse ${pct0(a.now.down)}"

    /** "Test : 3 achats (−7,12 points vs hasard) · 3 ventes (cours ensuite −1,42 point vs hasard) · attente 90 % · …". */
    fun assetTestText(a: BotAssetRow): String =
        "Test : ${a.buys} achat${s(a.buys)}${a.buyExcess?.let { " (${points(it)} vs hasard)" } ?: ""} · ${a.sells} vente${s(a.sells)}" +
            "${a.sellAvoided?.let { " (cours ensuite ${points(-it)} vs hasard)" } ?: ""} · attente ${pct0(a.waitShare)} · achats cumulés ${signedPct(a.botReturn)}, " +
            "détention ${signedPct(a.holdReturn)}" +
            (if (a.outShare != null) " · hors marché après VENDRE ${pct0(a.outShare)} du temps, pire baisse ${signedPct(a.botMaxDrawdown)} contre ${signedPct(a.holdMaxDrawdown)} en gardant" else "") +
            " (${a.source}${if (a.years != null) ", ${plain(a.years, 1)} ans" else ""})"

    /** One watched asset: "Hausse 54 % (seuil 60 %), baisse 44 % (seuil 58 %)" (+ " · hors du panier testé"), or its text. */
    fun viewText(v: BotView): String =
        if (v.available && v.action != null) {
            "Hausse ${pct0(v.up)} (seuil ${pct0(v.thresholdUp)}), baisse ${pct0(v.down)} (seuil ${pct0(v.thresholdDown)})${modelSuffix(v)}${if (v.inBasket) "" else " · hors du panier testé"}"
        } else {
            v.text
        }

    private fun modelSuffix(v: BotView) = if (v.modelLabel.isNullOrEmpty()) "" else " · ${v.modelLabel}"

    /** Decision line: "Probabilités à 20 jours : hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)" (+ the model since v2, + the untested note). */
    fun probabilitiesText(v: BotView): String =
        "Probabilités à 20 jours : hausse ${pct0(v.up)} (seuil ${pct0(v.thresholdUp)}), baisse ${pct0(v.down)} (seuil ${pct0(v.thresholdDown)})${modelSuffix(v)}" +
            if (v.inBasket) "" else " · modèle des ${if (v.group == "crypto") "cryptos" else "actions"}, non testé sur cet actif"

    /** "RSI 14 : 40 (pèse contre la hausse) · …", empty without contributions. */
    fun contributionsText(v: BotView): String = v.contributions.joinToString(" · ") { it.text }

    private fun frDate(s: String) = s.split("-").reversed().joinToString("/")

    /** "Panier fixé le 29/09/2026, univers élargi le 29/09/2026. Source : …. Calcul : 13 s de téléchargement, 35 s d'entraînement et de test." */
    fun sourceText(r: BotReport): String {
        val t = r.timing
        val extra = r.extraFixedOn
        return "Panier fixé le ${frDate(r.basketFixedOn)}${if (!extra.isNullOrEmpty()) ", univers élargi le ${frDate(extra)}" else ""}. Source : ${r.source}." +
            if (t != null) " Calcul : ${Math.round(t.fetchMs / 1000)} s de téléchargement, ${Math.round(t.computeMs / 1000)} s d'entraînement et de test." else ""
    }

    /** "Univers élargi : 2 actifs indisponibles (A, B).", null when none failed. */
    fun extraFailuresText(r: BotReport): String? {
        val n = r.extraFailures.size
        if (n == 0) return null
        return "Univers élargi : $n actif${s(n)} indisponible${s(n)} (${r.extraFailures.joinToString(", ") { it.symbol }})."
    }

    /** "Données : 22 actifs du panier et 72 de plus à l'entraînement · …", null for a v1 answer. */
    fun dataLine(g: BotGroupStat): String? = dataText(g)?.let { "Données : $it." }

    /** "nov. 2009 → oct. 2012 (4 fois)" or "nov. 2009". */
    fun runPeriod(x: SelectionRun): String =
        "${ModelValidation.monthYear(x.from)}${if (x.count > 1) " → ${ModelValidation.monthYear(x.to)} (${x.count} fois)" else ""}"

    /** The model of a run, "aucun (trop peu de données)" when none could be trained. */
    fun runChoice(x: SelectionRun): String = x.chosen?.let(::candidateShort) ?: "aucun (trop peu de données)"

    /** "Aujourd'hui : Logistique 24, choisi de la même façon sur la dernière année connue.", null for a v1 answer. */
    fun liveModelText(g: BotGroupStat): String? =
        g.model?.candidate?.let { "Aujourd'hui : ${candidateShort(it)}, choisi de la même façon sur la dernière année connue." }

    /** "retenu 4 fois sur 17". */
    fun candidateChosenText(c: BotCandidateStat): String = "retenu ${c.chosenBlocks} fois sur ${c.trainedBlocks}"

    /** Candidate rows: label to value. */
    fun candidateBuyRow(c: BotCandidateStat): Pair<String, String> =
        "${c.buy.signals} achats · écart au hasard" to "${points(c.buy.excess)} (t ${plain(c.buy.tStat, 1)})"

    fun candidateSellRow(c: BotCandidateStat): Pair<String, String> =
        "${c.sell.signals} ventes · baisse évitée" to "${points(c.sell.avoided)} (t ${plain(c.sell.tStat, 1)})"

    fun candidateSkillRow(c: BotCandidateStat): Pair<String, String> =
        "Précision hausse / baisse" to "${plain(c.brierSkillUp, 1)} % / ${plain(c.brierSkillDown, 1)} %"

    /** "Achats : avantage non démontré" (the verdict label with a lower-case first letter after the prefix). */
    fun prefixedVerdict(prefix: String, label: String): String = "$prefix : ${label.take(1).lowercase()}${label.drop(1)}"

    /** "29/09/2025" (UTC), "?" when unknown. */
    fun day(ms: Double?): String =
        if (ms == null) "?" else DateTimeFormatter.ofPattern("dd/MM/yyyy", Locale.FRANCE).withZone(ZoneOffset.UTC).format(Instant.ofEpochMilli(ms.toLong()))

    fun holdoutTitle(g: BotGroupStat): String = "${g.label} · 12 derniers mois"

    fun holdoutNote(h: BotHoldout): String = "Du ${day(h.from)} au ${day(h.to)}, présentés à part (même modèle choisi ; rien n'est choisi sur cette période)."

    fun extraTitle(g: BotGroupStat): String = "${g.label} · actifs d'entraînement hors panier"

    fun extraNote(g: BotGroupStat): String = "${g.universe?.extra ?: 0} actifs fixés d'avance, hors du test principal ; eux aussi jugés hors échantillon."

    /** Calibration row: "prévu 55 % · observé 57 %". */
    fun bucketText(b: BotBucket): String = "prévu ${pct0(b.predicted)} · observé ${pct0(b.realised)}"

    fun isPending(body: String): Boolean = Opportunities.isPending(body)
}
