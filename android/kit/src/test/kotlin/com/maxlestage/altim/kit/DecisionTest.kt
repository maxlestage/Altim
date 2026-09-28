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

    /** Older answers (before the P/S, ROIC, valuation history… fields) still parse, with the new fields empty. */
    @Test fun olderAnswersKeepParsing() {
        val a = assertIs<Fundamentals.Stock>(AltimJson.decodeFromString(Decision.serializer(), sample("decision-aapl.json")).fundamentals)
        assertNull(a.ps)
        assertNull(a.valuationHistory)
        assertNull(a.peers)
        assertEquals("", a.guidance)
        assertFalse(a.roicTaxStatutory)
        val b = AltimJson.decodeFromString(Decision.serializer(), sample("decision-btc.json"))
        val c = assertIs<Fundamentals.Crypto>(b.fundamentals)
        assertFalse(c.devActivityKnown)
        assertNull(c.stablecoins)
        val t = assertNotNull(b.track)
        assertFalse(t.hasDetails)
        assertNull(t.regimes)
        assertTrue(t.biasNotes.isEmpty())
    }

    /** A real answer of the current server (Apple, 28/09/2026): every added field, as the server writes it. */
    @Test fun stockFundamentalsAndTrackDetails() {
        val d = AltimJson.decodeFromString(Decision.serializer(), sample("decision-aapl-v2.json"))
        val f = assertIs<Fundamentals.Stock>(d.fundamentals)
        assertEquals(10.58, f.ps)
        assertEquals(45.93, f.pb)
        assertEquals(84.1, f.roic)
        assertEquals(17.3, f.roicTaxRate)
        assertFalse(f.roicTaxStatutory)
        assertEquals(1_782_518_400_000.0, f.periodEnd)
        assertEquals(1_785_456_000_000.0, f.filedAt)
        assertEquals("3571", f.sector?.sic)
        val h = assertNotNull(f.valuationHistory)
        assertEquals(1255, h.per?.days)
        assertEquals(94.0, h.per?.percentile)
        assertEquals(5.04, h.ps?.min)
        assertEquals(99.0, h.ps?.percentile)
        val p = assertNotNull(f.peers)
        assertEquals(listOf("DELL", "IBM", "SMCI", "HPQ", "OMCL", "OSS"), p.peers.map { it.symbol })
        assertEquals(26.35, p.medianPer)
        assertNull(p.peers[1].operatingMargin)
        assertTrue(f.valuationVerdict!!.startsWith("Valorisation élevée"))
        assertTrue(f.guidance.contains("non disponibles"))
        val t = assertNotNull(d.track)
        assertTrue(t.hasDetails)
        assertEquals(0.0059, t.spreadPct)
        assertTrue(t.spreadMeasured)
        assertEquals(0.1, t.avgR)
        assertEquals(1194, t.testedBars)
        assertEquals(listOf("bull", "bear", "range", "crisis", "unknown"), t.regimes!!.map { it.regime })
        assertNull(t.regimes!![1].winRate)
        assertEquals(5, t.biasNotes.size)
    }

    @Test fun ratingScoreRegimeHorizonStructure() {
        val d = AltimJson.decodeFromString(Decision.serializer(), sample("decision-aapl-v2.json"))
        assertEquals(Rating.HOLD, d.rating)
        assertEquals("ATTENDRE", d.headlineLabel)
        val s = assertNotNull(d.score)
        assertEquals(24.0, s.value)
        assertEquals(listOf("tech", "mom", "fund", "sent", "news", "macro"), s.factors.map { it.key })
        assertNull(s.factors[3].value)
        assertEquals(listOf("Sentiment"), s.missing)
        assertFalse(d.degraded!!.active)
        assertEquals(RegimeKind.NEUTRAL, d.marketRegime?.kind)
        assertEquals(3, d.marketRegime?.reasons?.size)
        assertEquals("mediumTerm", d.horizon?.kind)
        val plan = assertNotNull(d.plan)
        assertEquals(36.8, plan.reward3Pct)
        assertTrue(plan.target3Source!!.startsWith("Projection"))
        val st = assertNotNull(d.structure)
        assertEquals(70.0, st.score)
        assertEquals(Bias.BULLISH, st.ichimoku?.bias)
        assertEquals(16, st.supertrend?.bars)
        assertEquals(11, st.levels.size)
        assertEquals("fake", st.breakout?.kind)
        assertEquals(listOf("SPY", "QQQ"), st.relative.map { it.symbol })
        assertEquals(3, st.relative[0].periods.size)
        // The calendar was not verified here: events null, but the field is there.
        assertTrue(d.eventsKnown)
        assertNull(d.events)
        // The calendar check is one veto among the others (any code is shown as given).
        val a = d.vetoes.first { it.code == "announcement" }
        assertEquals("Annonce économique dans les 48 h", a.label)
        assertFalse(a.verifiable)
        assertEquals(16, d.vetoes.size)
        // Older answers: no rating, no score, no events field.
        val old = AltimJson.decodeFromString(Decision.serializer(), sample("decision-btc.json"))
        assertNull(old.rating)
        assertEquals(old.verdictLabel, old.headlineLabel)
        assertNull(old.score)
        assertNull(old.structure)
        assertFalse(old.eventsKnown)
        assertNull(old.plan!!.target3)
        // An unknown rating or bias falls back instead of failing the whole answer.
        val odd = AltimJson.decodeFromString(Decision.serializer(), sample("decision-btc.json").replaceFirst("{", "{\"rating\":\"moon\",\"events\":[],"))
        assertNull(odd.rating)
        assertEquals(emptyList(), odd.events)
        assertTrue(odd.eventsKnown)
    }

    @Test fun macroRegime() {
        val m = AltimJson.decodeFromString(
            MacroInfo.serializer(),
            """{"score":30,"level":"tense","regime":{"kind":"riskOff","label":"Risk-off","benchmark":"S&P 500 (SPY)","reasons":["VIX 31"]}}""",
        )
        assertEquals(RegimeKind.RISK_OFF, m.regime?.kind)
        assertNull(AltimJson.decodeFromString(MacroInfo.serializer(), """{"score":3,"level":"calm"}""").regime)
    }

    @Test fun scoreWeights() {
        // Same cases as web/test/decision.test.ts ("composite score weights").
        assertNull(ScoreWeights.DEFAULT.param())
        val w = ScoreWeights.DEFAULT.copy(tech = 50, macro = 0)
        assertEquals("tech:50,mom:18,fund:20,sent:10,news:10,macro:0", w.param())
        assertEquals(ScoreWeights(100, 0, 13, 10, 10, 10), ScoreWeights.parse("""{"tech":140,"mom":-3,"fund":12.6,"sent":"x"}"""))
        assertEquals(ScoreWeights.DEFAULT, ScoreWeights.parse(null))
        assertEquals(ScoreWeights.DEFAULT, ScoreWeights.parse("""{"tech":0,"mom":0,"fund":0,"sent":0,"news":0,"macro":0}"""))
        assertEquals("+34", signedScore(34.4))
        assertEquals("−12", signedScore(-12.0))
    }

    @Test fun scoreWeightsSentOnlyWhenCustom() = runBlocking {
        val server = MockWebServer().apply { start() }
        try {
            val client = AltimClient(server.url("/"), null)
            val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
            server.enqueue(MockResponse.Builder().code(200).body(sample("decision-btc.json")).build())
            client.decision(btc, scoreWeights = ScoreWeights.DEFAULT)
            assertNull(server.takeRequest().url.queryParameter("w"))
            server.enqueue(MockResponse.Builder().code(200).body(sample("decision-btc.json")).build())
            client.decision(btc, scoreWeights = ScoreWeights.DEFAULT.copy(tech = 50, macro = 0))
            assertEquals("tech:50,mom:18,fund:20,sent:10,news:10,macro:0", server.takeRequest().url.queryParameter("w"))
        } finally {
            server.close()
        }
    }

    /** Real answers with the calendar verified: 7 events, the announcement check active; Bitcoin's degraded signal. */
    @Test fun eventsAndDegradedSignal() {
        val a = AltimJson.decodeFromString(Decision.serializer(), sample("decision-aapl-events.json"))
        val ev = assertNotNull(a.events)
        assertEquals(7, ev.size)
        assertEquals("Croissance (PIB)", ev[0].title)
        assertEquals("2026-09-30", ev[0].day)
        val v = a.vetoes.first { it.code == "announcement" }
        assertTrue(v.active)
        assertTrue(v.detail.startsWith("Croissance (PIB)"))
        val b = AltimJson.decodeFromString(Decision.serializer(), sample("decision-btc-v2.json"))
        val g = assertNotNull(b.degraded)
        assertTrue(g.active)
        assertTrue(g.headline.startsWith("⚠️ Signal dégradé"))
        assertEquals(1, g.reasons.size)
        assertNull(b.structure?.nearestResistance)
    }

    @Test fun cryptoDevActivityAndStablecoins() {
        val d = AltimJson.decodeFromString(Decision.serializer(), sample("decision-btc-v2.json"))
        val f = assertIs<Fundamentals.Crypto>(d.fundamentals)
        // The field is there, null: "non disponible" is said (not hidden as for an older answer).
        assertTrue(f.devActivityKnown)
        assertNull(f.devActivity)
        val s = assertNotNull(f.stablecoins)
        assertEquals("Tous réseaux", s.scope)
        assertEquals(313_077_011_773.0, s.total)
        assertEquals(1.36, s.change30dPct)
        assertNull(f.chainStablecoins)
        assertTrue(f.notCovered.contains("baleines"))
        // The figures of CoinGecko's developer data (backend/tests/decision_data.rs), on Ethereum's chain.
        val eth = AltimJson.decodeFromString(
            FundamentalsSerializer,
            """{"kind":"crypto","unlocks":"","source":"x","devActivity":{"repo":null,"commits4w":97,"pullRequestsMerged":11200,"contributors":850,"stars":48000,""" +
                """"additions4w":5210,"deletions4w":3120,"smartContractPlatform":true,"source":"CoinGecko"},""" +
                """"chainStablecoins":{"scope":"Ethereum","date":1790553600000,"total":148452954020,"change7d":null,"change7dPct":null,"change30d":null,"change30dPct":-0.18,"source":"DefiLlama (stablecoins)"}}""",
        )
        val e = assertIs<Fundamentals.Crypto>(eth)
        assertTrue(e.devActivityKnown)
        assertEquals(97.0, e.devActivity?.commits4w)
        assertTrue(e.devActivity!!.smartContractPlatform)
        assertEquals("Ethereum", e.chainStablecoins?.scope)
        assertEquals(-0.18, e.chainStablecoins?.change30dPct)
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
