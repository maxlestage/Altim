package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

private fun sample(name: String): String =
    requireNotNull(DecisionTest::class.java.getResource("/fixtures/$name")) { name }.readText()

/** Contract of /api/decision (backend/tests/samples, same files as the server's test). */
class DecisionTest {
    @Test fun informationalBitcoin() {
        val d = AltimJson.decodeFromString(Decision.serializer(), sample("decision-btc.json"))
        assertEquals("BTC", d.symbol)
        assertEquals(Kind.CRYPTO, d.kind)
        assertFalse(d.isPersonal)
        assertEquals(Verdict.WAIT, d.verdict)
        assertEquals("ATTENDRE", d.verdictLabel)
        assertEquals(DecisionLevel.WAITING, d.level)
        assertEquals(67.0, d.confidence)
        assertTrue(d.families.size >= 9)
        assertEquals(FamilyStatus.UNAVAILABLE, d.families.first { it.key == "valuation" }.status)
        assertNull(d.families.first { it.key == "valuation" }.score)
        assertTrue(d.blocked)
        // Active vetoes first, "non vérifiable" last.
        assertTrue(d.sortedVetoes.first().active)
        assertFalse(d.sortedVetoes.last().verifiable)
        val setup = assertNotNull(d.setup)
        assertEquals(9, setup.steps.size)
        // The sample says 3 met while 4 steps are "ok": the app shows the server's count as given.
        assertEquals(3, setup.met)
        assertEquals(9, setup.total)
        val plan = assertNotNull(d.plan)
        assertFalse(plan.acceptable)
        assertEquals(92300.0, plan.target2)
        assertEquals(3, d.whyWait.size)
        assertEquals(listOf(ScenarioKind.BULL, ScenarioKind.NEUTRAL, ScenarioKind.BEAR), d.scenarios.map { it.kind })
        assertEquals(Uncertainty.MEDIUM, d.whyNot?.uncertainty)
        val f = assertIs<Fundamentals.Crypto>(d.fundamentals)
        assertEquals(0.948, f.mcFdv)
        assertNull(f.tvl)
        assertTrue(f.unlocks.startsWith("Sans objet"))
        assertEquals(14, d.track?.trades)
        assertNull(d.position)
        assertNull(d.exposure)
        assertTrue(d.disclaimer.startsWith("Mode informationnel"))
    }

    @Test fun personalApple() {
        val d = AltimJson.decodeFromString(Decision.serializer(), sample("decision-aapl.json"))
        assertEquals(Kind.STOCK, d.kind)
        assertTrue(d.isPersonal)
        assertEquals(Verdict.TRIM, d.verdict)
        assertEquals("ALLÉGER", d.verdictLabel)
        assertEquals(DecisionLevel.MODERATE, d.level)
        val f = assertIs<Fundamentals.Stock>(d.fundamentals)
        assertEquals(46.6, f.per)
        assertEquals(true, f.nextEarnings?.estimated)
        assertEquals("T2 2026", f.surprises.single().quarter)
        assertEquals(-0.2, f.revisions?.changePct)
        val p = assertNotNull(d.position)
        assertEquals(275.0, p.cost)
        assertEquals(4, p.exits.size)
        assertTrue(p.exits.first().now)
        assertEquals(ExitKind.MACRO, p.exits.last().kind)
        assertNull(p.exits.last().price)
        val e = assertNotNull(d.exposure)
        assertEquals("S&P 500", e.factor)
        assertEquals(62.0, e.weight)
        assertNotNull(e.warning)
        assertTrue(d.disclaimer.contains("jamais conservés"))
    }

    @Test fun unknownValuesDoNotBreakDecoding() {
        val raw = sample("decision-btc.json")
            .replace("\"verdict\": \"wait\"", "\"verdict\": \"accumulate\"")
            .replace("\"level\": \"waiting\"", "\"level\": \"extreme\"")
            .replace("\"status\": \"positive\"", "\"status\": \"mixed\"")
            .replace("\"state\": \"ok\"", "\"state\": \"partial\"")
            .replace("\"kind\": \"bull\"", "\"kind\": \"euphoria\"")
            .replace("\"uncertainty\": \"medium\"", "\"uncertainty\": \"huge\"")
            .replace("\"kind\": \"crypto\",\n  \"marketCap\"", "\"kind\": \"etf\",\n  \"marketCap\"")
            .replace("\"symbol\": \"BTC\",", "\"symbol\": \"BTC\", \"newField\": {\"a\": 1},")
        val d = AltimJson.decodeFromString(Decision.serializer(), raw)
        assertEquals(Verdict.WAIT, d.verdict)
        assertEquals("ATTENDRE", d.verdictLabel) // the server's label stays
        assertEquals(DecisionLevel.WAITING, d.level)
        assertEquals(FamilyStatus.UNAVAILABLE, d.families.first().status)
        assertEquals(StepState.UNKNOWN, d.setup?.steps?.first()?.state)
        assertEquals(ScenarioKind.NEUTRAL, d.scenarios.first().kind)
        assertEquals(Uncertainty.MEDIUM, d.whyNot?.uncertainty)
        assertEquals(Fundamentals.Other("etf"), d.fundamentals)
        // A minimal answer decodes too.
        val min = AltimJson.decodeFromString(Decision.serializer(), """{"symbol":"X","kind":"bond","verdict":"buy"}""")
        assertNull(min.kind)
        assertEquals(Verdict.BUY, min.verdict)
        assertEquals("ACHETER", min.verdictLabel)
    }

    @Test fun weightsAreSharesOnly() {
        val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
        val aapl = Asset("AAPL", Kind.STOCK, "Apple")
        val eth = Asset("ETH", Kind.CRYPTO, "Ethereum")
        val holdings = listOf(
            Holding(asset = btc, quantity = 0.5, averagePrice = 60_000.0), // 40 000
            Holding(asset = aapl, quantity = 100.0, averagePrice = 200.0), // 30 000
            Holding(asset = btc, quantity = 0.25, averagePrice = 90_000.0), // 20 000 (same asset, merged)
            Holding(asset = eth, quantity = 3.0), // no price: left out
        )
        val prices = mapOf(btc.id to 80_000.0, aapl.id to 300.0)
        val w = assertNotNull(Decision.weights(holdings, prices))
        assertEquals("BTC:crypto:66.7,AAPL:stock:33.3", w)
        // Never a quantity nor an amount.
        assertFalse(w.contains("0.5") || w.contains("60000") || w.contains("30000"))
        assertNull(Decision.weights(holdings, emptyMap()))
        // 20 lines at most, the largest kept.
        val many = (1..25).map { Holding(asset = Asset("S$it", Kind.STOCK, "S$it"), quantity = it.toDouble()) }
        val w2 = assertNotNull(Decision.weights(many, many.associate { it.asset.id to 1.0 }))
        assertEquals(20, w2.split(",").size)
        assertTrue(w2.startsWith("S25:stock:"))
        assertFalse(w2.contains("S5:stock"))
        // Average cost weighted by quantity; unknown when a line has none.
        assertEquals(70_000.0, Decision.cost(holdings, btc)!!, 1e-9)
        assertNull(Decision.cost(holdings, eth))
        assertNull(Decision.cost(holdings, Asset("SOL", Kind.CRYPTO, "Solana")))
    }

    @Test fun requestAndOfflineCache() = runBlocking {
        val server = MockWebServer().apply { start() }
        val dir = Files.createTempDirectory("altim-decision").toFile()
        try {
            AltimClient.retryDelaysMs = listOf(10L, 10L)
            val client = AltimClient(server.url("/"), null, cache = FileResponseCache(dir))
            val aapl = Asset("AAPL", Kind.STOCK, "Apple")
            server.enqueue(MockResponse.Builder().code(200).body(sample("decision-aapl.json")).build())
            assertEquals(Verdict.TRIM, client.decision(aapl, 275.5, "AAPL:stock:10,BTC:crypto:35.2").verdict)
            val url = server.takeRequest().url
            assertEquals("/api/decision", url.encodedPath)
            assertEquals("AAPL", url.queryParameter("symbol"))
            assertEquals("stock", url.queryParameter("kind"))
            assertEquals("275.5", url.queryParameter("cost"))
            assertEquals("AAPL:stock:10,BTC:crypto:35.2", url.queryParameter("weights"))
            // Informational: no personal parameter at all.
            server.enqueue(MockResponse.Builder().code(200).body(sample("decision-btc.json")).build())
            client.decision(Asset("BTC", Kind.CRYPTO, "Bitcoin"))
            val u2 = server.takeRequest().url
            assertNull(u2.queryParameter("cost"))
            assertNull(u2.queryParameter("weights"))
            // Offline: the last answer of the same request.
            server.close()
            assertEquals(Verdict.TRIM, client.decision(aapl, 275.5, "AAPL:stock:10,BTC:crypto:35.2").verdict)
        } finally {
            server.close()
            dir.deleteRecursively()
        }
    }
}
