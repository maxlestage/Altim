package com.maxlestage.altim.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Strategies
import com.maxlestage.altim.kit.StrategiesReport
import com.maxlestage.altim.kit.StrategyResult
import com.maxlestage.altim.kit.Tone

// "Comparer les stratégies" (asset screen, web StrategiesCard.tsx): textbook strategies with fixed parameters on this
// asset's daily history (/api/strategies). Chips to pick them (they wrap), one single-axis chart (value of 100 invested)
// drawn on a Canvas with the web palette and the dashed buy-and-hold reference, or its list view, then one stacked card
// of metrics per strategy and how the test avoids flattering itself. Nothing scrolls sideways.

private fun strategyColor(id: String) = Color(Strategies.COLORS[id] ?: 0xFF9AA0B4)

/** Loads /api/strategies for this asset (cached one hour by the server). */
@Composable
fun StrategiesCard(model: AppModel, asset: Asset) {
    var state by remember(asset.id) { mutableStateOf<Loadable<StrategiesReport>>(Loadable.Loading) }
    LaunchedEffect(asset.id) {
        val client = model.client ?: return@LaunchedEffect
        state = try {
            Loadable.Loaded(client.strategies(asset))
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
            return@LaunchedEffect
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            Loadable.Failed(e.message ?: "Comparaison indisponible")
        }
    }
    StrategiesView(asset.symbol, state)
}

/** The card from a loading state (no server: also rendered by the tests). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StrategiesView(symbol: String, state: Loadable<StrategiesReport>, initialList: Boolean = false, initialSelection: List<String> = Strategies.DEFAULT_SELECTION) {
    var selected by rememberSaveable(symbol) { mutableStateOf(initialSelection) }
    var asList by rememberSaveable(symbol) { mutableStateOf(initialList) }
    Card(title = "Comparer les stratégies") {
        Caption("Comment des stratégies classiques, avec leurs réglages de manuel, se seraient comportées sur l'historique de $symbol.")
        when (state) {
            is Loadable.Loading -> Loading()
            is Loadable.Failed -> Notice(state.message, Tone.WARN)
            is Loadable.Loaded -> {
                val report = state.value
                FlowRow(
                    Modifier.fillMaxWidth().semantics { contentDescription = "Stratégies à comparer" },
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Strategies.ORDER.forEach { id ->
                        val s = report.strategies.firstOrNull { it.id == id } ?: return@forEach
                        val on = id in selected
                        FilterChip(
                            selected = on,
                            onClick = { selected = Strategies.toggle(selected, id) },
                            label = { Text(Strategies.short(id)) },
                            leadingIcon = { Swatch(id) },
                            modifier = Modifier.semantics { contentDescription = s.name },
                            border = FilterChipDefaults.filterChipBorder(
                                enabled = true, selected = on,
                                borderColor = Color.White.copy(alpha = 0.25f), selectedBorderColor = strategyColor(id), selectedBorderWidth = 1.5.dp,
                            ),
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = strategyColor(id).copy(alpha = 0.14f),
                                selectedLabelColor = Color.White,
                                labelColor = AltimColors.textSecondary,
                            ),
                        )
                    }
                }
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.SemiBold, color = Color.White)) { append("Période testée") }
                        append(" : ${report.period} · source ${report.source}")
                    },
                    fontSize = 12.sp, color = AltimColors.textSecondary,
                )
                val series = Strategies.drawable(report, selected)
                when {
                    series.isEmpty() -> Caption("Choisissez au moins une stratégie disponible.")
                    asList -> StrategiesList(series)
                    else -> StrategiesChart(series)
                }
                if (series.isNotEmpty()) {
                    TextButton(onClick = { asList = !asList }) { Text(if (asList) "Voir le graphique" else "Voir en liste", color = AltimColors.cyan) }
                }
                Strategies.chosen(report, selected).forEach { StrategyBlock(it) }
                Text("Comment ce test évite de se flatter", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                report.notes.forEach { n ->
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text("•", color = AltimColors.textSecondary, fontSize = 13.sp)
                        Text(n, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f))
                    }
                }
            }
        }
    }
}

/** Short line in the strategy's colour; dashed for the buy-and-hold reference. */
@Composable
private fun Swatch(id: String) {
    val c = strategyColor(id)
    Canvas(Modifier.size(width = 14.dp, height = 8.dp).clearAndSetSemantics { }) {
        val y = size.height / 2
        drawLine(
            c, Offset(0f, y), Offset(size.width, y), strokeWidth = 3.dp.toPx(), cap = StrokeCap.Round,
            pathEffect = if (id == Strategies.REFERENCE) PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 2.5.dp.toPx())) else null,
        )
    }
}

/** One axis (value of 100 invested), direct labels up to 4 coloured curves, a readout of the touched date. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StrategiesChart(series: List<StrategyResult>) {
    var hover by remember(series.map { it.id }) { mutableStateOf<Int?>(null) }
    val coloured = series.count { it.id != Strategies.REFERENCE }
    val labeled = coloured <= Strategies.MAX_LABELED
    val first = series[0]
    val n = series.maxOf { it.equity.size }
    val at = (hover ?: (n - 1)).coerceIn(0, first.equity.size - 1)
    fun valueAt(s: StrategyResult) = s.value(minOf(at, s.equity.size - 1))
    val summary = series.joinToString(", ") { "${it.name} ${Strategies.value100(it.value(it.equity.size - 1))}" }
    val labelColor = AltimColors.textSecondary.toArgb()
    val axisColor = Color.White.copy(alpha = 0.25f)

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // Readout: the date and each curve's value there (the last date until the chart is touched).
        FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(2.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(Strategies.shortDate(first.time(at))) }
                    append(" · valeur de 100 investis${Strategies.NNBSP}:")
                },
                fontSize = 12.sp, color = AltimColors.textSecondary,
            )
            series.forEach { s ->
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Swatch(s.id)
                    Text(
                        buildAnnotatedString {
                            append("${Strategies.short(s.id)} ")
                            withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(Strategies.value100(valueAt(s))) }
                        },
                        fontSize = 12.sp, color = AltimColors.textSecondary,
                    )
                }
            }
        }
        Canvas(
            Modifier.fillMaxWidth().height(200.dp)
                .semantics { contentDescription = "Valeur de 100 investis à la fin de la période : $summary" }
                .pointerInput(series.map { it.id }) {
                    fun pick(x: Float) {
                        val box = Strategies.Box(size.width.toDouble(), size.height.toDouble(), 34.dp.toPx().toDouble(), (if (labeled) 62.dp else 6.dp).toPx().toDouble(), 10.dp.toPx().toDouble(), 18.dp.toPx().toDouble())
                        Strategies.geometry(series, box)?.let { g -> hover = Strategies.indexAt(x.toDouble(), g, box) }
                    }
                    detectTapGestures { pick(it.x) }
                }
                .pointerInput(series.map { it.id }) {
                    detectHorizontalDragGestures { change, _ ->
                        val box = Strategies.Box(size.width.toDouble(), size.height.toDouble(), 34.dp.toPx().toDouble(), (if (labeled) 62.dp else 6.dp).toPx().toDouble(), 10.dp.toPx().toDouble(), 18.dp.toPx().toDouble())
                        Strategies.geometry(series, box)?.let { g -> hover = Strategies.indexAt(change.position.x.toDouble(), g, box) }
                    }
                },
        ) {
            val box = Strategies.Box(size.width.toDouble(), size.height.toDouble(), 34.dp.toPx().toDouble(), (if (labeled) 62.dp else 6.dp).toPx().toDouble(), 10.dp.toPx().toDouble(), 18.dp.toPx().toDouble())
            val g = Strategies.geometry(series, box) ?: return@Canvas
            val paint = android.graphics.Paint().apply {
                color = labelColor
                textSize = 10.sp.toPx()
                isAntiAlias = true
            }
            val right = (box.w - box.r).toFloat()
            val l = box.l.toFloat()
            // Reference line at 100 and the axis texts.
            val y100 = g.y(100.0).toFloat()
            drawLine(axisColor, Offset(l, y100), Offset(right, y100), strokeWidth = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 3.dp.toPx())))
            paint.textAlign = android.graphics.Paint.Align.RIGHT
            drawContext.canvas.nativeCanvas.drawText("100", l - 4.dp.toPx(), y100 + 3.dp.toPx(), paint)
            drawContext.canvas.nativeCanvas.drawText(Strategies.value100(g.hi), l - 4.dp.toPx(), (box.t + 6.dp.toPx()).toFloat(), paint)
            drawContext.canvas.nativeCanvas.drawText(Strategies.value100(g.lo), l - 4.dp.toPx(), (box.h - box.b).toFloat(), paint)
            paint.textAlign = android.graphics.Paint.Align.LEFT
            drawContext.canvas.nativeCanvas.drawText(Strategies.shortDate(first.time(0)), l, size.height - 4.dp.toPx(), paint)
            paint.textAlign = android.graphics.Paint.Align.RIGHT
            drawContext.canvas.nativeCanvas.drawText(Strategies.shortDate(first.time(first.equity.size - 1)), right, size.height - 4.dp.toPx(), paint)
            // Curves: the reference dashed and neutral, the others in their colour.
            series.forEach { s ->
                val path = Path()
                s.equity.forEachIndexed { i, p -> if (i == 0) path.moveTo(g.x(i).toFloat(), g.y(p[1]).toFloat()) else path.lineTo(g.x(i).toFloat(), g.y(p[1]).toFloat()) }
                val ref = s.id == Strategies.REFERENCE
                drawPath(
                    path, strategyColor(s.id),
                    style = Stroke(
                        width = 2.dp.toPx(), join = StrokeJoin.Round, cap = StrokeCap.Round,
                        pathEffect = if (ref) PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 4.dp.toPx())) else null,
                    ),
                )
            }
            // Direct labels at the curves' ends, pushed apart so they never overlap.
            if (labeled) {
                val ends = series.map { g.y(it.value(it.equity.size - 1)) }
                val ys = Strategies.placeLabels(ends, 11.dp.toPx().toDouble(), box.t + 4.dp.toPx(), box.h - box.b)
                paint.textAlign = android.graphics.Paint.Align.LEFT
                paint.textSize = 9.sp.toPx()
                series.forEachIndexed { k, s ->
                    val y = ys[k].toFloat()
                    drawLine(
                        strategyColor(s.id), Offset(right + 3.dp.toPx(), y), Offset(right + 9.dp.toPx(), y), strokeWidth = 2.dp.toPx(), cap = StrokeCap.Round,
                        pathEffect = if (s.id == Strategies.REFERENCE) PathEffect.dashPathEffect(floatArrayOf(2.dp.toPx(), 2.dp.toPx())) else null,
                    )
                    drawContext.canvas.nativeCanvas.drawText(Strategies.short(s.id), right + 12.dp.toPx(), y + 3.dp.toPx(), paint)
                }
            }
            // The touched date: a vertical line and a dot on each curve.
            hover?.let { h ->
                val x = g.x(h).toFloat()
                drawLine(Color.White.copy(alpha = 0.4f), Offset(x, box.t.toFloat()), Offset(x, (box.h - box.b).toFloat()), strokeWidth = 1.dp.toPx())
                series.forEach { s -> drawCircle(strategyColor(s.id), 4.dp.toPx(), Offset(x, g.y(s.value(minOf(h, s.equity.size - 1))).toFloat())) }
            }
        }
        // Legend: every curve with its total return, stacked.
        series.forEach { s ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.semantics(mergeDescendants = true) {}) {
                Swatch(s.id)
                Text(
                    buildAnnotatedString {
                        append("${s.name} ")
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(Strategies.signedPct(s.metrics?.totalReturn)) }
                    },
                    fontSize = 12.sp, color = Color.White, modifier = Modifier.weight(1f),
                )
            }
        }
        if (!labeled) Caption("Plus de ${Strategies.MAX_LABELED} courbes : touchez le graphique pour lire les valeurs, ou passez en liste.")
    }
}

/** Stacked rows: the value of 100 invested at five dates of the period, per strategy. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StrategiesList(series: List<StrategyResult>) {
    val shape = RoundedCornerShape(12.dp)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        series.forEach { s ->
            Column(
                Modifier.fillMaxWidth().border(1.dp, Color.White.copy(alpha = 0.08f), shape).padding(horizontal = 10.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Swatch(s.id)
                    Text(
                        buildAnnotatedString {
                            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(s.name) }
                            append(" · ${Strategies.signedPct(s.metrics?.totalReturn)}")
                        },
                        fontSize = 13.sp, color = Color.White, modifier = Modifier.weight(1f),
                    )
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Strategies.checkpoints(s).forEach { c ->
                        Column(Modifier.semantics(mergeDescendants = true) {}) {
                            Caption(Strategies.shortDate(c.t))
                            Text(Strategies.value100(c.v), style = mono(13.sp, FontWeight.Bold), color = Color.White)
                        }
                    }
                }
            }
        }
    }
}

/** One stacked card of metrics per strategy (web StrategyBlock). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StrategyBlock(s: StrategyResult) {
    val m = s.metrics
    val dca = s.id == "dca"
    val low = Strategies.lowSampleText(s)
    fun tone(v: Double?) = v?.let { if (it >= 0) Tone.GOOD else Tone.BAD }
    val shape = RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth().border(1.dp, Color.White.copy(alpha = 0.1f), shape).padding(12.dp).semantics { contentDescription = s.name },
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Swatch(s.id)
            Text(s.name, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = Color.White)
            low?.let { Badge(it, Tone.NEUTRAL) }
        }
        Text(s.rule, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        Caption("Paramètres fixes : ${s.params}")
        if (!s.available || m == null) {
            Notice(s.unavailable ?: "Non couvert.", Tone.NEUTRAL)
        } else {
            KeyValue(if (dca) "Gain sur les sommes versées" else "Rendement total", Strategies.signedPct(m.totalReturn), tone(m.totalReturn))
            KeyValue(if (dca) "Rendement annuel (TRI)" else "Rendement annuel", if (m.cagr == null) "— (< 1 an)" else Strategies.signedPct(m.cagr), tone(m.cagr))
            KeyValue("Pire recul", Strategies.signedPct(m.maxDrawdown), Tone.BAD)
            KeyValue("Sharpe / Sortino", if (dca) "non pertinent" else "${Strategies.ratio(m.sharpe)} / ${Strategies.ratio(m.sortino)}")
            KeyValue(if (dca) "Achats" else "Trades", "${m.trades}")
            KeyValue("Temps investi", Strategies.plainPct(m.exposure))
            if (!dca) {
                KeyValue("Taux de réussite", Strategies.plainPct(m.winRate))
                KeyValue("Profit factor", Strategies.ratio(m.profitFactor))
                KeyValue("Espérance par trade", Strategies.signedPct(m.expectancy, 2), tone(m.expectancy))
            }
            m.avgR?.let { KeyValue("Multiple de R moyen", "${Strategies.ratio(it)} R") }
            if (s.regimes.isNotEmpty()) {
                s.regimes.forEach { g ->
                    FlowRow(
                        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
                        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp), itemVerticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(g.label.substringBefore(" ("), fontSize = 12.sp, color = AltimColors.textSecondary)
                        Text(
                            "${g.trades} ${if (dca) "achat" else "trade"}${if (g.trades > 1) "s" else ""}" + if (g.trades > 0) " · moy. ${Strategies.signedPct(g.avgReturn)}" else "",
                            fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Color.White,
                        )
                        if (g.trades > 0 && g.lowSample) Badge("échantillon trop faible", Tone.NEUTRAL)
                    }
                }
            }
        }
        s.note?.let { Caption(it) }
    }
}
