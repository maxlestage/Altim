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

    /** P/S, P/B, ROIC, valuation history, sector and peers, then the track details (expectancy, regimes, tax). */
    @Test @Config(qualifiers = "w360dp-h13000dp-xhdpi")
    fun appleFundamentalsAndTrackDetails() {
        show(sample("decision-aapl-v2.json"), expanded = true)
        expect(
            "Comptes arrêtés au 27 juin 2026, déposés à la SEC le 31 juillet 2026", "Secteur : Industrie · Electronic Computers (code SIC 3571)",
            "P/S (capitalisation ÷ ventes)", "10,7", "P/B (capitalisation ÷ fonds propres)", "46,3", "ROIC (rentabilité du capital investi)", "84,1 % (impôt 17,3 %, taux effectif)",
            "Valorisation par rapport à sa propre histoire", "Valorisation élevée par rapport à sa propre histoire", "PER sur la période", "39,2 aujourd'hui · médiane 31,3 · de 20,5 à 42,6",
            "Centile du PER", "plus haut que 95 % des 1\u202F255 jours (27 septembre 2021 – 25 septembre 2026)", "Centile du P/S", "Chaque jour : cours de clôture",
            "Comparaison sectorielle", "Comparée à 3 sociétés de même activité", "A (A) : PER 10, P/S —, marge opérationnelle 5 %, chiffre d'affaires — sur un an",
            "Cours du 25 septembre 2026", "Prévisions de la direction non disponibles",
            "Espérance par trade (coûts inclus)", "+1 %", "0,4 R", "Écart achat/vente supposé", "0,02 %", "Hypothèse : écart achat/vente",
            "Selon le régime de marché", "Marché haussier", "9 trades · réussite 56 % · moyenne +1,2 %", "échantillon trop faible", "0 trade",
            "Comment ce test évite de se flatter", "aucune optimisation", "flat tax de 30", "+42,8 %",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl-v2.png")
    }

    /** Stablecoin flows, developer activity said "non disponible", what is not covered; older answers show none of it. */
    @Test @Config(qualifiers = "w360dp-h9000dp-xhdpi")
    fun bitcoinOnChain() {
        show(sample("decision-btc-v2.json"), expanded = true)
        expect(
            "Flux de stablecoins", "Stablecoins (tous réseaux)", "313 Md$ au 28 septembre 2026", "… sur 7 jours", "+2,8 Md$ (+0,9 %)", "+4,23 Md$ (+1,37 %)",
            "Liquidité disponible sur le marché crypto. Source : DefiLlama (stablecoins).", "Activité de développement",
            "Non disponible (CoinGecko ne la publie plus", "Non couverts, faute de source gratuite et vérifiable", "baleines",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-btc-v2.png")
    }

    @Test @Config(qualifiers = "w360dp-h7600dp-xhdpi")
    fun olderAnswerShowsNothingMore() {
        show(sample("decision-btc.json"), expanded = true)
        assertTrue(!has("Espérance par trade"))
        assertTrue(!has("Activité de développement"))
        assertTrue(!has("Flux de stablecoins"))
    }

    @Test @Config(qualifiers = "w360dp-h9400dp-xhdpi")
    fun personalAppleOpen() {
        show(sample("decision-aapl.json"), expanded = true)
        expect("Chiffre d'affaires (TTM)", "421 Md$", "PEG", "EV/EBITDA", "date estimée", "T2 2026", "Révisions du consensus", "Comparaison au secteur non disponible", "Exposition du portefeuille", "S&P 500")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl-detail.png")
    }
}
