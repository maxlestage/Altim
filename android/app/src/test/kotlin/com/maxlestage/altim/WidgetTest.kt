package com.maxlestage.altim

import android.app.Application
import android.appwidget.AppWidgetManager
import android.content.Context
import android.widget.TextView
import androidx.test.core.app.ApplicationProvider
import com.maxlestage.altim.data.BuyWidget
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.Kind
import kotlinx.serialization.builtins.ListSerializer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** Home screen widget (the iPhone's Apple Watch content): before any check, with nothing buyable, and with buyable assets. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class WidgetTest {
    @Test fun showsTheLastCheck() {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val prefs = app.getSharedPreferences("altim", Context.MODE_PRIVATE)
        val widgets = shadowOf(AppWidgetManager.getInstance(app))
        val id = widgets.createWidget(BuyWidget::class.java, R.layout.widget_buy)
        fun shown(): String {
            BuyWidget.refresh(app)
            return widgets.getViewFor(id).findViewById<TextView>(R.id.widget_body).text.toString()
        }
        fun save(items: List<BuyAlert>) = prefs.edit().putString(BuyWidget.KEY_ITEMS, AltimJson.encodeToString(ListSerializer(BuyAlert.serializer()), items)).apply()

        assertEquals("Ouvrez Altim et activez les notifications d'achat.", shown())

        prefs.edit().putLong("lastAlertCheck", 1_790_582_400_000).apply()
        save(listOf(BuyAlert(symbol = "SOL", kind = Kind.CRYPTO, price = 150.0, buy = false)))
        assertEquals("Rien d'achetable pour l'instant selon la règle d'Altim.\nPas pour l'instant : SOL", shown())

        save(
            listOf(
                BuyAlert(symbol = "ETH", kind = Kind.CRYPTO, price = 2647.74, buy = true),
                BuyAlert(symbol = "AAPL", kind = Kind.STOCK, price = 341.07, buy = true, strong = true),
                BuyAlert(symbol = "SOL", kind = Kind.CRYPTO, price = 150.0, buy = false),
            ),
        )
        val text = shown()
        // Strong buys first; symbols and prices only, never the quantities held; then the others.
        assertTrue(text, text.startsWith("AAPL"))
        assertTrue(text, "· conseillé" in text && "ETH" in text && "· possible" in text && text.endsWith("Pas pour l'instant : SOL"))
        val footer = widgets.getViewFor(id).findViewById<TextView>(R.id.widget_footer).text.toString()
        assertTrue(footer, footer.startsWith("Vérifié le ") && footer.endsWith("Conseil indicatif : Altim ne passe aucun ordre."))
    }
}
