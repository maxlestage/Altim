package com.maxlestage.altim

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.printToString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.LaunchTrace
import com.maxlestage.altim.ui.LivePriceText
import com.maxlestage.altim.ui.PulseDot
import com.maxlestage.altim.ui.AltimColors
import com.maxlestage.altim.ui.Sparkline
import com.maxlestage.altim.ui.mono
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The app's motion on a paused clock: the launch trace frame by frame, a live tick's flash and roll, the sparkline
 * drawing itself. Screenshots in app/build/screens/motion-*.png. Nothing hides the figure: the new price is there from
 * the first frame of the tick.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h640dp-xhdpi", application = android.app.Application::class)
class MotionScreensTest {
    @get:Rule val compose = createComposeRule()

    @Test fun launchTraceDrawsTheLogoThenLeaves() {
        var done = false
        compose.mainClock.autoAdvance = false
        compose.setContent { AltimTheme { AppBackground { LaunchTrace { done = true } } } }
        for (t in listOf(150L, 450L, 900L)) {
            compose.mainClock.advanceTimeBy(if (t == 150L) 150 else if (t == 450L) 300 else 450)
            compose.onRoot().captureRoboImage("build/screens/motion-launch-$t.png")
        }
        compose.mainClock.autoAdvance = true
        compose.waitUntil(5_000) { done }
    }

    @Test fun aTickFlashesAndTheNewPriceIsShownAtOnce() {
        var price by mutableStateOf(73_560.31)
        compose.setContent {
            AltimTheme {
                AppBackground {
                    Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                        LivePriceText(price, Format.price(price), mono(32.sp))
                        PulseDot(AltimColors.buy, active = true)
                        Sparkline(listOf(1.0, 3.0, 2.0, 4.0, 3.5, 5.0), Modifier.width(120.dp).height(40.dp))
                    }
                }
            }
        }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { price = 73_564.08; Snapshot.sendApplyNotifications() }
        compose.mainClock.advanceTimeBy(120)
        assertTrue(compose.onRoot(useUnmergedTree = true).printToString(), compose.onAllNodes(hasText(Format.price(73_564.08)), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty())
        compose.onRoot().captureRoboImage("build/screens/motion-tick-up.png")
        compose.mainClock.advanceTimeBy(1_500)
        compose.runOnUiThread { price = 73_551.40; Snapshot.sendApplyNotifications() }
        compose.mainClock.advanceTimeBy(120)
        compose.onRoot().captureRoboImage("build/screens/motion-tick-down.png")
        compose.mainClock.advanceTimeBy(1_500)
        compose.onRoot().captureRoboImage("build/screens/motion-tick-still.png")
    }
}
