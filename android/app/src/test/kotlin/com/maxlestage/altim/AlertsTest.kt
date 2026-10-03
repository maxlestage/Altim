package com.maxlestage.altim

import android.Manifest
import android.app.Application
import android.app.NotificationManager
import androidx.test.core.app.ApplicationProvider
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import androidx.work.testing.WorkManagerTestInitHelper
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.NewsItem
import com.maxlestage.altim.kit.PriceTarget
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * Buy notifications against a running server (skipped without ALTIM_SERVER): the check posts one notification per
 * buyable asset, then nothing more while the situation stays the same.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AlertsTest {
    private class MemoryStore : SecretStore {
        val values = mutableMapOf<SecureStore.Key, String>()
        override fun set(k: SecureStore.Key, value: String?) {
            if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
        }
        override fun get(k: SecureStore.Key) = values[k]
        override fun clear() = values.clear()
    }

    @Test fun notifiesOnlyNewBuyOpportunities() = runBlocking {
        val server = System.getenv("ALTIM_SERVER")
        assumeTrue(server != null)
        val app = ApplicationProvider.getApplicationContext<Application>()
        shadowOf(app).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        WorkManagerTestInitHelper.initializeTestWorkManager(app)
        val model = AppModel(app, MemoryStore())
        val (url, mode) = model.probe(server!!)
        model.connect(url, mode, System.getenv("ALTIM_USER") ?: "", System.getenv("ALTIM_PASSWORD") ?: "", "")
        model.updateAlerts(app, enabled = true)
        // Two price alerts: one reached at once (BTC above 1 $), one never (above a billion).
        val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
        model.addTarget(app, PriceTarget(asset = btc, above = true, price = 1.0))
        model.addTarget(app, PriceTarget(asset = btc, above = true, price = 1e9))
        val result = model.checkAlerts()!!
        val first = result.buy
        assertEquals(1, result.targets.size)
        assertEquals(1.0, result.targets.single().first.price, 0.0)
        assertEquals(1, model.priceTargets.count { it.triggered != null })
        // Journal: every buy alert notified + the price alert, each with its price.
        assertEquals(first.size + 1, model.journal.size)
        assertTrue(model.journal.all { it.price > 0 })
        BuyAlerts.postAll(app, first)
        val nm = shadowOf(app.getSystemService(NotificationManager::class.java))
        assertEquals(if (first.size > 3) 1 else first.size, nm.allNotifications.size)
        assertEquals(model.lastAlerts.count { it.buy }, first.size)
        if (first.isNotEmpty()) {
            val n = nm.allNotifications.first()
            val title = n.extras.getString("android.title")!!
            assertTrue(title, "achat" in title || "achetables" in title)
            assertEquals(BuyAlerts.CHANNEL, n.channelId)
        }
        // Same situation a quarter of an hour later: no repeat.
        val again = model.checkAlerts()!!
        assertEquals(0, again.buy.size)
        assertEquals("a reached price alert does not fire twice", 0, again.targets.size)
        assertTrue("the other price alert is still armed", model.needsChecks)
        // News alerts on the real feeds: whatever the day brings, a story is never notified twice.
        model.updateNewsAlerts(app, true)
        val news = model.checkAlerts()!!.news
        println("Actualités notifiées : ${news.joinToString { it.title }}")
        assertTrue(news.all { (it.alert && it.alsoIn.isNotEmpty()) || it.alsoIn.size >= 2 })
        assertEquals(0, model.checkAlerts()!!.news.size)
        // Posting: its own channel, a tap opens the Actu tab.
        val before = nm.allNotifications.size
        BuyAlerts.postNews(app, listOf(NewsItem(id = "t", title = "Russia declares war on neighbour", link = "https://ex.com", time = System.currentTimeMillis().toDouble(), source = "Reuters", category = "monde", alert = true, alsoIn = listOf("CNBC"))))
        assertEquals(before + 1, nm.allNotifications.size)
        val posted = nm.allNotifications.single { it.channelId == BuyAlerts.NEWS_CHANNEL && it.extras.getString("android.title") == "Alerte actualité" }
        assertTrue(shadowOf(posted.contentIntent).savedIntent.getBooleanExtra(BuyAlerts.EXTRA_NEWS, false))
        println("Alertes : ${first.joinToString { it.title }}")
        check(AltimClient.normalize(server) != null)
    }
}
