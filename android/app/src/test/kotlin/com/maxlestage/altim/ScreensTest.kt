package com.maxlestage.altim

import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ApplicationProvider
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.Root
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

private class MemoryStore : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

/**
 * The real screens, driven like a user, against a running Altim server:
 * ALTIM_SERVER=http://localhost:4410 ALTIM_USER=… ALTIM_PASSWORD=… ./gradlew :app:testDebugUnitTest
 * Screenshots of each screen in app/build/screens.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w400dp-h1600dp-xxhdpi", application = android.app.Application::class)
class ScreensTest {
    @get:Rule val compose = createComposeRule()

    private fun shot(name: String) = compose.onRoot().captureRoboImage("build/screens/$name.png")

    private fun waitFor(text: String, timeout: Long = 60_000, substring: Boolean = true) =
        compose.waitUntil(timeout) { compose.onAllNodes(hasText(text, substring = substring)).fetchSemanticsNodes().isNotEmpty() }

    @Test fun fullFlow() {
        val server = System.getenv("ALTIM_SERVER")
        assumeTrue("ALTIM_SERVER absent : test des écrans ignoré", server != null)
        val store = MemoryStore()
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        // The user accepted the notifications (Android 13+ asks once); the background checks use a test WorkManager.
        org.robolectric.Shadows.shadowOf(app).grantPermissions(android.Manifest.permission.POST_NOTIFICATIONS)
        androidx.work.testing.WorkManagerTestInitHelper.initializeTestWorkManager(app)
        val model = AppModel(app, store)
        model.updateBiometricLock(false)
        compose.setContent { AltimTheme { Root(model) } }

        // 1. Warning, then entry.
        shot("1-avertissement")
        compose.onNode(isToggleable()).performClick()
        compose.onNode(hasText("ENTRER")).performClick()

        // 2. Private login.
        compose.onAllNodes(hasSetTextAction()).onFirst().performTextInput(server!!)
        compose.onNode(hasText("CONTINUER")).performClick()
        waitFor("Mot de passe")
        val fields = compose.onAllNodes(hasSetTextAction())
        fields[1].performTextInput(System.getenv("ALTIM_USER") ?: "")
        fields[2].performTextInput(System.getenv("ALTIM_PASSWORD") ?: "")
        shot("2-connexion")
        compose.onNode(hasText("SE CONNECTER")).performClick()
        waitFor("SIGNAL 4 H")
        check(model.phase == AppModel.Phase.READY)
        check(store.get(SecureStore.Key.SESSION) != null) { "session cookie not stored" }

        // 3. Radar with signals and live prices.
        // Any signal badge (the market decides which one).
        compose.waitUntil(90_000) {
            listOf("ACHAT", "ATTENDRE", "VENTE").any { compose.onAllNodes(hasText(it, substring = true)).fetchSemanticsNodes().isNotEmpty() }
        }
        compose.waitUntil(30_000) { model.live.lastTick != null }
        compose.waitForIdle()
        shot("3-radar")

        // 4. Asset page: chart, zones, guard, macro.
        compose.onAllNodes(hasText("Bitcoin")).onFirst().performClick()
        waitFor("Retracement 38,2 %", 90_000)
        waitFor("Tendance de fond", 90_000)
        waitFor("sources en accord", 90_000)
        compose.waitForIdle()
        shot("4-fiche-btc")
        compose.onNode(hasText("Retour", substring = false).or(androidx.compose.ui.test.hasContentDescription("Retour"))).performClick()

        // 5. Selection (cryptos, 1 month): first computation ≈ 30 s on the server.
        compose.onAllNodes(hasText("Sélection")).onFirst().performClick()
        compose.onNode(hasText("Cryptos")).performClick()
        waitFor("Ce que cette méthode aurait donné", 240_000)
        waitFor("À acheter")
        compose.waitForIdle()
        shot("5-selection-crypto")

        // 6. Holdings: add 0.5 BTC bought 60 000 $.
        compose.onAllNodes(hasText("Mes avoirs")).onFirst().performClick()
        compose.onAllNodes(hasText("Ajouter un avoir")).onFirst().performClick()
        compose.onAllNodes(hasSetTextAction()).onFirst().performTextInput("btc")
        waitFor("Bitcoin", 30_000)
        compose.onAllNodes(hasText("Bitcoin")).onFirst().performClick()
        val form = compose.onAllNodes(hasSetTextAction())
        form[0].performTextInput("0,5")
        form[1].performTextInput("60000")
        compose.onNode(hasText("Enregistrer")).performClick()
        waitFor("Valeur totale")
        waitFor("depuis l'achat", 30_000)
        compose.waitForIdle()
        check(model.holdings.single().quantity == 0.5)
        shot("6-avoirs")

        // 7. Price alert from the Bitcoin page, then the Alerts tab.
        compose.onAllNodes(hasText("Radar")).onFirst().performClick()
        compose.onAllNodes(hasText("Bitcoin")).onFirst().performClick()
        waitFor("Retracement 38,2 %", 90_000)
        compose.onNode(androidx.compose.ui.test.hasContentDescription("Alerte de prix")).performClick()
        waitFor("Alerte de prix · BTC")
        compose.onNode(hasText("Passe au-dessus")).performClick()
        val field = compose.onAllNodes(hasSetTextAction()).onFirst()
        field.performTextClearance()
        field.performTextInput("1000000")
        compose.onNode(hasText("Créer l'alerte")).performClick()
        check(model.priceTargets.single().above && model.priceTargets.single().price == 1_000_000.0)
        compose.onNode(androidx.compose.ui.test.hasContentDescription("Retour")).performClick()
        compose.onAllNodes(hasText("Alertes")).onFirst().performClick()
        waitFor("Achetables maintenant")
        waitFor("au-dessus de 1", 30_000)
        compose.waitUntil(90_000) {
            compose.onAllNodes(hasText("ACHAT POSSIBLE", substring = true)).fetchSemanticsNodes().isNotEmpty() ||
                compose.onAllNodes(hasText("ACHAT CONSEILLÉ", substring = true)).fetchSemanticsNodes().isNotEmpty() ||
                compose.onAllNodes(hasText("Rien d'achetable", substring = true)).fetchSemanticsNodes().isNotEmpty()
        }
        waitFor("Prix actuel", 30_000)
        compose.waitForIdle()
        shot("7-alertes")

        // 8. Settings.
        compose.onAllNodes(hasText("Réglages")).onFirst().performClick()
        waitFor("Se déconnecter")
        shot("8-reglages")
    }
}
