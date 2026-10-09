package com.maxlestage.altim.kit

/**
 * The pure side of the app's motion (the web's frontend/door.js and app.css micro-animations, iOS AltimKit `Motion`):
 * the Altim logo's strokes, the launch trace's timing, the direction of a price tick, whether the system lets the app
 * animate. Every animation has a still state, and none hides or delays a figure.
 */
object Motion {
    /** A point of a drawing in a unit box (0…1, y down). */
    data class Point(val x: Double, val y: Double)

    /** The Altim logo (web/public/logo.svg, 1024 box): the peak, the rising line in the accent colour, its dot. */
    object Logo {
        val peak = listOf(pt(232, 780), pt(512, 214), pt(792, 780))
        val line = listOf(pt(330, 600), pt(430, 520), pt(520, 575), pt(700, 420))
        val dot = pt(700, 420)
        const val DOT_RADIUS = 30.0 / 1024
        /** Stroke width of the logo, relative to its box. */
        const val STROKE = 54.0 / 1024

        private fun pt(x: Int, y: Int) = Point(x / 1024.0, y / 1024.0)
    }

    /** The launch trace (milliseconds): the peak first, the line from 75 % of it, then the dot; then the app. */
    object LaunchTrace {
        val peak = 0L..550L
        val line = 420L..800L
        val dot = 780L..950L
        /** When the trace is over and the app takes the screen. */
        const val DURATION = 1050L

        /** How much of a stroke is drawn at `t` ms after launch (0 → 1, eased). */
        fun progress(span: LongRange, t: Long): Double = ease((t - span.first).toDouble() / (span.last - span.first))
    }

    /** Hermite smoothstep 0 → 1. */
    fun ease(x: Double): Double {
        val t = x.coerceIn(0.0, 1.0)
        return t * t * (3 - 2 * t)
    }

    /** Direction of a live tick: cyan flash when it rises, red when it falls; nothing for a first or equal price. */
    enum class Tick {
        UP, DOWN;

        companion object {
            fun between(old: Double?, new: Double?): Tick? {
                if (old == null || new == null || !old.isFinite() || !new.isFinite() || old == new) return null
                return if (new > old) UP else DOWN
            }
        }
    }

    /**
     * Whether the app may animate: Settings › Accessibility › « Supprimer les animations » (or the developer options)
     * sets `Settings.Global.ANIMATOR_DURATION_SCALE` to 0. An unreadable setting counts as on.
     */
    fun animationsOn(animatorDurationScale: Float?): Boolean = animatorDurationScale == null || animatorDurationScale > 0f

    /** How long a tick's flash lasts (ms), like the web's `.live-price.flash-up`. */
    const val FLASH = 900
    /** How long a sparkline takes to draw itself the first time (ms), like the web's `.spark`. */
    const val SPARK_DRAW = 900
    /** One beat of the « EN DIRECT » dot (ms), like the web's `.live-badge`. */
    const val LIVE_PULSE = 1600
}
