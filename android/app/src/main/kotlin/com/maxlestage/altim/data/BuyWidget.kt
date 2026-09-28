package com.maxlestage.altim.data

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews
import com.maxlestage.altim.MainActivity
import com.maxlestage.altim.R
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.Format
import kotlinx.serialization.builtins.ListSerializer

/**
 * Home screen widget: the assets of the radar and the holdings buyable now, from the last background check (every
 * 15 minutes while the buy notifications are on). Only symbols and prices: no quantity or amount of the holdings.
 */
class BuyWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val views = render(context)
        ids.forEach { manager.updateAppWidget(it, views) }
    }

    companion object {
        const val KEY_ITEMS = "widgetBuyable"

        /** Redraws every widget placed on the home screen (after each check, or when the notifications are switched). */
        fun refresh(context: Context) {
            val manager = AppWidgetManager.getInstance(context) ?: return
            val ids = manager.getAppWidgetIds(ComponentName(context, BuyWidget::class.java))
            if (ids.isNotEmpty()) manager.updateAppWidget(ids, render(context))
        }

        /** Text of the widget, from what the last check saved. */
        fun text(context: Context): Pair<String, String> {
            val prefs = context.getSharedPreferences("altim", Context.MODE_PRIVATE)
            if (!prefs.getBoolean("alertsEnabled", false)) {
                return "Activez « Me prévenir quand je peux acheter » dans Réglages : le widget se met à jour à chaque vérification." to "Altim"
            }
            val checked = prefs.getLong("lastAlertCheck", 0L)
            val items = prefs.getString(KEY_ITEMS, null)?.let { runCatching { AltimJson.decodeFromString(ListSerializer(BuyAlert.serializer()), it) }.getOrNull() }
            if (checked == 0L || items == null) return "Première vérification dans quelques minutes." to "Conseil indicatif · aucun ordre passé"
            val body = if (items.isEmpty()) "Rien d'achetable pour l'instant selon la règle d'Altim."
            else items.sortedByDescending { it.strong }.take(5).joinToString("\n") { a ->
                "${a.symbol}  ${a.price?.let { Format.price(it) } ?: ""}  ${if (a.strong) "· conseillé" else "· possible"}"
            } + if (items.size > 5) "\n+ ${items.size - 5} autre(s)" else ""
            return body to "Vérifié le ${Format.date(checked.toDouble(), time = true)} · conseil indicatif"
        }

        private fun render(context: Context): RemoteViews {
            val (body, footer) = text(context)
            val open = PendingIntent.getActivity(
                context, "widget".hashCode(),
                Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            return RemoteViews(context.packageName, R.layout.widget_buy).apply {
                setTextViewText(R.id.widget_body, body)
                setTextViewText(R.id.widget_footer, footer)
                setOnClickPendingIntent(R.id.widget_root, open)
            }
        }
    }
}
