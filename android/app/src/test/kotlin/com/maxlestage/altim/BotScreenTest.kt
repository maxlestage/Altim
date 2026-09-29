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
import androidx.compose.ui.test.hasScrollToIndexAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performScrollToNode
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
 * « Bot Altim » rendered from the real /api/bot samples (v2 backend/tests/samples/bot.json, and v1 bot-v1.json which must
 * still render) on a narrow 360 dp phone with the no-overflow check, its card in Réglages and its line in the Décision
 * card. Screenshots in app/build/screens/bot*.png.
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

    /** Like [expect], scrolling the list to each text first (the v2 screen is longer than the test window). */
    private fun reach(vararg texts: String) = texts.map(::fr).forEach {
        if (!has(it)) {
            compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(it, substring = true))
            compose.waitForIdle()
        }
        assertTrue("absent : $it", has(it))
        fitsWidth()
    }

    private fun top() {
        compose.onNode(hasScrollToIndexAction()).performScrollToIndex(0)
        compose.waitForIdle()
    }

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
                view.copy(model = "logit", modelLabel = "Régression logistique 24 mesures"),
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
        val stock = report.groups[0]
        expect(
            "Bot Altim", "Jugés seulement sur des périodes qu'ils n'avaient pas vues, sur 34 actifs fixés d'avance. Altim ne passe aucun ordre.",
            "Actifs testés", "34 / 34", "Jours testés", "126 990", "Achats / ventes", "2723 / 1326",
            "Ce qui change avec la v2",
            "Vos actifs aujourd'hui", "BTC", "ne compte pas", "compte",
            "Hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %) · Régression logistique 24 mesures", "· hors du panier testé",
            "Pas assez d'historique pour un avis.", "pas d'avis",
            "Comment il apprend et comment il est jugé", "Les 24 mesures lues à chaque clôture",
            "Résultats hors échantillon", "Modèle choisi à chaque réentraînement sur une validation interne (jamais sur le test)",
            "Actions et ETF américains", "22 actifs testés · test du nov. 2009 au sept. 2026 · 17 réentraînements",
            "Données : 22 actifs du panier et 72 de plus à l'entraînement · historique médian 20,1 ans (le plus long 20,1 ans) · marché : S&P 500 (SPY).",
            "ACHETER", "VENDRE", "ATTENDRE",
            "Moins bien qu'une entrée au hasard (t ≤ −2 par jour)", "Ventes à contretemps : la hausse a suivi (t ≤ −2 par jour)",
            "(−0,38 point, t par jour = −2,4)", "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8",
            "Achats gagnants / jours gagnants", "Achats cumulés / détention (médianes)", "Suivies d'une baisse / tous les jours", "Pire recul moyen ensuite / au hasard",
            "En sortant 20 jours à chaque VENDRE : hors marché 16 % du temps",
            "ATTENDRE 57 % des jours testés",
            "Modèle retenu à chaque réentraînement", "nov. 2009 → nov. 2012 (4 fois)", "Tendance", "Logistique v1",
            "Aujourd'hui : Logistique 24, choisi de la même façon sur la dernière année connue.",
        )
        assertTrue(has(report.headline))
        assertTrue(has("coûts d'un aller-retour 0,31 % (actions), 0,32 % (cryptos). Calculé le "))
        report.changes.forEach { assertTrue("changement absent : $it", has(it)) }
        report.method.forEach { assertTrue("méthode absente : $it", has(it)) }
        assertTrue(!has(report.features[0].help))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/bot.png")

        // Further down: the cryptos, each model alone, the last 12 months, the extra universe, calibration, assets, limits.
        reach(
            "Cryptos (bitcoin, ether, altcoins)", "marché : bitcoin",
            "Chaque modèle seul", "À titre d'information, non utilisé pour choisir : ce qu'aurait donné chaque candidat retenu partout, sur les mêmes jours.",
            stock.candidates[0].label, "retenu 3 fois sur 17", "419 achats · écart au hasard", "+0,81 point (t 2)", "Précision hausse / baisse",
            "Achats : avantage non démontré", "Ventes : ventes à contretemps : la hausse a suivi (t ≤ −2 par jour)",
            stock.candidates[3].label,
            "Dernière année et univers élargi", "Actions et ETF américains · 12 derniers mois",
            "Du 29/09/2025 au 28/09/2026, présentés à part (même modèle choisi ; rien n'est choisi sur cette période).",
            "Actions et ETF américains · actifs d'entraînement hors panier", "72 actifs fixés d'avance, hors du test principal ; eux aussi jugés hors échantillon.",
            "Cryptos (bitcoin, ether, altcoins) · 12 derniers mois", "Cryptos (bitcoin, ether, altcoins) · actifs d'entraînement hors panier",
            "Calibration", "Actions et ETF américains · hausse", "Précision : 0,6 % moins bien que la fréquence de base.", "prévu", "observé",
        )
        assertTrue(has("40 à 50 %")) // bucket label as served (plain space)
        reach(
            "Cryptos (bitcoin, ether, altcoins) · baisse",
            "Actif par actif", "« Aujourd'hui » : avis du modèle retenu, entraîné sur tout l'historique connu.", "AAPL", "Apple", Bot.todayText(report.assets[0]),
            "Test : 83 achats (−0,09 point vs hasard)", "hors marché après VENDRE 14 % du temps, pire baisse −38,4 % contre −44,8 % en gardant",
            "(Yahoo Finance (20 ans max.), 20,1 ans)",
        )
        reach("Limites", "Panier fixé le 29/09/2026, univers élargi le 29/09/2026. Source : ", "Calcul : 13 s de téléchargement, 35 s d'entraînement et de test.")
        report.limits.forEach { reach(it) }

        // The 24 measures, unfolded.
        top()
        click("Les 24 mesures lues à chaque clôture")
        assertTrue(has("${report.features[0].label} — ${report.features[0].help}"))
        fitsWidth()

        // Opening an asset of the basket (the basket's order is kept: AAPL first).
        reach(Bot.todayText(report.assets[0]))
        click(Bot.todayText(report.assets[0]))
        assertEquals(Asset("AAPL", Kind.STOCK, "Apple"), opened)

        // While the first training runs.
        shown = null
        pending = true
        compose.waitForIdle()
        expect("Téléchargement des historiques, entraînement et test en cours (une à deux minutes la première fois)…")
        fitsWidth()
    }

    /** A v1 answer (before the v2 contract) still renders, without the v2 parts. */
    @Test fun v1AnswerStillRenders() {
        val report = AltimJson.decodeFromString(BotReport.serializer(), fixture("bot-v1.json"))
        compose.setContent { AltimTheme { AppBackground { BotAltimView(report, null, false, Modifier.fillMaxSize()) } } }
        compose.waitForIdle()
        expect(
            "Actifs testés", "34 / 34", "12 504", "207 / 54", "Les 16 mesures lues à chaque clôture", "Résultats hors échantillon",
            "22 actifs testés · test du oct. 2024 au sept. 2026 · 4 réentraînements", "(−0,15 point, t = −0,1)", "Trop peu de ventes pour conclure",
            "Précision : 1,1 % moins bien que la fréquence de base.",
        )
        for (absent in listOf("Ce qui change avec la v2", "Chaque modèle seul", "Modèle retenu à chaque réentraînement", "12 derniers mois", "Données : ", "t par jour")) {
            assertTrue("présent en v1 : $absent", !has(absent))
        }
        fitsWidth()
        reach("(Robinhood)", "Panier fixé le 29/09/2026. Source : ")
        assertTrue(!has("univers élargi"))
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
        var shown by mutableStateOf(d.copy(bot = view.copy(inBasket = false, model = "logit", modelLabel = "Régression logistique 24 mesures")))
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
            "Probabilités à 20 jours : hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %) · Régression logistique 24 mesures · modèle des cryptos, non testé sur cet actif",
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
