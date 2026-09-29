package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import kotlin.math.abs
import kotlin.math.pow
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

// Same cases and expected numbers as web/test/sectors.test.ts, plus the decoding of the /api/sectors contract (every
// field optional) and the address asked.

private fun close(expected: Double, actual: Double?, digits: Int = 9) =
    assertTrue(actual != null && abs(expected - actual) < 0.5 * 10.0.pow(-digits), "attendu $expected, obtenu $actual")

class SectorsTest {
    private fun item(symbol: String, sector: String?, classification: String?, reason: String? = null) =
        SectorItem(symbol, sector, classification, if (classification != null) "test" else null, reason, classification == "etf")

    private val sectors = mapOf(
        "AAPL" to item("AAPL", "Technologie", "nasdaq"),
        "NVDA" to item("NVDA", "Technologie", "nasdaq"),
        "JPM" to item("JPM", "Finance", "nasdaq"),
        "BRK-B" to item("BRK-B", "Finance et immobilier", "sec"),
        "SPY" to item("SPY", "ETF / fonds indiciel (plusieurs secteurs)", "etf"),
        "ABCD" to item("ABCD", null, null, "Secteur non couvert : aucun dépôt à la SEC"),
    )

    private fun stock(s: String, v: Double) = SectorLine(s, Kind.STOCK, v)
    private fun crypto(s: String, v: Double) = SectorLine(s, Kind.CRYPTO, v)

    @Test fun blocksInPercentOfThePortfolioHeaviestFirst() {
        val e = Sectors.exposure(
            listOf(stock("AAPL", 3000.0), stock("NVDA", 1000.0), stock("JPM", 1000.0), stock("SPY", 1000.0), stock("ABCD", 500.0), crypto("BTC", 2000.0), crypto("ETH", 500.0)),
            1000.0,
            sectors,
        )
        assertEquals(10000.0, e.total)
        assertEquals(6500.0, e.stockValue)
        assertEquals(
            listOf("Technologie" to 40.0, "Crypto" to 25.0, Sectors.ETF_BLOCK_LABEL to 10.0, "Finance" to 10.0, "Liquidités" to 10.0, Sectors.UNKNOWN_LABEL to 5.0),
            e.blocks.map { it.label to it.weight },
        )
        val tech = e.blocks[0]
        close(4000.0 / 6500 * 100, tech.stockWeight)
        assertEquals(listOf("AAPL", "NVDA"), tech.symbols)
        assertNull(e.blocks.first { it.kind == BlockKind.CRYPTO }.stockWeight)
        // Weights add up to 100 %.
        close(100.0, e.blocks.sumOf { it.weight })
        // Effective sectors over Technologie 4000 / Finance 1000: 1 / (0.8² + 0.2²).
        close(1 / 0.68, e.effectiveSectors)
        close(5000.0 / 6500 * 100, e.classifiedShare)
        assertEquals(mapOf(SectorClassification.NASDAQ to 3, SectorClassification.SEC to 0, SectorClassification.ETF to 1), e.bySource)
        assertEquals(listOf(SectorUnknown("ABCD", "Secteur non couvert : aucun dépôt à la SEC")), e.unknown)
        assertEquals(listOf("sector_heavy", "sector_effective", "sector_etf", "sector_unknown"), e.insights.map { it.code })
        assertEquals(
            "Technologie pèse 40 % de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois.",
            Sectors.insightText(e.insights[0]),
        )
        assertEquals("Nasdaq (secteur du screener) · 3 actions ; Nasdaq Trader / SEC (ETF) · 1 action", Sectors.sourceLine(e.bySource))
    }

    @Test fun concentrationOfTheStockPartOnlySecAndNasdaqNeverMerged() {
        val e = Sectors.exposure(listOf(stock("AAPL", 2000.0), stock("JPM", 1000.0), stock("BRK-B", 500.0), crypto("BTC", 6500.0)), 0.0, sectors)
        assertEquals(listOf("Crypto", "Technologie", "Finance", "Finance et immobilier (SIC)"), e.blocks.map { it.label })
        assertEquals(SectorInsight(SectorInsightLevel.WARNING, "sector_heavy_stocks", mapOf("sector" to "Technologie", "share" to 2000.0 / 3500 * 100)), e.insights[0])
        close(1 / ((2 / 3.5).pow(2) + (1 / 3.5).pow(2) + (0.5 / 3.5).pow(2)), e.effectiveSectors)
        // One stock line only: 100 % of the stock part is not flagged (the line concentration is, elsewhere).
        val one = Sectors.exposure(listOf(stock("AAPL", 1000.0), crypto("BTC", 9000.0)), 0.0, sectors)
        assertEquals(emptyList(), one.insights.map { it.code })
    }

    @Test fun serverUnreachableEveryStockIsUnknownWithTheReason() {
        val e = Sectors.exposure(listOf(stock("AAPL", 1000.0), crypto("BTC", 1000.0)), 0.0, null, "HTTP 502")
        assertEquals(listOf(BlockKind.CRYPTO to 50.0, BlockKind.UNKNOWN to 50.0), e.blocks.map { it.kind to it.weight })
        assertNull(e.effectiveSectors)
        assertEquals(0.0, e.classifiedShare)
        assertEquals(listOf(SectorUnknown("AAPL", "HTTP 502")), e.unknown)
        // Empty portfolio: no block, no division by zero.
        val empty = Sectors.exposure(emptyList(), 0.0, emptyMap())
        assertEquals(Triple(0.0, 0, null), Triple(empty.total, empty.blocks.size, empty.effectiveSectors))
    }

    @Test fun absentFromTheAnswerOrUnknownClassificationIsUnknown() {
        val e = Sectors.exposure(listOf(stock("ZZZ", 100.0), stock("NEW", 100.0)), 0.0, mapOf("NEW" to item("NEW", "Espace", "autre")))
        assertEquals(listOf(Sectors.UNKNOWN_LABEL), e.blocks.map { it.label })
        assertEquals(listOf(SectorUnknown("ZZZ", "absent de la réponse du serveur"), SectorUnknown("NEW", "absent de la réponse du serveur")), e.unknown)
        assertEquals("", Sectors.sourceLine(e.bySource))
    }

    @Test fun decodesThePartialAnswersOfTheServer() {
        val full = """{"asOf":1790000000000,"items":[{"symbol":"NVDA","sector":"Technologie","classification":"nasdaq",
            "source":"Nasdaq (secteur du screener des actions US), activité : Semiconductors","reason":null,"etf":false,
            "sec":{"label":"Industrie manufacturière","sic":"3674","sicDescription":"Semiconductors","source":"SEC"},
            "nasdaq":{"sector":"Technology","sectorFr":"Technologie","industry":"Semiconductors"}},
            {"symbol":"ABCD","sector":null,"classification":null,"source":null,"reason":"Secteur non couvert","etf":false,"sec":null,"nasdaq":null}],
            "sources":[{"name":"Nasdaq","ok":true,"error":null},{"name":"SEC","ok":false,"error":"HTTP 503"}]}"""
        val r = AltimJson.decodeFromString(SectorsReport.serializer(), full)
        assertEquals("Semiconductors", r.items[0].nasdaq?.industry)
        assertEquals("3674", r.items[0].sec?.sic)
        assertNull(r.items[1].sector)
        assertEquals("HTTP 503", r.sources[1].error)
        val bare = AltimJson.decodeFromString(SectorsReport.serializer(), """{"items":[{"symbol":"AAPL","etf":null}]}""")
        assertEquals(SectorItem("AAPL"), bare.items.single())
        assertEquals(emptyList(), AltimJson.decodeFromString(SectorsReport.serializer(), "{}").sources)
    }

    @Test fun asksTheStockSymbolsOnce() = runBlocking {
        val server = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse.Builder().code(200).body("""{"asOf":1,"items":[{"symbol":"AAPL","sector":"Technologie","classification":"nasdaq"}],"sources":[]}""").build())
            val c = AltimClient(server.url("/"), null)
            assertEquals("Technologie", c.sectors(listOf("AAPL", "NVDA", "AAPL")).items.single().sector)
            val u = server.takeRequest().url
            assertEquals("/api/sectors", u.encodedPath)
            assertEquals("AAPL,NVDA", u.queryParameter("symbols"))
            assertTrue("/api/sectors" in AltimClient.CACHEABLE)
        } finally {
            server.close()
        }
    }
}
