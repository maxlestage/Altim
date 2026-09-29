package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import java.nio.file.Files
import java.time.ZoneOffset
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

// Same cases and expected values as web/test/bot.test.ts, on the real /api/bot sample (backend/tests/samples/bot.json),
// plus the partial decoding, the `bot` field of a decision, the 202 polling answer, the views route and the offline cache.

private const val N = "\u202F"

class BotTest {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()
    private val report = AltimJson.decodeFromString(BotReport.serializer(), raw("bot.json"))

    @Test fun groupsAssetsAndSignalsAddUp() {
        assertEquals("/api/bot", Bot.PATH)
        assertEquals(listOf("stock", "crypto"), report.groups.map { it.id })
        assertEquals(34, report.assets.size + report.failures.size)
        assertEquals(report.groups.sumOf { it.buy.signals }, report.overall.buy.signals)
        assertEquals(report.groups.sumOf { it.sell.signals }, report.overall.sell.signals)
        for (g in report.groups) {
            assertEquals(g.labelled, g.calibrationUp.sumOf { it.rows })
            assertEquals(report.features.size, g.model!!.up.weights.size)
            val b = g.buy
            if (b.meanNet != null && b.baselineNet != null && b.excess != null) assertTrue(abs(b.meanNet!! - b.baselineNet!! - b.excess!!) < 0.02)
        }
        assertEquals(20, report.parameters.horizonDays)
        // The real run found no out-of-sample edge: the headline says it does not count.
        assertEquals(false, Bot.anyEdge(report))
        assertTrue(report.headline.startsWith("Hors échantillon, le bot n'a pas fait mieux"))
        assertTrue(report.headline.contains("il ne compte pas dans les décisions"))
        // Sample figures, as served.
        assertEquals(12504, report.overall.labelled)
        assertEquals("34 / 34", Bot.assetsTile(report))
        assertEquals("207 / 54", Bot.signalsTile(report))
        assertEquals(16, report.features.size)
        val stock = report.groups[0]
        assertEquals("Actions et ETF américains", stock.label)
        assertEquals("Avantage non démontré", stock.buy.verdictLabel)
        assertEquals("unproven", stock.buy.verdict)
        assertEquals(-1.1, stock.brierSkillUp)
        assertEquals("Trop peu de ventes pour conclure", report.groups[1].sell.verdictLabel)
        val aapl = report.assets[0]
        assertEquals(Asset("AAPL", Kind.STOCK, "Apple"), aapl.asset)
        assertEquals("wait", aapl.now.action)
        assertEquals(56.0, aapl.now.up)
    }

    @Test fun screenTexts() {
        val stock = report.groups[0]
        assertEquals(
            "108 achats : +1,7$N% en moyenne sur 20 jours, contre +1,85$N% pour une entrée au hasard sur le même actif et la même période (−0,15 point, t = −0,1).",
            Bot.buyText(stock.buy),
        )
        assertEquals(
            "47 ventes : le cours a fait +2,01$N% dans les 20 jours suivants, contre +1,74$N% après un jour au hasard (cours ensuite supérieur de 0,27 point, t = −0,3).",
            Bot.sellText(stock.sell),
        )
        assertEquals("ATTENDRE 85$N% des jours testés, suivis en moyenne de +0,98$N% (tous les jours : +1,39$N%).", Bot.waitText(stock.wait))
        assertEquals("22 actifs · test du oct. 2024 au sept. 2026 · 4 réentraînements", Bot.groupSubtitle(stock))
        assertEquals(
            "Horizon 20 jours · seuil : fréquence d'entraînement + 5 points · coûts d'un aller-retour 0,31 % (actions), 0,32 % (cryptos). Calculé le 29/09/2026 11:36, réentraîné toutes les 12 h.",
            Bot.parametersText(report, ZoneOffset.UTC),
        )
        assertEquals("Aujourd'hui : hausse 56$N%, baisse 40$N%", Bot.todayText(report.assets[0]))
        assertEquals(
            "Test : 3 achats (−7,12 points vs hasard) · 3 ventes (cours ensuite −1,42 point vs hasard) · attente 90$N% · achats cumulés −19,6$N%, détention +45,7$N% (Robinhood)",
            Bot.assetTestText(report.assets[0]),
        )
        assertEquals(listOf("40 à 50 %", "50 à 60 %", "60 à 70 %", "70 % et plus"), Bot.calibrationRows(stock.calibrationUp).map { it.label })
        assertTrue(Bot.sourceText(report).startsWith("Panier fixé le 29/09/2026. Source : "))
        assertEquals("2 actifs non utilisés :", Bot.failuresTitle(2))
        assertEquals("1 actif non utilisé :", Bot.failuresTitle(1))
    }

    @Test fun pointsSkillCalibrationRows() {
        assertEquals("+0,52 point", Bot.points(0.52))
        assertEquals("−2,44 points", Bot.points(-2.44))
        assertEquals("—", Bot.points(null))
        assertEquals("1,1$N% moins bien que la fréquence de base", Bot.skillText(-1.1))
        assertEquals("2$N% mieux que la fréquence de base", Bot.skillText(2.0))
        assertEquals("pas mieux que la fréquence de base", Bot.skillText(0.2))
        assertEquals("non calculable", Bot.skillText(null))
        assertEquals(emptyList(), Bot.calibrationRows(listOf(BotBucket(0.0, 30.0, "x", 0, null, null))))
        assertEquals(mapOf("symbols" to "BTC:crypto,AAPL:stock"), Bot.viewsQuery(listOf(Asset("BTC", Kind.CRYPTO, "Bitcoin"), Asset("AAPL", Kind.STOCK, "Apple"))))
        assertEquals(20, Bot.viewsQuery(List(30) { Asset("A$it", Kind.STOCK, "") }).getValue("symbols").split(",").size)
        assertEquals("—", Bot.pct0(null))
        assertEquals("prévu 55$N% · observé —", Bot.bucketText(BotBucket(predicted = 55.2)))
    }

    @Test fun sellTextSaysWhetherThePriceDidBetterOrWorseWithoutASignToDecode() {
        val s = BotSellStats(
            signals = 7, meanAfter = -1.01, baselineAfter = -3.45, allDaysAfter = 1.6, avoided = -2.44, tStat = -0.66, fallRate = 71.4, baselineFallRate = 45.7,
            meanDrawdown = -11.0, baselineDrawdown = -8.8, verdict = "insufficient", verdictLabel = "Trop peu de ventes pour conclure",
        )
        assertTrue(Bot.sellText(s).contains("cours ensuite supérieur de 2,44 points"))
        assertTrue(Bot.sellText(s.copy(avoided = 1.2)).contains("cours ensuite inférieur de 1,2 point"))
        assertEquals("Aucune vente pendant les périodes de test.", Bot.sellText(s.copy(signals = 0)))
        assertEquals("Aucun achat pendant les périodes de test.", Bot.buyText(report.groups[0].buy.copy(signals = 0)))
        assertTrue(Bot.waitText(report.groups[0].wait).startsWith("ATTENDRE"))
        assertEquals("Jamais sur ATTENDRE pendant les tests.", Bot.waitText(BotWaitStats()))
    }

    private val view = BotView(
        available = true, group = "crypto", groupLabel = "Cryptos", inBasket = true, action = "wait", actionLabel = "ATTENDRE",
        up = 46.3, down = 52.0, thresholdUp = 49.7, thresholdDown = 58.4, baseUp = 44.7, baseDown = 53.4, buyVerdict = "unproven", sellVerdict = "insufficient",
        counts = false, time = 1.0, contributions = listOf(BotContribution("rsi14", "RSI 14", 40.0, "40", -0.2, "down", "RSI 14 : 40 (pèse contre la hausse)")),
        text = "ATTENDRE : …", note = "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.", asOf = 1.0, link = "/app/bot",
    )

    @Test fun decisionLine() {
        assertEquals("ATTENDRE · hausse 46$N%, baisse 52$N%", Bot.summary(view))
        assertEquals("unproven", view.tone)
        assertEquals("edge", view.copy(counts = true).tone)
        assertEquals("na", view.copy(available = false).tone)
        assertEquals("Probabilités à 20 jours : hausse 46$N% (seuil 50$N%), baisse 52$N% (seuil 58$N%)", Bot.probabilitiesText(view))
        assertTrue(Bot.probabilitiesText(view.copy(inBasket = false)).endsWith(" · modèle des cryptos, non testé sur cet actif"))
        assertTrue(Bot.probabilitiesText(view.copy(inBasket = false, group = "stock")).endsWith(" · modèle des actions, non testé sur cet actif"))
        assertEquals("RSI 14 : 40 (pèse contre la hausse)", Bot.contributionsText(view))
        assertEquals("Hausse 46$N% (seuil 50$N%), baisse 52$N% (seuil 58$N%)", Bot.viewText(view))
        assertTrue(Bot.viewText(view.copy(inBasket = false)).endsWith(" · hors du panier testé"))
        val na = view.copy(available = false, action = null, contributions = emptyList(), text = "Bot pas encore entraîné")
        assertEquals("Bot pas encore entraîné", Bot.summary(na))
        assertEquals("Bot pas encore entraîné", Bot.viewText(na))
        assertEquals("VENDRE", Bot.actionUi("sell")?.label)
        assertEquals(Tone.GOOD, Bot.actionUi("buy")?.tone)
        assertNull(Bot.actionUi("hold"))
    }

    @Test fun partialAnswersDecode() {
        val r = AltimJson.decodeFromString(BotReport.serializer(), """{"headline":"x","groups":[{"id":"crypto","buy":{"signals":3}}],"assets":[{"symbol":"BTC","kind":"crypto","now":{}}]}""")
        assertEquals("crypto", r.groups[0].id)
        assertEquals(3, r.groups[0].buy.signals)
        assertEquals(Kind.CRYPTO, r.assets[0].kind)
        assertNull(r.assets[0].now.action)
        assertEquals(20, r.parameters.horizonDays)
        // A decision without `bot` (older server) and with it.
        val old = AltimJson.decodeFromString(Decision.serializer(), raw("decision-aapl-v2.json"))
        assertNull(old.bot)
        val withBot = AltimJson.decodeFromString(
            Decision.serializer(),
            """{"symbol":"BTC","bot":{"available":true,"group":"crypto","action":"sell","up":30,"down":62.4,"counts":true,"contributions":[{"text":"t"}],"future":1}}""",
        )
        assertEquals("sell", withBot.bot?.action)
        assertEquals(true, withBot.bot?.counts)
        assertEquals("/app/bot", withBot.bot?.link)
        assertEquals("VENDRE · hausse 30$N%, baisse 62$N%", Bot.summary(withBot.bot!!))
        val views = AltimJson.decodeFromString(BotViews.serializer(), """{"asOf":null,"views":[{"available":false,"text":"Bot pas encore entraîné","symbol":"AAPL","kind":"stock"}]}""")
        assertEquals("AAPL", views.views[0].symbol)
        assertEquals(Kind.STOCK, views.views[0].kind)
        assertNull(views.asOf)
    }

    @Test fun pendingThenReadyThenOfflineAndViews() = runBlocking {
        val server = MockWebServer().apply { start() }
        val dir = Files.createTempDirectory("altim-bot").toFile()
        try {
            AltimClient.retryDelaysMs = listOf(10L, 10L)
            assertTrue("/api/bot" in AltimClient.CACHEABLE)
            assertTrue("/api/bot/views" in AltimClient.CACHEABLE)
            val c = AltimClient(server.url("/"), null, cache = FileResponseCache(dir))
            server.enqueue(MockResponse.Builder().code(202).body("""{"pending":true}""").build())
            server.enqueue(MockResponse.Builder().code(200).body(raw("bot.json")).build())
            assertIs<BotResult.Pending>(c.bot())
            assertEquals("/api/bot", server.takeRequest().url.encodedPath)
            assertEquals(34, assertIs<BotResult.Ready>(c.bot()).report.assets.size)
            server.takeRequest()
            server.enqueue(MockResponse.Builder().code(200).body("""{"asOf":1,"views":[{"available":true,"action":"buy","symbol":"BTC","kind":"crypto"}]}""").build())
            val v = c.botViews(listOf(Asset("BTC", Kind.CRYPTO, "Bitcoin")))
            assertEquals("buy", v.views[0].action)
            val req = server.takeRequest()
            assertEquals("/api/bot/views", req.url.encodedPath)
            assertEquals("BTC:crypto", req.url.queryParameter("symbols"))
            assertEquals(BotViews(), c.botViews(emptyList()))
            // Offline: the last report.
            server.close()
            assertEquals(207, assertIs<BotResult.Ready>(c.bot()).report.overall.buy.signals)
        } finally {
            server.close()
            dir.deleteRecursively()
        }
    }
}
