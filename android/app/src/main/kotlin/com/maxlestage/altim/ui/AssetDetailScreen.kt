package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Money
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.NotificationAdd
import androidx.compose.material.icons.filled.Sensors
import androidx.compose.material.icons.filled.StopCircle
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.FibZone
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.GuardFactor
import com.maxlestage.altim.kit.GuardReport
import com.maxlestage.altim.kit.MacroInfo
import com.maxlestage.altim.kit.RadarDecisions
import com.maxlestage.altim.kit.RadarRow
import com.maxlestage.altim.kit.Snapshot
import com.maxlestage.altim.kit.Timeframe
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.ZonesReport
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope

/** Everything about one asset: live price, chart, signal, Fibonacci buy zones by horizon, market guard, macro. */
@Composable
fun AssetDetailScreen(model: AppModel, asset: Asset, modifier: Modifier, onBack: () -> Unit) {
    // Shared with the Radar's chooser (saved; the Radar has no 1 h).
    val timeframe = model.timeframe
    val interval = timeframe.raw
    var snapshot by remember(asset.id) { mutableStateOf<Loadable<Snapshot>>(Loadable.Loading) }
    var signal by remember(asset.id) { mutableStateOf<RadarRow?>(null) }
    var zones by remember(asset.id) { mutableStateOf<Loadable<ZonesReport>>(Loadable.Loading) }
    var guard by remember(asset.id) { mutableStateOf<Loadable<GuardReport>>(Loadable.Loading) }
    var reload by remember { mutableStateOf(0) }
    var targetOpen by remember(asset.id) { mutableStateOf(false) }
    var decision by remember(asset.id) { mutableStateOf<Loadable<Decision>>(Loadable.Loading) }
    var decisionReload by remember { mutableStateOf(0) }
    var simulateOpen by remember(asset.id) { mutableStateOf(false) }
    var simulated by remember(asset.id) { mutableStateOf<String?>(null) }
    val held = model.holdings.any { it.asset.id == asset.id }

    // Personal mode when held: the average cost and the portfolio weights (percentages only) go with the request,
    // for this answer only; otherwise the informational decision.
    LaunchedEffect(asset.id, reload, decisionReload, held, model.holdings, model.scoreWeights) {
        val client = model.client ?: return@LaunchedEffect
        // A reload keeps the decision shown until the new one arrives (iOS: an error only when there is none).
        if (decision.value == null) decision = Loadable.Loading
        decision = try {
            var cost: Double? = null
            var weights: String? = null
            if (held) {
                // In dollars like the prices (a euro cost converted at the current rate; unknown without a rate).
                cost = Decision.cost(model.usdHoldings.holdings, asset)
                val assets = model.holdings.map { it.asset }.distinctBy { it.id }
                val prices = runCatching { client.quotes(assets).associate { "${it.kind.raw}:${it.symbol}" to it.price } }.getOrDefault(emptyMap()) +
                    assets.mapNotNull { a -> model.live.price(a)?.let { a.id to it.price } }
                weights = Decision.weights(model.holdings, prices)
            }
            Loadable.Loaded(client.decision(asset, cost, weights, model.scoreWeights).also { model.recordDecision(it, personal = held) })
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
            return@LaunchedEffect
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            decision.value?.let { Loadable.Loaded(it) } ?: Loadable.Failed(e.message ?: "Décision indisponible")
        }
    }

    LaunchedEffect(asset.id, reload) {
        val client = model.client ?: return@LaunchedEffect
        coroutineScope {
            val z = async { runCatching { client.zones(asset) } }
            val g = async { runCatching { client.guardReport(asset) } }
            zones = z.await().fold({ Loadable.Loaded(it) }, { Loadable.Failed(it.message ?: "Erreur") })
            guard = g.await().fold({ Loadable.Loaded(it) }, { Loadable.Failed(it.message ?: "Erreur") })
        }
        model.persistSession()
    }
    LaunchedEffect(asset.id, interval, reload) {
        val client = model.client ?: return@LaunchedEffect
        snapshot = Loadable.Loading
        coroutineScope {
            val r = async { runCatching { client.radar(listOf(asset), interval) }.getOrNull() }
            snapshot = try {
                Loadable.Loaded(client.candles(asset, interval))
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@coroutineScope
            } catch (e: Exception) {
                Loadable.Failed(e.message ?: "Erreur")
            }
            signal = r.await()?.firstOrNull()
        }
    }

    val context = androidx.compose.ui.platform.LocalContext.current
    val livePrice = model.live.price(asset)
    val tracking = model.activityAsset?.id == asset.id
    val toggleTracking = {
        if (tracking) model.stopLiveTrack()
        else (livePrice?.price ?: signal?.price ?: zones.value?.price)?.let { model.startLiveTrack(asset, it, livePrice?.change ?: signal?.change) }
        Unit
    }
    val trackPermission = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) toggleTracking()
    }
    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Retour") }
            Text(asset.symbol, fontSize = 18.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            IconButton(onClick = { targetOpen = true }) {
                Icon(Icons.Filled.NotificationAdd, contentDescription = "Alerte de prix", tint = AltimColors.cyan)
            }
            // Live following (iOS Live Activity): an ongoing notification with the price and the buy verdict.
            if (model.liveActivityEnabled) {
                IconButton(onClick = {
                    if (!tracking && android.os.Build.VERSION.SDK_INT >= 33 && !com.maxlestage.altim.data.BuyAlerts.canNotify(context)) {
                        trackPermission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                    } else toggleTracking()
                }) {
                    Icon(
                        if (tracking) Icons.Filled.StopCircle else Icons.Filled.Sensors,
                        contentDescription = if (tracking) "Arrêter le suivi en direct" else "Suivre sur l'écran verrouillé et dans les notifications",
                        tint = AltimColors.cyan,
                    )
                }
            }
            val watched = model.isWatched(asset)
            IconButton(onClick = { if (watched) model.unwatch(asset) else model.watch(asset) }) {
                Icon(if (watched) Icons.Filled.Star else Icons.Filled.StarBorder, contentDescription = if (watched) "Retirer du radar" else "Ajouter au radar", tint = AltimColors.cyan)
            }
        }
        androidx.compose.material3.pulltorefresh.PullToRefreshBox(isRefreshing = false, onRefresh = { reload++ }) {
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Header(model, asset, signal, zones.value)
            DecisionCard(decision, held, change = decision.value?.let { ConfigChanges.latestChange(model.configChanges.transitions, it) }, onSimulate = { simulateOpen = true }) { decisionReload++ }
            simulated?.let { Notice(it, Tone.GOOD) }
            Card {
                ChoiceRow(Timeframe.entries.map { it to it.label }, timeframe, { model.updateTimeframe(it) }, description = "Unité de temps")
                val chartZone = zones.value?.zones?.firstOrNull { it.horizon == timeframe.zoneHorizon && it.zone != null }
                when (val s = snapshot) {
                    is Loadable.Loading -> Loading()
                    is Loadable.Failed -> ErrorBox(s.message) { reload++ }
                    is Loadable.Loaded -> {
                        PriceChart(s.value.candles.takeLast(timeframe.chartCandles), chartZone, Modifier.fillMaxWidth().height(220.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Caption("${s.value.agreeing} sources en accord", modifier = Modifier.weight(1f))
                            Badge(s.value.reliability.label.uppercase(), s.value.reliability.tone)
                        }
                        chartZone?.zone?.let { z ->
                            Caption("Bande colorée : zone d'achat ${chartZone.label.lowercase()} (${Format.price(z.from)} – ${Format.price(z.to)}).")
                        }
                    }
                }
            }
            signal?.signal?.let { s ->
                // A direction, not a verdict: the only verdict and plan (stop, target) are the Décision card's.
                Card(title = "Signal technique · ${timeframe.label}") {
                    Caption("Un indice parmi d'autres : le verdict à suivre est celui de la carte Décision, qui y ajoute les interdictions d'achat, la zone d'achat, le gain/risque, l'agenda et la preuve du modèle.")
                    val direction = RadarDecisions.technicalTone(s.action)
                    Text(
                        "Tendance technique : ${RadarDecisions.technicalText(s.action)}",
                        color = if (direction == Tone.NEUTRAL) Color.White else AltimColors.of(direction),
                        fontSize = 15.sp, fontWeight = FontWeight.Bold,
                    )
                    Text("score ${Math.round(s.score)} · confiance ${Math.round(s.confidence)} %", style = mono(13.sp), color = AltimColors.textSecondary)
                    Caption("Calculé sur les bougies médianes de toutes les sources ; la confiance tient compte de l'accord entre indicateurs et unités de temps.")
                }
            }
            when (val z = zones) {
                is Loadable.Loading -> Card(title = "Zones d'achat (Fibonacci)") { Loading() }
                is Loadable.Failed -> Card(title = "Zones d'achat (Fibonacci)") { ErrorBox(z.message) { reload++ } }
                is Loadable.Loaded -> Card(title = "Zones d'achat (Fibonacci)", glow = AltimColors.violet) {
                    Caption("Retracement 38,2 % – 65 % du dernier mouvement haussier ; « zone d'or » 61,8 – 65 %.")
                    z.value.zones.forEach { ZoneRow(it) }
                }
            }
            when (val g = guard) {
                is Loadable.Loading -> Card(title = "Garde-fou marché") { Loading() }
                is Loadable.Failed -> Card(title = "Garde-fou marché") { ErrorBox(g.message) { reload++ } }
                is Loadable.Loaded -> GuardCard(g.value)
            }
            AnomaliesCard(model, asset)
            WhyCard(model, asset)
            (guard.value?.macro ?: zones.value?.macro)?.let { MacroCard(it) }
            guard.value?.inputs?.headlines?.takeIf { it.isNotEmpty() }?.let { news ->
                Card(title = "Actualités (24 h)") {
                    news.take(5).forEach { h ->
                        Column {
                            Text(h.title, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
                            Text(Format.date(h.time, time = true), fontSize = 11.sp, color = AltimColors.textSecondary)
                        }
                    }
                }
            }
            NoteCard(asset)
            PositionCard(model, asset, model.live.price(asset)?.price ?: signal?.price ?: zones.value?.price, zones.value?.zones.orEmpty())
            DcaCard(model, asset)
            StrategiesCard(model, asset)
            Caption("Altim ne passe aucun ordre : ces analyses sont des probabilités, à confronter à votre propre jugement.")
        }
        }
    }
    if (targetOpen) {
        PriceTargetSheet(model, asset, model.live.price(asset)?.price ?: signal?.price ?: zones.value?.price) { targetOpen = false }
    }
    val d = decision.value
    if (simulateOpen && d != null) {
        SimulateSheet(
            model, asset, d, model.live.price(asset)?.price ?: signal?.price ?: d.price ?: zones.value?.price,
            onDone = {
                simulated = it
                simulateOpen = false
            },
            onDismiss = { simulateOpen = false },
        )
    }
}

@Composable
private fun Caption(text: String, modifier: Modifier) = Text(text, color = AltimColors.textSecondary, fontSize = 12.sp, modifier = modifier)

@Composable
private fun Header(model: AppModel, asset: Asset, signal: RadarRow?, zones: ZonesReport?) {
    val tick = model.live.price(asset)
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(asset.name, fontSize = 20.sp, fontWeight = FontWeight.Bold, maxLines = 2, modifier = Modifier.weight(1f))
            Badge(asset.kind.label, Tone.NEUTRAL)
        }
        Text(Format.price(tick?.price ?: signal?.price ?: zones?.price), style = mono(32.sp, FontWeight.Bold))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ChangeText(tick?.change ?: signal?.change)
            Caption("24 h")
            androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
            LiveBadge(model.live)
        }
        val a = tick?.agreeing
        val n = tick?.total
        if (a != null && n != null) {
            Caption("Prix médian de $a/$n sources en accord" + if (tick.market == "closed") " · Bourse de New York fermée" else "")
        }
    }
}

/** Closing prices, buy zone (violet) and golden pocket (amber) of the matching horizon, price scale on the right. */
@Composable
fun PriceChart(candles: List<Candle>, zone: FibZone?, modifier: Modifier) {
    val band = zone?.zone
    val golden = zone?.golden
    val lo0 = (candles.map { it.low } + listOfNotNull(band?.from)).minOrNull() ?: 0.0
    val hi0 = (candles.map { it.high } + listOfNotNull(band?.to)).maxOrNull() ?: 1.0
    val pad = (hi0 - lo0) * 0.05
    val lo = lo0 - pad
    val hi = hi0 + pad
    val labelColor = AltimColors.textSecondary.toArgb()
    Canvas(modifier.semantics { contentDescription = "Graphique du prix, de ${Format.price(candles.firstOrNull()?.close)} à ${Format.price(candles.lastOrNull()?.close)}" }) {
        if (candles.size < 2 || hi <= lo) return@Canvas
        val right = size.width - 64.dp.toPx() // room for the price scale
        fun y(v: Double) = (size.height * (1 - (v - lo) / (hi - lo))).toFloat()
        fun x(i: Int) = right * i / (candles.size - 1)
        band?.let { drawRect(AltimColors.violet.copy(alpha = 0.18f), Offset(0f, y(it.to)), Size(right, y(it.from) - y(it.to))) }
        golden?.let { drawRect(AltimColors.warning.copy(alpha = 0.22f), Offset(0f, y(it.to)), Size(right, y(it.from) - y(it.to))) }
        val paint = android.graphics.Paint().apply {
            color = labelColor
            textSize = 10.sp.toPx()
            isAntiAlias = true
        }
        for (k in 0..3) {
            val v = lo + (hi - lo) * (k + 0.5) / 4
            drawLine(Color.White.copy(alpha = 0.06f), Offset(0f, y(v)), Offset(right, y(v)))
            drawContext.canvas.nativeCanvas.drawText(Format.price(v).removeSuffix(" ${Money.symbol()}"), right + 6.dp.toPx(), y(v) + 4.dp.toPx(), paint)
        }
        val path = Path()
        candles.forEachIndexed { i, c -> if (i == 0) path.moveTo(x(i), y(c.close)) else path.lineTo(x(i), y(c.close)) }
        drawPath(path, AltimColors.cyan, style = Stroke(width = 2.dp.toPx(), join = StrokeJoin.Round))
    }
}

@Composable
fun ZoneRow(zone: FibZone) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(zone.label, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                Caption("bougies ${zone.unit} · détention ${zone.holding}")
            }
            Badge(zone.statusLabel.uppercase(), zone.tone)
        }
        zone.zone?.let { KeyValue("Zone d'achat", "${Format.price(it.to)} – ${Format.price(it.from)}") }
        zone.golden?.let { KeyValue("Zone d'or", "${Format.price(it.to)} – ${Format.price(it.from)}", Tone.WARN) }
        zone.invalidation?.let { KeyValue("Invalidée sous", Format.price(it), Tone.BAD) }
        if (zone.targets.isNotEmpty()) KeyValue("Objectifs", zone.targets.take(3).joinToString("\n") { Format.price(it) }, Tone.GOOD)
        Text(zone.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        zone.macroNote?.let { Notice(it, Tone.WARN) }
        Caption(zone.evidenceText)
    }
}

@Composable
fun GuardCard(report: GuardReport) {
    Card(title = "Garde-fou marché", glow = AltimColors.of(report.shockTone)) {
        KeyValue("Tendance de fond", report.trendLabel, when (report.regime.trend) { "up" -> Tone.GOOD; "down" -> Tone.BAD; else -> Tone.NEUTRAL })
        Text(report.regime.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.85f))
        Meter("Risque de choc (${report.shockLabel})", report.shock.score, report.shockTone)
        Factors(report.shock.factors)
        val dir = when (report.reversal.direction) { "up" -> " (à la hausse)"; "down" -> " (à la baisse)"; else -> "" }
        val revTone = if (report.reversal.score >= 50) Tone.BAD else if (report.reversal.score >= 25) Tone.WARN else Tone.GOOD
        Meter("Risque de retournement$dir", report.reversal.score, revTone)
        Factors(report.reversal.factors)
        KeyValue("Trading court terme", report.policyLabel, when (report.policy.scalping) { "ok" -> Tone.GOOD; "pause" -> Tone.BAD; else -> Tone.WARN })
        if (report.policy.sizeMultiplier < 1) KeyValue("Taille conseillée", "× ${Format.plain(report.policy.sizeMultiplier)}", Tone.WARN)
        report.policy.notes.forEach { Text("• $it", fontSize = 13.sp, color = Color.White.copy(alpha = 0.85f)) }
    }
}

@Composable
private fun Factors(list: List<GuardFactor>) {
    if (list.isEmpty()) return Caption("Aucun signal.")
    list.forEach { f ->
        Column {
            Text(f.text, fontSize = 13.sp, color = if (f.status == "rejected") AltimColors.textSecondary else Color.White.copy(alpha = 0.9f))
            Text(f.detail, fontSize = 11.sp, color = AltimColors.textSecondary)
        }
    }
}

@Composable
fun MacroCard(macro: MacroInfo) {
    Card(title = "Contexte macro et géopolitique", glow = AltimColors.of(macro.tone)) {
        Row {
            Text(macro.levelLabel, fontWeight = FontWeight.Bold, color = AltimColors.of(macro.tone), modifier = Modifier.weight(1f))
            Text("${Math.round(macro.score)}/100", style = mono(13.sp))
        }
        macro.regime?.let { RegimeText(it) }
        macro.factors.forEach { Text("• ${it.text}", fontSize = 13.sp) }
        MacroInfo.series.forEach { (key, label) ->
            macro.values[key]?.let { v -> KeyValue(label, "${Format.plain(v.value)} (${Format.percent(v.change5d, 1)} 5 j)") }
        }
        if (macro.themes.isNotEmpty()) {
            Text("Actualité mondiale (titres des dernières 48 h)", fontSize = 12.sp, fontWeight = FontWeight.Bold)
            macro.themes.forEach { Caption("${it.label} : ${it.count} titres") }
        }
        if (macro.factors.isEmpty()) Caption("Aucun signe de stress sur la peur (VIX), le S&P 500, le pétrole, l'or, le dollar ni les taux.")
        macro.evidence?.takeIf { it.samples >= 20 }?.let { e ->
            Caption(
                "Sur cet actif, les jours de stress macro ont été suivis d'une forte baisse dans ${Math.round(e.rate)} % des cas en 5 jours, contre ${Math.round(e.base)} % d'habitude" +
                    if (e.lift >= 1.1) " : à prendre au sérieux." else " : pas d'effet mesurable ici.",
            )
        }
    }
}
