package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.time.Instant
import java.time.ZoneOffset
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

// Risk of the portfolio the user holds: market stress scenarios through each line's beta, checks against the user's
// risk settings (weight per line, crypto share, correlated clusters, daily loss) and the positions that became
// dangerous (stop broken or close, loss beyond the risk accepted per idea). Exact port of web/src/engine/portfolio-risk.ts
// (and of the parts of holdings.ts it relies on), tested with the same cases (PortfolioRiskTest ↔ portfolio-risk.test.ts).
// Pure, deterministic functions.

/** The user's limits (Réglages → Prudence des conseils), same defaults as the web app (DEFAULT_RISK). */
@Serializable
data class RiskSettings(
    val riskPerTradePercent: Double = 1.0,
    val maxPositionPercent: Double = 20.0,
    val dailyLossLimitPercent: Double = 3.0,
    val minRiskReward: Double = 1.5,
    /** Cap of the crypto share of the portfolio (%, cash included). */
    val maxCryptoPercent: Double = 60.0,
) {
    /** Values out of the steppers' range (damaged storage) fall back to the default. */
    fun sanitized(): RiskSettings {
        val d = RiskSettings()
        fun ok(v: Double, lo: Double, hi: Double, def: Double) = if (v.isFinite() && v in lo..hi) v else def
        return RiskSettings(
            ok(riskPerTradePercent, 0.25, 5.0, d.riskPerTradePercent),
            ok(maxPositionPercent, 5.0, 100.0, d.maxPositionPercent),
            ok(dailyLossLimitPercent, 0.5, 10.0, d.dailyLossLimitPercent),
            ok(minRiskReward, 1.0, 5.0, d.minRiskReward),
            ok(maxCryptoPercent, 0.0, 100.0, d.maxCryptoPercent),
        )
    }

    companion object {
        val DEFAULT = RiskSettings()
    }
}

// ---------- The portfolio as the risk functions see it (holdings.ts analyzePortfolio, the fields they use) ----------

data class RiskLine(
    val id: String,
    val symbol: String,
    val kind: Kind,
    val name: String,
    val quantity: Double,
    /** Current price; null when unknown (the line is then valued at its average cost, like the web app). */
    val price: Double?,
    val value: Double,
    /** Quantity × average cost; null without an average cost (Android makes it optional). */
    val invested: Double?,
    /** % of the total (cash included). */
    val weight: Double,
    /** Protective stop of the analysis: price − 2 × daily ATR (14), at least 1 % of the price. */
    val stop: Double?,
) {
    val key: String get() = "${kind.raw}:$symbol"
}

data class RiskPortfolio(
    val total: Double,
    val cash: Double,
    val marketValue: Double,
    /** Crypto share of the total, %. */
    val crypto: Double,
    val lines: List<RiskLine>,
) {
    companion object {
        /** `prices` and `daily` by asset id ("crypto:BTC"). */
        fun of(holdings: List<Holding>, cash: Double, prices: Map<String, Double>, daily: Map<String, List<Candle>>): RiskPortfolio {
            val base = holdings.map { h ->
                val price = prices[h.asset.id]?.takeIf { it > 0 }
                val value = h.quantity * (price ?: h.averagePrice ?: 0.0)
                Triple(h, price, value)
            }
            val marketValue = base.sumOf { it.third }
            val safeCash = max(0.0, cash)
            val total = marketValue + safeCash
            val lines = base.map { (h, price, value) ->
                val d = daily[h.asset.id].orEmpty()
                val a = if (d.isNotEmpty()) PortfolioRisk.atr(d)[d.size - 1] else null
                val stop = if (price != null && a != null && a != 0.0) max(price - 2 * a, price * 0.01) else null
                RiskLine(
                    h.id, h.asset.symbol, h.asset.kind, h.asset.name, h.quantity, price, value,
                    h.averagePrice?.let { it * h.quantity }, if (total > 0) value / total * 100 else 0.0, stop,
                )
            }
            val crypto = lines.filter { it.kind == Kind.CRYPTO }.sumOf { it.value }
            return RiskPortfolio(total, safeCash, marketValue, if (total > 0) crypto / total * 100 else 0.0, lines)
        }
    }
}

// ---------- Results ----------

data class BetaEstimate(
    val beta: Double,
    /** Shared daily returns used; 0 with the fallback. */
    val days: Int,
    /** false: history too short or missing, beta 1 assumed. */
    val estimated: Boolean,
    /** The asset is the benchmark itself (beta 1 by definition). */
    val reference: Boolean = false,
)

/** Shock of each benchmark, % (negative = fall). */
data class StressScenario(val key: String, val label: String, val crypto: Double, val stock: Double)

data class StressWorst(val symbol: String, val name: String, val loss: Double, val movePercent: Double)

data class StressResult(
    val scenario: StressScenario,
    /** Amount lost (USD, positive = loss; negative when the scenario would gain). */
    val loss: Double,
    /** Of the whole portfolio (cash included). */
    val lossPercent: Double,
    /** Of the invested part only (without cash): the difference is what the cash cushions. */
    val investedLossPercent: Double,
    val worst: StressWorst?,
)

data class Cluster(val symbols: List<String>, val averageCorrelation: Double, val weight: Double, val days: Int)

data class ClusterInput(val symbol: String, val weight: Double, val daily: List<Candle>)

data class DailyChange(
    /** USD, negative = loss today. */
    val change: Double,
    /** Of the portfolio value at the previous close (cash included). */
    val percent: Double,
    /** Lines with a live price and a previous close / all lines. */
    val covered: Int,
    val lines: Int,
)

enum class LimitLevel { DANGER, WARNING, OK, NA }

data class LimitCheck(val code: String, val level: LimitLevel, val label: String, val detail: String)

@Serializable
data class DangerReason(val code: String, val text: String)

@Serializable
data class Danger(val id: String, val symbol: String, val kind: Kind, val name: String, val reasons: List<DangerReason>)

object PortfolioRisk {
    /** Window of the beta (same as the correlations of the analysis) and minimum of shared daily returns. */
    const val BETA_DAYS = 90
    const val MIN_BETA_DAYS = 30
    const val CLUSTER_CORRELATION = 0.7
    const val DAILY_LOSS_REACHED = "Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui."

    /** Benchmark of each asset class: Bitcoin for cryptos, the S&P 500 (via the SPY ETF) for stocks. */
    fun benchmark(kind: Kind): Asset = if (kind == Kind.CRYPTO) Asset("BTC", Kind.CRYPTO, "Bitcoin") else Asset("SPY", Kind.STOCK, "S&P 500")
    fun benchmarkLabel(kind: Kind): String = if (kind == Kind.CRYPTO) "Bitcoin" else "S&P 500"

    val STRESS_SCENARIOS = listOf(
        StressScenario("all5", "Marchés −5 %", -5.0, -5.0),
        StressScenario("all10", "Marchés −10 %", -10.0, -10.0),
        StressScenario("all20", "Marchés −20 %", -20.0, -20.0),
        StressScenario("all30", "Marchés −30 %", -30.0, -30.0),
        StressScenario("crypto20", "Crypto −20 %, actions stables", -20.0, 0.0),
        StressScenario("stock10crypto30", "Actions −10 %, crypto −30 %", -30.0, -10.0),
    )

    // ---------- Shared maths (signal.ts, holdings.ts) ----------

    /** Wilder's ATR, null before `period` candles (signal.ts `atr`). */
    fun atr(c: List<Candle>, period: Int = 14): List<Double?> {
        val out = MutableList<Double?>(c.size) { null }
        if (c.size < period) return out
        val tr = c.mapIndexed { i, x ->
            if (i == 0) x.high - x.low else maxOf(x.high - x.low, abs(x.high - c[i - 1].close), abs(x.low - c[i - 1].close))
        }
        var prev = tr.take(period).sum() / period
        out[period - 1] = prev
        for (i in period until c.size) {
            prev = (prev * (period - 1) + tr[i]) / period
            out[i] = prev
        }
        return out
    }

    private fun utcDay(ms: Double): String = Instant.ofEpochMilli(ms.toLong()).atOffset(ZoneOffset.UTC).toLocalDate().toString()

    /** Daily closes indexed by UTC date (crypto and stocks aligned by calendar day); the last candle of a day wins. */
    private fun closesByDay(c: List<Candle>): LinkedHashMap<String, Double> {
        val m = LinkedHashMap<String, Double>()
        for (x in c.sortedBy { it.time }) m[utcDay(x.time)] = x.close
        return m
    }

    /** Daily returns over the last `days` shared calendar days, carrying the price forward when a market is closed. */
    fun alignedReturns(series: List<List<Candle>>, days: Int = 90): List<List<Double>> {
        val maps = series.map(::closesByDay)
        val all = maps.flatMap { it.keys }.toSortedSet().toList()
        val start = all.indexOfFirst { d -> maps.all { m -> m.keys.any { it <= d } } }
        if (start < 0) return series.map { emptyList() }
        val dates = all.drop(start).takeLast(days + 1)
        return maps.map { m ->
            var last: Double? = null
            for (k in m.keys.sorted()) if (k <= dates[0]) last = m[k]
            val closes = dates.map { d -> m[d]?.also { last = it } ?: last!! }
            closes.drop(1).mapIndexed { i, c -> if (closes[i] > 0) c / closes[i] - 1 else 0.0 }
        }
    }

    private fun mean(v: List<Double>) = if (v.isEmpty()) 0.0 else v.sum() / v.size

    /** Pearson correlation of the last shared returns, null under 10 values or without variance. */
    fun correlation(a: List<Double>, b: List<Double>): Double? {
        val n = min(a.size, b.size)
        if (n < 10) return null
        val x = a.takeLast(n)
        val y = b.takeLast(n)
        val mx = mean(x)
        val my = mean(y)
        var sxy = 0.0
        var sxx = 0.0
        var syy = 0.0
        for (i in 0 until n) {
            sxy += (x[i] - mx) * (y[i] - my)
            sxx += (x[i] - mx) * (x[i] - mx)
            syy += (y[i] - my) * (y[i] - my)
        }
        return if (sxx > 0 && syy > 0) sxy / Math.sqrt(sxx * syy) else null
    }

    // ---------- Beta ----------

    /** Beta of an asset to its benchmark = cov(asset, benchmark) ÷ var(benchmark) on the aligned daily returns. */
    fun estimateBeta(asset: List<Candle>, benchmark: List<Candle>, days: Int = BETA_DAYS): BetaEstimate {
        val fallback = BetaEstimate(1.0, 0, false)
        if (asset.size < 2 || benchmark.size < 2) return fallback
        val (a, b) = alignedReturns(listOf(asset, benchmark), days)
        val n = min(a.size, b.size)
        if (n < MIN_BETA_DAYS) return fallback
        val x = a.takeLast(n)
        val y = b.takeLast(n)
        val mx = x.sum() / n
        val my = y.sum() / n
        var cov = 0.0
        var vary = 0.0
        for (i in 0 until n) {
            cov += (x[i] - mx) * (y[i] - my)
            vary += (y[i] - my) * (y[i] - my)
        }
        return if (vary > 0) BetaEstimate(cov / vary, n, true) else fallback
    }

    /** Beta of each held asset (key "kind:SYMBOL"): the benchmark itself is 1 by definition. */
    fun betas(holdings: List<Holding>, daily: Map<String, List<Candle>>): Map<String, BetaEstimate> =
        holdings.associate { h ->
            val ref = benchmark(h.asset.kind)
            h.asset.id to if (h.asset.id == ref.id) BetaEstimate(1.0, 0, false, reference = true)
            else estimateBeta(daily[h.asset.id].orEmpty(), daily[ref.id].orEmpty())
        }

    // ---------- Stress scenarios ----------

    /**
     * Each line moves by beta × the shock of its benchmark (never below −100 %); cash does not move. Lines of the same
     * asset share its beta (key "kind:SYMBOL", beta 1 when missing).
     */
    fun stressTest(a: RiskPortfolio, betas: Map<String, BetaEstimate>, scenarios: List<StressScenario> = STRESS_SCENARIOS): List<StressResult> =
        scenarios.map { scenario ->
            val byAsset = LinkedHashMap<String, StressWorst>()
            for (l in a.lines) {
                val beta = betas[l.key]?.beta ?: 1.0
                val move = max(-100.0, beta * (if (l.kind == Kind.CRYPTO) scenario.crypto else scenario.stock))
                val prev = byAsset[l.key]
                val loss = -l.value * move / 100
                byAsset[l.key] = StressWorst(l.symbol, l.name, (prev?.loss ?: 0.0) + loss, move)
            }
            val loss = byAsset.values.sumOf { it.loss }
            var worst: StressWorst? = null
            for (x in byAsset.values) if (x.loss > 0 && (worst == null || x.loss > worst.loss)) worst = x
            StressResult(
                scenario, loss,
                if (a.total > 0) loss / a.total * 100 else 0.0,
                if (a.marketValue > 0) loss / a.marketValue * 100 else 0.0,
                worst,
            )
        }

    // ---------- Correlated clusters ----------

    /**
     * Groups of assets linked by a correlation above 0.7 (connected by pairs), kept when their average pairwise
     * correlation stays above 0.7. `series`: one entry per asset (weight in % of the portfolio, daily candles).
     */
    fun correlatedClusters(series: List<ClusterInput>, days: Int = BETA_DAYS): List<Cluster> {
        val usable = series.filter { it.daily.size >= 20 }
        if (usable.size < 2) return emptyList()
        val returns = alignedReturns(usable.map { it.daily }, days)
        val n = usable.size
        val corr = Array(n) { arrayOfNulls<Double>(n) }
        for (i in 0 until n) for (j in i + 1 until n) {
            val c = correlation(returns[i], returns[j])
            corr[i][j] = c
            corr[j][i] = c
        }
        val seen = HashSet<Int>()
        val clusters = mutableListOf<Cluster>()
        for (s in 0 until n) {
            if (s in seen) continue
            val group = mutableListOf(s)
            seen += s
            var k = 0
            while (k < group.size) {
                for (j in 0 until n) {
                    if (j !in seen && (corr[group[k]][j] ?: -1.0) > CLUSTER_CORRELATION) {
                        seen += j
                        group += j
                    }
                }
                k++
            }
            if (group.size < 2) continue
            val pairs = mutableListOf<Double>()
            for (x in group.indices) for (y in x + 1 until group.size) corr[group[x]][group[y]]?.let { pairs += it }
            val average = pairs.sum() / pairs.size
            if (!(average > CLUSTER_CORRELATION)) continue
            clusters += Cluster(group.map { usable[it].symbol }, average, group.sumOf { usable[it].weight }, returns.minOf { it.size })
        }
        return clusters.sortedByDescending { it.weight }
    }

    // ---------- Today's change ----------

    /** Previous close: the close of the last daily candle before today's (UTC day; for stocks, the last session). */
    fun previousClose(daily: List<Candle>, now: Double): Double? {
        if (daily.isEmpty()) return null
        val sorted = daily.sortedBy { it.time }
        val last = sorted.last()
        if (utcDay(last.time) == utcDay(now)) return if (sorted.size > 1) sorted[sorted.size - 2].close else null
        return last.close
    }

    /** Portfolio change since the previous close, from the current prices of the analysis. */
    fun dailyChange(a: RiskPortfolio, daily: Map<String, List<Candle>>, now: Double): DailyChange? {
        var change = 0.0
        var covered = 0
        for (l in a.lines) {
            val prev = previousClose(daily[l.key].orEmpty(), now)
            if (l.price == null || prev == null || prev <= 0) continue
            change += l.quantity * (l.price - prev)
            covered++
        }
        if (covered == 0) return null
        val before = a.total - change
        return DailyChange(change, if (before > 0) change / before * 100 else 0.0, covered, a.lines.size)
    }

    // ---------- Limits of the user's settings ----------

    private fun fr(v: Double, d: Int = 1) = Format.plain(v, d)
    private fun usd0(v: Double) = Format.amount(abs(v))
    /** A dollar price in the display currency with up to 4 decimals ("140 €", "0,1234 €"). */
    private fun px4(v: Double) = "${fr(Money.toDisplay(v), 4)} ${Money.symbol()}"

    /** Loss (USD) if the stop of the line is hit: the user's stop when set, else the protective stop of the analysis. */
    private fun lossAtStop(l: RiskLine, userStop: Double?): Double? {
        val stop = if (userStop != null && userStop > 0) userStop else l.stop
        if (l.price == null || stop == null) return null
        return max(0.0, l.quantity * (l.price - stop))
    }

    private class AssetRisk(val symbol: String, val weight: Double, val loss: Double?)

    /**
     * Checks of the portfolio against the settings: loss at the stop per asset vs risk per idea, weight per asset vs
     * maximum, crypto share vs cap, correlated clusters above twice the maximum weight, today's loss vs the daily limit.
     * `stops`: the user's stop per holding id.
     */
    fun checkLimits(a: RiskPortfolio, s: RiskSettings, clusters: List<Cluster>, daily: DailyChange?, stops: Map<String, Double?> = emptyMap()): List<LimitCheck> {
        val out = mutableListOf<LimitCheck>()
        if (a.lines.isEmpty() || a.total <= 0) return out
        val assets = LinkedHashMap<String, AssetRisk>()
        for (l in a.lines) {
            val loss = lossAtStop(l, stops[l.id])
            val p = assets[l.key]
            assets[l.key] = AssetRisk(l.symbol, (p?.weight ?: 0.0) + l.weight, if (p?.loss == null && loss == null) null else (p?.loss ?: 0.0) + (loss ?: 0.0))
        }

        val budget = a.total * s.riskPerTradePercent / 100
        val risky = assets.values.filter { it.loss != null && it.loss > budget * 1.0001 }
        val riskLabel = "Risque par ligne (max ${fr(s.riskPerTradePercent, 2)} %)"
        out += if (risky.isNotEmpty()) {
            LimitCheck(
                "risk_per_trade", LimitLevel.WARNING, riskLabel,
                "Si le stop était touché, ${risky.joinToString(", ") { "${it.symbol} coûterait ${usd0(it.loss!!)} (${fr(it.loss / a.total * 100)} %)" }} : " +
                    "plus que votre risque accepté par idée (${usd0(budget)}). Réduisez la ligne ou rapprochez le stop.",
            )
        } else LimitCheck("risk_per_trade", LimitLevel.OK, riskLabel, "Aucune ligne ne perdrait plus de ${usd0(budget)} à son stop.")

        val heavy = assets.values.filter { it.weight > s.maxPositionPercent }
        val weightLabel = "Poids max d'une ligne (${fr(s.maxPositionPercent, 0)} %)"
        out += if (heavy.isNotEmpty() && assets.size > 1) {
            LimitCheck("max_weight", LimitLevel.DANGER, weightLabel, "${heavy.joinToString(", ") { "${it.symbol} ${fr(it.weight)} %" }} : au-dessus de votre maximum. Surexposition à un seul actif.")
        } else LimitCheck("max_weight", LimitLevel.OK, weightLabel, "Aucune ligne au-dessus de votre maximum.")

        val crypto = a.crypto
        val cryptoLabel = "Part crypto (max ${fr(s.maxCryptoPercent, 0)} %)"
        out += if (crypto > s.maxCryptoPercent) {
            LimitCheck("crypto_cap", LimitLevel.WARNING, cryptoLabel, "${fr(crypto)} % du patrimoine en crypto, au-dessus de votre plafond : un repli des cryptos toucherait tout le portefeuille.")
        } else LimitCheck("crypto_cap", LimitLevel.OK, cryptoLabel, "${fr(crypto)} % du patrimoine en crypto.")

        val clusterMax = min(100.0, s.maxPositionPercent * 2)
        val over = clusters.filter { it.weight > clusterMax }
        val clusterLabel = "Actifs corrélés (groupe max ${fr(clusterMax, 0)} %)"
        out += if (over.isNotEmpty()) {
            LimitCheck(
                "cluster", LimitLevel.WARNING, clusterLabel,
                over.joinToString(" ; ") { "${it.symbols.joinToString(" + ")} : ${fr(it.weight)} % du patrimoine, corrélation moyenne ${fixed2(it.averageCorrelation)} sur ${it.days} jours" } +
                    ". Ils se comportent comme une seule grosse ligne.",
            )
        } else LimitCheck(
            "cluster", LimitLevel.OK, clusterLabel,
            if (clusters.isNotEmpty()) "Groupe(s) corrélé(s) sous le seuil : ${clusters.joinToString(" ; ") { "${it.symbols.joinToString(" + ")} ${fr(it.weight)} %" }}."
            else "Aucun groupe d'actifs corrélés à plus de 0,7.",
        )

        val label = "Perte du jour (max ${fr(s.dailyLossLimitPercent, 2)} %)"
        if (daily == null) out += LimitCheck("daily_loss", LimitLevel.NA, label, "Clôture de la veille indisponible : variation du jour inconnue.")
        else {
            val partial = if (daily.covered < daily.lines) " (${daily.covered} ligne(s) sur ${daily.lines} mesurées)" else ""
            val text = "${if (daily.change >= 0) "+" else "−"}${usd0(daily.change)} (${if (daily.percent >= 0) "+" else "−"}${fr(abs(daily.percent), 2)} %) depuis la clôture de la veille$partial."
            out += if (daily.percent <= -s.dailyLossLimitPercent) LimitCheck("daily_loss", LimitLevel.DANGER, label, "$DAILY_LOSS_REACHED $text")
            else LimitCheck("daily_loss", LimitLevel.OK, label, text)
        }
        return out
    }

    /** JavaScript's toFixed(2): dot decimal (as the web text shows it). */
    private fun fixed2(v: Double) = String.format(Locale.ROOT, "%.2f", v)

    // ---------- Positions that became dangerous ----------

    /**
     * A line is dangerous when its price broke the user's stop, is within one daily ATR (14) of it, or when its
     * unrealised loss exceeds the risk accepted per idea (% of the portfolio).
     */
    fun dangerousPositions(a: RiskPortfolio, s: RiskSettings, daily: Map<String, List<Candle>>, stops: Map<String, Double?>): List<Danger> {
        val out = mutableListOf<Danger>()
        val budget = a.total * s.riskPerTradePercent / 100
        for (l in a.lines) {
            val reasons = mutableListOf<DangerReason>()
            val stop = stops[l.id]
            val candles = daily[l.key].orEmpty()
            val range = if (candles.isNotEmpty()) atr(candles)[candles.size - 1] else null
            if (l.price != null && stop != null && stop > 0) {
                if (l.price <= stop) reasons += DangerReason("stop_broken", "Stop cassé : cours ${px4(l.price)} sous votre stop ${px4(stop)}.")
                else if (range != null && range != 0.0 && l.price - stop <= range) {
                    reasons += DangerReason("near_stop", "À moins d'une volatilité journalière (ATR ${px4(range)}) de votre stop ${px4(stop)}.")
                }
            }
            val loss = l.invested?.let { it - l.value }
            if (l.price != null && loss != null && loss > budget && budget > 0) {
                reasons += DangerReason(
                    "loss_over_risk",
                    "Perte latente de ${usd0(loss)} (${fr(loss / a.total * 100)} % du patrimoine), au-delà de votre risque accepté par idée (${fr(s.riskPerTradePercent, 2)} %).",
                )
            }
            if (reasons.isNotEmpty()) out += Danger(l.id, l.symbol, l.kind, l.name, reasons)
        }
        return out
    }

    /** What the Radar shows of the last dangers measured on Mes avoirs (validated when read back). */
    @Serializable
    data class DangerState(val version: Int, val at: Double, val items: List<Danger>)

    private val DANGER_CODES = setOf("stop_broken", "near_stop", "loss_over_risk")

    /** A damaged or older saved state is ignored (null); bad items are dropped. */
    fun parseDangers(raw: String?): DangerState? {
        if (raw == null) return null
        val o = runCatching { AltimJson.parseToJsonElement(raw) as? JsonObject }.getOrNull() ?: return null
        val version = (o["version"] as? JsonPrimitive)?.content
        val at = (o["at"] as? JsonPrimitive)?.content?.toDoubleOrNull()
        val items = o["items"] as? JsonArray
        if (version != "1" || at == null || !at.isFinite() || items == null) return null
        val valid = items.mapNotNull { e ->
            runCatching { AltimJson.decodeFromJsonElement(Danger.serializer(), e) }.getOrNull()
                ?.takeIf { d -> d.reasons.all { it.code in DANGER_CODES } && strictDanger(e) }
        }
        return DangerState(1, at, valid)
    }

    /** The kit's JSON is tolerant (defaults); a saved danger must have every field. */
    private fun strictDanger(e: JsonElement): Boolean {
        val o = e as? JsonObject ?: return false
        val kind = (o["kind"] as? JsonPrimitive)?.content
        val reasons = o["reasons"] as? JsonArray ?: return false
        return listOf("id", "symbol", "name").all { (o[it] as? JsonPrimitive)?.isString == true } &&
            (kind == "crypto" || kind == "stock") &&
            reasons.all { r -> ((r as? JsonObject)?.get("text") as? JsonPrimitive)?.isString == true }
    }

    fun encodeDangers(items: List<Danger>, at: Double): String = AltimJson.encodeToString(DangerState.serializer(), DangerState(1, at, items))
}
