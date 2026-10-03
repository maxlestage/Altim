package com.maxlestage.altim

import android.content.Context
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyWidget
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.Currency
import com.maxlestage.altim.kit.Fx
import com.maxlestage.altim.kit.FxResponse
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.PaperState
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.PaperView
import com.maxlestage.altim.ui.SettingsScreen
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId

private class CurrencyMemory : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

/** Euros by default (Réglages → Devise d'affichage), the rate line, amounts on the screens and the widget in €. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h3200dp-xhdpi", application = android.app.Application::class)
class CurrencyTest {
    @get:Rule val compose = createComposeRule()
    private val app = ApplicationProvider.getApplicationContext<android.app.Application>()
    private val prefs get() = app.getSharedPreferences("altim", Context.MODE_PRIVATE)
    /** Today 17:15 in Paris: the rate line gives the time only. */
    private val quoteTime = LocalDate.now(ZoneId.of("Europe/Paris")).atTime(LocalTime.of(17, 15)).atZone(ZoneId.of("Europe/Paris")).toInstant().toEpochMilli()

    @After fun reset() = Money.set(Currency.USD, null)

    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d)"), " ")
    private fun expect(vararg texts: String) = texts.map(::fr).forEach { assertTrue("absent : $it", has(it)) }

    /** A saved answer of /api/fx (as the phone keeps it), 1 $ = 0,8819 €. */
    private fun saveRate() {
        val body = FxResponse(rate = 0.8819, usdPerEur = 1 / 0.8819, time = quoteTime.toDouble(), source = "Yahoo Finance", fetchedAt = System.currentTimeMillis().toDouble())
        prefs.edit().clear().putString(Fx.KEY, Fx.encode(body)).commit()
    }

    @Test fun settingsEuroByDefaultWithRateLine() {
        saveRate()
        val model = AppModel(app, CurrencyMemory())
        assertEquals(Currency.EUR, model.currency)
        assertEquals(Currency.EUR, Money.displayCurrency())
        compose.setContent { AltimTheme { AppBackground { SettingsScreen(model, Modifier.fillMaxSize()) } } }
        compose.waitForIdle()
        expect("Devise d'affichage", "Euro (€)", "Dollar ($)", "1 $ = 0,8819 € · Yahoo Finance, 17:15 (dernier taux connu)", "Les montants que vous saisissez")
        compose.onRoot().captureRoboImage("build/screens/currency-settings.png")
        // Dollars on request: no conversion, no rate line, and the choice is kept.
        compose.onNodeWithText("Dollar ($)").performClick()
        compose.waitForIdle()
        assertEquals(Currency.USD, Money.displayCurrency())
        assertTrue(!has("1 $ = 0,8819 €"))
        assertEquals("USD", prefs.getString("currency", null))
        assertEquals(Currency.USD, AppModel(app, CurrencyMemory()).currency)
    }

    @Test fun noRateSaysSoAndKeepsDollars() {
        prefs.edit().clear().commit()
        val model = AppModel(app, CurrencyMemory())
        assertNull(model.fx)
        assertEquals(Currency.USD, Money.displayCurrency())
        compose.setContent { AltimTheme { AppBackground { SettingsScreen(model, Modifier.fillMaxSize()) } } }
        compose.waitForIdle()
        expect("Taux EUR/USD indisponible : montants affichés en $.")
    }

    @Test fun simulationAmountsInEuros() {
        saveRate()
        AppModel(app, CurrencyMemory())
        val steps = Json.parseToJsonElement(File("../kit/src/test/resources/fixtures/paper-fixture.json").readText()).jsonArray.map { it.jsonObject }
        val s = AltimJson.decodeFromJsonElement(PaperState.serializer(), steps.last().getValue("state"))
        compose.setContent {
            AltimTheme { AppBackground { PaperView(s, emptyMap(), emptyList(), error = null, modifier = Modifier.fillMaxSize(), onReset = {}, onSell = { null }, onDismissClosed = {}) } }
        }
        compose.waitForIdle()
        // 10 509,12 $ × 0,8819 = 9 267,99 €; 10 000 $ = 8 819 €.
        expect("9 267,99 €", "8 819 €", "Portefeuille simulé tenu en $ comme les cours", "1 $ = 0,8819 € · Yahoo Finance")
        assertTrue(!has("10 509,12 $"))
        compose.onRoot().captureRoboImage("build/screens/currency-paper.png")
    }

    @Test fun euroCostSavedWithCurrencyAndWidgetInEuros() {
        saveRate()
        val model = AppModel(app, CurrencyMemory())
        val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
        model.updateHoldings(listOf(Holding("h", btc, 0.1, averagePrice = 44_000.0, costCurrency = Currency.EUR)))
        // The engines get dollars: 44 000 € ÷ 0,8819.
        assertEquals(44_000 / 0.8819, model.usdHoldings.holdings.single().averagePrice!!, 1e-6)
        val saved = AltimJson.decodeFromString(ListSerializer(Holding.serializer()), prefs.getString("holdings", null)!!).single()
        assertEquals(Currency.EUR, saved.costCurrency)
        // Widget: the price of the last check in euros.
        prefs.edit().putBoolean("alertsEnabled", true).putLong("lastAlertCheck", 1L)
            .putString(BuyWidget.KEY_ITEMS, AltimJson.encodeToString(ListSerializer(BuyAlert.serializer()), listOf(BuyAlert("BTC", Kind.CRYPTO, price = 100_000.0, buy = true)))).commit()
        assertTrue(BuyWidget.text(app).first, BuyWidget.text(app).first.contains(fr("88 190,00 €")))
    }
}
