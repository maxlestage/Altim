package com.maxlestage.altim

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.hasText
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
import com.maxlestage.altim.kit.Bot
import com.maxlestage.altim.kit.BotContribution
import com.maxlestage.altim.kit.BotReport
import com.maxlestage.altim.kit.BotView
import com.maxlestage.altim.kit.BotViews
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.BotAltimView
import com.maxlestage.altim.ui.DecisionView
import com.maxlestage.altim.ui.LocalOpenBot
import com.maxlestage.altim.ui.LocalOpenValidation
import com.maxlestage.altim.ui.SettingsScreen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

private class BotMemory : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

/**
 * « Bot Altim » rendered from the real /api/bot sample (backend/tests/samples/bot.json) on a narrow 360 dp phone with
 * the no-overflow check, its card in Réglages and its line in the Décision card. Screenshots in app/build/screens/bot*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h22000dp-xhdpi", application = android.app.Application::class)
class BotScreenTest {
    @get:Rule val compose = createComposeRule()

    private fun fixture(name: String) = File("../kit/src/test/resources/fixtures/$name").readText()
    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    /** "12 504" and "54 %" written with the narrow no-break space of the French format. */
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d|%)"), "\u202F")
    private fun expect(vararg texts: String) = texts.map(::fr).forEach { assertTrue("absent : $it", has(it)) }

    private fun fitsWidth() {
        val width = compose.onRoot().fetchSemanticsNode().boundsInRoot.width
        fun walk(n: SemanticsNode) {
            assertTrue("dépasse à droite : ${n.config} (${n.boundsInRoot.right} > $width)", n.boundsInRoot.right <= width + 1f)
            n.children.forEach(::walk)
        }
        walk(compose.onRoot(useUnmergedTree = true).fetchSemanticsNode())
    }

    private fun click(text: String) {
        compose.onAllNodes(hasText(fr(text), substring = true), useUnmergedTree = true)[0].performClick()
        compose.waitForIdle()
    }

    private val view = BotView(
        available = true, group = "crypto", groupLabel = "Cryptos (bitcoin, ether, altcoins)", inBasket = true, action = "wait", actionLabel = "ATTENDRE",
        up = 46.3, down = 52.0, thresholdUp = 49.7, thresholdDown = 58.4, baseUp = 44.7, baseDown = 53.4, buyVerdict = "unproven", sellVerdict = "insufficient",
        counts = false, time = 1.0, contributions = listOf(BotContribution("rsi14", "RSI 14", 40.0, "40", -0.2, "down", "RSI 14 : 40 (pèse contre la hausse)")),
        text = "ATTENDRE : …", note = "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.", asOf = 1_790_681_774_058.0, link = "/app/bot",
        symbol = "BTC", kind = Kind.CRYPTO,
    )

    @Test fun wholeScreenFromTheSample() {
        val report = AltimJson.decodeFromString(BotReport.serializer(), fixture("bot.json"))
        val views = BotViews(
            asOf = report.asOf,
            views = listOf(
                view,
                view.copy(symbol = "PEPE", action = "buy", counts = true, inBasket = false, up = 61.0),
                BotView(available = false, text = "Pas assez d'historique pour un avis.", symbol = "XYZ", kind = Kind.STOCK),
            ),
        )
        var shown by mutableStateOf<BotReport?>(report)
        var pending by mutableStateOf(false)
        var opened: Asset? = null
        compose.setContent {
            AltimTheme { AppBackground { BotAltimView(shown, null, pending, Modifier.fillMaxSize(), watched = true, views = views, open = { opened = it }) } }
        }
        compose.waitForIdle()
        expect(
            "Bot Altim", "Il n'est jugé que sur des périodes qu'il n'avait pas vues. Altim ne passe aucun ordre.",
            "Actifs", "34 / 34", "Jours testés", "12 504", "Achats / ventes", "207 / 54",
            "Vos actifs aujourd'hui", "BTC", "ne compte pas", "compte", "Hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)", "· hors du panier testé",
            "Pas assez d'historique pour un avis.", "pas d'avis",
            "Comment il apprend et comment il est jugé", "Les 16 mesures lues à chaque clôture",
            "Résultats hors échantillon", "Actions et ETF américains", "Cryptos (bitcoin, ether, altcoins)",
            "22 actifs · test du oct. 2024 au sept. 2026 · 4 réentraînements", "ACHETER", "VENDRE", "ATTENDRE",
            "Avantage non démontré", "Trop peu de ventes pour conclure", "pour une entrée au hasard",
            "Achats gagnants / jours gagnants", "Achats cumulés / détention (médianes)", "Suivies d'une baisse / tous les jours", "Pire recul moyen ensuite / au hasard",
            "ATTENDRE 85 % des jours testés",
            "Calibration", "Actions et ETF américains · hausse", "Cryptos (bitcoin, ether, altcoins) · baisse", "Précision : 1,1 % moins bien que la fréquence de base.",
            "prévu", "observé",
            "Actif par actif", "Dans l'ordre du panier, jamais classés par performance.", "AAPL", "Apple", "Aujourd'hui : hausse 56 %, baisse 40 %",
            "Test : 3 achats (−7,12 points vs hasard)", "(Robinhood)", "Limites", "Panier fixé le 29/09/2026. Source : ",
        )
        assertTrue(has(report.headline))
        assertTrue(has("40 à 50 %")) // bucket label as served (plain space)
        assertTrue(has("coûts d'un aller-retour 0,31 % (actions), 0,32 % (cryptos). Calculé le "))
        report.method.forEach { assertTrue("méthode absente : $it", has(it)) }
        report.limits.forEach { assertTrue("limite absente : $it", has(it)) }
        assertTrue(!has(report.features[0].help))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/bot.png")

        // The 16 measures, unfolded.
        click("Les 16 mesures lues à chaque clôture")
        assertTrue(has("${report.features[0].label} — ${report.features[0].help}"))
        fitsWidth()

        // Opening an asset of the basket (the basket's order is kept: AAPL first).
        click(Bot.todayText(report.assets[0]))
        assertEquals(Asset("AAPL", Kind.STOCK, "Apple"), opened)

        // While the first training runs.
        shown = null
        pending = true
        compose.waitForIdle()
        expect("Entraînement et test sur tout le panier en cours (environ une minute la première fois)…")
        fitsWidth()
    }

    /** Réglages: the « Bot Altim » card opens the screen. */
    @Test @Config(qualifiers = "w360dp-h5000dp-xhdpi")
    fun settingsCard() {
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        val model = AppModel(app, BotMemory())
        var opens = 0
        compose.setContent { AltimTheme { AppBackground { CompositionLocalProvider(LocalOpenBot provides { opens++ }) { SettingsScreen(model, Modifier.fillMaxSize()) } } } }
        compose.waitForIdle()
        expect("Bot Altim", "Un modèle appris qui dit ACHETER, ATTENDRE ou VENDRE, jugé seulement sur des périodes qu'il n'avait pas vues, avec ses résultats réels et ses limites.")
        click("Voir le bot")
        assertEquals(1, opens)
        fitsWidth()
    }

    /** Décision card: the « Bot Altim » line (action, probabilities, whether it counts, link), and without a report. */
    @Test @Config(qualifiers = "w360dp-h17000dp-xhdpi")
    fun decisionLine() {
        val d = AltimJson.decodeFromString(Decision.serializer(), fixture("decision-aapl-v2.json"))
        var shown by mutableStateOf(d.copy(bot = view.copy(inBasket = false)))
        var opens = 0
        compose.setContent {
            AltimTheme {
                AppBackground {
                    CompositionLocalProvider(LocalOpenBot provides { opens++ }, LocalOpenValidation provides {}) {
                        Box(Modifier.fillMaxWidth().padding(16.dp)) { DecisionView(shown, true) }
                    }
                }
            }
        }
        compose.waitForIdle()
        expect(
            "Bot Altim", "ATTENDRE", "ne compte pas",
            "Probabilités à 20 jours : hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %) · modèle des cryptos, non testé sur cet actif",
            "RSI 14 : 40 (pèse contre la hausse)", "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.",
            "Voir le bot et ses résultats →", " · entraîné le 29/09 à ",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/bot-decision.png")
        click("Voir le bot et ses résultats →")
        assertEquals(1, opens)

        shown = d.copy(bot = BotView(available = false, text = "Bot pas encore entraîné : ouvrez l'écran « Bot Altim » pour lancer l'entraînement (quelques minutes)."))
        compose.waitForIdle()
        expect("Bot pas encore entraîné", "Voir le bot et ses résultats →")
        assertTrue(!has("ne compte pas"))
        assertTrue(!has("Probabilités à 20 jours"))
        fitsWidth()
    }
}
