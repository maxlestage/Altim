package com.maxlestage.altim

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.AnomalyReport
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.OppSaved
import com.maxlestage.altim.kit.OpportunityReport
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AnomaliesView
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.Loadable
import com.maxlestage.altim.ui.OpportunitiesLink
import com.maxlestage.altim.ui.OpportunitiesView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * "Opportunités du moment" and the anomalies card, rendered from the kit's samples (shaped like backend/src/app/scan.rs)
 * on a narrow 360 dp phone with the no-overflow check. Screenshots in app/build/screens/opportunities-*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h5000dp-xhdpi", application = android.app.Application::class)
class OpportunitiesScreenTest {
    @get:Rule val compose = createComposeRule()

    private fun fixture(name: String) = File("../kit/src/test/resources/fixtures/$name").readText()
    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    /** "79 450" written with the narrow no-break space of the French format between digits. */
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d)"), "\u202F")
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
        compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true)[0].performClick()
        compose.waitForIdle()
    }

    @Test fun opportunitiesFiltersAndStorage() {
        val report = AltimJson.decodeFromString(OpportunityReport.serializer(), fixture("opportunities-sample.json"))
        var saved by mutableStateOf(OppSaved())
        var opened: Asset? = null
        var pending by mutableStateOf(false)
        var shown by mutableStateOf<OpportunityReport?>(report)
        compose.setContent {
            AltimTheme { AppBackground { OpportunitiesView(saved, shown, null, pending, Modifier.fillMaxSize(), onSaved = { saved = it }, open = { opened = it }) } }
        }
        compose.waitForIdle()
        expect(
            "Opportunités du moment", "Des pistes à examiner, pas des ordres d'achat.", "Actions", "Cryptos", "Catégories", "Cassures · 1", "Volume anormal · 1",
            "Survendus · 1", "Fondamentaux · 1", "Configurations · 0", "Capitalisation", "≥ 200 Md$", "Liquidité (volume échangé)", "Volatilité max (ATR)",
            "Fondamentaux : analysé sur les 30 plus liquides", "RÉSULTATS · 3 SUR 150 ANALYSÉS", "Micron Technology, Inc.", "MU · Technologie", "+4,2 %",
            "Clôture 123,45 \$ au-dessus du plus haut 55 j", "RSI 14 : 68", "échangé 2,9 Md\$/j", "capitalisation 138 Md\$", "Voir la fiche", "Règles, sources et limites",
            "Conseil indicatif, pas une recommandation personnalisée",
        )
        // The unknown category of a newer server is not shown.
        assertTrue(!has("catégorie inconnue"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/opportunities.png")
        // Filters: volatility ≤ 2 %/day keeps Apple only; a category chip off hides its reasons.
        click("≤ 2 % / jour")
        expect("RÉSULTATS · 1 SUR 150")
        assertTrue(!has("Micron Technology"))
        assertEquals(2.0, saved.filters.maxVolatility)
        click("Fondamentaux · 1")
        expect("Aucun actif ne remplit ces critères en ce moment.")
        click("Règles, sources et limites")
        expect("Cassures (breakouts) (150 analysés) : Clôture au-dessus du plus haut", "Non couvert — Révisions d'analystes", "Séances closes uniquement.")
        // Opening an asset.
        // Volatility back to "Toutes" (the third "Toutes": capitalisation, liquidity, volatility).
        compose.onAllNodes(hasText("Toutes"), useUnmergedTree = true)[2].performClick()
        click("Fondamentaux · 1")
        click("Micron Technology, Inc.")
        assertEquals(Asset("MU", Kind.STOCK, "Micron Technology, Inc."), opened)
        // Crypto market: the rank filter; while the first scan runs, the pending text.
        click("Cryptos")
        assertEquals(Kind.CRYPTO, saved.market)
        pending = true
        shown = null
        compose.waitForIdle()
        expect("Rang (capitalisation)", "Top 20", "Analyse des 120 cryptos en cours (environ 30 secondes la première fois)…")
        fitsWidth()
    }

    @Test fun anomaliesCryptoWithDerivatives() {
        val r = AltimJson.decodeFromString(AnomalyReport.serializer(), fixture("anomalies-sample.json"))
        compose.setContent {
            AltimTheme {
                AppBackground {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        OpportunitiesLink {}
                        AnomaliesView(Loadable.Loaded(r))
                        AnomaliesView(Loadable.Loaded(r.copy(symbol = "AAPL", kind = Kind.STOCK, anomalies = emptyList(), derivatives = null)))
                    }
                }
            }
        }
        compose.waitForIdle()
        expect(
            "Opportunités du moment →", "Détection d'anomalies", "⚠️ Open interest +18,4 % en 24 h", "+18,4 % (seuil ±15 %)", "à confirmer par le funding",
            "Source : OKX", "Mesures dans la normale · 2", "DÉRIVÉS ET GROS MOUVEMENTS", "Liquidations 24 h", "20,2 M\$", "Acheteurs liquidés (1486)", "14,4 M\$ · 71 %",
            "Vendeurs liquidés (764)", "Plus grosse", "843 k\$ · acheteur à 79 450,2 \$", "Open interest", "3,1 Md\$ · +18,4 % 24 h · −3,1 % 7 j",
            "Funding (dernier règlement, toutes les 8 h)", "+0,0081 % · habituel −0,0021 % à +0,0103 %", "Ratio comptes acheteurs / vendeurs", "1,42 · habituel 0,91 à 2,35",
            "Non couvert — Baleines (gros portefeuilles)", "Une anomalie est un écart mesuré, pas une prévision", "Rien d'inhabituel sur les mesures ci-dessous (séance du ",
        )
        click("Mesures dans la normale")
        expect("Écart à la moyenne 20 j : −1,2 σ", "−1,2 σ (seuil ±2,5 σ)")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/opportunities-anomalies.png")
    }
}
