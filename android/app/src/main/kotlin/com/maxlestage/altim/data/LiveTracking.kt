package com.maxlestage.altim.data

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.maxlestage.altim.AltimApplication
import com.maxlestage.altim.MainActivity
import com.maxlestage.altim.R
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.LiveTrackState
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * The closest Android equivalent of the iOS Live Activity (lock screen and Dynamic Island): one ongoing, silent
 * notification with the followed asset's price, its 24 h change and Altim's buy verdict. Updated by the live price stream
 * while the app runs (at most every 5 s) and by each alert check, also in the background; "mis à jour à …" tells how old
 * it is. A tap opens the asset; "Arrêter le suivi" (or Réglages) ends it.
 */
object LiveTracking {
    const val CHANNEL = "suivi"
    private const val ID = 4_242
    private const val ACTION_STOP = "com.maxlestage.altim.STOP_LIVE"

    fun createChannel(context: Context) {
        val channel = NotificationChannel(CHANNEL, "Suivi en direct", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Le prix d'un actif suivi et le verdict d'achat d'Altim, sur l'écran verrouillé et dans le volet des notifications."
            setShowBadge(false)
        }
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    /** "88 162,40 € · +1,25 %" (the display currency of Réglages). */
    fun priceLine(s: LiveTrackState): String = "${Format.price(s.price)} · ${Format.percent(s.change)}"

    /** "ACHAT POSSIBLE · Zone d'achat moyen terme atteinte." */
    fun verdictLine(s: LiveTrackState): String = "${s.verdict} · ${s.note}"

    /** "Altim · mis à jour à 14:05". */
    fun updatedLine(s: LiveTrackState): String =
        "Altim · mis à jour à ${DateTimeFormatter.ofPattern("HH:mm", Locale.FRANCE).withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(s.updated))}"

    fun show(context: Context, s: LiveTrackState) {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(BuyAlerts.EXTRA_ASSET, s.id)
        }
        val tap = PendingIntent.getActivity(context, "live:${s.id}".hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val stop = PendingIntent.getBroadcast(
            context, "live:stop".hashCode(),
            Intent(context, StopReceiver::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val body = "${verdictLine(s)}\n${updatedLine(s)}"
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("${s.symbol} · ${priceLine(s)}")
            .setContentText(verdictLine(s))
            .setSubText(s.name)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setShowWhen(false)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setContentIntent(tap)
            .setDeleteIntent(stop)
            .addAction(0, "Arrêter le suivi", stop)
            .build()
        try {
            NotificationManagerCompat.from(context).notify(ID, n)
        } catch (_: SecurityException) {
            // Permission withdrawn in the meantime: nothing to show.
        }
    }

    fun cancel(context: Context) = NotificationManagerCompat.from(context).cancel(ID)

    /** "Arrêter le suivi", or the notification dismissed: the following ends. */
    class StopReceiver : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            (context.applicationContext as? AltimApplication)?.model?.stopLiveTrack() ?: cancel(context)
        }
    }
}
