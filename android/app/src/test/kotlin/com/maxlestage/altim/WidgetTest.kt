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

/** Home screen widget: what it shows before any check, with nothing buyable, and with buyable assets. */
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

        assertTrue(shown().startsWith("Activez"))
        prefs.edit().putBoolean("alertsEnabled", true).apply()
        assertEquals("Première vérification dans quelques minutes.", shown())

        prefs.edit().putLong("lastAlertCheck", 1_790_582_400_000).putString(BuyWidget.KEY_ITEMS, "[]").apply()
        assertEquals("Rien d'achetable pour l'instant selon la règle d'Altim.", shown())

        val items = listOf(
            BuyAlert(symbol = "ETH", kind = Kind.CRYPTO, price = 2647.74, buy = true),
            BuyAlert(symbol = "AAPL", kind = Kind.STOCK, price = 341.07, buy = true, strong = true),
        )
        prefs.edit().putString(BuyWidget.KEY_ITEMS, AltimJson.encodeToString(ListSerializer(BuyAlert.serializer()), items)).apply()
        val text = shown()
        // Strong buys first; symbols and prices only, never the quantities held.
        assertTrue(text, text.startsWith("AAPL"))
        assertTrue(text, "· conseillé" in text && "ETH" in text && "· possible" in text)
        val footer = widgets.getViewFor(id).findViewById<TextView>(R.id.widget_footer).text.toString()
        assertTrue(footer, footer.startsWith("Vérifié le "))
    }
}
