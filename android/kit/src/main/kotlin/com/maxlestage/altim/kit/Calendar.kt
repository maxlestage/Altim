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

    /**
     * Query of /api/calendar: no symbol (or none given) → the whole calendar, filtered on the phone for "Mes actifs".
     * [top]: the company events of [symbols] and of the largest companies together (the risk view), only with symbols
     * (without, the calendar already keeps the largest companies).
     */
    fun query(days: Int, symbols: List<String>?, top: Boolean = false): Map<String, String> =
        mapOf("days" to days.toString()) +
            (if (symbols.isNullOrEmpty()) emptyMap() else mapOf("symbols" to symbols.joinToString(",")) + (if (top) mapOf("top" to "1") else emptyMap()))

    // ---------- Risk by day (web calendar.ts `riskDays`) ----------

    /** 🔴 high, 🟠 medium, 🟢 low. */
    enum class RiskLevel(val icon: String, val label: String, val rank: Int) {
        HIGH("🔴", "Risque élevé", 2),
        MEDIUM("🟠", "Risque modéré", 1),
        LOW("🟢", "Risque faible", 0),
    }

    /** The categories whose high-importance releases make a 🔴 day. */
    private val MAJOR = setOf("tauxDirecteurs", "inflation", "emploi", "pib")

    /**
     * Risk of one event, null when it does not count (IPOs, other companies' dividends and splits).
     * - 🔴: a high-importance central bank decision / inflation (CPI) / jobs / GDP release, or the earnings of a stock the
     *   user holds or watches;
     * - 🟠: the other macro and central bank events, the earnings of other companies (the calendar keeps the largest US
     *   ones only), a dividend or split of a held stock.
     */
    fun eventRisk(e: CalendarEvent, held: List<String>, watched: List<String>): RiskLevel? {
        val sym = e.symbol?.let(::norm) ?: ""
        val isHeld = sym.isNotEmpty() && held.any { norm(it) == sym }
        val isMine = isHeld || (sym.isNotEmpty() && watched.any { norm(it) == sym })
        return when (e.kind) {
            "macro", "centralBank" -> if (e.importance == "high" && e.category in MAJOR) RiskLevel.HIGH else RiskLevel.MEDIUM
            "earnings" -> if (isMine) RiskLevel.HIGH else RiskLevel.MEDIUM
            "dividend", "split" -> if (isHeld) RiskLevel.MEDIUM else null
            else -> null
        }
    }

    private val ACRONYM = Regex("\\(([^)]*[A-Z]{2,}[^)]*)\\)\\s*$")

    /** Short name of an event for the risk row: "CPI", "Décision de taux de la Fed", "Résultats AAPL"… */
    fun shortTitle(e: CalendarEvent): String {
        if (e.kind == "earnings" && !e.symbol.isNullOrEmpty()) return "Résultats ${e.symbol}"
        if (e.kind == "dividend" && !e.symbol.isNullOrEmpty()) return "Dividende ${e.symbol}"
        if (e.kind == "split" && !e.symbol.isNullOrEmpty()) return "Split ${e.symbol}"
        // The acronym in brackets when there is one ("Inflation (CPI)" → "CPI"), else the whole title.
        val paren = if (e.kind == "macro") ACRONYM.find(e.title)?.groupValues?.get(1) else null
        val name = paren ?: e.title
        return if (!e.country.isNullOrEmpty() && e.country != "États-Unis" && e.kind == "macro") "$name (${e.country})" else name
    }

    data class RiskEvent(val event: CalendarEvent, val risk: RiskLevel)

    data class RiskDay(
        /** "YYYY-MM-DD". */
        val day: String,
        /** "Lun. 28 sept." */
        val label: String,
        val weekend: Boolean,
        val level: RiskLevel,
        /** Short names of the events at the day's level, in the calendar's order, without repeats. */
        val main: List<String>,
        /** Every event that counts, with its risk. */
        val events: List<RiskEvent>,
        /** A source could not be read for this day: 🟢 is then not asserted. */
        val incomplete: Boolean,
    ) {
        /** Sources incomplete and nothing found: "⚪ Risque non évalué". */
        val unknown: Boolean get() = incomplete && level == RiskLevel.LOW
    }

    /** "Lun. 28 sept." (a calendar day, read without time zone). */
    fun riskDayLabel(day: String): String {
        val d = runCatching { LocalDate.parse(day) }.getOrNull() ?: return day
        return DateTimeFormatter.ofPattern("EEE d MMM", Locale.FRANCE).format(d).replaceFirstChar { it.titlecase(Locale.FRANCE) }
    }

    /**
     * The [n] days from [from] ("YYYY-MM-DD", the report's first day, Paris time), weekends included, each with its
     * risk: the highest of its events' ([eventRisk]), 🟢 when none counts. [failed]: days a source could not read.
     */
    fun riskDays(events: List<CalendarEvent>, from: String, held: List<String>, watched: List<String>, n: Int = 7, failed: List<String> = emptyList()): List<RiskDay> {
        val start = runCatching { LocalDate.parse(from) }.getOrNull() ?: return emptyList()
        return (0 until n).map { i ->
            val date = start.plusDays(i.toLong())
            val day = date.toString()
            val counted = events.filter { it.day == day }.mapNotNull { e -> eventRisk(e, held, watched)?.let { RiskEvent(e, it) } }
            val level = counted.fold(RiskLevel.LOW) { l, x -> if (x.risk.rank > l.rank) x.risk else l }
            val main = counted.filter { it.risk == level }.map { shortTitle(it.event) }.distinct()
            val dow = date.dayOfWeek
            RiskDay(day, riskDayLabel(day), dow == java.time.DayOfWeek.SATURDAY || dow == java.time.DayOfWeek.SUNDAY, level, main, counted, day in failed)
        }
    }

    /** Days a source could not read ("YYYY-MM-DD" only: the IPO months do not change the risk). */
    fun failedDays(r: CalendarReport): List<String> = r.sources.flatMap { s -> s.failed.filter { it.length == 10 } }.distinct()

    /** Text of a risk row: the day's main events (3 at most, "+2" for the rest), or why there is none. */
    fun riskText(d: RiskDay): String = when {
        d.unknown -> "Sources incomplètes : risque non évalué"
        d.level == RiskLevel.LOW -> "Aucun événement majeur"
        else -> d.main.take(3).joinToString(" · ") + if (d.main.size > 3) " +${d.main.size - 3}" else ""
    }

    const val RISK_RULE = "🔴 décision de taux d'une banque centrale, inflation (CPI), emploi ou PIB d'importance haute, ou résultats d'une action de vos avoirs ou de votre radar. " +
        "🟠 autres publications économiques et banques centrales, résultats des grandes capitalisations américaines, dividende ou split d'une action détenue. " +
        "🟢 aucun de ces événements. Heures et jours de Paris ; seules les sources de l'Agenda sont prises en compte (voir « Non couvert »)."

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
