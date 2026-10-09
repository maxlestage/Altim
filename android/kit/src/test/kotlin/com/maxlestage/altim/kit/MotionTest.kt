package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Same logo, timing and tick rules as iOS (AltimKit MotionTests) and the web (frontend/door.js, app.css). */
class MotionTest {
    @Test fun logoFitsTheUnitBox() {
        val all = Motion.Logo.peak + Motion.Logo.line + Motion.Logo.dot
        assertTrue(all.all { it.x in 0.0..1.0 && it.y in 0.0..1.0 })
        assertEquals(Motion.Logo.peak[1].y, Motion.Logo.peak.minOf { it.y })
        assertEquals(Motion.Logo.dot, Motion.Logo.line.last())
    }

    @Test fun launchTraceDrawsThePeakThenTheLineThenTheDot() {
        val t = Motion.LaunchTrace
        assertEquals(0.0, t.progress(t.peak, 0))
        assertEquals(1.0, t.progress(t.peak, 550))
        assertEquals(0.0, t.progress(t.line, 300))
        assertEquals(0.5, t.progress(t.line, 610), 0.01)
        assertEquals(1.0, t.progress(t.dot, 2000))
        assertTrue(t.dot.last < t.DURATION)
        assertTrue(t.DURATION <= 1200)
    }

    @Test fun tickDirection() {
        assertEquals(Motion.Tick.UP, Motion.Tick.between(100.0, 101.0))
        assertEquals(Motion.Tick.DOWN, Motion.Tick.between(100.0, 99.5))
        assertNull(Motion.Tick.between(100.0, 100.0))
        assertNull(Motion.Tick.between(null, 100.0))
        assertNull(Motion.Tick.between(100.0, null))
        assertNull(Motion.Tick.between(Double.NaN, 100.0))
    }

    @Test fun removeAnimationsTurnsMotionOff() {
        assertTrue(Motion.animationsOn(1f))
        assertTrue(Motion.animationsOn(0.5f))
        assertTrue(Motion.animationsOn(null))
        assertFalse(Motion.animationsOn(0f))
    }
}
