package com.maxlestage.altim

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.DecisionView
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * The "Décision" card rendered from the contract's samples (backend/tests/samples, copied in the kit's fixtures), no
 * server needed, on a narrow 360 dp phone. Screenshots in app/build/screens/decision-*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h2700dp-xhdpi", application = android.app.Application::class)
class DecisionCardTest {
    @get:Rule val compose = createComposeRule()

    private fun sample(name: String): Decision =
        AltimJson.decodeFromString(Decision.serializer(), File("../kit/src/test/resources/fixtures/$name").readText())

    private fun show(d: Decision, expanded: Boolean) {
        compose.setContent {
            AltimTheme {
                AppBackground {
                    Box(Modifier.fillMaxWidth().padding(16.dp)) { DecisionView(d, expanded) }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun expect(vararg texts: String) = texts.forEach { assertTrue("absent : $it", has(it)) }

    /** Nothing wider than the screen: no horizontal scroll at 360 dp. */
    private fun fitsWidth() {
        val root = compose.onRoot().fetchSemanticsNode()
        val width = root.boundsInRoot.width
        fun walk(n: SemanticsNode) {
            assertTrue("dépasse à droite : ${n.config} (${n.boundsInRoot.right} > $width)", n.boundsInRoot.right <= width + 1f)
            n.children.forEach(::walk)
        }
        walk(compose.onRoot(useUnmergedTree = true).fetchSemanticsNode())
    }

    @Test fun informationalBitcoin() {
        show(sample("decision-btc.json"), expanded = false)
        expect(
            "Mode informationnel", "ATTENDRE", "Attente", "Confiance du modèle", "6 familles sur 9",
            "Tendance · favorable", "Valorisation · non disponible", "Zone d'achat", "Objectif 2", "insuffisant",
            "Pourquoi attendre ?", "Pour passer en ACHAT", "Pour passer en VENTE", "Interdictions d'achat", "rien ne garantit l'avenir",
        )
        // Folded until opened.
        assertTrue(!has("Déblocage de jetons imminent"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-btc.png")
        compose.onNode(hasText("Interdictions d'achat")).performClick()
        compose.waitForIdle()
        expect("Déblocage de jetons imminent", "NON VÉRIFIABLE", "ACTIVE")
    }

    @Test @Config(qualifiers = "w360dp-h7600dp-xhdpi")
    fun informationalBitcoinOpen() {
        show(sample("decision-btc.json"), expanded = true)
        expect("Setup : Achat sur repli", "Scénario haussier", "Pourquoi pas ?", "Capitalisation", "FDV", "Financement (8 h)", "0,0069 %", "Sans objet", "Profit factor", "Sortino", "A fait moins bien")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-btc-detail.png")
    }

    @Test @Config(qualifiers = "w360dp-h3500dp-xhdpi")
    fun personalApple() {
        show(sample("decision-aapl.json"), expanded = false)
        expect(
            "Mode personnel", "jamais conservés", "ALLÉGER", "Signal modéré", "62 % du portefeuille",
            // Progressive exits open by default: one is due now.
            "Vendre 20 % si objectif 1 atteint", "MAINTENANT", "Votre prix d'achat moyen",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl.png")
    }

    @Test @Config(qualifiers = "w360dp-h9400dp-xhdpi")
    fun personalAppleOpen() {
        show(sample("decision-aapl.json"), expanded = true)
        expect("Chiffre d'affaires (TTM)", "421 Md$", "PEG", "EV/EBITDA", "date estimée", "T2 2026", "Révisions du consensus", "Comparaison au secteur non disponible", "Exposition du portefeuille", "S&P 500")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl-detail.png")
    }
}
