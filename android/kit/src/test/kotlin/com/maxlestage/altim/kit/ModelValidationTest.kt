package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import java.nio.file.Files
import java.text.Collator
import java.time.ZoneOffset
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

// Same cases and expected values as web/test/validation.test.ts, on the real /api/validation sample
// (backend/tests/samples/validation.json), plus the partial decoding, the 202 polling answer and the offline cache.

private const val N = " "

class ModelValidationTest {
    private fun raw(name: String) = requireNotNull(javaClass.getResource("/fixtures/$name")).readText()
    private val report = AltimJson.decodeFromString(ValidationReport.serializer(), raw("validation.json"))

    @Test fun everyBasketAssetIsTestedOrListedAsAFailureAndGroupsAddUp() {
        assertEquals("/api/validation", ModelValidation.PATH)
        assertEquals(report.basket.size, report.assets.size + report.failures.size)
        assertEquals(report.assets.sumOf { it.trades }, report.overall.trades)
        assertEquals(report.overall.assets, report.classes.sumOf { it.assets })
        assertEquals(report.overall.trades, report.overall.regimes.sumOf { it.trades })
        assertEquals(report.assets.count { it.beatHold }, report.overall.beatHold)
        assertEquals(ModelValidation.CLASS_ORDER.filter { c -> report.classes.any { it.id == c } }, report.classes.map { it.id })
        assertEquals("notVerifiable", report.outOfSample.status)
        assertEquals(30.0, report.parameters.taxRatePct)
        assertTrue(report.headline.startsWith("Sur ${report.overall.assets} actifs"))
        // Sample figures, as served.
        assertEquals(34, report.overall.assets)
        assertEquals(822, report.overall.trades)
        assertEquals("Avantage non démontré", report.overall.verdictLabel)
        assertEquals("LINK", report.overall.worstReturn?.symbol)
    }

    @Test fun theListKeepsTheBasketOrderByDefaultNeverRankedByPerformance() {
        val byClass = ModelValidation.sortAssets(report.assets, ModelValidation.Sort.CLASS)
        assertEquals(report.assets.map { it.symbol }, byClass.map { it.symbol })
        val byGap = ModelValidation.sortAssets(report.assets, ModelValidation.Sort.GAP).map { it.gap }
        assertEquals(byGap.sortedDescending(), byGap)
        val byName = ModelValidation.sortAssets(report.assets, ModelValidation.Sort.NAME).map { it.symbol }
        val collator = Collator.getInstance(Locale.FRENCH)
        assertEquals(byName.sortedWith(collator), byName)
        assertEquals(report.basket[0].symbol, report.assets[0].symbol)
        // Sorting never changes the report.
        assertEquals("AAPL", report.assets[0].symbol)
        assertEquals("AVAX", ModelValidation.sortAssets(report.assets, ModelValidation.Sort.GAP).first().symbol)
        assertEquals("NVDA", ModelValidation.sortAssets(report.assets, ModelValidation.Sort.GAP).last().symbol)
    }

    private val g = ValRegimeGroup(
        regime = "bear", label = "Marché baissier", trades = 7, winRate = 42.9, profitFactor = 0.8, expectancy = -0.31, avgR = -0.1, tStat = -0.4, days = 3922, assets = 20,
        beatHold = 7, beatShare = 35.0, medianSignal = -1.5, medianHold = -9.25, lowSample = true, verdict = "insufficient", verdictLabel = "Échantillon trop faible",
    )

    @Test fun numbersInFrenchWithSigns() {
        assertEquals("+12,3$N%", ModelValidation.signedPct(12.345))
        assertEquals("−4$N%", ModelValidation.signedPct(-4.0))
        assertEquals("—", ModelValidation.signedPct(null))
        // Rounded like Intl (from the shortest decimal writing): 9.01 − 95.56 = −86.55 → −86,6.
        assertEquals("−86,6$N%", ModelValidation.signedPct(9.01 - 95.56))
        assertEquals("6 sur 34 (18$N%)", ModelValidation.beatText(6, 34, 17.6))
        assertEquals("aucun actif comparable", ModelValidation.beatText(0, 0, null))
        assertEquals("7 trades · réussite 43$N% · espérance −0,31$N% · facteur de profit 0,8 · t = −0,4", ModelValidation.pooledText(g))
        assertEquals("Aucun trade.", ModelValidation.pooledText(g.copy(trades = 0)))
        assertTrue(ModelValidation.regimeDaysText(g).contains("7 sur 20 (35$N%) actifs"))
        assertTrue(ModelValidation.regimeDaysText(g.copy(assets = 0)).contains("Aucun actif"))
        assertEquals(Tone.GOOD, ModelValidation.verdictTone("edge"))
        assertEquals(Tone.WARN, ModelValidation.verdictTone("negative"))
        assertEquals(Tone.NEUTRAL, ModelValidation.verdictTone("unproven"))
        assertEquals("4,8${N}ans", ModelValidation.years(4.8))
        assertEquals("1${N}an", ModelValidation.years(1.0))
        assertEquals("0,5 à 4,8${N}ans", ModelValidation.historyTile(report))
        assertEquals("14${N}469 jours-actifs dans ce régime.", ModelValidation.regimeDays(report.overall.regimes[0]))
        assertEquals("déc. 2021", ModelValidation.monthYear(report.from))
        assertTrue(ModelValidation.periodText(report, ZoneOffset.UTC).startsWith("Période : déc. 2021 – sept. 2026 (bougies journalières). Calculé le 29/09/2026 "))
        assertEquals("Signal +9$N% · détention +95,6$N% (moins bien, écart −86,6$N%)", ModelValidation.assetLine(report.assets[0]))
        assertTrue(ModelValidation.costsText(report).contains("frais 0,1$N%, glissement 0,05$N%"))
        assertTrue(ModelValidation.costsText(report).contains("Panier fixé le 29/09/2026."))
        assertEquals("2 actifs non testés :", ModelValidation.failuresTitle(2))
    }

    @Test fun unknownRegimeLast() {
        val order = ModelValidation.regimesOf(report.overall).map { it.regime }
        assertEquals("unknown", order.last())
        assertEquals(listOf("bull", "bear", "range", "crisis"), order.take(4))
    }

    @Test fun partialAnswerDecodes() {
        val r = AltimJson.decodeFromString(ValidationReport.serializer(), """{"headline":"x","assets":[{"symbol":"BTC","kind":"crypto","winRate":null}],"overall":{"regimes":[{}]}}""")
        assertEquals("BTC", r.assets[0].symbol)
        assertEquals(Kind.CRYPTO, r.assets[0].kind)
        assertEquals(0, r.overall.trades)
        assertEquals("Aucun trade.", ModelValidation.pooledText(r.overall))
        assertEquals("—", ModelValidation.historyTile(r))
        assertEquals("?", ModelValidation.monthYear(r.from))
    }

    @Test fun pendingThenReadyThenOffline() = runBlocking {
        val server = MockWebServer().apply { start() }
        val dir = Files.createTempDirectory("altim-validation").toFile()
        try {
            AltimClient.retryDelaysMs = listOf(10L, 10L)
            assertTrue("/api/validation" in AltimClient.CACHEABLE)
            val c = AltimClient(server.url("/"), null, cache = FileResponseCache(dir))
            server.enqueue(MockResponse.Builder().code(202).body("""{"pending":true}""").build())
            server.enqueue(MockResponse.Builder().code(200).body(raw("validation.json")).build())
            assertIs<ValidationResult.Pending>(c.validation())
            assertEquals("/api/validation", server.takeRequest().url.encodedPath)
            assertEquals(34, assertIs<ValidationResult.Ready>(c.validation()).report.assets.size)
            // Offline: the last report.
            server.close()
            assertEquals(822, assertIs<ValidationResult.Ready>(c.validation()).report.overall.trades)
        } finally {
            server.close()
            dir.deleteRecursively()
        }
    }
}
