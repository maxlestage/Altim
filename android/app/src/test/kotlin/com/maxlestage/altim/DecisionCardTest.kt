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
import com.maxlestage.altim.kit.ModelEvidence
import com.maxlestage.altim.kit.Rating
import com.maxlestage.altim.ui.DecisionView
import com.maxlestage.altim.ui.LocalOpenValidation
import androidx.compose.runtime.CompositionLocalProvider
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

    /**
     * A real answer of the current server (Apple, 28/09/2026, calendar not verified): rating first, regime, score bars,
     * target 3 and horizon, structure, "Agenda (7 jours)" not verified, fundamentals and track details.
     */
    @Test @Config(qualifiers = "w360dp-h17000dp-xhdpi")
    fun appleFull() {
        show(sample("decision-aapl-v2.json"), expanded = true)
        expect(
            // Rating and summaries.
            "ATTENDRE", "Verdict du plan : ATTENDRE", "la note résume verdict, niveau et confiance", "Régime de marché : ⚪ Neutre", "Stress macro 30/100 (tendu)",
            "Score composite", "+24 · Plutôt favorable", "Technique", "poids 35,6 %", "Sentiment", "non mesuré", "Score composite +24 sur 5 facteur(s) mesuré(s)",
            "Objectif 3", "399,54 $", "+36,8 %", "Horizon : Moyen terme — Plan sur bougies journalières", "Objectif 3 : projection : objectif 2",
            // Structure.
            "Structure technique", "direction +70", "Bougies journalières clôturées. Direction d'ensemble : +70/100.", "Ichimoku (9, 26, 52)", "↗ haussier",
            "Supertrend haussier depuis 16 bougies", "Canal de Donchian (20)", "VWAP glissant (20 bougies)", "Profil de volume (approximation)",
            "Approximation à partir des bougies", "Points pivots (dernière séance)", "Supports et résistances", "Résistance 344,95 $ · 2 contacts",
            "Fausse cassure baissière", "Structure (sommets et creux)", "Force relative contre S&P 500 (SPY)", "1 mois : +8,8 % contre +0,7 % (+8,1 pts)",
            "Force relative contre Nasdaq-100 (QQQ)",
            // Calendar not verified.
            "Agenda (7 jours)", "non vérifié", "Calendrier indisponible ou incomplet : les annonces à venir n'ont pas pu être vérifiées.",
            "Annonce économique dans les 48 h",
            // Fundamentals (real figures).
            "Comptes arrêtés au 27 juin 2026, déposés à la SEC le 31 juillet 2026", "Secteur : Industrie · Electronic Computers (code SIC 3571)",
            "P/S (capitalisation ÷ ventes)", "10,6", "P/B (capitalisation ÷ fonds propres)", "45,9", "84,1 % (impôt 17,3 %, taux effectif)",
            "Valorisation élevée par rapport à sa propre histoire", "médiane 31,3 · de 20,5 à 42,6", "plus haut que 94 % des 1\u202F255 jours (27 septembre 2021 – 25 septembre 2026)",
            "Comparaison sectorielle", "Comparée à 6 sociétés de même activité", "Dell Technologies Inc. Class C (DELL) : PER 32,7, P/S 2,4, marge opérationnelle 9,4 %",
            "International Business Machines Corporation (IBM) : PER 20, P/S 3,1, marge opérationnelle —", "Cours du 28 septembre 2026", "Prévisions de la direction non disponibles",
            // Track details.
            "Espérance par trade (coûts inclus)", "+0,5 %", "0,1 R", "Écart achat/vente mesuré", "0,006 %", "33 trades · réussite 42 % · moyenne +0,6 %",
            "échantillon trop faible", "Comment ce test évite de se flatter", "flat tax de 30", "+7,3 %",
        )
        // The rating comes first, then the plan's verdict.
        val top = { t: String -> compose.onAllNodes(hasText(t, substring = true), useUnmergedTree = true).fetchSemanticsNodes().first().boundsInRoot.top }
        assertTrue(top("Verdict du plan") > top("Mode informationnel"))
        assertTrue(!has("Signal dégradé"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl-v2.png")
    }

    /** Real answer with the calendar verified: 7 events in "Agenda (7 jours)", the announcement check active. */
    @Test @Config(qualifiers = "w360dp-h17000dp-xhdpi")
    fun appleEvents() {
        show(sample("decision-aapl-events.json"), expanded = true)
        expect("Agenda (7 jours)", "7 événements", "Croissance (PIB)", "· 14:30", "Inflation PCE sous-jacente", "Taux de chômage", "Source : ", "Annonce économique dans les 48 h", "ACTIVE")
        assertTrue(!has("Calendrier indisponible"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl-events.png")
    }

    /** An empty calendar says so; a stock adds its own events to the sentence. */
    @Test @Config(qualifiers = "w360dp-h17000dp-xhdpi")
    fun appleNoEvents() {
        show(sample("decision-aapl-events.json").copy(events = emptyList()), expanded = true)
        expect("rien de majeur", "Aucune annonce majeure (banques centrales, inflation, emploi, PIB), ni résultats, dividende ou split dans les 7 jours.")
    }

    /** Real Bitcoin answer: degraded signal banner with its reason, stablecoin flows, developer activity "non disponible". */
    @Test @Config(qualifiers = "w360dp-h14000dp-xhdpi")
    fun bitcoinDegradedAndOnChain() {
        show(sample("decision-btc-v2.json"), expanded = true)
        val d = sample("decision-btc-v2.json")
        expect(
            d.degraded!!.headline, "Aucune avance prouvée : sur cet actif, le signal a perdu de l'argent",
            "Flux de stablecoins", "Stablecoins (tous réseaux)", "313 Md$ au 28 septembre 2026", "… sur 7 jours", "+2,79 Md$ (+0,9 %)", "+4,21 Md$ (+1,36 %)",
            "Liquidité disponible sur le marché crypto. Source : DefiLlama (stablecoins).", "Activité de développement",
            "Non disponible (CoinGecko ne la publie plus", "Non couverts, faute de source gratuite et vérifiable", "baleines",
            "Aucune annonce majeure (banques centrales, inflation, emploi, PIB)".takeIf { d.events!!.isEmpty() } ?: "Croissance (PIB)",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-btc-v2.png")
    }

    /** Top of the card, folded: the degraded banner is never folded away. */
    @Test @Config(qualifiers = "w360dp-h1800dp-xhdpi")
    fun bitcoinDegradedTop() {
        val d = sample("decision-btc-v2.json")
        show(d, expanded = false)
        expect(d.degraded!!.headline, "Verdict du plan", "Score composite")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-btc-v2-top.png")
    }

    @Test @Config(qualifiers = "w360dp-h7600dp-xhdpi")
    fun olderAnswerShowsNothingMore() {
        show(sample("decision-btc.json"), expanded = true)
        assertTrue(!has("Verdict du plan"))
        assertTrue(!has("Structure technique"))
        assertTrue(!has("Agenda (7 jours)"))
        assertTrue(!has("Score composite"))
        assertTrue(!has("Espérance par trade"))
        assertTrue(!has("Activité de développement"))
        assertTrue(!has("Flux de stablecoins"))
        assertTrue(!has("Preuve du modèle"))
    }

    /**
     * « Preuve du modèle » under the confidence (real Apple answer with the evidence block the server adds, see the
     * kit's DecisionTest.modelEvidence) and a lowered rating's reason under the rating; the link opens the validation.
     */
    @Test @Config(qualifiers = "w360dp-h1800dp-xhdpi")
    fun modelEvidenceTop() {
        val d = sample("decision-evidence.json").copy(
            rating = Rating.BUY, ratingLabel = "ACHAT",
            ratingReason = "ACHAT plutôt que ACHAT FORT : la validation du modèle sur les actions et ETF américains (22 actifs) ne montre pas d'avantage du signal (la simple détention a fait mieux sur la plupart des actifs).",
        )
        var opens = 0
        compose.setContent {
            AltimTheme {
                AppBackground {
                    CompositionLocalProvider(LocalOpenValidation provides { opens++ }) {
                        Box(Modifier.fillMaxWidth().padding(16.dp)) { DecisionView(d, false) }
                    }
                }
            }
        }
        compose.waitForIdle()
        expect(
            "Preuve du modèle", "bat la détention : 2/22", "la simple détention a fait mieux dans 20 cas sur 22",
            "Voir la validation du modèle →", "calculée le 29/09 à", "ACHAT plutôt que ACHAT FORT",
        )
        // Under the confidence, and the reason under the rating.
        fun top(t: String) = compose.onAllNodes(hasText(t, substring = true), useUnmergedTree = true).fetchSemanticsNodes().first().boundsInRoot.top
        assertTrue(top("Preuve du modèle") > top(d.confidenceText.take(30)))
        assertTrue(top("ACHAT plutôt que") > top("Verdict du plan"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-evidence-top.png")
        compose.onNode(hasText("Voir la validation du modèle", substring = true)).performClick()
        assertEquals(1, opens)
    }

    /** Validation not computed yet: said plainly, no chip, no date. */
    @Test @Config(qualifiers = "w360dp-h1800dp-xhdpi")
    fun modelEvidenceNotComputed() {
        val na = ModelEvidence(
            available = false, assetClass = "btc", classLabel = "Bitcoin", regimeLabel = "régime inconnu (historique trop court)",
            text = "Validation pas encore calculée : ouvrez l'écran « Validation du modèle » pour la lancer (quelques minutes).",
        )
        show(sample("decision-btc-v2.json").copy(modelEvidence = na), expanded = false)
        expect("Preuve du modèle", "Validation pas encore calculée", "Voir la validation du modèle →")
        assertTrue(!has("bat la détention"))
        assertTrue(!has("calculée le"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-evidence-na.png")
    }

    @Test @Config(qualifiers = "w360dp-h9400dp-xhdpi")
    fun personalAppleOpen() {
        show(sample("decision-aapl.json"), expanded = true)
        expect("Chiffre d'affaires (TTM)", "421 Md$", "PEG", "EV/EBITDA", "date estimée", "T2 2026", "Révisions du consensus", "Comparaison au secteur non disponible", "Exposition du portefeuille", "S&P 500")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/decision-aapl-detail.png")
    }
}
