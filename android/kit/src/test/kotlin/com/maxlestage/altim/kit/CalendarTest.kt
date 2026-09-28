package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import java.time.LocalDate
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** Same cases as web/test/agenda.test.ts, on the same real answer (web/test/calendar-sample.json, copied in the fixtures). */
class CalendarTest {
    private val raw = requireNotNull(javaClass.getResource("/fixtures/calendar-sample.json")).readText()
    private val report = AltimJson.decodeFromString(CalendarReport.serializer(), raw)

    @Test fun dayLabels() {
        val today = LocalDate.of(2026, 9, 28)
        assertEquals("Aujourd'hui", Calendar.dayLabel("2026-09-28", today))
        assertEquals("Demain", Calendar.dayLabel("2026-09-29", today))
        assertEquals("mer. 30 sept.", Calendar.dayLabel("2026-09-30", today))
        // Month end: tomorrow is the 1st of the next month.
        assertEquals("Demain", Calendar.dayLabel("2026-10-01", LocalDate.of(2026, 9, 30)))
        // Paris day: 23:30 UTC on 28/09 is already the 29th in Paris.
        assertEquals(LocalDate.of(2026, 9, 29), Calendar.today(1_790_638_200_000))
    }

    @Test fun groupedByDayInOrder() {
        val groups = Calendar.groupByDay(report.events)
        assertEquals(report.events.map { it.day }.distinct().sorted(), groups.map { it.day })
        assertEquals(report.events.size, groups.sumOf { it.events.size })
    }

    @Test fun filtersByKindAndMine() {
        assertTrue(Calendar.filterEvents(report.events, "ipo", null).all { it.kind == "ipo" })
        assertTrue(Calendar.filterEvents(report.events, "centralBank", null).isNotEmpty())
        // Mine: MU's earnings stay, other companies and IPOs go, the economy and central banks stay.
        val mine = Calendar.filterEvents(report.events, "all", listOf("MU"))
        assertEquals(listOf("MU"), mine.filter { it.kind == "earnings" }.map { it.symbol })
        assertTrue(mine.none { it.kind == "ipo" || it.kind == "dividend" })
        assertEquals(report.events.count { it.kind == "macro" }, mine.count { it.kind == "macro" })
        // No stock at all: only the economy and central banks.
        assertTrue(Calendar.filterEvents(report.events, "all", emptyList()).all { it.kind == "macro" || it.kind == "centralBank" })
    }

    @Test fun everyEventHasItsSourceAndALink() {
        for (e in report.events) {
            assertTrue(e.source.length > 3)
            assertTrue(e.url.startsWith("https://"))
            assertTrue(e.importance in setOf("high", "medium"))
            assertTrue(e.categoryLabel.isNotEmpty())
        }
        assertTrue(report.notCovered.isNotEmpty())
        assertEquals(null, Calendar.failedSources(report))
    }

    @Test fun symbolsSentAndAddress() = runBlocking {
        assertEquals(
            listOf("AAPL"),
            Calendar.stockSymbols(listOf(Asset("aapl", Kind.STOCK, "Apple"), Asset("BTC", Kind.CRYPTO, "Bitcoin"), Asset("AAPL", Kind.STOCK, "Apple"))),
        )
        assertEquals(mapOf("days" to "14"), Calendar.query(14, null))
        assertEquals(mapOf("days" to "14"), Calendar.query(14, emptyList()))
        val server = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse.Builder().code(200).body(raw).build())
            val r = AltimClient(server.url("/"), null).calendar(30, listOf("AAPL", "BRK-B"))
            assertEquals(report.events.size, r.events.size)
            val url = server.takeRequest().url
            assertEquals("/api/calendar", url.encodedPath)
            assertEquals("30", url.queryParameter("days"))
            assertEquals("AAPL,BRK-B", url.queryParameter("symbols"))
        } finally {
            server.close()
        }
    }

    @Test fun missingSourcesAreSaid() {
        val r = report.copy(sources = listOf(CalendarReport.Source("Nasdaq (calendrier économique)", false, listOf("2026-10-01", "2026-10-02"))))
        assertEquals("Sources incomplètes : Nasdaq (calendrier économique) (2 jours manquants).", Calendar.failedSources(r))
    }
}
