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
 * Home screen widget: what the iPhone shows on the Apple Watch (the assets of the radar and the holdings buyable now,
 * then the others), from the last check (every 15 minutes in the background, and at each opening).
 */
class BuyWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val views = render(context)
        ids.forEach { manager.updateAppWidget(it, views) }
    }

    companion object {
        /** The last alerts saved by a check (AppModel.lastAlerts). */
        const val KEY_ITEMS = "lastAlerts"
        const val DISCLAIMER = "Conseil indicatif : Altim ne passe aucun ordre."

        /** Redraws every widget placed on the home screen (after each check, or when the notifications are switched). */
        fun refresh(context: Context) {
            val manager = AppWidgetManager.getInstance(context) ?: return
            val ids = manager.getAppWidgetIds(ComponentName(context, BuyWidget::class.java))
            if (ids.isNotEmpty()) manager.updateAppWidget(ids, render(context))
        }

        /**
         * Text of the widget, from what the last check saved: the content of the iPhone's Apple Watch app (buyable
         * assets first, then the others). Only symbols and prices: no quantity or amount of the holdings.
         */
        fun text(context: Context): Pair<String, String> {
            val prefs = context.getSharedPreferences("altim", Context.MODE_PRIVATE)
            val items = prefs.getString(KEY_ITEMS, null)?.let { runCatching { AltimJson.decodeFromString(ListSerializer(BuyAlert.serializer()), it) }.getOrNull() }
                ?: emptyList()
            if (items.isEmpty()) return "Ouvrez Altim et activez les notifications d'achat." to DISCLAIMER
            val buyable = items.filter { it.buy }.sortedByDescending { it.strong }
            val others = items.filterNot { it.buy }
            val lines = mutableListOf<String>()
            if (buyable.isEmpty()) lines += "Rien d'achetable pour l'instant selon la règle d'Altim."
            lines += buyable.take(5).map { a -> "${a.symbol}  ${a.price?.let { Format.price(it) } ?: ""}  ${if (a.strong) "· conseillé" else "· possible"}" }
            if (buyable.size > 5) lines += "+ ${buyable.size - 5} autre(s)"
            if (others.isNotEmpty()) lines += "Pas pour l'instant : ${others.take(6).joinToString(", ") { it.symbol }}${if (others.size > 6) "…" else ""}"
            val checked = prefs.getLong("lastAlertCheck", 0L)
            return lines.joinToString("\n") to (if (checked > 0) "Vérifié le ${Format.date(checked.toDouble(), time = true)} · $DISCLAIMER" else DISCLAIMER)
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
