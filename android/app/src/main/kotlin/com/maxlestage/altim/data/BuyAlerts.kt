package com.maxlestage.altim.data

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.maxlestage.altim.AltimApplication
import com.maxlestage.altim.MainActivity
import com.maxlestage.altim.R
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.LocalNotice
import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.NewsItem
import com.maxlestage.altim.kit.PriceTarget
import java.util.concurrent.TimeUnit

/**
 * Notifications "you can buy" (same rules as the iPhone's BuyNotifications). The check runs in the background every 15
 * minutes (the shortest period Android allows; Android decides according to usage and battery) and each time the app
 * comes to the foreground (at most every 15 minutes): the server applies its rule (/api/alerts: the asset's full
 * decision says ACHETER or ZONE D'ACHAT) to the watch list and the holdings; a notification is posted only when an asset
 * becomes buyable or its reason changes. The same check posts the price alerts, the news alerts, the Radar's
 * configuration changes and the positions that became dangerous (each on its own channel, each told once: kit
 * ChangeNotices), and refreshes the widget and the live following.
 */
object BuyAlerts {
    const val CHANNEL = "achats"
    private const val WORK = "altim.alerts"
    const val EXTRA_ASSET = "altim.asset"
    const val EXTRA_NEWS = "altim.news"
    const val NEWS_CHANNEL = "actualites"
    const val CONFIG_CHANNEL = "configurations"
    const val DANGER_CHANNEL = "dangers"
    const val DISCLAIMER = "Conseil indicatif : Altim ne passe aucun ordre."
    /** Shown instead of the details on the lock screen (portfolio notices are private). */
    const val PRIVATE_TEXT = "Ouvrez Altim pour le détail"
    /** The app in the foreground checks at most this often. */
    const val FOREGROUND_EVERY_MS = 15 * 60_000L

    @Volatile private var lastForegroundCheck = 0L

    fun schedule(context: Context, enabled: Boolean) {
        val wm = WorkManager.getInstance(context)
        if (!enabled) return wm.cancelUniqueWork(WORK).let { }
        val request = PeriodicWorkRequestBuilder<AlertWorker>(15, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        wm.enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    fun createChannel(context: Context) {
        val channel = NotificationChannel(CHANNEL, "Achats possibles", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Quand un actif de votre radar ou de vos avoirs devient achetable selon Altim."
        }
        val news = NotificationChannel(NEWS_CHANNEL, "Actualités importantes", NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = "Escalade grave (guerre, panique bancaire…) reprise par au moins 2 sources, ou sujet sur un de vos actifs repris par au moins 3 sources."
        }
        val config = NotificationChannel(CONFIG_CHANNEL, "Changements de configuration", NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = "Quand la décision d'un actif de votre radar change (ATTENDRE → ZONE D'ACHAT…), avec les conditions manquantes."
        }
        val dangers = NotificationChannel(DANGER_CHANNEL, "Positions dangereuses", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Quand une ligne de vos avoirs casse son stop, s'en approche à moins d'une volatilité journalière ou perd plus que votre risque accepté."
        }
        context.getSystemService(NotificationManager::class.java).createNotificationChannels(listOf(channel, news, config, dangers))
        LiveTracking.createChannel(context)
    }

    /** App in the foreground: a check at most every 15 minutes (the background one may not have run). */
    suspend fun foregroundCheck(context: Context, model: AppModel) {
        if (!model.needsChecks) return
        val now = System.currentTimeMillis()
        if (now - lastForegroundCheck < FOREGROUND_EVERY_MS) return
        lastForegroundCheck = now
        try {
            run(context, model)
        } catch (_: AltimException.Unauthorized) {
            // The screens' own calls handle the expired session.
        }
        schedule(context, model.needsChecks)
    }

    /** One check: new alerts notified, the widget and the live following refreshed. False on failure (network…). */
    suspend fun run(context: Context, model: AppModel): Boolean {
        if (!model.needsChecks && model.liveTrack == null) return true
        // The texts of the notifications (server's and the phone's) use the current EUR/USD rate.
        model.refreshFxIfOld()
        try {
            val r = model.checkAlerts() ?: return false
            if (model.alertsEnabled) postAll(context, r.buy)
            postNews(context, r.news)
            r.targets.forEach { (t, price) -> postTarget(context, t, price) }
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: AltimException.Unauthorized) {
            throw e
        } catch (_: Exception) {
            return false
        }
        try {
            val (danger, config) = model.checkChanges(canNotify(context))
            danger?.let { postNotice(context, DANGER_CHANNEL, it) }
            config?.let { postNotice(context, CONFIG_CHANNEL, it) }
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (_: Exception) {
            // The next check tries again.
        }
        return true
    }

    /** Up to 2 stories: one notification each; beyond, a single summary. A tap opens the Actu tab. */
    fun postNews(context: Context, items: List<NewsItem>) {
        if (items.isEmpty() || !canNotify(context)) return
        val list = if (items.size <= 2) items.map { n ->
            Triple("altim.news.${n.id}", if (n.alert) "Alerte actualité" else "Actualité : ${n.assets.joinToString { it.substringAfter(":") }}", "${n.title} (${n.source}${if (n.alsoIn.isNotEmpty()) " +${n.alsoIn.size}" else ""})")
        } else listOf(Triple("altim.news.summary", "${items.size} actualités importantes", items.take(3).joinToString(" · ") { it.title }))
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(EXTRA_NEWS, true)
        }
        val pending = PendingIntent.getActivity(context, "news".hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        for ((tag, title, body) in list) {
            val n = NotificationCompat.Builder(context, NEWS_CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(NotificationCompat.BigTextStyle().bigText(body))
                .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
                .setContentIntent(pending)
                .setAutoCancel(true)
                .build()
            if (!notify(context, tag, n)) return
        }
    }

    /**
     * A kit [LocalNotice] (configuration changes, dangerous positions): its own identifier (never the same change twice),
     * a tap opens its asset when it names one, otherwise the app; on the lock screen "Ouvrez Altim pour le détail".
     */
    fun postNotice(context: Context, channel: String, notice: LocalNotice) {
        if (!canNotify(context)) return
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            notice.asset?.let { putExtra(EXTRA_ASSET, it) }
        }
        // One request code per notice: a notice never reuses another one's asset.
        val pending = PendingIntent.getActivity(context, notice.id.hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(notice.title)
            .setContentText(notice.body.lineSequence().first())
            .setStyle(NotificationCompat.BigTextStyle().bigText(notice.body))
            .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(NotificationCompat.Builder(context, channel).setSmallIcon(R.drawable.ic_notification).setContentTitle("Altim").setContentText(PRIVATE_TEXT).build())
            .setContentIntent(pending)
            .setAutoCancel(true)
            .build()
        notify(context, notice.id, n)
    }

    fun canNotify(context: Context) =
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED &&
            NotificationManagerCompat.from(context).areNotificationsEnabled()

    /** Title of a reached price alert: "BTC en dessous de 80 000,00 €" (the threshold in its own currency) or the move. */
    fun targetTitle(t: PriceTarget, price: Double): String =
        if (t.move != null) "${t.asset.symbol} a bougé de ${Format.percent((t.inCurrency(price) / t.price - 1) * 100, 1)}"
        else "${t.asset.symbol} ${if (t.above) "au-dessus de" else "en dessous de"} ${Money.threshold(t.price, t.cur)}"

    /** A price alert reached, with the price now. */
    fun postTarget(context: Context, t: PriceTarget, price: Double) = add(
        context, "altim.target.${t.id}", targetTitle(t, price),
        "Prix actuel ${Format.price(price)} : votre alerte de prix est atteinte. Réarmez-la dans l'onglet Alertes si besoin.", t.asset.id,
    )

    /** Body of the summary of more than 3 buy alerts at once. */
    fun summaryBody(alerts: List<BuyAlert>): String {
        val strong = alerts.filter { it.strong }.map { it.symbol }
        val others = alerts.filterNot { it.strong }.map { it.symbol }
        var body = if (strong.isEmpty()) "" else "Achat conseillé : ${strong.joinToString(", ")}. "
        if (others.isNotEmpty()) body += "Achat possible : ${others.joinToString(", ")}. "
        return body + "Ouvrez Altim pour le détail de chacun."
    }

    /** Up to 3 alerts: one notification each; beyond, a single summary (the first check can find many at once). */
    fun postAll(context: Context, alerts: List<BuyAlert>) {
        if (!canNotify(context)) return
        if (alerts.size > 3) return add(context, "altim.summary", "${alerts.size} actifs achetables", summaryBody(alerts), null)
        alerts.forEach { add(context, "altim.${it.id}", it.title, it.body, it.id) }
    }

    /** One buy or price notification; the body ends with the disclaimer, a tap opens its asset when it has one. */
    fun add(context: Context, id: String, title: String, body: String, asset: String?) {
        if (!canNotify(context)) return
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            asset?.let { putExtra(EXTRA_ASSET, it) }
        }
        val pending = PendingIntent.getActivity(context, id.hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val text = "$body $DISCLAIMER"
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            // On the lock screen: only "Altim", the details once unlocked (the holdings are private).
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(
                NotificationCompat.Builder(context, CHANNEL).setSmallIcon(R.drawable.ic_notification).setContentTitle("Altim").setContentText("Achat possible sur un de vos actifs").build(),
            )
            .setContentIntent(pending)
            .setAutoCancel(true)
            .build()
        notify(context, id, n)
    }

    private fun notify(context: Context, id: String, n: android.app.Notification): Boolean {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return false
        return try {
            NotificationManagerCompat.from(context).notify(id.hashCode(), n)
            true
        } catch (_: SecurityException) {
            // Permission withdrawn in the meantime: nothing to post.
            false
        }
    }
}

class AlertWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val model = (applicationContext as AltimApplication).model
        if (!model.needsChecks) return Result.success()
        return try {
            val ok = BuyAlerts.run(applicationContext, model)
            if (!model.needsChecks) BuyAlerts.schedule(applicationContext, false)
            if (ok) Result.success() else Result.retry()
        } catch (e: AltimException.Unauthorized) {
            // Session expired and the password is locked (phone locked): the next run after unlocking will log in.
            Result.success()
        }
    }
}
