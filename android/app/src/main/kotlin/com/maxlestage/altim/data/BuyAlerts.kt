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
import com.maxlestage.altim.kit.NewsItem
import com.maxlestage.altim.kit.Notice
import com.maxlestage.altim.kit.ConfigNotices
import com.maxlestage.altim.kit.DangerNotices
import com.maxlestage.altim.kit.PriceTarget
import java.util.concurrent.TimeUnit

/**
 * Notifications "you can buy": every 15 minutes (the shortest period Android allows in the background), the server
 * applies its rule (/api/alerts: buy signal or price in a Fibonacci zone, nothing blocking) to the watch list and the
 * holdings; a notification is posted only when an asset becomes buyable or its reason changes. The same worker posts
 * the price alerts, the news alerts, the Radar's configuration changes and the positions that became dangerous
 * (each on its own channel, each told once: kit ConfigNotices / DangerNotices).
 */
object BuyAlerts {
    const val CHANNEL = "achats"
    private const val WORK = "altim.alerts"
    const val EXTRA_ASSET = "altim.asset"
    const val EXTRA_NEWS = "altim.news"
    const val NEWS_CHANNEL = "actualites"
    const val CONFIG_CHANNEL = "configurations"
    const val DANGER_CHANNEL = "dangers"
    /** Tags of the grouped notifications: a newer one replaces the previous (one each at most). */
    const val CONFIG_TAG = "config:changes"
    const val DANGER_TAG = "dangers:positions"

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
            description = "Escalade grave (guerre, panique bancaire…) ou sujet sur un de vos actifs repris par au moins 3 sources."
        }
        val config = NotificationChannel(CONFIG_CHANNEL, "Changements de configuration", NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = "Quand la décision d'un actif de votre radar change (ATTENDRE → ZONE D'ACHAT…), avec les conditions manquantes."
        }
        val dangers = NotificationChannel(DANGER_CHANNEL, "Positions dangereuses", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Quand une ligne de vos avoirs casse son stop, s'en approche à moins d'une volatilité journalière ou perd plus que votre risque accepté."
        }
        context.getSystemService(NotificationManager::class.java).createNotificationChannels(listOf(channel, news, config, dangers))
    }

    /** Up to 2 stories: one notification each; beyond, a single summary. A tap opens the Actu tab. */
    fun postNews(context: Context, items: List<NewsItem>) {
        if (items.isEmpty() || !canNotify(context)) return
        val list = if (items.size <= 2) items.map { n ->
            Triple("news:${n.id}", if (n.alert) "Alerte actualité" else "Actualité : ${n.assets.joinToString { it.substringAfter(":") }}", "${n.title} (${n.source}${if (n.alsoIn.isNotEmpty()) " +${n.alsoIn.size}" else ""})")
        } else listOf(Triple("news:summary", "${items.size} actualités importantes", items.take(3).joinToString(" · ") { it.title }))
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
            if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
            try {
                NotificationManagerCompat.from(context).notify(tag.hashCode(), n)
            } catch (_: SecurityException) {
                return
            }
        }
    }

    /** New configuration changes of the Radar: ONE notification (the newest replaces the previous one). */
    fun postChanges(context: Context, notice: Notice?) = notice?.let { postNotice(context, CONFIG_CHANNEL, CONFIG_TAG, it, "Changement de configuration sur votre radar") }

    /** Lines that newly became dangerous: ONE notification, details hidden on the lock screen (holdings are private). */
    fun postDangers(context: Context, notice: Notice?) = notice?.let { postNotice(context, DANGER_CHANNEL, DANGER_TAG, it, "Position à surveiller dans vos avoirs") }

    /** A kit [Notice]: a tap opens its asset when it names one, otherwise the app. */
    fun postNotice(context: Context, channel: String, tag: String, notice: Notice, publicText: String) {
        if (!canNotify(context)) return
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            notice.asset?.let { putExtra(EXTRA_ASSET, it) }
        }
        // One request code per target: a grouped notification never reuses the previous one's asset.
        val pending = PendingIntent.getActivity(context, "$tag:${notice.asset.orEmpty()}".hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(notice.title)
            .setContentText(notice.body.lineSequence().first())
            .setStyle(NotificationCompat.BigTextStyle().bigText(notice.body + "\nConseil indicatif : Altim ne passe aucun ordre."))
            .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(NotificationCompat.Builder(context, channel).setSmallIcon(R.drawable.ic_notification).setContentTitle("Altim").setContentText(publicText).build())
            .setContentIntent(pending)
            .setAutoCancel(true)
            .build()
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        try {
            NotificationManagerCompat.from(context).notify(tag.hashCode(), n)
        } catch (_: SecurityException) {
            // Permission withdrawn in the meantime: nothing to post.
        }
    }

    fun canNotify(context: Context) =
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED &&
            NotificationManagerCompat.from(context).areNotificationsEnabled()

    /** A price alert reached: "BTC en dessous de 80 000 $" with the price now. */
    fun postTarget(context: Context, t: PriceTarget, price: Double) = post(
        context,
        BuyAlert(
            symbol = t.asset.symbol,
            kind = t.asset.kind,
            name = t.asset.name,
            price = price,
            buy = true,
            title = if (t.move != null) "${t.asset.symbol} a bougé de ${Format.percent((price / t.price - 1) * 100, 1)}"
            else "${t.asset.symbol} ${if (t.above) "au-dessus de" else "en dessous de"} ${Format.price(t.price)}",
            body = "Prix actuel ${Format.price(price)} : votre alerte de prix est atteinte. Réarmez-la dans l'onglet Alertes si besoin.",
        ),
        tag = "target:${t.id}",
    )

    /** Up to 3 alerts: one notification each; beyond, a single summary (the first check can find many at once). */
    fun postAll(context: Context, alerts: List<BuyAlert>) {
        if (alerts.size <= 3) return alerts.forEach { post(context, it) }
        val strong = alerts.filter { it.strong }
        post(
            context,
            BuyAlert(
                symbol = "RESUME",
                kind = alerts.first().kind,
                name = "Altim",
                buy = true,
                title = "${alerts.size} actifs achetables",
                body = (if (strong.isNotEmpty()) "Achat conseillé : ${strong.joinToString { it.symbol }}. " else "") +
                    "Achat possible : ${alerts.filterNot { it.strong }.joinToString { it.symbol }}. Ouvrez Altim pour le détail de chacun.",
            ),
            openAsset = false,
        )
    }

    fun post(context: Context, alert: BuyAlert, openAsset: Boolean = true, tag: String = alert.id) {
        if (!canNotify(context)) return
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            if (openAsset) putExtra(EXTRA_ASSET, alert.id)
        }
        val pending = PendingIntent.getActivity(context, alert.id.hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(alert.title)
            .setContentText(alert.body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(alert.body + "\nConseil indicatif : Altim ne passe aucun ordre."))
            .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
            // On the lock screen: only "Altim", the details once unlocked (the holdings are private).
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(
                NotificationCompat.Builder(context, CHANNEL).setSmallIcon(R.drawable.ic_notification).setContentTitle("Altim").setContentText("Achat possible sur un de vos actifs").build(),
            )
            .setContentIntent(pending)
            .setAutoCancel(true)
            .build()
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        try {
            NotificationManagerCompat.from(context).notify(tag.hashCode(), n)
        } catch (_: SecurityException) {
            // Permission withdrawn in the meantime: nothing to post.
        }
    }
}

class AlertWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val model = (applicationContext as AltimApplication).model
        if (!model.needsChecks) return Result.success()
        return try {
            model.checkAlerts()?.let { r ->
                BuyAlerts.postAll(applicationContext, r.buy)
                r.targets.forEach { (t, price) -> BuyAlerts.postTarget(applicationContext, t, price) }
                BuyAlerts.postNews(applicationContext, r.news)
                BuyAlerts.postChanges(applicationContext, ConfigNotices.notice(r.changes))
                BuyAlerts.postDangers(applicationContext, DangerNotices.notice(r.dangers))
            }
            Result.success()
        } catch (e: AltimException.Unauthorized) {
            // Session expired and the password is locked (phone locked): the next run after unlocking will log in.
            Result.success()
        } catch (e: Exception) {
            Result.retry()
        }
    }
}
