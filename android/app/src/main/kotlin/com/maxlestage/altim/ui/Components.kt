package com.maxlestage.altim.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.LivePrices
import com.maxlestage.altim.kit.Action
import com.maxlestage.altim.kit.ConfigSnapshot
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.RadarDecisions
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.delay

/**
 * Discreet line under amounts (FxNote of the web): "1 $ = 0,8819 € · Yahoo Finance, 17:15", or why they are still in
 * dollars (euros asked but no rate: never a made-up conversion). Nothing when dollars are chosen.
 */
@Composable
fun FxNote(model: com.maxlestage.altim.data.AppModel) {
    // Read so that the line follows the setting and the rate.
    model.currency
    model.fx
    val text = com.maxlestage.altim.kit.Money.note() ?: return
    Caption(text)
}

/** Card with title (same structure as the web and iPhone cards). */
@Composable
fun Card(title: String? = null, glow: Color = AltimColors.cyan, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Column(
        modifier
            .fillMaxWidth()
            .clip(shape)
            .background(AltimColors.surface.copy(alpha = 0.85f))
            .border(BorderStroke(1.dp, Brush.linearGradient(listOf(glow.copy(alpha = 0.7f), glow.copy(alpha = 0.05f), AltimColors.magenta.copy(alpha = 0.35f)))), shape)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (title != null) Text(title, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = Color.White)
        content()
    }
}

@Composable
fun Badge(text: String, tone: Tone, modifier: Modifier = Modifier) {
    val c = AltimColors.of(tone)
    Text(
        text,
        modifier
            .clip(CircleShape)
            .background(c.copy(alpha = 0.14f))
            .border(1.dp, c.copy(alpha = 0.5f), CircleShape)
            .padding(horizontal = 8.dp, vertical = 3.dp),
        color = c,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 0.5.sp,
        maxLines = 1,
    )
}

/**
 * The full decision's verdict (the asset's Décision card) as seen on this phone less than 12 h ago; "Décision…" while
 * none is known. The 4 h technical signal is never shown as a verdict: see [TechnicalText].
 */
@Composable
fun DecisionBadge(snapshot: ConfigSnapshot?, modifier: Modifier = Modifier) {
    if (snapshot == null) Badge("Décision…", Tone.NEUTRAL, modifier.alpha(0.7f))
    else Badge(RadarDecisions.label(snapshot), RadarDecisions.tone(snapshot), modifier)
}

/** « technique 4 h : haussier »: the technical signal as a small direction, one input of the decision. */
@Composable
fun TechnicalText(action: Action, interval: String = "4 h", modifier: Modifier = Modifier) =
    Text("technique $interval : ${RadarDecisions.technicalText(action)}", color = AltimColors.textSecondary, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = modifier)

@Composable
fun ChangeText(value: Double?) {
    Text(Format.percent(value), style = mono(13.sp), color = if (value == null) AltimColors.textSecondary else AltimColors.forChange(value))
}

/** Short line of recent closes (radar). */
@Composable
fun Sparkline(values: List<Double>, modifier: Modifier = Modifier) {
    val up = (values.lastOrNull() ?: 0.0) >= (values.firstOrNull() ?: 0.0)
    val color = if (up) AltimColors.buy else AltimColors.sell
    Canvas(modifier.clearAndSetSemantics { }) {
        if (values.size < 2) return@Canvas
        val lo = values.min()
        val hi = values.max()
        val span = (hi - lo).takeIf { it > 0 } ?: 1.0
        val path = Path()
        values.forEachIndexed { i, v ->
            val x = size.width * i / (values.size - 1)
            val y = size.height * (1 - ((v - lo) / span).toFloat())
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        drawPath(path, color, style = Stroke(width = 1.5.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
    }
}

/** "EN DIRECT" while ticks keep coming, otherwise the reconnection state. */
@Composable
fun LiveBadge(live: LivePrices) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(2_000)
            now = System.currentTimeMillis()
        }
    }
    val fresh = live.lastTick?.let { now - it < 20_000 } ?: false
    val c = if (fresh) AltimColors.buy else AltimColors.warning
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(c))
        Text(if (fresh) "EN DIRECT" else "CONNEXION…", color = c, fontSize = 10.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
    }
}

@Composable
fun KeyValue(key: String, value: String, tone: Tone? = null) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
        Text(key, color = AltimColors.textSecondary, fontSize = 14.sp, modifier = Modifier.weight(1f))
        Spacer(Modifier.width(12.dp))
        // A long value wraps on its half instead of squeezing the key to one letter per line.
        Text(value, style = mono(14.sp), color = tone?.let { AltimColors.of(it) } ?: Color.White, textAlign = TextAlign.End, modifier = Modifier.weight(1f))
    }
}

@Composable
fun Notice(text: String, tone: Tone = Tone.WARN) {
    val c = AltimColors.of(tone)
    val shape = RoundedCornerShape(14.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.copy(alpha = 0.1f))
            .border(1.dp, c.copy(alpha = 0.4f), shape)
            .padding(12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            when (tone) { Tone.BAD -> Icons.Filled.Error; Tone.GOOD -> Icons.Filled.CheckCircle; else -> Icons.Filled.Warning },
            contentDescription = null,
            tint = c,
            modifier = Modifier.size(18.dp),
        )
        Text(text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
    }
}

/** Meter 0–100 (guard). */
@Composable
fun Meter(label: String, value: Double, tone: Tone) {
    Column(
        Modifier.semantics(mergeDescendants = true) { contentDescription = "$label : ${Math.round(value)} sur 100" },
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row {
            Text(label, color = AltimColors.textSecondary, fontSize = 14.sp, modifier = Modifier.weight(1f))
            Text("${Math.round(value)}/100", style = mono(13.sp))
        }
        Bar(value, AltimColors.of(tone), 6.dp)
    }
}

@Composable
fun Bar(value: Double, color: Color, height: Dp) {
    Box(Modifier.fillMaxWidth().height(height).clip(CircleShape).background(Color.White.copy(alpha = 0.08f))) {
        Box(Modifier.fillMaxWidth((value.coerceIn(0.0, 100.0) / 100).toFloat().coerceAtLeast(0.02f)).height(height).clip(CircleShape).background(color))
    }
}

@Composable
fun ErrorBox(message: String, retry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Notice(message, Tone.BAD)
        OutlinedButton(onClick = retry, modifier = Modifier.fillMaxWidth(), border = BorderStroke(1.2.dp, AltimColors.cyan)) {
            Text("Réessayer", color = AltimColors.cyan)
        }
    }
}

@Composable
fun NeonButton(text: String, enabled: Boolean = true, busy: Boolean = false, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        enabled = enabled && !busy,
        modifier = modifier.fillMaxWidth().heightIn(min = 52.dp),
        shape = RoundedCornerShape(16.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = AltimColors.cyan,
            contentColor = Color.Black,
            disabledContainerColor = AltimColors.cyan.copy(alpha = 0.35f),
            disabledContentColor = Color.Black.copy(alpha = 0.6f),
        ),
    ) {
        if (busy) CircularProgressIndicator(Modifier.size(22.dp), color = Color.Black, strokeWidth = 2.dp)
        else Text(text, fontWeight = FontWeight.Bold, letterSpacing = 1.5.sp, fontSize = 16.sp)
    }
}

@Composable
fun Loading(text: String? = null) {
    Column(
        Modifier.fillMaxWidth().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CircularProgressIndicator(color = AltimColors.cyan)
        if (text != null) Text(text, color = AltimColors.textSecondary, fontSize = 13.sp, textAlign = TextAlign.Center)
    }
}

@Composable
fun Caption(text: String, color: Color = AltimColors.textSecondary) = Text(text, color = color, fontSize = 12.sp)

@Composable
fun SectionTitle(text: String) = Text(text, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = Color.White, modifier = Modifier.widthIn(max = 600.dp))

/** Loading / error / content of an asynchronous call. */
sealed interface Loadable<out T> {
    data object Loading : Loadable<Nothing>
    data class Failed(val message: String) : Loadable<Nothing>
    data class Loaded<T>(val value: T) : Loadable<T>
}

val <T> Loadable<T>.value: T? get() = (this as? Loadable.Loaded<T>)?.value

/** Offset helper for charts. */
internal fun off(x: Float, y: Float) = Offset(x, y)
