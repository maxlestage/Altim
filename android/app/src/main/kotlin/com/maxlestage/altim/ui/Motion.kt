package com.maxlestage.altim.ui

import android.provider.Settings
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathMeasure
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.maxlestage.altim.kit.Motion
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// The app's motion, in the spirit of the web site (frontend/door.js, app.css): the logo traced at launch and after the
// login, prices that roll and flash on a live tick, a verdict that cross-fades when it changes, the « EN DIRECT » dot's
// pulse, sparklines that draw themselves. Restrained (it is a tool): nothing hides or delays a figure, and with
// « Supprimer les animations » (ANIMATOR_DURATION_SCALE = 0) everything is still. Timings and geometry: kit `Motion`.

/** Whether the system lets the app animate (`Motion.animationsOn` of ANIMATOR_DURATION_SCALE). */
@Composable
fun animationsOn(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        Motion.animationsOn(runCatching { Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) }.getOrNull())
    }
}

private fun DrawScope.stroke(points: List<Motion.Point>, progress: Float, color: Color, width: Float, measure: PathMeasure) {
    if (progress <= 0f) return
    val side = size.minDimension
    val full = Path()
    points.forEachIndexed { i, p ->
        val x = (p.x * side).toFloat()
        val y = (p.y * side).toFloat()
        if (i == 0) full.moveTo(x, y) else full.lineTo(x, y)
    }
    measure.setPath(full, false)
    val part = Path()
    measure.getSegment(0f, measure.length * progress.coerceIn(0f, 1f), part, true)
    // A soft glow under the line, then the line.
    drawPath(part, color.copy(alpha = 0.25f), style = Stroke(width * 2.4f, cap = StrokeCap.Round, join = StrokeJoin.Round))
    drawPath(part, color, style = Stroke(width, cap = StrokeCap.Round, join = StrokeJoin.Round))
}

/**
 * The Altim logo traced stroke by stroke (`PathMeasure.getSegment` + `drawPath` with the progress): the peak, then the
 * rising line in cyan, then its dot; over the app's background, gone after `Motion.LaunchTrace.DURATION`. Shown at
 * launch and after the login, never when animations are off (the caller does not show it). No semantics, no touch.
 */
@Composable
fun LaunchTrace(onDone: () -> Unit) {
    val peak = remember { Animatable(0f) }
    val line = remember { Animatable(0f) }
    val dot = remember { Animatable(0f) }
    val shown = remember { Animatable(1f) }
    val measure = remember { PathMeasure() }
    val t = Motion.LaunchTrace
    LaunchedEffect(Unit) {
        launch { peak.animateTo(1f, tween((t.peak.last - t.peak.first).toInt(), easing = FastOutSlowInEasing)) }
        launch { line.animateTo(1f, tween((t.line.last - t.line.first).toInt(), delayMillis = t.line.first.toInt(), easing = FastOutSlowInEasing)) }
        launch { dot.animateTo(1f, tween((t.dot.last - t.dot.first).toInt(), delayMillis = t.dot.first.toInt())) }
        delay(t.DURATION)
        shown.animateTo(0f, tween(250))
        onDone()
    }
    Box(
        Modifier.fillMaxSize().alpha(shown.value).background(AltimColors.background).clearAndSetSemantics { },
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(112.dp).scale(1f + 0.12f * (1f - shown.value))) {
            val side = size.minDimension
            val width = (Motion.Logo.STROKE * side).toFloat()
            stroke(Motion.Logo.peak, peak.value, Color.White, width, measure)
            stroke(Motion.Logo.line, line.value, AltimColors.cyan, width, measure)
            if (dot.value > 0f) {
                drawCircle(
                    AltimColors.cyan,
                    radius = (Motion.Logo.DOT_RADIUS * side * 1.1).toFloat() * (0.2f + 0.8f * dot.value),
                    center = Offset((Motion.Logo.dot.x * side).toFloat(), (Motion.Logo.dot.y * side).toFloat()),
                    alpha = dot.value,
                )
            }
        }
    }
}

/**
 * A live price: the new figure rolls in (`AnimatedContent`, from below when it rises, from above when it falls) and
 * flashes cyan or red for `Motion.FLASH` ms; its size does not animate (nothing around it moves). Animations off:
 * the new figure, still.
 */
@Composable
fun LivePriceText(value: Double?, text: String, style: TextStyle, modifier: Modifier = Modifier, textAlign: TextAlign? = null) {
    val on = animationsOn()
    val last = remember { doubleArrayOf(Double.NaN) }
    // The tick being shown now (for the roll's direction), and the last one (for the flash's colour while it fades).
    val tick = if (on) Motion.Tick.between(last[0].takeIf { !it.isNaN() }, value) else null
    var lastTick by remember { mutableStateOf<Motion.Tick?>(null) }
    val flash = remember { Animatable(0f) }
    val tint = if ((tick ?: lastTick) == Motion.Tick.DOWN) AltimColors.sell else AltimColors.cyan
    LaunchedEffect(value) {
        val t = Motion.Tick.between(last[0].takeIf { !it.isNaN() }, value)
        last[0] = value ?: Double.NaN
        if (on && t != null) {
            lastTick = t
            flash.snapTo(1f)
            flash.animateTo(0f, tween(Motion.FLASH))
        }
    }
    AnimatedContent(
        targetState = text,
        modifier = modifier.clip(RoundedCornerShape(4.dp)).background(tint.copy(alpha = 0.25f * flash.value)),
        transitionSpec = {
            val dir = if (tick == Motion.Tick.DOWN) -1 else 1
            val spec = if (on && tick != null) {
                (slideInVertically(tween(350)) { dir * it / 2 } + fadeIn(tween(350))) togetherWith (slideOutVertically(tween(350)) { -dir * it / 2 } + fadeOut(tween(200)))
            } else {
                fadeIn(snap()) togetherWith fadeOut(snap())
            }
            spec using SizeTransform(clip = true) { _, _ -> snap() }
        },
        label = "prix",
    ) { shown ->
        Text(shown, style = style, textAlign = textAlign, color = if (flash.value > 0.05f) lerpColor(Color.White, tint, flash.value) else Color.White)
    }
}

private fun lerpColor(a: Color, b: Color, t: Float) = androidx.compose.ui.graphics.lerp(a, b, t.coerceIn(0f, 1f))

/** The « EN DIRECT » dot: a ring leaves it on each beat while the stream is live; animations off, the dot alone. */
@Composable
fun PulseDot(color: Color, active: Boolean) {
    val on = animationsOn() && active
    Box(Modifier.size(7.dp), contentAlignment = Alignment.Center) {
        if (on) {
            val beat = rememberInfiniteTransition(label = "direct").animateFloat(
                0f, 1f, infiniteRepeatable(tween(Motion.LIVE_PULSE, easing = LinearEasing), RepeatMode.Restart), label = "battement",
            )
            Canvas(Modifier.size(7.dp)) {
                val b = beat.value
                drawCircle(color, radius = size.minDimension / 2 * (0.6f + 1.8f * b), alpha = 0.9f * (1f - b), style = Stroke(1.dp.toPx()))
            }
        }
        Box(Modifier.size(7.dp).clip(CircleShape).background(color))
    }
}

/** How much of a sparkline is drawn (0 → 1), left to right, the first time it shows; animations off: all of it. */
@Composable
fun rememberDrawIn(): Float {
    val on = animationsOn()
    val shown = remember { Animatable(if (on) 0f else 1f) }
    LaunchedEffect(Unit) { shown.animateTo(1f, tween(Motion.SPARK_DRAW, easing = FastOutSlowInEasing)) }
    return shown.value
}

/** A new verdict cross-fades in, rising slightly (web `.dec-changed`); animations off: replaced at once. */
@Composable
fun <T> VerdictSwap(target: T, content: @Composable (T) -> Unit) {
    val on = animationsOn()
    AnimatedContent(
        targetState = target,
        transitionSpec = {
            if (on) {
                (fadeIn(tween(300)) + slideInVertically(spring()) { it / 3 } + scaleIn(initialScale = 0.92f)) togetherWith fadeOut(tween(150))
            } else {
                fadeIn(snap()) togetherWith fadeOut(snap())
            }
        },
        label = "verdict",
    ) { content(it) }
}
