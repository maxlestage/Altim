package com.maxlestage.altim

import android.app.Application
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.fragment.app.FragmentActivity
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.Root

class AltimApplication : Application() {
    lateinit var model: AppModel
        private set

    override fun onCreate() {
        super.onCreate()
        model = AppModel(this)
        BuyAlerts.createChannel(this)
        BuyAlerts.schedule(this, model.needsChecks)
    }
}

/** FragmentActivity: needed by BiometricPrompt (fingerprint / face / screen lock). */
class MainActivity : FragmentActivity() {
    private val model get() = (application as AltimApplication).model

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Holdings must not appear in the recent-apps overview (Android 13+ keeps screenshots possible);
        // on older versions the screen is marked secure.
        if (android.os.Build.VERSION.SDK_INT >= 33) setRecentsScreenshotEnabled(false)
        else window.setFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE, android.view.WindowManager.LayoutParams.FLAG_SECURE)
        model.openFromNotification(intent?.getStringExtra(BuyAlerts.EXTRA_ASSET))
        setContent {
            AltimTheme { Root(model) }
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        model.openFromNotification(intent.getStringExtra(BuyAlerts.EXTRA_ASSET))
    }

    override fun onStart() {
        super.onStart()
        model.willEnterForeground()
    }

    override fun onStop() {
        super.onStop()
        if (!isChangingConfigurations) model.didEnterBackground()
    }
}
