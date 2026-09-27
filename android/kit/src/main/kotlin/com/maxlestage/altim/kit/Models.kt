package com.maxlestage.altim.kit

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

// Shapes of the Altim server's JSON (web/src/webapp/api.ts), same as the iPhone app's AltimKit. Every figure is
// computed on the server (median consensus of 40 sources); the app only displays it. Optional fields stay optional
// and unknown keys are ignored, so a server change never breaks decoding.

val AltimJson = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
    explicitNulls = false
}

@Serializable
enum class Kind {
    @SerialName("crypto") CRYPTO,
    @SerialName("stock") STOCK;

    val raw: String get() = if (this == CRYPTO) "crypto" else "stock"
    val label: String get() = if (this == CRYPTO) "Crypto" else "Action"

    companion object {
        fun of(raw: String?): Kind? = entries.firstOrNull { it.raw == raw }
    }
}

/** Colour meaning, mapped to the theme by the app. */
enum class Tone { GOOD, WARN, BAD, NEUTRAL }

@Serializable
enum class Action {
    @SerialName("strongBuy") STRONG_BUY,
    @SerialName("buy") BUY,
    @SerialName("hold") HOLD,
    @SerialName("sell") SELL,
    @SerialName("strongSell") STRONG_SELL;

    val label: String
        get() = when (this) {
            STRONG_BUY -> "ACHAT FORT"
            BUY -> "ACHAT"
            HOLD -> "ATTENDRE"
            SELL -> "VENTE"
            STRONG_SELL -> "VENTE FORTE"
        }
    val tone: Tone get() = when (this) { STRONG_BUY, BUY -> Tone.GOOD; HOLD -> Tone.NEUTRAL; else -> Tone.BAD }

    companion object {
        fun of(raw: String): Action? = when (raw) {
            "strongBuy" -> STRONG_BUY
            "buy" -> BUY
            "hold" -> HOLD
            "sell" -> SELL
            "strongSell" -> STRONG_SELL
            else -> null
        }
    }
}

@Serializable
data class Asset(val symbol: String, val kind: Kind, val name: String) {
    val id: String get() = "${kind.raw}:$symbol"

    companion object {
        val defaults = listOf(
            Asset("BTC", Kind.CRYPTO, "Bitcoin"),
            Asset("ETH", Kind.CRYPTO, "Ethereum"),
            Asset("SOL", Kind.CRYPTO, "Solana"),
            Asset("BNB", Kind.CRYPTO, "BNB"),
            Asset("XRP", Kind.CRYPTO, "XRP"),
            Asset("AAPL", Kind.STOCK, "Apple"),
            Asset("NVDA", Kind.STOCK, "NVIDIA"),
            Asset("MSFT", Kind.STOCK, "Microsoft"),
        )
    }
}

@Serializable
data class Reliability(val score: Double, val level: String, val independent: Int? = null, val conflict: Boolean? = null) {
    val label: String
        get() = when (level) {
            "high" -> "Fiabilité élevée"
            "medium" -> "Fiabilité moyenne"
            else -> "Fiabilité faible"
        }
    val tone: Tone get() = when (level) { "high" -> Tone.GOOD; "medium" -> Tone.WARN; else -> Tone.BAD }
}

@Serializable
data class SignalSummary(val action: Action, val score: Double, val confidence: Double)

@Serializable
data class RadarRow(
    val symbol: String,
    val kind: Kind,
    val name: String? = null,
    val price: Double? = null,
    val change: Double? = null,
    val priceSources: String? = null,
    val signal: SignalSummary? = null,
    val reliability: Reliability? = null,
    val agreeing: Int? = null,
    val sources: Int? = null,
    val sparkline: List<Double>? = null,
    val error: String? = null,
) {
    val id: String get() = "${kind.raw}:$symbol"
}

@Serializable
data class Quote(
    val symbol: String,
    val kind: Kind,
    val name: String,
    val price: Double,
    val change: Double? = null,
    val agreeing: Int,
    val total: Int,
    val sources: List<Source> = emptyList(),
) {
    @Serializable
    data class Source(val name: String, val ok: Boolean, val price: Double? = null, val error: String? = null)
}

@Serializable
data class SearchItem(val symbol: String, val name: String, val kind: Kind, val rank: Int? = null, val etf: Boolean? = null) {
    val id: String get() = "${kind.raw}:$symbol"
    val asset: Asset get() = Asset(symbol, kind, name)
}

@Serializable
data class Candle(val time: Double, val open: Double, val high: Double, val low: Double, val close: Double, val volume: Double? = null)

@Serializable
data class Snapshot(
    val symbol: String,
    val kind: Kind,
    val interval: String,
    val candles: List<Candle>,
    val source: String,
    val sources: List<Source> = emptyList(),
    val agreeing: Int,
    val reliability: Reliability,
) {
    @Serializable
    data class Source(val name: String, val ok: Boolean, val deviation: Double? = null, val error: String? = null)
}

// ---------- Guard (regime, shock, reversal) ----------

@Serializable
data class Evidence(val samples: Int, val rate: Double, val base: Double, val lift: Double)

@Serializable
data class GuardFactor(
    val code: String,
    val points: Double,
    val basePoints: Double? = null,
    val text: String,
    val status: String? = null,
    val evidence: Evidence? = null,
) {
    val statusText: String?
        get() = when (status) {
            "verified" -> "vérifié sur cet actif"
            "unproven" -> "peu d'historique : compté à moitié"
            "rejected" -> "jamais prédictif ici : ignoré"
            "unverifiable" -> "sans historique : non vérifié"
            else -> null
        }

    /** "+8 pts · peu d'historique… (31 % des 13 cas passés contre 18 % d'habitude)", as on the web. */
    val detail: String
        get() {
            var t = "+${Math.round(points)} pts"
            statusText?.let { t += " · $it" }
            evidence?.takeIf { it.samples > 0 }?.let { e ->
                t += " (${Math.round(e.rate)} % des ${e.samples} cas passés contre ${Math.round(e.base)} % d'habitude)"
            }
            return t
        }
}

@Serializable
data class GuardReport(
    val symbol: String,
    val kind: Kind,
    val price: Double? = null,
    val asOf: Double,
    val regime: Regime,
    val shock: Shock,
    val reversal: Reversal,
    val policy: Policy,
    val inputs: Inputs? = null,
    val macro: MacroInfo? = null,
) {
    @Serializable data class Regime(val trend: String, val strength: Double, val text: String)
    @Serializable data class Shock(val score: Double, val level: String, val factors: List<GuardFactor> = emptyList())
    @Serializable data class Reversal(val score: Double, val direction: String? = null, val factors: List<GuardFactor> = emptyList())
    @Serializable data class Policy(val scalping: String, val sizeMultiplier: Double, val stopMultiplier: Double, val notes: List<String> = emptyList())
    @Serializable data class Headline(val title: String, val time: Double, val source: String? = null)
    @Serializable data class Inputs(val fearGreed: Double? = null, val news24h: Int? = null, val headlines: List<Headline>? = null, val vix: Double? = null)

    val trendLabel: String get() = when (regime.trend) { "up" -> "Haussière"; "down" -> "Baissière"; else -> "Sans direction" }
    val shockLabel: String get() = when (shock.level) { "agitated" -> "Agité"; "shock" -> "Choc"; else -> "Calme" }
    val shockTone: Tone get() = when (shock.level) { "shock" -> Tone.BAD; "agitated" -> Tone.WARN; else -> Tone.GOOD }
    val policyLabel: String get() = when (policy.scalping) { "reduce" -> "Taille réduite"; "pause" -> "Suspendu"; else -> "Autorisé" }
}

// ---------- Macro ----------

@Serializable
data class MacroInfo(
    val score: Double,
    val level: String,
    val marketScore: Double? = null,
    val factors: List<Factor> = emptyList(),
    val themes: List<Theme> = emptyList(),
    val values: Map<String, Value> = emptyMap(),
    val asOf: Double? = null,
    val evidence: Evidence? = null,
) {
    @Serializable data class Factor(val code: String, val points: Double, val text: String)
    @Serializable data class Theme(val theme: String, val label: String, val count: Int, val examples: List<String> = emptyList())
    @Serializable data class Value(val value: Double, val change5d: Double)

    val levelLabel: String get() = when (level) { "tense" -> "Tendu"; "high" -> "Très tendu"; else -> "Calme" }
    val tone: Tone get() = when (level) { "high" -> Tone.BAD; "tense" -> Tone.WARN; else -> Tone.GOOD }

    companion object {
        /** Display order and French names of the market series. */
        val series = listOf(
            "vix" to "VIX (peur)", "spx" to "S&P 500", "oil" to "Pétrole", "gold" to "Or", "dollar" to "Dollar", "rates" to "Taux 10 ans",
        )
    }
}

// ---------- Fibonacci buy zones ----------

@Serializable
data class Band(val from: Double, val to: Double)

@Serializable
data class FibZone(
    val horizon: String,
    val label: String,
    val unit: String,
    val holding: String,
    val status: String,
    val levels: List<Level> = emptyList(),
    val zone: Band? = null,
    val golden: Band? = null,
    val invalidation: Double? = null,
    val targets: List<Double> = emptyList(),
    val distance: Double? = null,
    val evidence: Evidence? = null,
    val text: String,
    val macroNote: String? = null,
) {
    @Serializable data class Level(val ratio: Double, val price: Double)

    val statusLabel: String
        get() = when (status) {
            "above" -> "Attendre le repli"
            "inZone" -> "Dans la zone"
            "golden" -> "Zone d'or"
            "deep" -> "Repli profond"
            "broken" -> "Zone invalidée"
            "downtrend" -> "Tendance baissière"
            else -> "Pas de niveau net"
        }
    val tone: Tone
        get() = when (status) {
            "inZone", "golden" -> Tone.GOOD
            "above", "deep" -> Tone.WARN
            "broken", "downtrend" -> Tone.BAD
            else -> Tone.NEUTRAL
        }

    /** Same wording and thresholds as the web app (weigh(): 20 cases minimum, lift ≥ 1.1 to count). */
    val evidenceText: String
        get() {
            val e = evidence ?: return "Historique : aucun repli comparable sur cet actif pour cet horizon, zone non vérifiée."
            val verdict = when {
                e.samples < 20 -> "trop peu de cas pour conclure"
                e.lift < 1.1 -> "ces zones n'ont pas fait mieux qu'une entrée au hasard sur cet actif"
                else -> "ces zones ont mieux tenu qu'une entrée au hasard sur cet actif"
            }
            return "Historique : sur ${e.samples} repli${if (e.samples > 1) "s" else ""} dans la zone, ${Math.round(e.rate)} % sont remontés au plus haut avant de casser le plus bas, contre ${Math.round(e.base)} % pour une entrée au hasard : $verdict."
        }
}

@Serializable
data class ZonesReport(
    val symbol: String,
    val kind: Kind,
    val price: Double? = null,
    val asOf: Double,
    val zones: List<FibZone> = emptyList(),
    val macro: MacroInfo? = null,
)

// ---------- Selection (which stocks / cryptos to buy) ----------

@Serializable
enum class Horizon(val raw: String, val label: String) {
    @SerialName("30m") M30("30m", "30 min"),
    @SerialName("1h") H1("1h", "1 h"),
    @SerialName("5h") H5("5h", "5 h"),
    @SerialName("7d") D7("7d", "7 j"),
    @SerialName("14d") D14("14d", "14 j"),
    @SerialName("1m") MO1("1m", "1 mois"),
    @SerialName("3m") MO3("3m", "3 mois"),
    @SerialName("6m") MO6("6m", "6 mois");

    val isIntraday: Boolean get() = this == M30 || this == H1 || this == H5

    companion object {
        fun of(raw: String?): Horizon? = entries.firstOrNull { it.raw == raw }
    }
}

@Serializable
enum class Criterion(val raw: String) {
    @SerialName("signal") SIGNAL("signal"),
    @SerialName("trend") TREND("trend"),
    @SerialName("momentum") MOMENTUM("momentum"),
    @SerialName("zone") ZONE("zone"),
    @SerialName("risk") RISK("risk"),
}

@Serializable
data class Plan(val entry: Double, val limit: Double? = null, val stop: Double, val target: Double, val atrPct: Double)

@Serializable
data class Check(val label: String, val ok: Boolean, val detail: String)

@Serializable
data class Candidate(
    val rank: Int,
    val symbol: String,
    val name: String,
    val sector: String,
    val marketCap: Double? = null,
    val price: Double,
    val scores: Map<String, Double> = emptyMap(),
    val why: Map<String, String> = emptyMap(),
    val action: String,
    val zoneStatus: String,
    val plan: Plan? = null,
    val track: Track? = null,
    val checks: List<Check> = emptyList(),
    /** Only for the "à surveiller" list: why it did not make the cut. */
    val reason: String? = null,
) {
    @Serializable data class Track(val trades: Int, val winRate: Double, val avgReturn: Double)
}

@Serializable
data class SetAside(val symbol: String, val name: String, val reason: String)

@Serializable
data class Validation(
    val periods: Int,
    val top: Double,
    val universe: Double,
    val beatRate: Double,
    val topN: Int,
    val from: Double? = null,
    val to: Double? = null,
    val benchmark: Double? = null,
    val cost: Double,
    val edge: String,
    val noEdge: Boolean? = null,
)

@Serializable
data class SelectionReport(
    val market: Kind,
    val horizon: Horizon,
    val asOf: Double,
    val scanned: Int,
    val rankBy: Criterion,
    val rankRule: String,
    val holdText: String,
    val evidence: String,
    val marketClosed: Boolean = false,
    val criteria: Map<String, String> = emptyMap(),
    val roles: Map<String, String> = emptyMap(),
    val buy: List<Candidate> = emptyList(),
    val watch: List<Candidate> = emptyList(),
    val setAside: List<SetAside> = emptyList(),
    val validation: Validation? = null,
) {
    val rankText: String
        get() = when (rankRule) {
            "signal" -> "signal technique d'Altim"
            "momentum" -> "force relative, les plus en hausse d'abord"
            "reversal" -> "rebond, les plus en baisse d'abord"
            "lowRisk" -> "les plus calmes d'abord"
            else -> rankRule
        }

    /** Criteria in display order, the ranking one first. */
    val orderedCriteria: List<Criterion>
        get() = listOf(rankBy) + listOf(Criterion.MOMENTUM, Criterion.ZONE, Criterion.TREND, Criterion.RISK, Criterion.SIGNAL).filter { it != rankBy }

    /**
     * Amount for each pick: the budget is split so that each line risks the same sum if its stop is hit
     * (a volatile asset gets less money), and no line exceeds [maxPercent] of the budget (same rule as the web app).
     */
    fun allocate(budget: Double, maxPercent: Double = 20.0): Map<String, Double> {
        val withPlan = buy.mapNotNull { c -> c.plan?.let { c.symbol to it } }
        if (budget <= 0 || withPlan.isEmpty()) return emptyMap()
        val inv = withPlan.map { (_, p) -> 1 / maxOf(0.5, (1 - p.stop / (p.limit ?: p.entry)) * 100) }
        val sum = inv.sum()
        val cap = budget * maxPercent / 100
        return withPlan.mapIndexed { i, (s, _) -> s to minOf(cap, budget * inv[i] / sum) }.toMap()
    }
}

/** The screener answers 202 `{pending: true}` while the first computation runs (≈ 30 s). */
sealed interface SelectionResult {
    data class Ready(val report: SelectionReport) : SelectionResult
    data object Pending : SelectionResult
}

// ---------- Live prices (Server-Sent Events) ----------

@Serializable
data class LiveTick(
    val symbol: String,
    val kind: Kind,
    val price: Double,
    val change: Double? = null,
    val agreeing: Int? = null,
    val total: Int? = null,
    val sources: List<String>? = null,
    val time: Double,
    /** Stocks only: "open" / "closed" (New York session). */
    val market: String? = null,
) {
    val key: String get() = "${kind.raw}:$symbol"
}
