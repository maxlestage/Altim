package com.maxlestage.altim

import android.Manifest
import android.app.Application
import android.app.NotificationManager
import androidx.test.core.app.ApplicationProvider
import androidx.work.testing.WorkManagerTestInitHelper
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.ConfigLabel
import com.maxlestage.altim.kit.ConfigNotices
import com.maxlestage.altim.kit.ConfigTransition
import com.maxlestage.altim.kit.Danger
import com.maxlestage.altim.kit.DangerNotices
import com.maxlestage.altim.kit.DangerReason
import com.maxlestage.altim.kit.DecisionLevel
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.Verdict
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** Configuration-change and danger notifications: their channels, texts, tap target, and the settings' defaults. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class WatchNotificationsTest {
    private class MemoryStore : SecretStore {
        val values = mutableMapOf<SecureStore.Key, String>()
        override fun set(k: SecureStore.Key, value: String?) {
            if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
        }
        override fun get(k: SecureStore.Key) = values[k]
        override fun clear() = values.clear()
    }

    private val app = ApplicationProvider.getApplicationContext<Application>()
    private val nm get() = shadowOf(app.getSystemService(NotificationManager::class.java))

    private fun transition(symbol: String, at: Double) = ConfigTransition(
        symbol, Kind.CRYPTO, symbol, false, at, at - 1000,
        ConfigLabel(Verdict.WAIT, DecisionLevel.MODERATE, "ATTENDRE", "Signal modéré"),
        ConfigLabel(Verdict.BUY_ZONE, DecisionLevel.MODERATE, "ZONE D'ACHAT", "Signal modéré"),
        missing = listOf("Cassure de la résistance : pas encore"), triggers = emptyList(),
    )

    @Test fun oneTransitionOpensItsAssetSeveralMakeOneNotification() {
        shadowOf(app).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        BuyAlerts.postChanges(app, ConfigNotices.notice(listOf(transition("BTC", 2000.0))))
        val n = nm.allNotifications.single()
        assertEquals(BuyAlerts.CONFIG_CHANNEL, n.channelId)
        assertEquals("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT", n.extras.getString("android.title"))
        assertEquals("Conditions manquantes : Cassure de la résistance : pas encore.", n.extras.getCharSequence("android.text").toString())
        assertEquals("crypto:BTC", shadowOf(n.contentIntent).savedIntent.getStringExtra(BuyAlerts.EXTRA_ASSET))
        // Several at once: still one notification (it replaces the previous one), the tap opens the app.
        BuyAlerts.postChanges(app, ConfigNotices.notice(listOf(transition("ETH", 3000.0), transition("SOL", 2500.0))))
        val grouped = nm.allNotifications.single()
        assertEquals("🚨 2 changements de configuration", grouped.extras.getString("android.title"))
        assertNull(shadowOf(grouped.contentIntent).savedIntent.getStringExtra(BuyAlerts.EXTRA_ASSET))
        BuyAlerts.postChanges(app, ConfigNotices.notice(emptyList()))
        assertEquals(1, nm.allNotifications.size)
    }

    @Test fun dangersHaveTheirOwnChannelAndTheWebWording() {
        shadowOf(app).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        val d = Danger("h1", "AAPL", Kind.STOCK, "Apple", listOf(DangerReason("stop_broken", "Stop cassé : cours 140 $ sous votre stop 150 $.")))
        BuyAlerts.postDangers(app, DangerNotices.notice(listOf(d)))
        val n = nm.allNotifications.single()
        assertEquals(BuyAlerts.DANGER_CHANNEL, n.channelId)
        assertEquals("⚠ Position devenue dangereuse dans vos avoirs", n.extras.getString("android.title"))
        assertEquals("AAPL : Stop cassé : cours 140 $ sous votre stop 150 $.", n.extras.getCharSequence("android.text").toString())
        assertEquals("stock:AAPL", shadowOf(n.contentIntent).savedIntent.getStringExtra(BuyAlerts.EXTRA_ASSET))
        val channels = app.getSystemService(NotificationManager::class.java).notificationChannels.map { it.id }
        assertTrue(channels.containsAll(listOf(BuyAlerts.CHANNEL, BuyAlerts.CONFIG_CHANNEL, BuyAlerts.DANGER_CHANNEL)))
    }

    @Test fun nothingIsPostedWithoutThePermission() {
        shadowOf(app).denyPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        BuyAlerts.postChanges(app, ConfigNotices.notice(listOf(transition("BTC", 2000.0))))
        assertEquals(0, nm.allNotifications.size)
    }

    @Test fun switchesAreOnByDefaultOnlyWhenBuyNotificationsAre() {
        WorkManagerTestInitHelper.initializeTestWorkManager(app)
        val prefs = app.getSharedPreferences("altim", android.content.Context.MODE_PRIVATE)
        prefs.edit().clear().apply()
        val off = AppModel(app, MemoryStore())
        assertFalse(off.configAlertsEnabled)
        assertFalse(off.dangerAlertsEnabled)
        prefs.edit().clear().putBoolean("alertsEnabled", true).apply()
        val on = AppModel(app, MemoryStore())
        assertTrue(on.configAlertsEnabled && on.dangerAlertsEnabled && on.needsChecks)
        // Written once: turning the buy notifications off later does not change them.
        on.updateAlerts(app, enabled = false)
        assertTrue(AppModel(app, MemoryStore()).configAlertsEnabled)
        on.updateConfigAlerts(app, false)
        on.updateDangerAlerts(app, false)
        assertFalse(on.needsChecks)
        assertFalse(AppModel(app, MemoryStore()).dangerAlertsEnabled)
    }
}
