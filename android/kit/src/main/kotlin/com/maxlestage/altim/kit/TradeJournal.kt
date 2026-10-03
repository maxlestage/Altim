package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

// Automatic trading journal: every simulated purchase (paper) and every real purchase or sale recorded in « Mes avoirs »
// is written down with WHY it was taken (the decision shown at that moment), the price, the stop, the targets, the
// signal used and the market conditions. Everything stays on the phone (like the holdings).
//
// Later, when the journal is opened, each entry is reviewed on the daily candles AFTER the entry (never the entry day
// itself: its low may be earlier than the purchase, same rule as Paper), at 3, 10 and 30 calendar days: max favourable /
// adverse excursion, whether the stop or a target was reached first (a candle reaching both counts as the stop: the
// worst case), result against the plan in R (multiples of the risk taken: entry − stop), facts on what worked or not,
// and whether the entry was coherent with the data available at that moment. The profile groups the results by rating
// at entry, plan respected or not, and market regime, in R and drawdown.
// Exact port of web/src/engine/journal.ts (same texts, same rules), tested with the same cases (TradeJournalTest ↔
// web/test/journal.test.ts). Pure, deterministic functions. Named "trade journal" here: the alerts already have a journal.

/** The decision shown when the entry was written (a snapshot: the full decision is not kept). */
@Serializable
data class JournalDecision(
    val asOf: Double,
    val verdict: String,
    val label: String,
    val rating: String? = null,
    val ratingLabel: String? = null,
    val levelLabel: String = "",
    val headline: String = "",
    val confidence: Double = 0.0,
    /** Composite score /100 (null: not computed or older server). */
    val score: Double? = null,
    /** First three pros / cons. */
    val pros: List<String> = emptyList(),
    val cons: List<String> = emptyList(),
    val degraded: Boolean = false,
    val degradedHeadline: String? = null,
    val setup: Setup = Setup(),
    /** Labels of the buy vetoes active at that moment. */
    val vetoes: List<String> = emptyList(),
    val plan: Plan? = null,
    val horizon: String? = null,
) {
    @Serializable
    data class Step(val label: String = "", val state: String = "unknown")

    @Serializable
    data class Setup(val name: String = "", val met: Int = 0, val total: Int = 0, val steps: List<Step> = emptyList())

    @Serializable
    data class Plan(
        val zoneFrom: Double,
        val zoneTo: Double,
        val entry: Double,
        val stop: Double,
        val target1: Double,
        val target2: Double? = null,
        val riskReward: Double = 0.0,
        val minRiskReward: Double = 2.0,
        val acceptable: Boolean = false,
    )
}

/** Market conditions at the entry; null = not known (not loaded, source failed). */
@Serializable
data class JournalMarket(
    /** "riskOn" | "neutral" | "riskOff". */
    val regime: String? = null,
    val regimeLabel: String? = null,
    /** Macro stress /100 and its level (calm / tense / high), from /api/macro. */
    val macroScore: Double? = null,
    val macroLevel: String? = null,
    /** Daily ATR (14) in % of the price, on the daily candles closed before the entry. */
    val atrPct: Double? = null,
    /** Volume of the last closed day ÷ average of the 20 before (or the decision's liquidity figure). */
    val relativeVolume: Double? = null,
    /** Events of the next 7 days in the decision (economy, central banks, company). */
    val events: Int? = null,
)

@Serializable
data class TradeEntry(
    val id: String,
    val createdAt: Double,
    /** "paper" | "real". */
    val source: String,
    /** "buy" | "sell". */
    val side: String,
    val symbol: String,
    val kind: Kind,
    val name: String = "",
    /** Price of the purchase or sale (paper: fill price with slippage; real: price entered or live price). */
    val price: Double,
    val quantity: Double? = null,
    val amount: Double? = null,
    /** The user's own stop and target(s) (paper order, holding's stop); the decision's plan is kept apart. */
    val stop: Double? = null,
    val targets: List<Double> = emptyList(),
    /** "Signal utilisé" in words. */
    val signal: String = "",
    /** "Pourquoi je suis entré" (optional). */
    val note: String = "",
    /** Paper position or holding line this entry is about. */
    val refId: String? = null,
    val decision: JournalDecision? = null,
    val market: JournalMarket,
) {
    val buy: Boolean get() = side == "buy"
    val asset: Asset get() = Asset(symbol, kind, name.ifEmpty { symbol })
}

@Serializable
data class TradeJournalState(val version: Int = 1, val entries: List<TradeEntry> = emptyList())

/** A new entry from what the screen knows at that moment (the decision already loaded, if any). */
data class NewTradeEntry(
    val id: String,
    val now: Double,
    val source: String,
    val side: String,
    val symbol: String,
    val kind: Kind,
    val name: String,
    val price: Double,
    val quantity: Double? = null,
    val amount: Double? = null,
    val stop: Double? = null,
    val targets: List<Double?> = emptyList(),
    val note: String = "",
    val refId: String? = null,
    val decision: Decision? = null,
)

/** A purchase or sale read from a holding edit. */
data class HoldingChange(val side: String, val quantity: Double, val price: Double, val implied: Boolean)

/** Stop and targets the review uses: the user's own, else the plan of the decision shown at the entry. source: "user" | "plan" | "none". */
data class JournalLevels(val stop: Double?, val stopSource: String, val targets: List<Double>, val targetsSource: String)

data class HorizonReview(
    val days: Int,
    /** "pending": not reached yet; "noData": no candle after the entry in the history; "ready": computed. */
    val status: String,
    val availableAt: Double,
    val candles: Int = 0,
    val lastClose: Double? = null,
    /** Price change since the entry (a sale: since the sale), %. */
    val returnPct: Double? = null,
    /** Max favourable / adverse excursion since the entry, %. */
    val mfePct: Double? = null,
    val maePct: Double? = null,
    val mfeR: Double? = null,
    val maeR: Double? = null,
    /** "stop" | "target1" | "none": first level reached and after how many days (calendar days since the entry). */
    val first: String = "none",
    val firstDays: Int? = null,
    /** Target 2 reached before the stop within the window. */
    val target2: Boolean = false,
    /** Result following the plan (exit at the first level reached, else the last close), in R; null without stop. */
    val resultR: Double? = null,
)

/** A coherence check: ok null = not verifiable. */
data class JournalCheck(val code: String, val label: String, val ok: Boolean?, val detail: String)

data class ClosedInfo(val at: Double, val price: Double, val reason: String)

data class EntryReview(
    val entry: TradeEntry,
    val levels: JournalLevels,
    /** Risk per unit (entry − stop), null without stop. */
    val risk: Double?,
    /** Planned reward at target 1, in R. */
    val planR: Double?,
    val horizons: List<HorizonReview>,
    /** Most advanced ready horizon (the facts are built on it). */
    val latest: HorizonReview?,
    val worked: List<String>,
    val failed: List<String>,
    val coherence: List<JournalCheck>,
    /** true: every verifiable check passes; false: at least one fails; null: nothing verifiable. */
    val coherent: Boolean?,
    /** Plan respected at the entry (same checks). */
    val planRespected: Boolean?,
    val closed: ClosedInfo?,
    val realizedR: Double?,
)

data class GroupStat(
    val key: String,
    val label: String,
    /** Entries reviewed at this horizon. */
    val n: Int,
    /** Entries with a stop (results in R). */
    val withStop: Int,
    val lowSample: Boolean,
    val avgR: Double?,
    val medianR: Double?,
    /** Share of results above 0 R (or above 0 % without stop), %. */
    val winRate: Double?,
    /** Average and worst adverse excursion (drawdown during the trade), in R. */
    val avgMaeR: Double?,
    val worstMaeR: Double?,
    val avgReturnPct: Double?,
    val avgMaePct: Double?,
)

data class JournalProfile(
    val horizon: Int,
    /** Purchases reviewed at this horizon / all purchases. */
    val reviewed: Int,
    val purchases: Int,
    val pending: Int,
    val all: GroupStat?,
    val byRating: List<GroupStat>,
    val byPlan: List<GroupStat>,
    val byRegime: List<GroupStat>,
)

/** The saved journal and, when it could not be read, why (never a crash). */
data class SavedJournal(val state: TradeJournalState, val error: String?)

object TradeJournal {
    const val DAY = 86_400_000.0
    val REVIEW_DAYS = listOf(3, 10, 30)
    /** Under this many entries a group's figures are shown with "échantillon trop faible". */
    const val MIN_SAMPLE = 5
    /** Beyond this age the decision is not "the one of the moment" anymore. */
    const val STALE_DECISION_MS = 24 * 3_600_000.0
    const val MAX_ENTRIES = 500
    /** Storage key on this phone (the alerts' journal is "journal"). */
    const val KEY = "tradeJournal.v1"
    /** A decision seen on the phone is used for an entry when it is younger than this. */
    const val DECISION_MAX_AGE = 6 * 3_600_000.0
    private val BUYING = setOf("buy", "buyZone")

    private val Strict = Json { ignoreUnknownKeys = true; encodeDefaults = true }

    /** Math.round of JavaScript to [d] decimals. */
    private fun round(v: Double, d: Int = 4): Double {
        val k = 10.0.pow(d)
        return floor(v * k + 0.5) / k
    }

    private fun fr(v: Double, d: Int = 1) = JsFormat.fr(v, d)
    private fun signed(v: Double, d: Int = 1) = "${if (v > 0) "+" else if (v < 0) "−" else ""}${fr(abs(v), d)}"
    private fun plural(n: Int, w: String) = "$n $w${if (n > 1) "s" else ""}"

    fun empty() = TradeJournalState()

    // ---------- Writing an entry ----------

    fun verdictRaw(v: Verdict): String = when (v) {
        Verdict.BUY -> "buy"
        Verdict.BUY_ZONE -> "buyZone"
        Verdict.WAIT -> "wait"
        Verdict.NO_POSITION -> "noPosition"
        Verdict.TRIM -> "trim"
        Verdict.SELL -> "sell"
    }

    fun ratingRaw(r: Rating): String = when (r) {
        Rating.STRONG_BUY -> "strongBuy"
        Rating.BUY -> "buy"
        Rating.HOLD -> "hold"
        Rating.REDUCE -> "reduce"
        Rating.SELL -> "sell"
        Rating.STRONG_SELL -> "strongSell"
    }

    fun regimeRaw(k: RegimeKind): String = when (k) {
        RegimeKind.RISK_ON -> "riskOn"
        RegimeKind.NEUTRAL -> "neutral"
        RegimeKind.RISK_OFF -> "riskOff"
    }

    private fun stepRaw(s: StepState) = when (s) {
        StepState.OK -> "ok"
        StepState.NO -> "no"
        StepState.UNKNOWN -> "unknown"
    }

    /** Snapshot of the decision (only what the journal shows and checks). */
    fun snapshotDecision(d: Decision): JournalDecision {
        val p = d.plan
        val s = d.setup
        return JournalDecision(
            asOf = d.asOf,
            verdict = verdictRaw(d.verdict),
            label = d.verdictLabel,
            rating = d.rating?.let(::ratingRaw),
            ratingLabel = d.rating?.let { r -> d.ratingLabel.ifBlank { r.label } },
            levelLabel = d.levelText,
            headline = d.headline,
            confidence = d.confidence,
            score = d.score?.value,
            pros = d.pros.take(3),
            cons = d.cons.take(3),
            degraded = d.degraded?.active == true,
            degradedHeadline = d.degraded?.takeIf { it.active }?.headline,
            setup = JournalDecision.Setup(s?.name ?: "", s?.met ?: 0, s?.total ?: 0, s?.steps.orEmpty().map { JournalDecision.Step(it.label, stepRaw(it.state)) }),
            vetoes = d.vetoes.filter { it.active }.map { it.label },
            plan = p?.let { JournalDecision.Plan(it.zoneFrom, it.zoneTo, it.entry, it.stop, it.target1, it.target2, it.riskReward, it.minRiskReward, it.acceptable) },
            horizon = d.horizon?.label?.takeIf { it.isNotEmpty() } ?: p?.horizon?.takeIf { it.isNotEmpty() },
        )
    }

    /** "Signal utilisé": the decision, its level and the setup's progress. */
    fun signalText(d: JournalDecision?): String {
        if (d == null) return "Aucune décision chargée pour cet actif à ce moment-là"
        val parts = mutableListOf("${d.ratingLabel ?: d.label} (${d.levelLabel})", "configuration « ${d.setup.name} » ${d.setup.met}/${d.setup.total}")
        d.horizon?.let { parts += "horizon $it" }
        return parts.joinToString(" · ")
    }

    /** Market conditions the decision already carries (regime, relative volume, events). */
    fun marketFromDecision(d: Decision?): JournalMarket {
        if (d == null) return JournalMarket()
        return JournalMarket(
            regime = d.marketRegime?.let { regimeRaw(it.kind) },
            regimeLabel = d.marketRegime?.label,
            relativeVolume = d.liquidity?.relativeVolume,
            events = d.events?.size,
        )
    }

    /**
     * ATR % and relative volume on the daily candles CLOSED before [at] (no look-ahead: the entry day's candle is
     * excluded). null when the history is too short.
     */
    fun marketFromCandles(candles: List<Candle>, at: Double): Pair<Double?, Double?> {
        val before = candles.filter { it.time + DAY <= at && it.close > 0 }.sortedBy { it.time }
        val last = before.lastOrNull()
        val a = if (before.size >= 15) PortfolioRisk.atr(before)[before.size - 1] else null
        val prev = before.dropLast(1).takeLast(20).mapNotNull { it.volume }.filter { it.isFinite() && it > 0 }
        val avg = if (prev.size >= 20) prev.sum() / prev.size else 0.0
        val lastVolume = last?.volume ?: 0.0
        return Pair(
            if (a != null && last != null) round(a / last.close * 100, 3) else null,
            if (last != null && avg > 0 && lastVolume > 0) round(lastVolume / avg, 3) else null,
        )
    }

    private fun ok(v: Double?): Boolean = v != null && v.isFinite() && v > 0

    fun createEntry(n: NewTradeEntry): TradeEntry {
        val decision = n.decision?.let(::snapshotDecision)
        val buy = n.side == "buy"
        return TradeEntry(
            id = n.id,
            createdAt = n.now,
            source = n.source,
            side = n.side,
            symbol = n.symbol,
            kind = n.kind,
            name = n.name,
            price = n.price,
            quantity = n.quantity?.takeIf(::ok),
            amount = n.amount?.takeIf(::ok),
            stop = if (buy && ok(n.stop) && n.stop!! < n.price) n.stop else null,
            targets = if (buy) n.targets.filterNotNull().filter(::ok).filter { it > n.price }.sorted() else emptyList(),
            signal = signalText(decision),
            note = n.note.trim().take(1000),
            refId = n.refId,
            decision = decision,
            market = marketFromDecision(n.decision),
        )
    }

    /**
     * A holding line edited in « Mes avoirs » → the purchase or sale it records (null: no quantity change, or no usable
     * price). Purchase price: the one implied by the new average cost ((q2 × PRU2 − q1 × PRU1) ÷ (q2 − q1)), else the
     * live price; a sale is priced at the live price (the average cost does not change on a sale). Without an average
     * cost on either side (optional on the phone), the live price.
     */
    fun holdingChange(beforeQty: Double, beforeAvg: Double?, afterQty: Double, afterAvg: Double?, livePrice: Double?): HoldingChange? {
        val dq = afterQty - beforeQty
        if (!(abs(dq) > 1e-12)) return null
        val live = livePrice?.takeIf { it.isFinite() && it > 0 }
        if (dq < 0) return live?.let { HoldingChange("sell", -dq, it, false) }
        if (beforeAvg != null && afterAvg != null) {
            val implied = (afterQty * afterAvg - beforeQty * beforeAvg) / dq
            if (afterAvg != beforeAvg && implied.isFinite() && implied > 0) return HoldingChange("buy", dq, implied, true)
        }
        return live?.let { HoldingChange("buy", dq, it, false) }
    }

    fun addEntry(s: TradeJournalState, e: TradeEntry): TradeJournalState =
        TradeJournalState(1, (s.entries.filter { it.id != e.id } + e).sortedBy { it.createdAt }.takeLast(MAX_ENTRIES))

    /** Completes an entry (market data arrived after it was written, the user's note). Only the non-null fields of [market] are applied. */
    fun patchEntry(s: TradeJournalState, id: String, note: String? = null, market: JournalMarket? = null, decision: JournalDecision? = null): TradeJournalState =
        TradeJournalState(
            1,
            s.entries.map { e ->
                if (e.id != id) e
                else {
                    val m = e.market
                    val merged = if (market == null) m else JournalMarket(
                        regime = market.regime ?: m.regime,
                        regimeLabel = market.regimeLabel ?: m.regimeLabel,
                        macroScore = market.macroScore ?: m.macroScore,
                        macroLevel = market.macroLevel ?: m.macroLevel,
                        atrPct = market.atrPct ?: m.atrPct,
                        relativeVolume = market.relativeVolume ?: m.relativeVolume,
                        events = market.events ?: m.events,
                    )
                    val d = decision ?: e.decision
                    e.copy(
                        note = note?.trim()?.take(1000) ?: e.note,
                        market = merged,
                        decision = d,
                        signal = if (decision != null) signalText(d) else e.signal,
                    )
                }
            },
        )

    fun removeEntry(s: TradeJournalState, id: String) = TradeJournalState(1, s.entries.filter { it.id != id })

    // ---------- Storage (this phone only) ----------

    /** Saved journal: empty, valid, or unreadable (ignored with a message, never a crash). */
    fun parseSaved(raw: String?): SavedJournal {
        if (raw.isNullOrEmpty()) return SavedJournal(empty(), null)
        val s = runCatching { Strict.decodeFromString(TradeJournalState.serializer(), raw) }.getOrNull()
        val valid = s != null && s.version == 1 && s.entries.all { e ->
            e.createdAt.isFinite() && e.price.isFinite() && e.price > 0 && (e.side == "buy" || e.side == "sell")
        }
        return if (valid) SavedJournal(s!!, null)
        else SavedJournal(empty(), "Le journal enregistré sur ce téléphone est illisible : il a été ignoré.")
    }

    fun encode(s: TradeJournalState): String = Strict.encodeToString(TradeJournalState.serializer(), s)

    // ---------- Review ----------

    /** Stop and targets the review uses: the user's own, else the plan of the decision shown at the entry. */
    fun levelsOf(e: TradeEntry): JournalLevels {
        val p = e.decision?.plan
        val stop = e.stop ?: p?.stop?.takeIf { it < e.price }
        val planTargets = if (p != null) listOfNotNull(p.target1, p.target2).filter { it > e.price } else emptyList()
        val targets = e.targets.ifEmpty { planTargets }
        return JournalLevels(
            stop,
            if (e.stop != null) "user" else if (stop != null) "plan" else "none",
            targets,
            if (e.targets.isNotEmpty()) "user" else if (targets.isNotEmpty()) "plan" else "none",
        )
    }

    /** Checks of the entry against the data available at that moment (a sale is checked against the decision only). */
    fun coherenceChecks(e: TradeEntry, levels: JournalLevels = levelsOf(e)): List<JournalCheck> {
        val d = e.decision ?: return listOf(
            JournalCheck("decision", "Décision disponible", null, "Aucune décision chargée pour cet actif à ce moment-là : cohérence non vérifiable."),
        )
        val out = mutableListOf<JournalCheck>()
        val age = e.createdAt - d.asOf
        out += if (age > STALE_DECISION_MS) {
            JournalCheck("fresh", "Décision récente", false, "La décision affichée datait de ${plural(Math.round(age / 3_600_000).toInt(), "heure")} : les données avaient pu changer.")
        } else JournalCheck("fresh", "Décision récente", true, "Décision calculée moins de 24 h avant.")
        if (e.side == "sell") {
            val selling = d.verdict == "trim" || d.verdict == "sell"
            out += JournalCheck(
                "verdict", "Vente conforme à la décision", selling,
                if (selling) "Décision « ${d.label} » : la vente allait dans son sens." else "Décision « ${d.label} » : la vente allait contre elle.",
            )
            return out
        }
        val buying = d.verdict in BUYING
        out += JournalCheck("verdict", "Achat conforme à la décision", buying, if (buying) "Décision « ${d.label} »." else "Achat alors que la décision était « ${d.label} ».")
        out += if (d.vetoes.isNotEmpty()) {
            JournalCheck(
                "vetoes", "Aucune interdiction d'achat active", false,
                "Entrée malgré ${if (d.vetoes.size > 1) "des interdictions d'achat actives" else "une interdiction d'achat active"} : ${d.vetoes.joinToString(", ")}.",
            )
        } else JournalCheck("vetoes", "Aucune interdiction d'achat active", true, "Aucune interdiction d'achat active.")
        out += if (d.degraded) JournalCheck("degraded", "Signal non dégradé", false, d.degradedHeadline ?: "Le signal était dégradé à l'entrée.")
        else JournalCheck("degraded", "Signal non dégradé", true, "Signal complet à l'entrée.")
        val p = d.plan
        if (p == null) {
            out += JournalCheck("rr", "Rapport gain / risque ≥ 2", null, "Pas de plan chiffré dans la décision.")
            out += JournalCheck("zone", "Entrée dans la zone d'achat", null, "Pas de zone d'achat dans la décision.")
        } else {
            val min = if (p.minRiskReward > 0) p.minRiskReward else 2.0
            out += if (p.riskReward >= min) JournalCheck("rr", "Rapport gain / risque ≥ ${fr(min)}", true, "Rapport du plan : ${fr(p.riskReward)}.")
            else JournalCheck("rr", "Rapport gain / risque ≥ ${fr(min)}", false, "Rapport du plan : ${fr(p.riskReward)}, sous le minimum de ${fr(min)}.")
            val lo = min(p.zoneFrom, p.zoneTo)
            val hi = max(p.zoneFrom, p.zoneTo)
            out += when {
                e.price > hi -> JournalCheck("zone", "Entrée dans la zone d'achat", false, "Entrée hors zone d'achat : ${signed((e.price / hi - 1) * 100)} % au-dessus du haut de la zone.")
                e.price < lo -> JournalCheck("zone", "Entrée dans la zone d'achat", false, "Entrée hors zone d'achat : ${fr((1 - e.price / lo) * 100)} % sous la zone (support peut-être cassé).")
                else -> JournalCheck("zone", "Entrée dans la zone d'achat", true, "Entrée dans la zone d'achat.")
            }
        }
        out += if (levels.stop == null) JournalCheck("stop", "Stop défini", false, "Aucun stop : risque non borné, résultat en R non mesurable.")
        else JournalCheck("stop", "Stop défini", true, if (levels.stopSource == "user") "Stop fixé à l'entrée." else "Pas de stop saisi : celui du plan de la décision est utilisé.")
        val r = e.market.regime
        out += when (r) {
            null -> JournalCheck("regime", "Pas contre le régime de marché", null, "Régime de marché inconnu à l'entrée.")
            "riskOff" -> JournalCheck("regime", "Pas contre le régime de marché", false, "Achat en régime ${e.market.regimeLabel ?: "risk-off"} : à contre-courant du marché.")
            else -> JournalCheck("regime", "Pas contre le régime de marché", true, "Régime ${e.market.regimeLabel ?: r}.")
        }
        return out
    }

    private fun horizonReview(e: TradeEntry, days: Int, after: List<Candle>, now: Double, levels: JournalLevels, risk: Double?): HorizonReview {
        val availableAt = e.createdAt + days * DAY
        val base = HorizonReview(days, "pending", availableAt)
        if (now < availableAt) return base
        val w = after.filter { it.time <= availableAt }
        if (w.isEmpty()) return base.copy(status = "noData")
        val last = w.last()
        // Excursions are measured from the entry: never below it for the favourable one, never above it for the adverse one.
        val hi = max(e.price, w.maxOf { it.high })
        val lo = min(e.price, w.minOf { it.low })
        fun r(p: Double): Double? = risk?.takeIf { it != 0.0 }?.let { round((p - e.price) / it, 3) }
        var first = "none"
        var firstDays: Int? = null
        var exit: Double? = null
        var target2 = false
        if (e.side == "buy") {
            val t1 = levels.targets.getOrNull(0)
            val t2 = levels.targets.getOrNull(1)
            for (c in w) {
                val stopHit = levels.stop != null && c.low <= levels.stop
                if (stopHit) {
                    if (first == "none") {
                        first = "stop"
                        firstDays = max(1, ceil((c.time - e.createdAt) / DAY).toInt())
                        exit = min(levels.stop!!, c.open)
                    }
                    break
                }
                if (t1 != null && c.high >= t1 && first == "none") {
                    first = "target1"
                    firstDays = max(1, ceil((c.time - e.createdAt) / DAY).toInt())
                    exit = t1
                }
                if (t2 != null && c.high >= t2) target2 = true
            }
        }
        val mark = exit ?: last.close
        return base.copy(
            status = "ready",
            candles = w.size,
            lastClose = last.close,
            returnPct = round((last.close / e.price - 1) * 100, 3),
            mfePct = round((hi / e.price - 1) * 100, 3),
            maePct = round((lo / e.price - 1) * 100, 3),
            mfeR = r(hi),
            maeR = r(lo),
            first = first,
            firstDays = firstDays,
            target2 = target2,
            resultR = if (e.side == "buy") r(mark) else null,
        )
    }

    /**
     * Review of one entry on its asset's daily candles (any order). [closed]: the position was actually closed (paper
     * trade, recorded sale) — its realized result in R is added.
     */
    fun reviewEntry(e: TradeEntry, candles: List<Candle>, now: Double, closed: ClosedInfo? = null): EntryReview {
        val levels = levelsOf(e)
        val risk = levels.stop?.takeIf { e.price > it }?.let { e.price - it }
        val planR = if (risk != null && levels.targets.isNotEmpty()) round((levels.targets[0] - e.price) / risk, 3) else null
        // Candles starting after the entry (the entry day's own candle may predate it).
        // A history starting after the entry would leave a hole at its start: nothing is computed then.
        val covers = candles.any { it.time <= e.createdAt }
        val after = if (covers) candles.filter { it.time > e.createdAt && it.low > 0 && it.high >= it.low }.sortedBy { it.time } else emptyList()
        val horizons = REVIEW_DAYS.map { horizonReview(e, it, after, now, levels, risk) }
        val latest = horizons.lastOrNull { it.status == "ready" }
        val coherence = coherenceChecks(e, levels)
        val verifiable = coherence.filter { it.ok != null }
        val coherent = if (verifiable.isNotEmpty()) verifiable.all { it.ok == true } else null
        val realizedR = if (closed != null && risk != null) round((closed.price - e.price) / risk, 3) else null
        val (worked, failed) = facts(e, latest, coherence, levels, planR, closed, realizedR)
        return EntryReview(e, levels, risk, planR, horizons, latest, worked, failed, coherence, coherent, if (e.side == "buy") coherent else null, closed, realizedR)
    }

    fun rText(r: Double) = "${signed(r)} R"

    /** "Qu'est-ce qui a fonctionné ? / Qu'est-ce qui n'a pas fonctionné ?" — facts only, from the review. */
    private fun facts(
        e: TradeEntry, h: HorizonReview?, checks: List<JournalCheck>, levels: JournalLevels, planR: Double?, closed: ClosedInfo?, realizedR: Double?,
    ): Pair<List<String>, List<String>> {
        val worked = mutableListOf<String>()
        val failed = mutableListOf<String>()
        fun failing(code: String) = checks.firstOrNull { it.code == code && it.ok == false }
        if (e.side == "sell") {
            if (h?.returnPct != null) {
                val txt = "${h.days} jours après la vente, le cours est à ${signed(h.returnPct)} % du prix de vente (plus haut ${signed(h.mfePct!!)} %, plus bas ${signed(h.maePct!!)} %)."
                (if (h.returnPct <= 0) worked else failed) += txt
            }
            failing("verdict")?.let { failed += it.detail }
            return worked to failed
        }
        if (h != null) {
            when {
                h.first == "target1" ->
                    worked += "L'objectif 1 a été atteint en ${plural(h.firstDays!!, "jour")}${h.resultR?.let { " (${rText(it)})" } ?: ""}${if (h.target2) ", puis l'objectif 2" else ""}."
                h.first == "stop" -> {
                    val why = listOfNotNull(
                        failing("degraded")?.let { "le signal était dégradé à l'entrée" },
                        failing("vetoes")?.let { "des interdictions d'achat étaient actives" },
                        failing("zone")?.let { "l'entrée était hors zone d'achat" },
                    )
                    failed += "Le stop a été touché en ${plural(h.firstDays!!, "jour")}${h.resultR?.let { " (${rText(it)})" } ?: ""}${if (why.isNotEmpty()) " alors que ${why.joinToString(" et que ")}" else ""}."
                }
                h.resultR != null -> {
                    val txt = "Ni stop ni objectif en ${h.days} jours : ${rText(h.resultR)} au dernier cours."
                    (if (h.resultR > 0) worked else failed) += txt
                }
                h.returnPct != null -> {
                    val txt = "${signed(h.returnPct)} % en ${h.days} jours (sans stop, résultat en R non mesurable)."
                    (if (h.returnPct > 0) worked else failed) += txt
                }
            }
            if (h.first != "target1" && h.mfeR != null && h.mfeR >= 1 && (h.resultR ?: 0.0) < 0) {
                failed += "Le cours est monté jusqu'à ${rText(h.mfeR)} avant de repasser sous l'entrée : le gain latent n'a pas été conservé."
            }
            if (h.first != "stop" && h.maeR != null && h.maeR <= -0.8) failed += "Recul jusqu'à ${rText(h.maeR)} : le stop a failli être touché."
            if (h.first != "stop" && h.maeR != null && h.maeR > -0.3 && h.resultR != null && h.resultR > 0) worked += "Recul limité à ${rText(h.maeR)} depuis l'entrée."
        }
        if (closed != null && realizedR != null) {
            (if (realizedR > 0) worked else failed) += "Position clôturée (${closed.reason}) : ${rText(realizedR)} réalisé${planR?.let { " pour ${rText(it)} prévu à l'objectif 1" } ?: ""}."
        }
        for (code in listOf("zone", "rr", "regime", "verdict", "stop")) failing(code)?.let { failed += it.detail }
        val zone = checks.firstOrNull { it.code == "zone" && it.ok == true }
        if (zone != null && checks.all { it.ok != false }) worked += "Entrée dans la zone d'achat, plan respecté (aucune interdiction, signal complet, rapport gain / risque suffisant)."
        if (levels.stopSource == "plan") failed += "Aucun stop saisi : la revue utilise le stop du plan de la décision."
        return worked to failed
    }

    // ---------- Profile ----------

    private fun mean(v: List<Double>): Double? = if (v.isEmpty()) null else round(v.sum() / v.size, 3)

    private fun median(v: List<Double>): Double? {
        if (v.isEmpty()) return null
        val s = v.sorted()
        val m = s.size / 2
        return round(if (s.size % 2 == 1) s[m] else (s[m - 1] + s[m]) / 2, 3)
    }

    private fun stat(key: String, label: String, rows: List<HorizonReview>): GroupStat {
        val inR = rows.filter { it.resultR != null }
        val rs = inR.map { it.resultR!! }
        val wins = rows.count { (it.resultR ?: it.returnPct ?: 0.0) > 0 }
        val maes = inR.mapNotNull { it.maeR }
        return GroupStat(
            key, label, rows.size, inR.size, rows.size < MIN_SAMPLE,
            mean(rs), median(rs),
            if (rows.isNotEmpty()) round(wins.toDouble() / rows.size * 100, 2) else null,
            mean(maes), maes.minOrNull(),
            mean(rows.mapNotNull { it.returnPct }),
            mean(rows.mapNotNull { it.maePct }),
        )
    }

    private val REGIME_LABEL = mapOf("riskOn" to "Risk-on", "neutral" to "Neutre", "riskOff" to "Risk-off", "unknown" to "Régime inconnu")
    private val RATING_ORDER = listOf("strongBuy", "buy", "hold", "reduce", "sell", "strongSell")

    /**
     * Results of the purchases at one horizon, grouped by rating at entry, plan respected or not, and market regime.
     * Groups are in R and drawdown; ordered by a fixed scale, never by performance.
     */
    fun profile(reviews: List<EntryReview>, horizon: Int): JournalProfile {
        val buys = reviews.filter { it.entry.side == "buy" }
        val rows = buys.mapNotNull { r -> r.horizons.firstOrNull { it.days == horizon }?.takeIf { it.status == "ready" }?.let { r to it } }
        fun group(keyOf: (EntryReview) -> Pair<String, String>, order: (String) -> Int): List<GroupStat> {
            val m = LinkedHashMap<String, Pair<String, MutableList<HorizonReview>>>()
            for ((r, h) in rows) {
                val (k, label) = keyOf(r)
                m.getOrPut(k) { label to mutableListOf() }.second += h
            }
            return m.map { (k, g) -> stat(k, g.first, g.second) }.sortedWith(compareBy<GroupStat> { order(it.key) }.thenBy { it.key })
        }
        return JournalProfile(
            horizon = horizon,
            reviewed = rows.size,
            purchases = buys.size,
            pending = buys.count { r -> r.horizons.firstOrNull { it.days == horizon }?.status == "pending" },
            all = if (rows.isNotEmpty()) stat("all", "Tous les achats", rows.map { it.second }) else null,
            byRating = group(
                { r -> r.entry.decision?.let { d -> (d.rating ?: d.verdict) to (d.ratingLabel ?: d.label) } ?: ("none" to "Sans décision") },
                { k -> if (k in RATING_ORDER) RATING_ORDER.indexOf(k) else if (k == "none") 99 else 50 },
            ),
            byPlan = group(
                { r -> when (r.planRespected) { null -> "unknown" to "Non vérifiable"; true -> "yes" to "Plan respecté"; false -> "no" to "Plan non respecté" } },
                { k -> listOf("yes", "no", "unknown").indexOf(k) },
            ),
            byRegime = group(
                { r -> (r.entry.market.regime ?: "unknown").let { k -> k to (REGIME_LABEL[k] ?: k) } },
                { k -> listOf("riskOn", "neutral", "riskOff", "unknown").indexOf(k) },
            ),
        )
    }
}
