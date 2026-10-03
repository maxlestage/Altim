package com.maxlestage.altim

import android.Manifest
import android.app.Application
import android.app.NotificationManager
import androidx.test.core.app.ApplicationProvider
import androidx.work.testing.WorkManagerTestInitHelper
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.data.LiveTracking
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.ConfigLabel
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.ChangeNoticeState
import com.maxlestage.altim.kit.ChangeNotices
import com.maxlestage.altim.kit.ConfigTransition
import com.maxlestage.altim.kit.Danger
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
        val (one, state) = ChangeNotices.configNotice(listOf(transition("BTC", 2000.0)), ChangeNoticeState())
        BuyAlerts.postNotice(app, BuyAlerts.CONFIG_CHANNEL, one!!)
        val n = nm.allNotifications.single()
        assertEquals(BuyAlerts.CONFIG_CHANNEL, n.channelId)
        assertEquals("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT", n.extras.getString("android.title"))
        assertEquals("Conditions manquantes : Cassure de la résistance : pas encore.", n.extras.getCharSequence("android.text").toString())
        assertEquals("crypto:BTC", shadowOf(n.contentIntent).savedIntent.getStringExtra(BuyAlerts.EXTRA_ASSET))
        // Several at once: one grouped notification, the tap opens the app.
        val (grouped, _) = ChangeNotices.configNotice(listOf(transition("ETH", 3000.0), transition("SOL", 2500.0)), state)
        BuyAlerts.postNotice(app, BuyAlerts.CONFIG_CHANNEL, grouped!!)
        val g = nm.allNotifications.first { it.extras.getString("android.title")!!.startsWith("🚨 Changements") }
        assertEquals("🚨 Changements de configuration · 2", g.extras.getString("android.title"))
        assertNull(shadowOf(g.contentIntent).savedIntent.getStringExtra(BuyAlerts.EXTRA_ASSET))
        // The same change is never told twice.
        assertNull(ChangeNotices.configNotice(listOf(transition("BTC", 2000.0)), state).first)
    }

    @Test fun dangersHaveTheirOwnChannelAndNoFigures() {
        shadowOf(app).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        val d = Danger("h1", "AAPL", Kind.STOCK, "Apple", listOf(DangerReason("stop_broken", "Stop cassé : cours 140 $ sous votre stop 150 $.")))
        BuyAlerts.postNotice(app, BuyAlerts.DANGER_CHANNEL, ChangeNotices.dangerNotice(listOf(d), ChangeNoticeState(), 1000.0).first!!)
        val n = nm.allNotifications.single()
        assertEquals(BuyAlerts.DANGER_CHANNEL, n.channelId)
        assertEquals("⚠ Position devenue dangereuse : AAPL", n.extras.getString("android.title"))
        assertEquals("AAPL : stop cassé.", n.extras.getCharSequence("android.text").toString())
        assertEquals("stock:AAPL", shadowOf(n.contentIntent).savedIntent.getStringExtra(BuyAlerts.EXTRA_ASSET))
        val channels = app.getSystemService(NotificationManager::class.java).notificationChannels.map { it.id }
        assertTrue(channels.containsAll(listOf(BuyAlerts.CHANNEL, BuyAlerts.CONFIG_CHANNEL, BuyAlerts.DANGER_CHANNEL, LiveTracking.CHANNEL)))
    }

    @Test fun buySummaryAndDisclaimer() {
        shadowOf(app).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        val alerts = listOf("BTC", "ETH", "SOL", "ADA").mapIndexed { i, s -> BuyAlert(s, Kind.CRYPTO, buy = true, strong = i == 0, title = s, body = "corps") }
        assertEquals("Achat conseillé : BTC. Achat possible : ETH, SOL, ADA. Ouvrez Altim pour le détail de chacun.", BuyAlerts.summaryBody(alerts))
        assertEquals("Achat conseillé : BTC, ETH. Ouvrez Altim pour le détail de chacun.", BuyAlerts.summaryBody(alerts.take(2).map { it.copy(strong = true) }))
        BuyAlerts.postAll(app, alerts)
        val n = nm.allNotifications.single()
        assertEquals("4 actifs achetables", n.extras.getString("android.title"))
        assertTrue(n.extras.getCharSequence("android.text").toString().endsWith(" Conseil indicatif : Altim ne passe aucun ordre."))
    }

    @Test fun nothingIsPostedWithoutThePermission() {
        shadowOf(app).denyPermissions(Manifest.permission.POST_NOTIFICATIONS)
        BuyAlerts.createChannel(app)
        BuyAlerts.postNotice(app, BuyAlerts.CONFIG_CHANNEL, ChangeNotices.configNotice(listOf(transition("BTC", 2000.0)), ChangeNoticeState()).first!!)
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
        // Configuration changes alone need a watched asset (the default radar has some), dangers a held line.
        on.updateAlerts(app, enabled = false)
        assertEquals(on.watchlist.isNotEmpty(), on.needsChecks)
        on.updateAlerts(app, enabled = true)
        // Written once: turning the buy notifications off later does not change them.
        on.updateAlerts(app, enabled = false)
        assertTrue(AppModel(app, MemoryStore()).configAlertsEnabled)
        on.updateConfigAlerts(app, false)
        on.updateDangerAlerts(app, false)
        assertFalse(on.needsChecks)
        assertFalse(AppModel(app, MemoryStore()).dangerAlertsEnabled)
    }
}
