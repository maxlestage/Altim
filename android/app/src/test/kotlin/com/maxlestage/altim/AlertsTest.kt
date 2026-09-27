package com.maxlestage.altim

import android.Manifest
import android.app.Application
import android.app.NotificationManager
import androidx.test.core.app.ApplicationProvider
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.AltimClient
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
        val model = AppModel(app, MemoryStore())
        val (url, mode) = model.probe(server!!)
        model.connect(url, mode, System.getenv("ALTIM_USER") ?: "", System.getenv("ALTIM_PASSWORD") ?: "", "")
        val first = model.checkAlerts()!!
        BuyAlerts.postAll(app, first)
        val nm = shadowOf(app.getSystemService(NotificationManager::class.java))
        assertEquals(if (first.size > 3) 1 else first.size, nm.allNotifications.size)
        assertEquals(model.lastBuyable, first.size)
        if (first.isNotEmpty()) {
            val n = nm.allNotifications.first()
            val title = n.extras.getString("android.title")!!
            assertTrue(title, "achat" in title || "achetables" in title)
            assertEquals(BuyAlerts.CHANNEL, n.channelId)
        }
        // Same situation a quarter of an hour later: no repeat.
        assertEquals(0, model.checkAlerts()!!.size)
        println("Alertes : ${first.joinToString { it.title }}")
        check(AltimClient.normalize(server) != null)
    }
}
