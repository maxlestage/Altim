package com.maxlestage.altim.kit

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonContentPolymorphicSerializer
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonTransformingSerializer
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.util.Locale

// Answer of GET /api/decision (backend/src/engine/decision_types.rs): one decision per asset, its reasons, what would
// change it and what could make it wrong. Tolerant like the rest of the kit: every enum has a default, so a value the
// server adds later ("coerceInputValues") falls back to it instead of breaking the whole answer.

@Serializable
enum class Verdict(val label: String) {
    @SerialName("buy") BUY("ACHETER"),
    @SerialName("buyZone") BUY_ZONE("ZONE D'ACHAT"),
    @SerialName("wait") WAIT("ATTENDRE"),
    @SerialName("noPosition") NO_POSITION("AUCUNE POSITION"),
    @SerialName("trim") TRIM("ALLÉGER"),
    @SerialName("sell") SELL("VENDRE"),
}

/** 🟢 strong · 🟡 moderate · ⚪ waiting · 🟠 highRisk · 🔴 exit — always shown with its text, never by colour alone. */
@Serializable
enum class DecisionLevel(val label: String, val emoji: String) {
    @SerialName("strong") STRONG("Signal fort", "🟢"),
    @SerialName("moderate") MODERATE("Signal modéré", "🟡"),
    @SerialName("waiting") WAITING("Attente", "⚪"),
    @SerialName("highRisk") HIGH_RISK("Risque élevé", "🟠"),
    @SerialName("exit") EXIT("Sortie / risque d'invalidation", "🔴"),
}

@Serializable
enum class FamilyStatus(val label: String, val tone: Tone) {
    @SerialName("positive") POSITIVE("favorable", Tone.GOOD),
    @SerialName("neutral") NEUTRAL("neutre", Tone.NEUTRAL),
    @SerialName("negative") NEGATIVE("défavorable", Tone.BAD),
    @SerialName("unavailable") UNAVAILABLE("non disponible", Tone.NEUTRAL),
}

@Serializable
enum class StepState(val label: String) {
    @SerialName("ok") OK("validée"),
    @SerialName("no") NO("pas encore"),
    @SerialName("unknown") UNKNOWN("inconnue"),
}

@Serializable
enum class ScenarioKind(val label: String) {
    @SerialName("bull") BULL("haussier"),
    @SerialName("neutral") NEUTRAL("neutre"),
    @SerialName("bear") BEAR("baissier"),
}

@Serializable
enum class Uncertainty(val label: String) {
    @SerialName("low") LOW("faible"),
    @SerialName("medium") MEDIUM("moyenne"),
    @SerialName("high") HIGH("élevée"),
}

@Serializable
enum class ExitKind(val label: String) {
    @SerialName("profit") PROFIT("prise de bénéfices"),
    @SerialName("defensive") DEFENSIVE("défensive"),
    @SerialName("macro") MACRO("macro"),
}

@Serializable
data class Decision(
    val symbol: String,
    val kind: Kind? = null,
    val name: String = "",
    val asOf: Double = 0.0,
    val price: Double? = null,
    /** "informational" (market data only) | "personal" (the user's position or weights were used, never stored). */
    val mode: String = "informational",
    val verdict: Verdict = Verdict.WAIT,
    val label: String = "",
    val level: DecisionLevel = DecisionLevel.WAITING,
    val levelLabel: String = "",
    /** 0–100. */
    val confidence: Double = 0.0,
    val confidenceText: String = "",
    val headline: String = "",
    val families: List<Family> = emptyList(),
    val vetoes: List<Veto> = emptyList(),
    val blocked: Boolean = false,
    val setup: Setup? = null,
    val plan: Plan? = null,
    val whyWait: List<String> = emptyList(),
    val toBuy: List<Condition> = emptyList(),
    val toSell: List<Condition> = emptyList(),
    val scenarios: List<Scenario> = emptyList(),
    val pros: List<String> = emptyList(),
    val cons: List<String> = emptyList(),
    val whyNot: WhyNot? = null,
    val fundamentals: Fundamentals? = null,
    val liquidity: Liquidity? = null,
    val track: Track? = null,
    val position: Position? = null,
    val exposure: Exposure? = null,
    val sources: List<DataSource> = emptyList(),
    val disclaimer: String = "",
) {
    val isPersonal: Boolean get() = mode == "personal"

    /** The server's French label, or ours when it is missing. */
    val verdictLabel: String get() = label.ifBlank { verdict.label }
    val levelText: String get() = levelLabel.ifBlank { level.label }

    /** Active vetoes first, then the checks that passed, then the ones no source can verify. */
    val sortedVetoes: List<Veto> get() = vetoes.sortedBy { if (it.active) 0 else if (it.verifiable) 1 else 2 }

    @Serializable
    data class Family(
        val key: String = "",
        val label: String = "",
        /** −100 … +100; null when unavailable. */
        val score: Double? = null,
        val status: FamilyStatus = FamilyStatus.UNAVAILABLE,
        val summary: String = "",
        val points: List<String> = emptyList(),
        val source: String = "",
    )

    @Serializable
    data class Veto(
        val code: String = "",
        val label: String = "",
        val active: Boolean = false,
        /** false: no source to check it, shown as "non vérifiable". */
        val verifiable: Boolean = true,
        val detail: String = "",
    )

    @Serializable
    data class Step(val label: String = "", val state: StepState = StepState.UNKNOWN, val detail: String = "")

    @Serializable
    data class Setup(val name: String = "", val steps: List<Step> = emptyList(), val met: Int = 0, val total: Int = 0)

    @Serializable
    data class Plan(
        val zoneFrom: Double,
        val zoneTo: Double,
        val entry: Double,
        val stop: Double,
        val target1: Double,
        val target2: Double? = null,
        val riskPct: Double = 0.0,
        val reward1Pct: Double = 0.0,
        val reward2Pct: Double? = null,
        val riskReward: Double = 0.0,
        val minRiskReward: Double = 2.0,
        val acceptable: Boolean = false,
        val horizon: String = "",
    )

    @Serializable
    data class Condition(val text: String = "", val level: Double? = null)

    @Serializable
    data class Scenario(
        val kind: ScenarioKind = ScenarioKind.NEUTRAL,
        val title: String = "",
        val condition: String = "",
        val consequence: String = "",
        val level: Double? = null,
    )

    @Serializable
    data class WhyNot(
        val risks: List<String> = emptyList(),
        val uncertainty: Uncertainty = Uncertainty.MEDIUM,
        val invalidation: List<String> = emptyList(),
    )

    @Serializable
    data class Liquidity(
        /** Percent (0.0012 = 0,0012 %). */
        val spreadPct: Double? = null,
        /** Average daily traded value over 20 days, USD. */
        val dailyValue: Double? = null,
        val relativeVolume: Double? = null,
        val source: String = "",
    )

    /** Past behaviour of the signal on this asset (walk-forward, fees and slippage included). */
    @Serializable
    data class Track(
        val period: String = "",
        val trades: Int = 0,
        val winRate: Double = 0.0,
        val avgWin: Double? = null,
        val avgLoss: Double? = null,
        val profitFactor: Double? = null,
        val sharpe: Double? = null,
        val sortino: Double? = null,
        val maxDrawdown: Double = 0.0,
        val totalReturn: Double = 0.0,
        val buyAndHold: Double = 0.0,
        val feesPct: Double = 0.0,
        val slippagePct: Double = 0.0,
        val losingStreak: Int = 0,
        val note: String = "",
        // Added later (absent from older answers): spread cost, expectancy, R multiples, results by market regime.
        /** Full bid/ask spread used, %: half paid on the buy, half on the sell. */
        val spreadPct: Double? = null,
        /** true: measured on the order book; false: default assumption. */
        val spreadMeasured: Boolean = false,
        val spreadNote: String = "",
        /** Average win × win rate − |average loss| × loss rate, % per trade, costs included. */
        val expectancy: Double? = null,
        /** Average of (trade return ÷ initial risk to the stop). */
        val avgR: Double? = null,
        /** null in an older answer (nothing more is shown then). */
        val regimes: List<RegimeStat>? = null,
        val testedBars: Int? = null,
        val biasNotes: List<String> = emptyList(),
    ) {
        /** Older answers (before these fields) show nothing more, like the web card. */
        val hasDetails: Boolean get() = regimes != null || expectancy != null
    }

    /** Results of the signal by market regime on the signal candle ("bull", "bear", "range", "crisis", "unknown"). */
    @Serializable
    data class RegimeStat(
        val regime: String = "unknown",
        val label: String = "",
        val trades: Int = 0,
        val winRate: Double? = null,
        val avgReturn: Double? = null,
        /** Fewer than 5 trades: "échantillon trop faible". */
        val lowSample: Boolean = false,
    )

    @Serializable
    data class Exit(
        val kind: ExitKind = ExitKind.DEFENSIVE,
        /** Share of the position, % (the rest is kept). */
        val share: Double = 0.0,
        val trigger: String = "",
        val price: Double? = null,
        /** Condition already met now. */
        val now: Boolean = false,
    )

    @Serializable
    data class Position(val cost: Double, val pnlPct: Double? = null, val advice: String = "", val exits: List<Exit> = emptyList())

    @Serializable
    data class Exposure(
        val factor: String = "",
        /** Share of the portfolio following that factor, %. */
        val weight: Double = 0.0,
        val assets: List<String> = emptyList(),
        val correlation: Double? = null,
        val warning: String? = null,
    )

    @Serializable
    data class DataSource(val name: String = "", val ok: Boolean = false, val detail: String = "")

    companion object {
        /**
         * `weights=` of the request: each holding's share of the portfolio value (quantity × price), largest first,
         * 20 lines at most, "SYMBOL:kind:35.2". Only percentages leave the phone, never the quantities nor the amounts.
         * Lines without a price are left out; null when nothing can be valued.
         */
        fun weights(holdings: List<Holding>, prices: Map<String, Double>): String? {
            val values = LinkedHashMap<String, Pair<Asset, Double>>()
            for (h in holdings) {
                val p = prices[h.asset.id] ?: continue
                val v = p * h.quantity
                if (!v.isFinite() || v <= 0) continue
                val prev = values[h.asset.id]?.second ?: 0.0
                values[h.asset.id] = h.asset to prev + v
            }
            val total = values.values.sumOf { it.second }
            if (total <= 0) return null
            return values.values
                .sortedByDescending { it.second }
                .take(20)
                .joinToString(",") { (a, v) -> "${a.symbol}:${a.kind.raw}:${number(v / total * 100, 1)}" }
        }

        /** Average cost of this asset over the holdings (weighted by quantity), or null when a line has none. */
        fun cost(holdings: List<Holding>, asset: Asset): Double? {
            val lines = holdings.filter { it.asset.id == asset.id && it.quantity > 0 }
            if (lines.isEmpty() || lines.any { it.averagePrice == null }) return null
            val q = lines.sumOf { it.quantity }
            return lines.sumOf { it.averagePrice!! * it.quantity } / q
        }

        /** "275.5", "0.00012": dot decimal, no trailing zeros (query parameter, not display). */
        internal fun number(v: Double, digits: Int): String =
            String.format(Locale.ROOT, "%.${digits}f", v).let { if (it.contains('.')) it.trimEnd('0').trimEnd('.') else it }
    }
}

/** `fundamentals`, tagged by `kind` ("stock" | "crypto"); another kind is kept as [Other] instead of failing. */
@Serializable(with = FundamentalsSerializer::class)
sealed interface Fundamentals {
    /** Company figures from its filings (SEC EDGAR), trailing twelve months. null = not reported. */
    @Serializable
    data class Stock(
        val period: String = "",
        val revenue: Double? = null,
        val revenueGrowth: Double? = null,
        val netIncome: Double? = null,
        val eps: Double? = null,
        val epsGrowth: Double? = null,
        val grossMargin: Double? = null,
        val operatingMargin: Double? = null,
        val netMargin: Double? = null,
        val freeCashFlow: Double? = null,
        val fcfMargin: Double? = null,
        val debt: Double? = null,
        val cash: Double? = null,
        val netDebt: Double? = null,
        val roe: Double? = null,
        val per: Double? = null,
        val peg: Double? = null,
        val evEbitda: Double? = null,
        val dividendYield: Double? = null,
        val shareChange: Double? = null,
        val nextEarnings: EarningsDate? = null,
        val surprises: List<Surprise> = emptyList(),
        val revisions: Revisions? = null,
        val sectorNote: String = "",
        val source: String = "",
        // Added later: absent from older answers (null / empty then).
        /** Price ÷ sales: market cap ÷ revenue (12 months). */
        val ps: Double? = null,
        /** Price ÷ book: market cap ÷ stockholders' equity. */
        val pb: Double? = null,
        /** Return on invested capital, %. */
        val roic: Double? = null,
        /** Tax rate used for the ROIC, %. */
        val roicTaxRate: Double? = null,
        /** true when the effective rate could not be computed and the 21 % US statutory rate is used. */
        val roicTaxStatutory: Boolean = false,
        /** End of the last period filed and date of that filing (ms). */
        val periodEnd: Double? = null,
        val filedAt: Double? = null,
        val sector: Sector? = null,
        val valuationHistory: ValuationHistory? = null,
        val peers: PeerComparison? = null,
        /** Valuation vs growth in one sentence. */
        val valuationVerdict: String? = null,
        /** Management guidance, or why it is not given. */
        val guidance: String = "",
    ) : Fundamentals {
        @Serializable
        data class Sector(val label: String = "", val sic: String = "", val sicDescription: String = "", val source: String = "")

        /** A ratio against its own daily history; `percentile`: % of days at or below today's value. */
        @Serializable
        data class RatioHistory(
            val current: Double,
            val median: Double,
            val min: Double,
            val max: Double,
            val percentile: Double,
            val days: Int = 0,
            val from: Double = 0.0,
            val to: Double = 0.0,
        )

        @Serializable
        data class ValuationHistory(val per: RatioHistory? = null, val ps: RatioHistory? = null, val method: String = "", val source: String = "")

        @Serializable
        data class Peer(
            val symbol: String,
            val name: String = "",
            val per: Double? = null,
            val ps: Double? = null,
            val operatingMargin: Double? = null,
            val netMargin: Double? = null,
            val revenueGrowth: Double? = null,
            val periodEnd: Double = 0.0,
        )

        @Serializable
        data class PeerComparison(
            val group: String = "",
            val peers: List<Peer> = emptyList(),
            val medianPer: Double? = null,
            val medianPs: Double? = null,
            val medianOperatingMargin: Double? = null,
            val medianNetMargin: Double? = null,
            val medianRevenueGrowth: Double? = null,
            val date: Double = 0.0,
            val source: String = "",
        )

        @Serializable
        data class EarningsDate(val date: Double, val estimated: Boolean = false)

        @Serializable
        data class Surprise(val quarter: String = "", val eps: Double, val consensus: Double, val surprisePct: Double)

        @Serializable
        data class Revisions(val monthAgo: Double, val now: Double, val changePct: Double)
    }

    /** Token and network figures. null = not given by a free verifiable source. */
    @Serializable
    data class Crypto(
        val marketCap: Double? = null,
        val fdv: Double? = null,
        val mcFdv: Double? = null,
        val circulatingSupply: Double? = null,
        val totalSupply: Double? = null,
        val maxSupply: Double? = null,
        val circulatingPct: Double? = null,
        val tvl: Double? = null,
        val fees30d: Double? = null,
        val btcDominance: Double? = null,
        /** Perpetual funding per 8 h, as a fraction (0.0001 = 0,01 %), like /api/guard. */
        val fundingRate: Double? = null,
        val openInterest: Double? = null,
        val txPerDay: Double? = null,
        /** Hashes per second. */
        val hashRate: Double? = null,
        val unlocks: String = "",
        val source: String = "",
        // Added later: absent from older answers.
        /** Developer activity of the project's code; null with [devActivityKnown] = "non disponible". */
        val devActivity: DevActivity? = null,
        /** The answer has the `devActivity` field (a newer server), even null: the section is shown. */
        val devActivityKnown: Boolean = false,
        /** Stablecoins on all chains, and on the asset's own chain when it is one. */
        val stablecoins: StablecoinFlows? = null,
        val chainStablecoins: StablecoinFlows? = null,
        /** What is not covered and why (no free verifiable source). */
        val notCovered: String = "",
    ) : Fundamentals {
        @Serializable
        data class DevActivity(
            val repo: String? = null,
            val commits4w: Double? = null,
            val pullRequestsMerged: Double? = null,
            val contributors: Double? = null,
            val stars: Double? = null,
            val additions4w: Double? = null,
            val deletions4w: Double? = null,
            val smartContractPlatform: Boolean = false,
            val source: String = "",
        )

        /** Stablecoins in circulation (USD value): the crypto market's cash, a liquidity indicator. */
        @Serializable
        data class StablecoinFlows(
            /** "Tous réseaux" or the chain's name. */
            val scope: String = "",
            val date: Double = 0.0,
            val total: Double,
            val change7d: Double? = null,
            val change7dPct: Double? = null,
            val change30d: Double? = null,
            val change30dPct: Double? = null,
            val source: String = "",
        )
    }

    @Serializable
    data class Other(val kind: String = "") : Fundamentals
}

/** Crypto fundamentals, noting whether the answer has the `devActivity` field at all (null is not the same as absent). */
internal object CryptoFundamentalsSerializer : JsonTransformingSerializer<Fundamentals.Crypto>(Fundamentals.Crypto.serializer()) {
    override fun transformDeserialize(element: JsonElement): JsonElement {
        val o = element as? JsonObject ?: return element
        return JsonObject(o + ("devActivityKnown" to JsonPrimitive(o.containsKey("devActivity") || o["devActivityKnown"] == JsonPrimitive(true))))
    }
}

internal object FundamentalsSerializer : JsonContentPolymorphicSerializer<Fundamentals>(Fundamentals::class) {
    override fun selectDeserializer(element: JsonElement): KSerializer<out Fundamentals> =
        when (runCatching { element.jsonObject["kind"]?.jsonPrimitive?.content }.getOrNull()) {
            "stock" -> Fundamentals.Stock.serializer()
            "crypto" -> CryptoFundamentalsSerializer
            else -> Fundamentals.Other.serializer()
        }
}
