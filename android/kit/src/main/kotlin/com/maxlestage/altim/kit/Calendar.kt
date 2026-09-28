package com.maxlestage.altim.kit

import kotlinx.serialization.Serializable
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

// Agenda (GET /api/calendar?days=&symbols=): economic releases, central bank decisions, earnings, dividends, splits and
// IPOs, each with its source. Types and the pure helpers of the view (day labels, grouping, filters), port of
// web/src/webapp/calendar.ts tested on the same sample (CalendarTest ↔ web/test/agenda.test.ts). Kinds and categories
// are kept as the server's strings: a value added later is shown as given instead of breaking the answer.

@Serializable
data class CalendarEvent(
    /** Exact instant (ms) when the source gives a time, otherwise 00:00 UTC of `day`. */
    val date: Double = 0.0,
    /** "YYYY-MM-DD" (Paris time when the time is known). */
    val day: String,
    /** "14:30" (Paris time) or, for earnings, "avant l'ouverture" / "après la clôture". */
    val time: String? = null,
    /** "macro" | "earnings" | "dividend" | "split" | "ipo" | "centralBank". */
    val kind: String,
    val category: String = "",
    /** "high" | "medium". */
    val importance: String = "medium",
    val title: String,
    val originalName: String? = null,
    val country: String? = null,
    val symbol: String? = null,
    val actual: String? = null,
    val consensus: String? = null,
    val previous: String? = null,
    val detail: String? = null,
    val note: String? = null,
    val source: String = "",
    val url: String = "",
) {
    val high: Boolean get() = importance == "high"
    val categoryLabel: String get() = Calendar.CATEGORY_LABEL[category] ?: category
}

@Serializable
data class CalendarReport(
    val asOf: Double = 0.0,
    val days: Int = 14,
    val from: String = "",
    val to: String = "",
    val events: List<CalendarEvent> = emptyList(),
    val sources: List<Source> = emptyList(),
    val notCovered: List<String> = emptyList(),
) {
    @Serializable
    data class Source(val name: String, val ok: Boolean, val failed: List<String> = emptyList(), val error: String? = null)
}

data class CalendarDay(val day: String, val events: List<CalendarEvent>)

object Calendar {
    private val PARIS: ZoneId = ZoneId.of("Europe/Paris")

    /** Filters of the view: all, then one per kind. */
    val FILTERS = listOf(
        "all" to "Tout",
        "macro" to "Macro",
        "centralBank" to "Banques centrales",
        "earnings" to "Résultats",
        "dividend" to "Dividendes",
        "split" to "Splits",
        "ipo" to "IPO",
    )

    val DAYS = listOf(7 to "7 jours", 14 to "14 jours", 30 to "30 jours")

    val CATEGORY_LABEL = mapOf(
        "tauxDirecteurs" to "Taux directeurs",
        "inflation" to "Inflation",
        "emploi" to "Emploi",
        "pib" to "PIB",
        "activite" to "Activité",
        "discours" to "Discours",
        "resultats" to "Résultats",
        "dividende" to "Dividende",
        "split" to "Split",
        "ipo" to "IPO",
    )

    /** Today in Paris: the days of the calendar are Paris days. */
    fun today(now: Long = System.currentTimeMillis()): LocalDate = java.time.Instant.ofEpochMilli(now).atZone(PARIS).toLocalDate()

    /** "Aujourd'hui", "Demain", else "mer. 30 sept." (`today`: the viewer's Paris day). */
    fun dayLabel(day: String, today: LocalDate = today()): String {
        val d = runCatching { LocalDate.parse(day) }.getOrNull() ?: return day
        if (d == today) return "Aujourd'hui"
        if (d == today.plusDays(1)) return "Demain"
        return DateTimeFormatter.ofPattern("EEE d MMM", Locale.FRANCE).format(d)
    }

    private val COMPANY = setOf("earnings", "dividend", "split")
    private fun norm(s: String) = s.uppercase(Locale.ROOT).replace('.', '-').replace('/', '-')

    /**
     * Events of the chosen filter. `mine` (the user's stock symbols) keeps the company events of these stocks only and
     * leaves the IPOs out; the economy and central banks concern every asset and always stay.
     */
    fun filterEvents(events: List<CalendarEvent>, filter: String, mine: List<String>?): List<CalendarEvent> {
        val own = mine?.map(::norm)?.toSet()
        return events.filter { e ->
            if (filter != "all" && e.kind != filter) return@filter false
            if (own == null) return@filter true
            if (e.kind == "ipo") return@filter false
            e.kind !in COMPANY || (e.symbol != null && e.symbol.isNotEmpty() && norm(e.symbol) in own)
        }
    }

    /** Days in order, each with its events in the server's order. */
    fun groupByDay(events: List<CalendarEvent>): List<CalendarDay> {
        val days = LinkedHashMap<String, MutableList<CalendarEvent>>()
        for (e in events.sortedBy { it.day }) days.getOrPut(e.day) { mutableListOf() } += e
        return days.map { (day, list) -> CalendarDay(day, list) }
    }

    /** The stock symbols of the user's radar and holdings (company events exist for stocks only), 50 at most. */
    fun stockSymbols(assets: List<Asset>): List<String> =
        assets.filter { it.kind == Kind.STOCK }.map { it.symbol.uppercase(Locale.ROOT) }.distinct().take(50)

    /** Query of /api/calendar: no symbol (or none given) → the whole calendar, filtered on the phone for "Mes actifs". */
    fun query(days: Int, symbols: List<String>?): Map<String, String> =
        mapOf("days" to days.toString()) + (if (symbols.isNullOrEmpty()) emptyMap() else mapOf("symbols" to symbols.joinToString(",")))

    /** "⚠ Sources incomplètes : …" line, null when every source answered. */
    fun failedSources(r: CalendarReport): String? {
        val failed = r.sources.filter { !it.ok }
        if (failed.isEmpty()) return null
        return "Sources incomplètes : " + failed.joinToString(", ") { s ->
            val n = s.failed.size
            s.name + if (n > 0) " ($n jour${if (n > 1) "s" else ""} manquant${if (n > 1) "s" else ""})" else ""
        } + "."
    }
}
