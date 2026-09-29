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
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import androidx.test.core.app.ApplicationProvider
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.ModelValidation
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.ValidationReport
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.DecisionView
import com.maxlestage.altim.ui.LocalOpenValidation
import com.maxlestage.altim.ui.SettingsScreen
import com.maxlestage.altim.ui.ValidationView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

private class ValidationMemory : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

/**
 * « Validation du modèle » rendered from the real /api/validation sample (backend/tests/samples/validation.json) on a
 * narrow 360 dp phone with the no-overflow check, and its link at the end of the signal's track record.
 * Screenshots in app/build/screens/validation*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h16000dp-xhdpi", application = android.app.Application::class)
class ValidationScreenTest {
    @get:Rule val compose = createComposeRule()

    private fun fixture(name: String) = File("../kit/src/test/resources/fixtures/$name").readText()
    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    /** "14 469" and "4,8 ans" written with the narrow no-break space of the French format. */
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d|%|an)"), " ")
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

    /** Top of the row of this asset ("AAPL Apple"). */
    private fun row(r: ValidationReport, symbol: String) = top("$symbol ${r.assets.first { it.symbol == symbol }.name}")

    /** Top of the list (y) of the first node with this text. */
    private fun top(text: String) = compose.onAllNodes(hasText(fr(text), substring = true), useUnmergedTree = true)[0].fetchSemanticsNode().boundsInRoot.top

    @Test fun wholeScreenFromTheSample() {
        val report = AltimJson.decodeFromString(ValidationReport.serializer(), fixture("validation.json"))
        var shown by mutableStateOf<ValidationReport?>(report)
        var pending by mutableStateOf(false)
        var opened: Asset? = null
        compose.setContent { AltimTheme { AppBackground { ValidationView(shown, null, pending, Modifier.fillMaxSize(), open = { opened = it }) } } }
        compose.waitForIdle()
        expect(
            "Validation du modèle", "Un test du passé, pas une promesse.", "Actifs testés", "34 / 34", "Trades", "822", "Historique", "0,5 à 4,8 ans",
            "A battu la simple détention", "6 sur 34 (18 %)", "Avantage non démontré", "Tous les trades ensemble : 822 trades · réussite 37 % · espérance −0,02 %",
            "Période : déc. 2021 – sept. 2026 (bougies journalières). Calculé le ", "mis à jour toutes les 12 h.",
            "Par classe d'actifs", "Actions et ETF américains", "22 actifs · historique médian", "Rendement médian du signal", "Détention médiane", "Pire actif (",
            "Recul max médian (signal / détention)", "Sharpe / Sortino médians", "Multiple de R moyen", "Temps investi médian", "Après impôt 30 % (signal / détention)",
            "Par régime de marché", "Tous", "Actions", "Bitcoin", "Ethereum", "Altcoins", "Marché haussier", "14 469 jours-actifs dans ce régime.",
            "Historique trop court pour classer", "Gain moyen positif (t ≥ 2), à confirmer", "Perte moyenne (t ≤ −2)",
            "Pendant ces jours, le signal a fait mieux que la détention sur 7 sur 20 (35 %) actifs", "Régime lu la veille, sans données futures",
            "Actif par actif", "Par classe", "Par nom", "Écart avec la détention", "AAPL", "Apple", "Actions · Technologie", "échantillon trop faible",
            "Signal +9 % · détention +95,6 % (moins bien, écart −86,6 %)", "36 trades · réussite 42 % · facteur de profit 1,16", "(StockAnalysis)",
            "Protections contre les biais", "Panier fixé le 29 septembre 2026", "Hors échantillon ?", "Non vérifiable.", "Biais et limites", "Biais du survivant",
            "Coûts par ordre : frais 0,1 %, glissement 0,05 %", "Panier fixé le 29/09/2026.",
        )
        assertTrue(has(report.headline))
        assertTrue(has("Crise (−30 % depuis le plus haut sur 1 an)"))
        // No regime text for the "unknown" regime (history too short), 5 regime cards for "Tous".
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/validation.png")

        // Basket order by default; by gap, AVAX first and NVDA last; by name, AAPL before ADA.
        assertTrue(row(report, "AAPL") < row(report, "BTC"))
        click("Écart avec la détention")
        assertTrue(row(report, "AVAX") < row(report, "SOL"))
        assertTrue(row(report, "SOL") < row(report, "NVDA"))
        click("Par nom")
        assertTrue(row(report, "AAPL") < row(report, "ADA"))
        assertTrue(row(report, "ADA") < row(report, "AMZN"))

        // Regimes of Bitcoin only (the chip, not the class card).
        val btcBull = report.classes.first { it.id == "btc" }.regimes.first { it.regime == "bull" }
        compose.onNode(hasText("Bitcoin") and hasAnyAncestor(hasContentDescription("Actifs pris en compte")), useUnmergedTree = true).performClick()
        compose.waitForIdle()
        assertTrue(has(ModelValidation.regimeDays(btcBull)))
        assertTrue(!has(fr("14 469 jours-actifs")))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/validation-btc.png")

        // Opening an asset.
        click("Signal +9 % · détention +95,6 %")
        assertEquals(Asset("AAPL", Kind.STOCK, "Apple"), opened)

        // While the first computation runs.
        shown = null
        pending = true
        compose.waitForIdle()
        expect("Calcul sur tout le panier en cours (environ 30 secondes la première fois)…")
    }

    /** Réglages: the « Validation du modèle » card opens the screen. */
    @Test @Config(qualifiers = "w360dp-h5000dp-xhdpi")
    fun settingsCard() {
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        val model = AppModel(app, ValidationMemory())
        var opens = 0
        compose.setContent { AltimTheme { AppBackground { CompositionLocalProvider(LocalOpenValidation provides { opens++ }) { SettingsScreen(model, Modifier.fillMaxSize()) } } } }
        compose.waitForIdle()
        expect("Validation du modèle", "Le signal testé sur 34 actions, cryptos et ETF choisis à l'avance, par classe d'actifs et par régime de marché, avec ses biais et limites.")
        click("Voir la validation")
        assertEquals(1, opens)
        fitsWidth()
    }

    @Test @Config(qualifiers = "w360dp-h17000dp-xhdpi")
    fun linkAtTheEndOfTheTrackRecord() {
        val d = AltimJson.decodeFromString(Decision.serializer(), fixture("decision-aapl-v2.json"))
        var opens = 0
        compose.setContent {
            AltimTheme {
                AppBackground {
                    CompositionLocalProvider(LocalOpenValidation provides { opens++ }) {
                        Box(Modifier.fillMaxWidth().padding(16.dp)) { DecisionView(d, true) }
                    }
                }
            }
        }
        compose.waitForIdle()
        expect("Historique du signal", "Validation du modèle : le même test sur 34 actifs →")
        click("Validation du modèle : le même test sur 34 actifs →")
        assertEquals(1, opens)
        fitsWidth()
    }
}
