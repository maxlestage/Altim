package com.maxlestage.altim

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.OpenOrder
import com.maxlestage.altim.kit.PaperState
import com.maxlestage.altim.kit.PaperTrade
import com.maxlestage.altim.kit.checkExits
import com.maxlestage.altim.kit.encodePaper
import com.maxlestage.altim.kit.newPaper
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.DecisionView
import com.maxlestage.altim.ui.HoldingsScreen
import com.maxlestage.altim.ui.PaperView
import com.maxlestage.altim.ui.SimulateForm
import com.maxlestage.altim.ui.suggestedAmount
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

private class Memory : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

/**
 * The "Simulation" part of Mes avoirs rendered from the shared paper-trading scenario (web/test/paper-fixture.json,
 * copied in the kit's fixtures), no server needed, on a narrow 360 dp phone. Screenshots in app/build/screens/paper-*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h3200dp-xhdpi", application = android.app.Application::class)
class PaperScreenTest {
    @get:Rule val compose = createComposeRule()

    private val steps = Json.parseToJsonElement(File("../kit/src/test/resources/fixtures/paper-fixture.json").readText()).jsonArray.map { it.jsonObject }

    private fun state(i: Int): PaperState = AltimJson.decodeFromJsonElement(PaperState.serializer(), steps[i].getValue("state"))

    private fun prices(i: Int): Map<String, Double?> =
        AltimJson.decodeFromJsonElement(MapSerializer(String.serializer(), Double.serializer().nullable), (steps[i].getValue("step") as JsonObject).getValue("prices"))

    private fun show(s: PaperState, prices: Map<String, Double?>, justClosed: List<PaperTrade> = emptyList()) {
        compose.setContent {
            AltimTheme {
                AppBackground {
                    PaperView(s, prices, justClosed, error = null, modifier = Modifier.fillMaxSize(), onReset = {}, onSell = { null }, onDismissClosed = {})
                }
            }
        }
        compose.waitForIdle()
    }

    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    /** "10 509,12 $" written with the narrow no-break space of the French format between digits. */
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d)"), "\u202F")
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

    /** Final state of the scenario: everything sold, journal and statistics. */
    @Test fun finalState() {
        show(state(steps.lastIndex), emptyMap())
        expect(
            "Portefeuille simulé — aucun argent réel, aucun ordre passé", "10 509,12 $", "+509,12 $", "+5,09 %",
            "Capital de départ", "10 000 $", "Liquidités", "Recommencer",
            "POSITIONS OUVERTES", "JOURNAL DES VENTES SIMULÉES", "objectif atteint", "stop touché", "vente manuelle",
            "Résultats par décision affichée à l'achat", "ZONE D'ACHAT", "Sans décision", "en dessous d'une vingtaine",
            "Positions (si vendues maintenant)", "0 $", "Gain moyen", "+15,90 %", "Perte moyenne", "−8,40 %", "Sorties",
            "Gagnantes", "40 %", "Profit factor", "2,47", "Comment c'est calculé", "Frais de 0,1 %", "Glissement de 0,05 %",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/paper-final.png")
    }

    /** Four open positions valued at the scenario's prices (ETH without a price: at its cost). */
    @Test @Config(qualifiers = "w360dp-h3600dp-xhdpi")
    fun openPositions() {
        show(state(8), prices(8))
        expect(
            "10 072,5 $", "+72,5 $", "POSITIONS OUVERTES", "BTC", "Bitcoin", "ACHETER", "confiance 70 %", "1 septembre 2026",
            "64 032,00 $", "66 000,00 $", "3 084,48 $", "+84,48 $", "+2,82 %", "60 000,00 $", "72 000,00 $",
            "ZONE D'ACHAT", "ATTENDRE", "Ouverte sans décision affichée", "sans prix : au coût", "1 position sans prix",
            "JOURNAL DES VENTES SIMULÉES", "Aucune position clôturée pour l'instant.",
        )
        assertTrue(compose.onAllNodes(hasContentDescription("Vendre BTC (simulé)", substring = true)).fetchSemanticsNodes().isNotEmpty())
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/paper-open.png")
        compose.onNode(hasContentDescription("Vendre BTC (simulé)", substring = true)).performClick()
        compose.waitForIdle()
        assertTrue(compose.onAllNodes(isDialog()).fetchSemanticsNodes().isNotEmpty())
        expect("Vendre (simulé) ?", "Vendre BTC (simulé)", "Aucun ordre réel")
        compose.onNode(isDialog()).captureRoboImage("build/screens/paper-sell.png")
    }

    /** The exits just made by the daily candles (step 9 of the scenario), shown on top. */
    @Test fun justClosed() {
        val before = state(8)
        val candles = AltimJson.decodeFromJsonElement(
            MapSerializer(String.serializer(), kotlinx.serialization.builtins.ListSerializer(Candle.serializer())),
            (steps[9].getValue("step") as JsonObject).getValue("candles"),
        )
        val r = checkExits(before, candles)
        assertEquals(listOf("p1", "p2", "p4"), r.closed.map { it.id })
        show(r.state, prices(8), r.closed)
        expect("Clôturée automatiquement : BTC, objectif atteint", "Clôturée automatiquement : AAPL, objectif atteint", "Clôturée automatiquement : ETH, stop touché", "J'ai vu")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/paper-exits.png")
    }

    @Test fun resetAsksFirst() {
        var reset: Double? = null
        compose.setContent {
            AltimTheme {
                AppBackground {
                    PaperView(state(steps.lastIndex), emptyMap(), emptyList(), error = null, modifier = Modifier.fillMaxSize(), onReset = { reset = it }, onSell = { null }, onDismissClosed = {})
                }
            }
        }
        compose.onNode(hasText("Recommencer")).performClick()
        compose.waitForIdle()
        expect("Recommencer la simulation", "Capital de départ en $", "10 000", "Par défaut 10")
        assertEquals(null, reset)
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/paper-reset.png")
        compose.onAllNodes(hasText("Recommencer")).onLast().performClick()
        compose.waitForIdle()
        expect("Effacer la simulation ?", "Toutes les positions et ventes simulées seront effacées.")
        compose.onNode(hasText("Recommencer avec", substring = true)).performClick()
        compose.waitForIdle()
        assertEquals(10_000.0, reset!!, 0.0)
    }

    /** "Simuler cet achat" from the Bitcoin decision (ATTENDRE: the warning says it is against the decision). */
    @Test @Config(qualifiers = "w360dp-h1400dp-xhdpi")
    fun simulateForm() {
        val d = AltimJson.decodeFromString(Decision.serializer(), File("../kit/src/test/resources/fixtures/decision-btc.json").readText())
        // Default amount: 10 % of the simulated value, at most the cash.
        assertEquals(1_000.0, suggestedAmount(newPaper(10_000.0, 0.0), emptyMap()), 0.0)
        assertEquals(1_007.25, suggestedAmount(state(8), prices(8)), 0.0)
        assertEquals(0.0, suggestedAmount(state(8).copy(cash = 0.0), prices(8)), 0.0)
        val amount = suggestedAmount(newPaper(10_000.0, 0.0), emptyMap())
        compose.setContent {
            AltimTheme {
                AppBackground {
                    Box(Modifier.fillMaxWidth()) {
                        SimulateForm(
                            Asset("BTC", Kind.CRYPTO, "Bitcoin"), d, d.price, 10_000.0, against = true,
                            amount = Format.plain(amount, 2), stop = Format.plain(d.plan!!.stop, 8), target = Format.plain(d.plan!!.target1, 8),
                            error = "Liquidités simulées insuffisantes (10000 $ disponibles).",
                            onAmount = {}, onStop = {}, onTarget = {}, onCancel = {}, onConfirm = {},
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
        expect("Simuler cet achat", "aucun argent réel", "83 120,50 $", "1 000", "74 900", "86 400", "ATTENDRE", "vous simulez contre la décision", "Montant en $", "Stop en $", "Objectif en $", "insuffisantes", "Simuler")
        assertTrue(!has("sera ignoré"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/paper-simuler.png")
    }

    /** Mes avoirs: the "Réel / Simulation" choice at the top, the simulated portfolio saved on the phone. */
    @Test @Config(qualifiers = "w360dp-h1200dp-xhdpi")
    fun holdingsTab() {
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        app.getSharedPreferences("altim", Context.MODE_PRIVATE).edit().putString("paper", encodePaper(state(8))).commit()
        val model = AppModel(app, Memory())
        compose.setContent { AltimTheme { AppBackground { HoldingsScreen(model, Modifier.fillMaxSize()) {} } } }
        expect("Mes avoirs", "Réel", "Simulation", "Ajoutez ce que vous possédez")
        compose.onNode(hasText("Simulation")).performClick()
        compose.waitForIdle()
        // No server here: the positions are valued at their cost.
        expect("Portefeuille simulé — aucun argent réel, aucun ordre passé", "10 000 $", "4 positions sans prix", "POSITIONS OUVERTES")
        assertTrue(!has("Ajoutez ce que vous possédez"))
        assertTrue(compose.onAllNodes(hasContentDescription("Ajouter un avoir")).fetchSemanticsNodes().isEmpty())
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/paper-avoirs.png")
    }

    /** The Décision card offers the simulated purchase (and only when asked to). */
    @Test fun decisionCardButton() {
        val d = AltimJson.decodeFromString(Decision.serializer(), File("../kit/src/test/resources/fixtures/decision-btc.json").readText())
        var clicked = false
        compose.setContent { AltimTheme { AppBackground { Box(Modifier.fillMaxWidth()) { DecisionView(d, onSimulate = { clicked = true }) } } } }
        expect("Simuler cet achat")
        fitsWidth()
        compose.onNode(hasText("Simuler cet achat")).performClick()
        assertTrue(clicked)
    }

    @Test fun ignoredLevelsSaid() {
        val d = AltimJson.decodeFromString(Decision.serializer(), File("../kit/src/test/resources/fixtures/decision-btc.json").readText())
        compose.setContent {
            AltimTheme {
                SimulateForm(
                    Asset("BTC", Kind.CRYPTO, "Bitcoin"), d, d.price, 10_000.0, against = false, amount = "500", stop = "90 000", target = "80 000", error = null,
                    onAmount = {}, onStop = {}, onTarget = {}, onCancel = {}, onConfirm = {},
                )
            }
        }
        expect("un stop au-dessus du prix d'achat ou un objectif en dessous est ignoré.")
        assertTrue(!has("contre la décision"))
    }

    /** Stored like the holdings; a damaged saved state is ignored (a fresh simulation), never a crash. */
    @Test fun persistence() {
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        val prefs = app.getSharedPreferences("altim", Context.MODE_PRIVATE)
        listOf("garbage", "{\"version\":1,\"startCapital\":-5,\"cash\":1,\"positions\":[],\"trades\":[]}", "{\"version\":1,\"startCapital\":1000,\"cash\":1000,\"positions\":[{\"id\":1}],\"trades\":[]}").forEach { bad ->
            prefs.edit().putString("paper", bad).commit()
            val m = AppModel(app, Memory())
            assertEquals(10_000.0, m.paper.startCapital, 0.0)
            assertTrue(m.paper.positions.isEmpty())
        }
        prefs.edit().putString("paper", encodePaper(state(8))).commit()
        val m = AppModel(app, Memory())
        assertEquals(4, m.paper.positions.size)
        // A purchase (refused, then accepted) and a sale, kept after a restart.
        assertEquals("Prix indisponible.", m.paperBuy(OpenOrder("x", "NVDA", Kind.STOCK, "Nvidia", Double.NaN, 100.0)))
        assertEquals(null, m.paperBuy(OpenOrder("x", "NVDA", Kind.STOCK, "Nvidia", 180.0, 1_000.0)))
        assertEquals(null, m.paperSell("p3", 160.0))
        val again = AppModel(app, Memory())
        assertEquals(listOf("p1", "p2", "p4", "x"), again.paper.positions.map { it.id })
        assertEquals(1, again.paper.trades.size)
        assertEquals(m.paper, again.paper)
        again.paperReset(5_000.0)
        assertEquals(5_000.0, AppModel(app, Memory()).paper.cash, 0.0)
    }
}
