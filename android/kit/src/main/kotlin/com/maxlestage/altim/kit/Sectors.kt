package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.text.Collator
import java.util.Locale
import kotlin.math.max
import kotlin.math.pow

// Sector exposure of the portfolio held: stocks grouped by sector (as returned by GET /api/sectors?symbols=), cryptos
// and cash as their own blocks, stocks without a known sector as "Secteur inconnu". Exact port of
// web/src/engine/sectors.ts, tested with the same cases (SectorsTest ↔ web/test/sectors.test.ts). Pure functions.
// Android keeps no cash in Mes avoirs: the screen passes 0, so no "Liquidités" block (like the portfolio risk).

/** SIC division filed at the SEC. */
@Serializable
data class SectorSec(val label: String = "", val sic: String = "", val sicDescription: String = "", val source: String = "")

/** Nasdaq classification: "Technology" / "Technologie" / "Semiconductors". */
@Serializable
data class SectorNasdaq(val sector: String = "", val sectorFr: String = "", val industry: String = "")

/** Sector of one stock. Every field optional: a partial answer never breaks the card. */
@Serializable
data class SectorItem(
    val symbol: String = "",
    /** French label ("Technologie", "Finance et immobilier", "ETF / fonds indiciel (plusieurs secteurs)"), null when unknown. */
    val sector: String? = null,
    /** "nasdaq" | "sec" | "etf", null when unknown. */
    val classification: String? = null,
    /** Named source of the label, null when unknown. */
    val source: String? = null,
    /** Why no source covers it (null when classified). */
    val reason: String? = null,
    val etf: Boolean = false,
    val sec: SectorSec? = null,
    val nasdaq: SectorNasdaq? = null,
)

@Serializable
data class SectorSource(val name: String = "", val ok: Boolean = false, val error: String? = null)

@Serializable
data class SectorsReport(
    val asOf: Double = 0.0,
    val items: List<SectorItem> = emptyList(),
    val sources: List<SectorSource> = emptyList(),
)

/** "nasdaq": sector of the Nasdaq screener; "sec": SIC division filed at the SEC; "etf": fund spanning several sectors. */
enum class SectorClassification(val raw: String, val source: String) {
    NASDAQ("nasdaq", "Nasdaq (secteur du screener)"),
    SEC("sec", "SEC EDGAR (code SIC, grandes divisions)"),
    ETF("etf", "Nasdaq Trader / SEC (ETF)");

    companion object {
        fun of(raw: String?): SectorClassification? = entries.firstOrNull { it.raw == raw }
    }
}

enum class BlockKind { SECTOR, ETF, CRYPTO, CASH, UNKNOWN }

data class ExposureBlock(
    val key: String,
    val label: String,
    val kind: BlockKind,
    val value: Double,
    /** % of the whole portfolio (cash included). */
    val weight: Double,
    /** % of the stock part (stocks only, else null). */
    val stockWeight: Double?,
    val symbols: List<String>,
)

enum class SectorInsightLevel { WARNING, INFO }

/** A remark on the exposure; `values` as in the web engine (sector, weight, share, effective, symbols). */
data class SectorInsight(val level: SectorInsightLevel, val code: String, val values: Map<String, Any> = emptyMap())

data class SectorUnknown(val symbol: String, val reason: String)

data class SectorLine(val symbol: String, val kind: Kind, val value: Double)

data class SectorExposure(
    val total: Double,
    val stockValue: Double,
    /** Sorted by weight, heaviest first. */
    val blocks: List<ExposureBlock>,
    /** 1 / Σ w² over the stocks with an operating sector (ETFs and unknown left out); null without any. */
    val effectiveSectors: Double?,
    /** % of the stock part whose operating sector is known (ETFs and unknown excluded). */
    val classifiedShare: Double,
    val unknown: List<SectorUnknown>,
    /** Number of stocks per classification, for the source line. */
    val bySource: Map<SectorClassification, Int>,
    val insights: List<SectorInsight>,
)

object Sectors {
    /** A sector above this share of the whole portfolio is flagged. */
    const val MAX_WEIGHT = 35.0
    /** A sector above this share of the stock part is flagged (from 2 stock lines). */
    const val MAX_STOCK_SHARE = 50.0
    const val ETF_BLOCK_LABEL = "ETF / fonds (plusieurs secteurs)"
    const val UNKNOWN_LABEL = "Secteur inconnu"
    /** The server classifies 50 stocks at most per request. */
    const val MAX_SYMBOLS = 50

    private class Acc(val key: String, val label: String, val kind: BlockKind) {
        var value = 0.0
        val symbols = mutableListOf<String>()
    }

    /**
     * Exposure by sector. `lines`: the portfolio lines with their current value (USD); `sectors`: items of
     * /api/sectors by symbol, null when the call failed (`failure` then says why: every stock is "Secteur inconnu").
     */
    fun exposure(
        lines: List<SectorLine>,
        cash: Double,
        sectors: Map<String, SectorItem>?,
        failure: String = "classement sectoriel indisponible",
    ): SectorExposure {
        val safeCash = max(0.0, cash)
        val total = lines.sumOf { max(0.0, it.value) } + safeCash
        val blocks = LinkedHashMap<String, Acc>()
        val unknown = LinkedHashMap<String, String>()
        val bySource = SectorClassification.entries.associateWith { 0 }.toMutableMap()
        val counted = mutableSetOf<String>()
        fun add(key: String, label: String, kind: BlockKind, value: Double, symbol: String? = null) {
            val b = blocks.getOrPut(key) { Acc(key, label, kind) }
            b.value += value
            if (symbol != null && symbol !in b.symbols) b.symbols += symbol
        }
        var stockValue = 0.0
        for (l in lines) {
            val value = max(0.0, l.value)
            if (l.kind == Kind.CRYPTO) {
                add("crypto", "Crypto", BlockKind.CRYPTO, value, l.symbol)
                continue
            }
            stockValue += value
            val s = sectors?.get(l.symbol)
            val sector = s?.sector?.takeIf { it.isNotEmpty() }
            val c = SectorClassification.of(s?.classification)
            if (sector != null && c != null) {
                if (counted.add(l.symbol)) bySource[c] = bySource.getValue(c) + 1
                if (c == SectorClassification.ETF) add("etf", ETF_BLOCK_LABEL, BlockKind.ETF, value, l.symbol)
                // Nasdaq and SEC are two classifications: never merged, the SEC's labelled "(SIC)".
                else add("${c.raw}:$sector", if (c == SectorClassification.SEC) "$sector (SIC)" else sector, BlockKind.SECTOR, value, l.symbol)
            } else {
                add("unknown", UNKNOWN_LABEL, BlockKind.UNKNOWN, value, l.symbol)
                unknown[l.symbol] = s?.reason ?: if (sectors != null) "absent de la réponse du serveur" else failure
            }
        }
        if (safeCash > 0) add("cash", "Liquidités", BlockKind.CASH, safeCash)

        fun pct(v: Double, of: Double) = if (of > 0) v / of * 100 else 0.0
        val collator = Collator.getInstance(Locale.FRENCH)
        val out = blocks.values.filter { it.value > 0 }.map { b ->
            val stock = b.kind == BlockKind.SECTOR || b.kind == BlockKind.ETF || b.kind == BlockKind.UNKNOWN
            ExposureBlock(b.key, b.label, b.kind, b.value, pct(b.value, total), if (stock) pct(b.value, stockValue) else null, b.symbols.toList())
        }.sortedWith { a, b -> if (a.weight != b.weight) b.weight.compareTo(a.weight) else collator.compare(a.label, b.label) }

        val sectorBlocks = out.filter { it.kind == BlockKind.SECTOR }
        val classified = sectorBlocks.sumOf { it.value }
        val hhi = if (classified > 0) sectorBlocks.sumOf { (it.value / classified).pow(2) } else 0.0
        val effectiveSectors = if (hhi > 0) 1 / hhi else null
        val stockLines = lines.filter { it.kind == Kind.STOCK && it.value > 0 }.map { it.symbol }.toSet().size

        val insights = mutableListOf<SectorInsight>()
        val top = sectorBlocks.firstOrNull()
        if (top != null && top.weight > MAX_WEIGHT) {
            insights += SectorInsight(SectorInsightLevel.WARNING, "sector_heavy", mapOf("sector" to top.label, "weight" to top.weight))
        } else if (top != null && stockLines >= 2 && top.stockWeight!! > MAX_STOCK_SHARE) {
            insights += SectorInsight(SectorInsightLevel.WARNING, "sector_heavy_stocks", mapOf("sector" to top.label, "share" to top.stockWeight))
        }
        if (effectiveSectors != null && sectorBlocks.sumOf { it.symbols.size } >= 2 && effectiveSectors < 2) {
            insights += SectorInsight(SectorInsightLevel.INFO, "sector_effective", mapOf("effective" to effectiveSectors))
        }
        out.firstOrNull { it.kind == BlockKind.ETF }?.let {
            insights += SectorInsight(SectorInsightLevel.INFO, "sector_etf", mapOf("weight" to it.weight, "symbols" to it.symbols.joinToString(", ")))
        }
        out.firstOrNull { it.kind == BlockKind.UNKNOWN }?.let {
            insights += SectorInsight(SectorInsightLevel.INFO, "sector_unknown", mapOf("weight" to it.weight, "symbols" to it.symbols.joinToString(", ")))
        }

        return SectorExposure(
            total, stockValue, out, effectiveSectors, pct(classified, stockValue),
            unknown.map { (symbol, reason) -> SectorUnknown(symbol, reason) }, bySource.toMap(), insights,
        )
    }

    private fun pc(v: Any?) = "${JsFormat.fr((v as? Double) ?: 0.0, 1)} %"

    fun insightText(i: SectorInsight): String {
        val v = i.values
        return when (i.code) {
            "sector_heavy" -> "${v["sector"]} pèse ${pc(v["weight"])} de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois."
            "sector_heavy_stocks" -> "${v["sector"]} représente ${pc(v["share"])} de vos actions : leur diversification sectorielle est faible."
            "sector_effective" -> "Vos actions classées équivalent à ${JsFormat.fr((v["effective"] as? Double) ?: 0.0, 1)} secteur(s) de même poids."
            "sector_etf" -> "ETF (${v["symbols"]}, ${pc(v["weight"])}) : leur répartition par secteur n'est pas couverte (composition non lue)."
            "sector_unknown" -> "Secteur non couvert pour ${v["symbols"]} (${pc(v["weight"])})."
            else -> i.code
        }
    }

    /** "Nasdaq (secteur du screener) · 3 actions ; Nasdaq Trader / SEC (ETF) · 1 action", empty without any stock classified. */
    fun sourceLine(by: Map<SectorClassification, Int>): String =
        SectorClassification.entries.filter { (by[it] ?: 0) > 0 }
            .joinToString(" ; ") { "${it.source} · ${by[it]} action${if (by.getValue(it) > 1) "s" else ""}" }
}
