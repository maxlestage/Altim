package com.maxlestage.altim

import android.app.Application
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.fragment.app.FragmentActivity
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.Root

class AltimApplication : Application() {
    lateinit var model: AppModel
        private set

    override fun onCreate() {
        super.onCreate()
        model = AppModel(this)
    }
}

/** FragmentActivity: needed by BiometricPrompt (fingerprint / face / screen lock). */
class MainActivity : FragmentActivity() {
    private val model get() = (application as AltimApplication).model

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AltimTheme { Root(model) }
        }
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
