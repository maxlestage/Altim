package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.Currency
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.HistoryResponse
import com.maxlestage.altim.kit.PortfolioHistory
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

// Categorical palette validated for the allocation (dark surface, colour-blind readers): same entity, same colour.
private val MINE = Color(0xFF3987E5)
private val BENCH = mapOf("crypto:BTC" to Color(0xFFD95926), "stock:SPY" to Color(0xFF199E70))
private val DAY_FMT = DateTimeFormatter.ofPattern("d MMM", Locale.FRANCE).withZone(ZoneOffset.UTC)
private val YEAR_FMT = DateTimeFormatter.ofPattern("d MMM yyyy", Locale.FRANCE).withZone(ZoneOffset.UTC)
private fun day(t: Long) = DAY_FMT.format(Instant.ofEpochMilli(t))
/** Axis dates: with the year when the period spans more than one (1 year: "27 sept. 2025" → "27 sept. 2026"). */
private fun axisDay(t: Long, long: Boolean) = (if (long) YEAR_FMT else DAY_FMT).format(Instant.ofEpochMilli(t))
private fun pct(v: Double) = (if (v >= 0) "+" else "−") + String.format(Locale.FRANCE, "%.1f", abs(v)).removeSuffix(",0") + " %"
private fun usd(v: Double) = Format.amount(v, 0)

/** "How did what I own now behave": value of today's lines over the period, against Bitcoin and the S&P 500. */
@Composable
fun HistoryCard(model: AppModel) {
    var days by rememberSaveable { mutableIntStateOf(90) }
    var data by remember { mutableStateOf<HistoryResponse?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val assets = model.holdings.map { it.asset }.distinctBy { it.id }
    LaunchedEffect(days, assets.joinToString { it.id }) {
        val client = model.client ?: return@LaunchedEffect
        data = null
        try {
            data = client.history(assets, days)
            error = null
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: Exception) {
            error = e.message ?: "Historique indisponible"
        }
    }
    val lines = model.holdings.groupBy { it.asset.id }.map { (id, hs) -> id to hs.sumOf { it.quantity } }
    val h = data?.let { PortfolioHistory.compute(lines, it.byId, days) }

    Card(title = "Évolution de mes lignes") {
        ChoiceRow(listOf(30 to "30 j", 90 to "90 j", 365 to "1 an"), days, { days = it }, description = "Période")
        when {
            error != null -> Notice(error!!, com.maxlestage.altim.kit.Tone.BAD)
            data == null -> Loading("Chargement de l'historique…")
            h == null -> Caption("Pas assez d'historique pour vos lignes sur cette période.")
            else -> HistoryBody(h)
        }
    }
}

@Composable
private fun HistoryBody(h: PortfolioHistory) {
    var selected by remember(h) { mutableStateOf<Int?>(null) }
    val mine = h.pct
    val at = selected ?: (mine.size - 1)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text("${usd(h.points.first().value)} → ${usd(h.points.last().value)}", fontSize = 14.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
        Text(pct(h.change), style = mono(16.sp, FontWeight.Bold), color = AltimColors.forChange(h.change))
    }
    // Readout of the touched day (the last one by default), above the chart so the finger does not hide it.
    Text(
        "${day(h.points[at].t)} · Mes lignes ${usd(h.points[at].value)} (${pct(mine[at])})" +
            h.benchmarks.joinToString("") { " · ${it.label.substringBefore(" (")} ${pct(it.pct[at])}" },
        fontSize = 12.sp, color = AltimColors.textSecondary,
    )
    val lo = (mine + h.benchmarks.flatMap { it.pct } + 0.0).min()
    val hi = (mine + h.benchmarks.flatMap { it.pct } + 0.0).max()
    val summary = "Mes lignes ${pct(h.change)}" + h.benchmarks.joinToString("") { ", ${it.label} ${pct(it.change)}" }
    Box(Modifier.fillMaxWidth().height(170.dp)) {
        Canvas(
            Modifier.fillMaxWidth().height(150.dp).padding(start = 44.dp)
                .semantics { contentDescription = summary }
                .pointerInput(h) {
                    fun pick(x: Float) { selected = ((x / size.width) * (mine.size - 1)).roundToInt().coerceIn(0, mine.size - 1) }
                    detectTapGestures { pick(it.x) }
                }
                .pointerInput(h) {
                    detectDragGestures(onDragEnd = {}, onDrag = { change, _ ->
                        selected = ((change.position.x / size.width) * (mine.size - 1)).roundToInt().coerceIn(0, mine.size - 1)
                    })
                },
        ) {
            val span = (hi - lo).takeIf { it > 0 } ?: 1.0
            fun x(i: Int) = size.width * i / (mine.size - 1)
            fun y(v: Double) = size.height * (1 - ((v - lo) / span).toFloat())
            drawLine(Color.White.copy(alpha = 0.18f), Offset(0f, y(0.0)), Offset(size.width, y(0.0)), strokeWidth = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)))
            fun line(vals: List<Double>, color: Color, width: Float) {
                val p = Path()
                vals.forEachIndexed { i, v -> if (i == 0) p.moveTo(x(i), y(v)) else p.lineTo(x(i), y(v)) }
                drawPath(p, color, style = Stroke(width = width, cap = StrokeCap.Round, join = StrokeJoin.Round))
            }
            h.benchmarks.forEach { line(it.pct, BENCH[it.id] ?: Color.Gray, 1.5.dp.toPx()) }
            line(mine, MINE, 2.dp.toPx())
            selected?.let { i ->
                drawLine(Color.White.copy(alpha = 0.35f), Offset(x(i), 0f), Offset(x(i), size.height), strokeWidth = 1.dp.toPx())
                drawCircle(AltimColors.background, radius = 6.dp.toPx(), center = Offset(x(i), y(mine[i])))
                drawCircle(MINE, radius = 4.dp.toPx(), center = Offset(x(i), y(mine[i])))
            }
        }
        Text(pct(hi), fontSize = 10.sp, color = AltimColors.textSecondary, modifier = Modifier.align(Alignment.TopStart))
        Text(pct(lo), fontSize = 10.sp, color = AltimColors.textSecondary, modifier = Modifier.align(Alignment.TopStart).padding(top = 140.dp))
        Row(Modifier.align(Alignment.BottomStart).fillMaxWidth().padding(start = 44.dp)) {
            Text(axisDay(h.points.first().t, h.points.size > 200), fontSize = 10.sp, color = AltimColors.textSecondary)
            Spacer(Modifier.weight(1f))
            Text(axisDay(h.points.last().t, h.points.size > 200), fontSize = 10.sp, color = AltimColors.textSecondary)
        }
    }
    LegendRow(MINE, "Mes lignes", h.change)
    h.benchmarks.forEach { LegendRow(BENCH[it.id] ?: Color.Gray, it.label, it.change) }
    KeyValue("Pire recul depuis un sommet", pct(h.maxDrawdown), com.maxlestage.altim.kit.Tone.BAD)
    h.best?.let { KeyValue("Meilleure journée (${day(it.t)})", pct(it.change), com.maxlestage.altim.kit.Tone.GOOD) }
    h.worst?.let { KeyValue("Pire journée (${day(it.t)})", pct(it.change), com.maxlestage.altim.kit.Tone.BAD) }
    Caption(
        "Valeur chaque jour des quantités que vous détenez aujourd'hui (cours de clôture) : vos achats et ventes passés ne sont pas connus, " +
            "ce n'est donc pas la performance de votre compte." +
            (if (h.shortened) " La courbe commence plus tard : une de vos lignes a un historique plus court." else "") +
            (if (h.missing.isNotEmpty()) " Sans historique : ${h.missing.joinToString { it.substringAfter(":") }}." else "") +
            (if (Money.displayCurrency() == Currency.EUR) " Cours en dollars convertis au taux du jour, pas au taux de chaque date : l'effet de change passé n'est pas compté (les % restent exacts en dollars)." else ""),
    )
}

@Composable
private fun LegendRow(color: Color, label: String, change: Double) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(14.dp).height(3.dp).clip(RoundedCornerShape(2.dp)).background(color))
        Spacer(Modifier.size(8.dp))
        Text(label, fontSize = 13.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
        Text(pct(change), style = mono(13.sp, FontWeight.SemiBold), color = Color.White)
    }
}
