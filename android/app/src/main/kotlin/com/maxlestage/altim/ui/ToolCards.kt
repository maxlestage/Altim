package com.maxlestage.altim.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.edit
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.FibZone
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.Portfolio
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Tools
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

// Categorical palette (fixed order, validated on the dark surface). The 4th is close to the 1st for deuteranopes
// (ΔE 6,4, allowed with a second cue): its line is dashed.
private val SERIES = listOf(Color(0xFF3987E5), Color(0xFFD95926), Color(0xFF199E70), Color(0xFFC24EC9))
private fun sgn(v: Double) = (if (v >= 0) "+" else "−") + String.format(Locale.FRANCE, "%.1f", abs(v)).removeSuffix(",0") + " %"
private fun usd0(v: Double) = String.format(Locale.FRANCE, "%,.0f", v).replace(' ', ' ').replace(' ', ' ') + " $"

@Composable
private fun NumberField(label: String, value: String, suffix: String, modifier: Modifier = Modifier, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label, fontSize = 12.sp) },
        suffix = { Text(suffix) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = modifier,
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
    )
}

// ---------- Comparison ----------

/** 2 to 4 assets of the radar over the same days: change, volatility, worst fall, correlation. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CompareCard(model: AppModel) {
    val assets = model.watchlist
    var picked by rememberSaveable { mutableStateOf(assets.take(2).map { it.id }) }
    var days by rememberSaveable { mutableIntStateOf(90) }
    var series by remember { mutableStateOf<Map<String, List<Pair<Long, Double>>>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(picked.sorted().joinToString(), days) {
        val client = model.client ?: return@LaunchedEffect
        series = null
        if (picked.size < 2) return@LaunchedEffect
        try {
            series = client.history(assets.filter { it.id in picked }, days).byId
            error = null
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: Exception) {
            error = e.message ?: "Historique indisponible"
        }
    }
    val c = series?.let { Tools.compare(it, picked, days) }

    Card(title = "Comparer") {
        Caption("Choisissez 2 à 4 actifs de votre radar.")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            assets.forEach { a ->
                val i = picked.indexOf(a.id)
                Text(
                    a.symbol,
                    fontSize = 13.sp,
                    color = if (i >= 0) Color.White else AltimColors.textSecondary,
                    modifier = Modifier.clip(RoundedCornerShape(50))
                        .border(if (i >= 0) 2.dp else 1.dp, if (i >= 0) SERIES[i] else Color.White.copy(alpha = 0.15f), RoundedCornerShape(50))
                        .semantics { selected = i >= 0 }
                        .clickable(role = Role.Checkbox) { picked = if (i >= 0) picked - a.id else if (picked.size >= 4) picked else picked + a.id }
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                )
            }
        }
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            listOf(30 to "30 j", 90 to "90 j", 365 to "1 an").forEachIndexed { i, (v, label) ->
                SegmentedButton(
                    selected = days == v,
                    onClick = { days = v },
                    shape = SegmentedButtonDefaults.itemShape(i, 3),
                    colors = SegmentedButtonDefaults.colors(activeContainerColor = AltimColors.cyan.copy(alpha = 0.2f), activeContentColor = AltimColors.cyan),
                ) { Text(label) }
            }
        }
        when {
            picked.size < 2 -> Caption("Sélectionnez au moins 2 actifs.")
            error != null -> Notice(error!!, Tone.BAD)
            series == null -> Loading("Chargement de l'historique…")
            c == null -> Caption("Pas assez d'historique commun.")
            else -> {
                val lo = (c.stats.flatMap { it.pct } + 0.0).min()
                val hi = (c.stats.flatMap { it.pct } + 0.0).max()
                Canvas(Modifier.fillMaxWidth().height(130.dp).semantics { contentDescription = c.stats.joinToString { "${it.id.substringAfter(":")} ${sgn(it.change)}" } }) {
                    val span = (hi - lo).takeIf { it > 0 } ?: 1.0
                    fun x(i: Int, n: Int) = size.width * i / maxOf(1, n - 1)
                    fun y(v: Double) = size.height * (1 - ((v - lo) / span).toFloat())
                    drawLine(Color.White.copy(alpha = 0.18f), Offset(0f, y(0.0)), Offset(size.width, y(0.0)), strokeWidth = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)))
                    c.stats.forEach { s ->
                        val p = Path()
                        s.pct.forEachIndexed { i, v -> if (i == 0) p.moveTo(x(i, s.pct.size), y(v)) else p.lineTo(x(i, s.pct.size), y(v)) }
                        val slot = picked.indexOf(s.id).coerceIn(0, 3)
                        drawPath(p, SERIES[slot], style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round, pathEffect = if (slot == 3) PathEffect.dashPathEffect(floatArrayOf(14f, 10f)) else null))
                    }
                }
                Row { Caption("${sgn(lo)} … ${sgn(hi)}") }
                Row {
                    Text("Actif", fontSize = 12.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1.1f))
                    Text("Variation", fontSize = 12.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
                    Text("Volatilité", fontSize = 12.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
                    Text("Pire recul", fontSize = 12.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
                }
                c.stats.forEach { s ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Row(Modifier.weight(1.1f), verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.size(8.dp).clip(CircleShape).background(SERIES[picked.indexOf(s.id).coerceIn(0, 3)]))
                            Text(" " + s.id.substringAfter(":"), fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        }
                        Text(sgn(s.change), style = mono(13.sp), color = AltimColors.forChange(s.change), modifier = Modifier.weight(1f))
                        Text("${s.volatility.roundToInt()} %/an", style = mono(13.sp), modifier = Modifier.weight(1f))
                        Text(sgn(s.maxDrawdown), style = mono(13.sp), color = AltimColors.sell, modifier = Modifier.weight(1f))
                    }
                }
                val pairs = c.stats.indices.flatMap { a -> (a + 1 until c.stats.size).map { b -> a to b } }
                Caption(
                    "Corrélation : " + pairs.joinToString(" · ") { (a, b) ->
                        "${c.stats[a].id.substringAfter(":")}/${c.stats[b].id.substringAfter(":")} " +
                            (c.correlation[a][b]?.let { String.format(Locale.FRANCE, "%.2f", it) } ?: "—")
                    } + ". Proche de 1 : ils montent et baissent ensemble (peu de diversification) ; proche de 0 : indépendants.",
                )
                Caption("Mêmes jours pour tous. Volatilité = écart type annualisé des variations journalières. Le passé ne dit pas ce qui arrivera.")
            }
        }
    }
}

// ---------- Position size ----------

/** How much to buy so that hitting the stop costs the chosen share of the capital. */
@Composable
fun PositionCard(model: AppModel, asset: Asset, price: Double?, zones: List<FibZone>) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim", android.content.Context.MODE_PRIVATE) }
    val holdingsValue = Portfolio(model.holdings, model.holdings.mapNotNull { h -> model.live.price(h.asset)?.let { h.asset.id to it.price } }.toMap()).total
    // Proposed stop: the low that invalidates the nearest buy zone below the price, else 5 % under the price.
    val zone = zones.firstOrNull { (it.invalidation ?: 0.0) > 0 && price != null && it.invalidation!! < price }
    val proposedStop = zone?.invalidation ?: price?.let { it * 0.95 }
    val proposedTarget = zone?.targets?.firstOrNull { price != null && it > price }
    var capitalText by rememberSaveable(asset.id) { mutableStateOf(prefs.getString("position.capital", null) ?: if (holdingsValue > 0) holdingsValue.roundToInt().toString() else "") }
    var riskText by rememberSaveable(asset.id) { mutableStateOf(prefs.getString("position.risk", null) ?: "1") }
    var stopText by rememberSaveable(asset.id) { mutableStateOf("") }
    var targetText by rememberSaveable(asset.id) { mutableStateOf("") }
    LaunchedEffect(proposedStop, proposedTarget) {
        if (stopText.isEmpty()) proposedStop?.let { stopText = Format.plain(it, if (it >= 1) 2 else 6) }
        if (targetText.isEmpty()) proposedTarget?.let { targetText = Format.plain(it, if (it >= 1) 2 else 6) }
    }
    val p = price?.let {
        Tools.positionSize(Format.parse(capitalText) ?: 0.0, Format.parse(riskText) ?: 0.0, it, Format.parse(stopText) ?: 0.0, Format.parse(targetText))
    }

    Card(title = "Taille de position") {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumberField("Capital", capitalText, "$", Modifier.weight(1f)) { capitalText = it; prefs.edit { putString("position.capital", it) } }
            NumberField("Risque accepté", riskText, "%", Modifier.weight(1f)) { riskText = it; prefs.edit { putString("position.risk", it) } }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumberField("Stop", stopText, "$", Modifier.weight(1f)) { stopText = it }
            NumberField("Objectif", targetText, "$", Modifier.weight(1f)) { targetText = it }
        }
        Caption("Entrée au prix actuel ${price?.let { Format.price(it) } ?: "…"} ; stop proposé : ${if (zone != null) "plus bas qui invalide la zone d'achat" else "5 % sous le prix (à ajuster)"}.")
        when {
            price == null -> Loading()
            p == null -> Caption("Le stop doit être sous le prix d'entrée, et le capital et le risque positifs.")
            else -> {
                KeyValue("Acheter", "${Format.plain(p.quantity, if (p.quantity >= 1) 2 else 6)} ${asset.symbol} · ${usd0(p.amount)}")
                KeyValue("Part du capital", "${Format.plain(p.capitalShare, 1)} %")
                KeyValue("Perte si le stop est touché (${sgn(-p.stopDistance)})", "−${usd0(p.risk)}", Tone.BAD)
                p.reward?.let { KeyValue("Gain à l'objectif · gain/risque", "+${usd0(it)} · ${Format.plain(p.ratio!!, 1)} R", Tone.GOOD) }
                if (p.capped) Notice("Stop très proche : la taille est limitée à votre capital, la perte au stop reste sous le risque choisi.", Tone.WARN)
                if ((p.ratio ?: 99.0) < 1.5) Notice("Rapport gain/risque sous 1,5 : l'idée rapporte peu au regard du risque.", Tone.WARN)
                Caption("Calcul, pas conseil : un écart de prix (gap) peut faire perdre plus que prévu au stop. Altim ne passe aucun ordre.")
            }
        }
    }
}

// ---------- Notes ----------

/** Personal notes on an asset (why I bought, my plan, when I sell): kept on the phone only (backups disabled). */
@Composable
fun NoteCard(asset: Asset) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim", android.content.Context.MODE_PRIVATE) }
    var text by rememberSaveable(asset.id) { mutableStateOf(prefs.getString("note.${asset.id}", "") ?: "") }
    var updated by remember(asset.id) { mutableStateOf(prefs.getLong("note.${asset.id}.updated", 0L).takeIf { it > 0 }) }
    Card(title = "Mes notes · ${asset.symbol}") {
        OutlinedTextField(
            value = text,
            onValueChange = { v ->
                text = v.take(2000)
                val clean = text.trim()
                val now = System.currentTimeMillis()
                prefs.edit {
                    if (clean.isEmpty()) remove("note.${asset.id}").remove("note.${asset.id}.updated")
                    else putString("note.${asset.id}", clean).putLong("note.${asset.id}.updated", now)
                }
                updated = if (clean.isEmpty()) null else now
            },
            placeholder = { Text("Pourquoi j'achète, mon plan, quand je vends…", fontSize = 13.sp) },
            minLines = 3,
            maxLines = 8,
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Mes notes sur ${asset.symbol}" },
            colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
        )
        Caption(
            (updated?.let { "Enregistré le ${Format.date(it.toDouble(), time = true)}. " } ?: "") +
                "Gardé sur ce téléphone seulement. Relire sa thèse avant d'acheter ou de vendre évite les décisions sur un coup de tête.",
        )
    }
}

// ---------- Sale after fees and tax ----------

/** What selling would leave once the fees and the flat tax on the gains are paid. */
@Composable
fun SaleCard(portfolio: Portfolio) {
    var taxText by rememberSaveable { mutableStateOf("30") }
    var feeText by rememberSaveable { mutableStateOf("0,1") }
    val lines = portfolio.lines.mapNotNull { l -> l.value?.let { Triple(l.holding.asset.symbol, it, l.cost) } }
    if (lines.isEmpty()) return
    val t = Tools.saleTotal(lines, Format.parse(taxText) ?: 0.0, Format.parse(feeText) ?: 0.0)
    Card(title = "Si je vendais") {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            NumberField("Impôt sur la plus-value", taxText, "%", Modifier.weight(1f)) { taxText = it }
            NumberField("Frais de vente", feeText, "%", Modifier.weight(1f)) { feeText = it }
        }
        t.lines.forEach { s ->
            KeyValue(
                "${s.id} · ${s.gain?.let { (if (it >= 0) "+" else "−") + usd0(abs(it)) } ?: "prix d'achat inconnu"}" + (if (s.tax > 0) " · impôt −${usd0(s.tax)}" else ""),
                usd0(s.net),
            )
        }
        KeyValue("Tout vendre : vous garderiez", usd0(t.net), Tone.GOOD)
        Caption("Frais ${usd0(t.fees)} · impôt ${usd0(t.tax)}" + (t.gain?.let { " · plus-value nette ${if (it >= 0) "+" else "−"}${usd0(abs(it))}" } ?: ""))
        Caption(
            "30 % = prélèvement forfaitaire unique en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux) ; les pertes de l'année compensent les gains. " +
                "Cryptos : l'impôt se calcule sur l'ensemble du portefeuille à chaque cession et les cessions de moins de 305 € par an sont exonérées, donc ce calcul ligne par ligne est une estimation." +
                (if (t.unknownCost > 0) " ${t.unknownCost} ligne(s) sans prix d'achat : plus-value non calculée." else "") +
                " Vérifiez votre situation (PEA, assurance-vie, option barème…) ; Altim ne passe aucun ordre.",
        )
    }
}

// ---------- Projection ----------

/** What the portfolio plus a monthly contribution would become, under three yearly returns (hypotheses). */
@Composable
fun ProjectionCard(start: Double) {
    var monthlyText by rememberSaveable { mutableStateOf("0") }
    var years by rememberSaveable { mutableIntStateOf(10) }
    val monthly = Format.parse(monthlyText) ?: 0.0
    val runs = Tools.PROJECTION_RATES.map { it to Tools.projection(start, monthly, years, it) }
    val paid = runs.first().second.last().paid
    Card(title = "Projection") {
        NumberField("Versement chaque mois, facultatif", monthlyText, "$", Modifier.fillMaxWidth()) { monthlyText = it }
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            listOf(5, 10, 20).forEachIndexed { i, v ->
                SegmentedButton(
                    selected = years == v,
                    onClick = { years = v },
                    shape = SegmentedButtonDefaults.itemShape(i, 3),
                    colors = SegmentedButtonDefaults.colors(activeContainerColor = AltimColors.cyan.copy(alpha = 0.2f), activeContentColor = AltimColors.cyan),
                ) { Text("$v ans") }
            }
        }
        KeyValue(if (monthly > 0) "Aujourd'hui ${usd0(start)} + versements" else "Vos avoirs aujourd'hui, sans rien ajouter", usd0(paid) + if (monthly > 0) " versés" else "")
        runs.forEach { (rate, points) ->
            val end = points.last().value
            KeyValue("Si ${rate.roundToInt()} % par an", "${usd0(end)} (${if (end - paid >= 0) "+" else "−"}${usd0(abs(end - paid))})", if (end - paid > 0) Tone.GOOD else null)
        }
        Caption("Trois hypothèses de rendement à comparer, pas des prévisions : une année peut perdre 30 % ou plus (cryptos : davantage), et l'inflation réduit ce que ces montants achèteront. Sans frais ni impôts.")
    }
}

// ---------- Rebalancing ----------

/** Buys and sells to reach a target split between cryptos and stocks. */
@Composable
fun RebalanceCard(portfolio: Portfolio) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim", android.content.Context.MODE_PRIVATE) }
    var target by remember { mutableFloatStateOf(prefs.getFloat("rebalance.crypto", 40f)) }
    val step = (target / 5).roundToInt() * 5.0
    val r = Tools.rebalance(portfolio.lines.mapNotNull { l -> l.value?.let { Triple(l.holding.asset.id, l.holding.asset.kind, it) } }, step)
    val small = { v: Double -> abs(v) < maxOf(10.0, (r?.total ?: 0.0) * 0.01) }

    Card(title = "Rééquilibrer") {
        Text("Cible : ${step.roundToInt()} % cryptos · ${100 - step.roundToInt()} % actions", fontSize = 14.sp)
        Slider(
            value = target,
            onValueChange = { target = it },
            onValueChangeFinished = { prefs.edit { putFloat("rebalance.crypto", target) } },
            valueRange = 0f..100f,
            steps = 19,
            colors = SliderDefaults.colors(thumbColor = AltimColors.cyan, activeTrackColor = AltimColors.cyan),
            modifier = Modifier.semantics { contentDescription = "Part des cryptos visée : ${step.roundToInt()} %" },
        )
        if (r == null) {
            Caption("Ajoutez des avoirs avec un prix pour calculer.")
        } else {
            listOf(Kind.CRYPTO to "Cryptos", Kind.STOCK to "Actions").forEach { (k, label) ->
                val m = r.moves.getValue(k)
                KeyValue(
                    "$label : ${r.current.getValue(k).roundToInt()} % → ${if (k == Kind.CRYPTO) step.roundToInt() else 100 - step.roundToInt()} %",
                    if (small(m)) "rien à faire" else "${if (m > 0) "acheter" else "vendre"} ${usd0(abs(m))}",
                    if (small(m)) null else if (m > 0) Tone.GOOD else Tone.BAD,
                )
            }
            r.lines.filterNot { small(it.second) }.forEach { (id, amount) ->
                Caption("${if (amount > 0) "Acheter" else "Vendre"} ${usd0(abs(amount))} de ${id.substringAfter(":")}")
            }
            Caption(
                "Réparti au prorata de vos lignes actuelles" +
                    (if (r.moves.getValue(Kind.STOCK) > 0 && portfolio.lines.none { it.holding.asset.kind == Kind.STOCK }) " (aucune action détenue : à répartir vous-même)" else "") +
                    (if (r.moves.getValue(Kind.CRYPTO) > 0 && portfolio.lines.none { it.holding.asset.kind == Kind.CRYPTO }) " (aucune crypto détenue : à répartir vous-même)" else "") +
                    ". Avant de vendre, pensez aux frais et à l'impôt sur les plus-values. Altim ne passe aucun ordre.",
            )
        }
    }
}
