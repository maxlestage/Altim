package com.maxlestage.altim

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.CalendarReport
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.ClusterInput
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.ConfigState
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.DecisionLevel
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.RiskPortfolio
import com.maxlestage.altim.kit.RiskSettings
import com.maxlestage.altim.kit.Verdict
import com.maxlestage.altim.ui.AgendaUi
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.ConfigChangesCard
import com.maxlestage.altim.ui.DangerBlock
import com.maxlestage.altim.ui.DangersNotice
import com.maxlestage.altim.ui.LimitsCard
import com.maxlestage.altim.ui.RiskBanners
import com.maxlestage.altim.ui.SettingsScreen
import com.maxlestage.altim.ui.StressCard
import com.maxlestage.altim.ui.agendaItems
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.time.LocalDate

private class NewCardsMemory : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

private const val DAY = 86_400_000.0
private const val T0 = 1_767_225_600_000.0

private fun series(closes: List<Double>, start: Double = T0): List<Candle> =
    closes.mapIndexed { i, c -> Candle(start + i * DAY, c, c * 1.01, c * 0.99, c, 1.0) }

private fun returns(n: Int, seed: Long): List<Double> {
    var x = seed
    return List(n) {
        x = (x * 16807) % 2147483647
        (x.toDouble() / 2147483647 - 0.5) * 0.06
    }
}

private fun path(r: List<Double>, start: Double = 100.0): List<Double> {
    val out = mutableListOf(start)
    r.forEach { out += out.last() * (1 + it) }
    return out
}

/**
 * The cards added with the portfolio risk, the configuration changes, the agenda and the new settings, rendered with
 * the shared samples (no server) on a narrow 360 dp phone. Screenshots in app/build/screens/new-*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h3000dp-xhdpi", application = android.app.Application::class)
class NewCardsTest {
    @get:Rule val compose = createComposeRule()

    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    /** "10 404 $" written with the narrow no-break space of the French format between digits. */
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d)"), " ")
    private fun expect(vararg texts: String) = texts.map(::fr).forEach { assertTrue("absent : $it", has(it)) }

    /** Nothing wider than the screen: no horizontal scroll at 360 dp. */
    private fun fitsWidth() {
        val width = compose.onRoot().fetchSemanticsNode().boundsInRoot.width
        fun walk(n: SemanticsNode) {
            assertTrue("dépasse à droite : ${n.config} (${n.boundsInRoot.right} > $width)", n.boundsInRoot.right <= width + 1f)
            n.children.forEach(::walk)
        }
        walk(compose.onRoot(useUnmergedTree = true).fetchSemanticsNode())
    }

    private fun screen(content: @androidx.compose.runtime.Composable () -> Unit) {
        compose.setContent { AltimTheme { AppBackground { content() } } }
        compose.waitForIdle()
    }

    private fun decision(name: String): Decision = AltimJson.decodeFromString(Decision.serializer(), File("../kit/src/test/resources/fixtures/$name").readText())

    /** Limits, crisis scenarios and dangerous positions of a portfolio in trouble (the kit test's generated data). */
    @Test fun riskCards() {
        val bench = returns(120, 11)
        val eth = series(path(bench))
        val sol = series(path(bench.mapIndexed { i, x -> x * 1.2 + (if (i % 2 == 1) 0.001 else -0.001) }))
        val aapl = series(path(returns(120, 99)))
        val btc = series(path(bench.map { it * 0.8 }))
        val daily = mapOf("crypto:ETH" to eth, "crypto:SOL" to sol, "stock:AAPL" to aapl, "crypto:BTC" to btc)
        val holdings = listOf(
            Holding("1", Asset("ETH", Kind.CRYPTO, "Ethereum"), 1.0, eth.last().close, stop = eth[eth.size - 2].close * 0.95),
            Holding("2", Asset("SOL", Kind.CRYPTO, "Solana"), 1.0, sol.last().close),
            Holding("3", Asset("AAPL", Kind.STOCK, "Apple"), 1.0, aapl.last().close),
        )
        val prices = mapOf("crypto:ETH" to eth[eth.size - 2].close * 0.9, "crypto:SOL" to sol[sol.size - 2].close * 0.9, "stock:AAPL" to aapl[aapl.size - 2].close * 0.9)
        val p = RiskPortfolio.of(holdings, 0.0, prices, daily)
        val settings = RiskSettings(maxPositionPercent = 20.0, maxCryptoPercent = 30.0)
        val stops = holdings.associate { it.id to it.stop }
        val clusters = PortfolioRisk.correlatedClusters(p.lines.map { ClusterInput(it.symbol, it.weight, daily.getValue(it.key)) })
        val limits = PortfolioRisk.checkLimits(p, settings, clusters, PortfolioRisk.dailyChange(p, daily, T0 + 120 * DAY + 3_600_000), stops)
        val dangers = PortfolioRisk.dangerousPositions(p, settings, daily, stops)
        val betas = PortfolioRisk.betas(holdings, daily)
        screen {
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                RiskBanners(limits, dangers)
                dangers.forEach { DangerBlock(it) }
                LimitsCard(limits)
                StressCard(p, PortfolioRisk.stressTest(p, betas), betas)
            }
        }
        expect(
            PortfolioRisk.DAILY_LOSS_REACHED, "Positions devenues dangereuses", "à vous de décider",
            "Vos limites de risque", "Poids max d'une ligne (20 %)", "Part crypto (max 30 %)", "Actifs corrélés", "Perte du jour (max 3 %)",
            "Réglages → Prudence des conseils", "Scénarios de crise", "Marchés −10 %", "Actions −10 %, crypto −30 %", "Ligne la plus touchée",
            "Bêta estimé sur 90 jours", "Historique trop court (moins de 30 jours communs) ou indisponible pour AAPL : bêta 1 retenu", "les corrélations montent",
        )
        assertTrue(dangers.any { d -> d.reasons.any { it.code == "stop_broken" } })
        expect("Stop cassé")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/new-risk.png")
    }

    /** Radar: the dangers kept from Mes avoirs and the configuration changes, then clearing asks first. */
    @Test fun radarCards() {
        val btc = decision("decision-btc.json")
        var s = ConfigChanges.apply(ConfigState(), btc, false, 1_790_600_000_000.0).state
        s = ConfigChanges.apply(s, btc.copy(verdict = Verdict.BUY_ZONE, label = "ZONE D'ACHAT", level = DecisionLevel.MODERATE, levelLabel = "Signal modéré"), false, 1_790_610_000_000.0).state
        s = ConfigChanges.apply(s, btc.copy(level = DecisionLevel.HIGH_RISK, levelLabel = "Risque élevé"), true, 1_790_620_000_000.0).state
        val dangers = PortfolioRisk.parseDangers(
            PortfolioRisk.encodeDangers(
                PortfolioRisk.dangerousPositions(
                    RiskPortfolio.of(listOf(Holding("a", Asset("BTC", Kind.CRYPTO, "Bitcoin"), 1.0, 100.0, 101.5)), 0.0, mapOf("crypto:BTC" to 101.0), emptyMap()),
                    RiskSettings.DEFAULT, emptyMap(), mapOf("a" to 101.5),
                ),
                1_790_620_000_000.0,
            ),
        )!!
        var cleared = false
        screen {
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                DangersNotice(dangers)
                ConfigChangesCard(s.transitions, open = {}, onClear = { cleared = true })
            }
        }
        expect(
            "Position devenue dangereuse dans vos avoirs", "BTC : Stop cassé", "Mesuré le", "ouvrez-le pour actualiser",
            "Changements de configuration · 1", "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT",
            "niveau Attente → Signal modéré", "configuration précédente vue le", "Conditions manquantes :", "Ce qui changerait la décision :",
            "50 derniers changements conservés",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/new-radar.png")
        compose.onNode(hasText("Effacer")).performClick()
        compose.waitForIdle()
        assertTrue(compose.onAllNodes(isDialog()).fetchSemanticsNodes().isNotEmpty())
        expect("Effacer l'historique des changements ?")
        assertTrue(!cleared)
    }

    private val report = AltimJson.decodeFromString(CalendarReport.serializer(), File("../kit/src/test/resources/fixtures/calendar-sample.json").readText())

    /** Agenda of the real sample (28/09/2026), all kinds, then "Mes actifs" with Micron only. */
    @Test @Config(qualifiers = "w360dp-h7000dp-xhdpi")
    fun agenda() {
        val mineState = androidx.compose.runtime.mutableStateOf(false)
        screen {
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                agendaItems(AgendaUi(report, null, false, 14, "all", mineState.value, listOf("MU"), today = LocalDate.of(2026, 9, 28), onMine = { mineState.value = !mineState.value }))
            }
        }
        expect(
            "Agenda", "Heures de Paris", "7 jours", "14 jours", "30 jours", "Tout", "Banques centrales", "Dividendes", "IPO", "Mes actifs",
            "AUJOURD'HUI", "DEMAIN", "Détachement du dividende de Keurig Dr Pepper Inc.", "Résultats de Carnival Corporation Ltd.", "avant l'ouverture",
            "Consensus BPA \$1.36", "Source : Nasdaq (calendrier des résultats)", "Non couvert", "Déblocages de jetons", "Sources · 7/7 en ligne",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/new-agenda.png")
        compose.onNode(hasText("Mes actifs")).performClick()
        compose.waitForIdle()
        expect("✓ Mes actifs", "Résultats, dividendes et splits de vos 1 action")
        assertTrue(!has("Carnival"))
        assertTrue(!has("Keurig"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/new-agenda-mine.png")
    }

    /** Réglages → Prudence des conseils: steppers within bounds, saved on this phone; the holding's stop is kept. */
    @Test fun settingsAndStorage() {
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        app.getSharedPreferences("altim", Context.MODE_PRIVATE).edit().clear()
            .putString("holdings", """[{"id":"1","asset":{"symbol":"BTC","kind":"crypto","name":"Bitcoin"},"quantity":1,"averagePrice":10,"stop":-4},{"id":"2","asset":{"symbol":"ETH","kind":"crypto","name":"Ethereum"},"quantity":1,"stop":5}]""")
            .putString(ConfigChanges.KEY, "{damaged").commit()
        val model = AppModel(app, NewCardsMemory())
        // A stop that is not a positive number is dropped, the line stays.
        assertEquals(listOf(null, 5.0), model.holdings.map { it.stop })
        assertEquals(ConfigState(), model.configChanges)
        assertEquals(RiskSettings.DEFAULT, model.risk)
        screen { SettingsScreen(model, Modifier.fillMaxSize()) }
        expect("Prudence des conseils", "Risque accepté par idée", "Taille max d'une ligne", "Perte max du jour", "Part crypto max", "60 %", "Règle professionnelle", "Valeurs recommandées")
        compose.onNode(hasContentDescription("Augmenter Part crypto max")).performClick()
        compose.onNode(hasContentDescription("Diminuer Perte max du jour")).performClick()
        compose.waitForIdle()
        assertEquals(65.0, model.risk.maxCryptoPercent, 0.0)
        assertEquals(2.5, model.risk.dailyLossLimitPercent, 0.0)
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/new-settings.png")
        // Kept after a restart, with the configuration changes seen meanwhile.
        val btc = decision("decision-btc.json")
        model.recordDecision(btc, false, 1.0)
        model.recordDecision(btc.copy(verdict = Verdict.BUY), false, 2.0)
        val again = AppModel(app, NewCardsMemory())
        assertEquals(65.0, again.risk.maxCryptoPercent, 0.0)
        assertEquals(listOf(Verdict.BUY), again.configChanges.transitions.map { it.to.verdict })
        again.clearTransitions()
        assertTrue(AppModel(app, NewCardsMemory()).configChanges.transitions.isEmpty())
    }
}
