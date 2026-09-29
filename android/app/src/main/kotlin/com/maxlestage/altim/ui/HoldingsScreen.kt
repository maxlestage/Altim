package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.Holdings
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Work
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.BetaEstimate
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.ClusterInput
import com.maxlestage.altim.kit.Danger
import com.maxlestage.altim.kit.LimitCheck
import com.maxlestage.altim.kit.LimitLevel
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.RiskPortfolio
import com.maxlestage.altim.kit.StressResult
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.HoldingChange
import com.maxlestage.altim.kit.TradeJournal
import com.maxlestage.altim.kit.Portfolio
import com.maxlestage.altim.kit.PortfolioLine
import com.maxlestage.altim.kit.ConfigSnapshot
import com.maxlestage.altim.kit.SearchItem
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay

/**
 * Portfolio: value at live prices, gain / loss, concentration, and the full decision last seen for each line (never the
 * technical signal alone). The lines stay on this phone; only the symbols are sent to the server to get prices.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HoldingsScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit) {
    val quotes = remember { mutableStateMapOf<String, Double>() }
    var error by remember { mutableStateOf<String?>(null) }
    var form by remember { mutableStateOf<Holding?>(null) }
    var adding by remember { mutableStateOf(false) }
    var refresh by remember { mutableIntStateOf(0) }
    // "real", "paper" (simulation, no real money) or "journal".
    var view by rememberSaveable { mutableStateOf("real") }
    // Daily candles of the lines and of the benchmarks (Bitcoin, S&P 500 via SPY): betas, limits, dangerous positions.
    val daily = remember { mutableStateMapOf<String, List<Candle>>() }
    var marketLoads by remember { mutableIntStateOf(0) }

    LaunchedEffect(model.holdings.joinToString { it.asset.id }, refresh) {
        val client = model.client ?: return@LaunchedEffect
        val assets = model.holdings.map { it.asset }.distinctBy { it.id }
        if (assets.isEmpty()) return@LaunchedEffect
        try {
            client.quotes(assets).forEach { quotes["${it.kind.raw}:${it.symbol}"] = it.price }
            error = null
            model.persistSession()
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
            return@LaunchedEffect
        } catch (e: Exception) {
            error = e.message
        }
        val benchmarks = assets.map { it.kind }.distinct().map { PortfolioRisk.benchmark(it) }.filter { b -> assets.none { it.id == b.id } }
        coroutineScope {
            (assets + benchmarks).map { a -> async { a.id to runCatching { client.candles(a, "1d").candles }.getOrNull() } }.awaitAll()
        }.forEach { (id, c) -> if (!c.isNullOrEmpty()) daily[id] = c }
        marketLoads++
    }

    // Live price when available, otherwise the consensus quote.
    val prices = quotes.toMutableMap().apply { model.holdings.forEach { h -> model.live.price(h.asset)?.let { put(h.asset.id, it.price) } } }
    // The engines work in dollars: a cost or stop typed in euros is converted at the current rate (Holdings.toUsd).
    val usd = model.usdHoldings
    val lines = usd.holdings
    val portfolio = Portfolio(lines, prices)

    // Risk beyond the summary: betas, stress scenarios, limits of the settings, dangerous positions (once priced).
    val candles = daily.toMap()
    // Betas depend on the candles only, not on the live prices: computed once per load.
    val betas = remember(lines, candles) { PortfolioRisk.betas(lines, candles) }
    val risk = remember(lines, prices, candles, model.risk) {
        if (lines.isEmpty() || quotes.isEmpty()) null
        else {
            val d = candles
            val p = RiskPortfolio.of(lines, 0.0, prices, d)
            val stops = lines.associate { it.id to it.stop }
            val clusters = PortfolioRisk.correlatedClusters(
                p.lines.groupBy { it.key }.map { (k, ls) -> ClusterInput(ls.first().symbol, ls.sumOf { it.weight }, d[k].orEmpty()) },
            )
            RiskView(
                p, betas, PortfolioRisk.stressTest(p, betas),
                PortfolioRisk.checkLimits(p, model.risk, clusters, PortfolioRisk.dailyChange(p, d, System.currentTimeMillis().toDouble()), stops),
                PortfolioRisk.dangerousPositions(p, model.risk, d, stops),
            )
        }
    }
    val dangerById = risk?.dangers?.associateBy { it.id }.orEmpty()
    // Kept for the Radar, once the market data of this visit is loaded (again when the lines or reasons change).
    LaunchedEffect(marketLoads, risk?.dangers?.map { it.id to it.reasons.map { r -> r.code } }) {
        if (marketLoads > 0 && risk != null) model.saveDangers(risk.dangers)
    }

    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(start = 16.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Mes avoirs", fontSize = 30.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            if (view == "real") IconButton(onClick = { adding = true }) { Icon(Icons.Filled.Add, contentDescription = "Ajouter un avoir", tint = AltimColors.cyan) }
        }
        // Real holdings, the simulated portfolio (no real money), or the automatic journal of both.
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
            val views = listOf("real" to "Réel", "paper" to "Simulation", "journal" to "Journal")
            views.forEachIndexed { i, (key, label) ->
                SegmentedButton(
                    selected = view == key,
                    onClick = { view = key },
                    shape = SegmentedButtonDefaults.itemShape(i, views.size),
                    colors = SegmentedButtonDefaults.colors(activeContainerColor = AltimColors.cyan.copy(alpha = 0.2f), activeContentColor = AltimColors.cyan),
                ) { Text(label) }
            }
        }
        if (view == "paper") {
            PaperPane(model, Modifier.fillMaxSize())
        } else if (view == "journal") {
            JournalPane(model, open, Modifier.fillMaxSize())
        } else PullToRefreshBox(isRefreshing = false, onRefresh = { refresh++ }) {
            LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxSize()) {
                if (model.holdings.isEmpty()) {
                    item {
                        Column(Modifier.fillMaxWidth().padding(vertical = 24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
                            Icon(Icons.Filled.Work, contentDescription = null, tint = AltimColors.cyan, modifier = Modifier.size(40.dp))
                            Text(
                                "Ajoutez ce que vous possédez (cryptos, actions) pour suivre sa valeur en direct et le signal de chaque ligne.",
                                color = AltimColors.textSecondary, textAlign = TextAlign.Center, fontSize = 14.sp,
                            )
                            NeonButton("Ajouter un avoir") { adding = true }
                        }
                    }
                    return@LazyColumn
                }
                item { Summary(model, portfolio) }
                error?.let { item { Notice(it, Tone.BAD) } }
                if (usd.unconverted.isNotEmpty()) item {
                    Notice("Taux EUR/USD indisponible : ${usd.unconverted.joinToString(", ")} saisi(es) en € ne peuvent pas être converti(es) ; plus-values de ces lignes non calculées jusqu'au retour du taux.", Tone.WARN)
                }
                item { HistoryCard(model) }
                item { RebalanceCard(portfolio) }
                item { SaleCard(portfolio) }
                item { ProjectionCard(portfolio.total) }
                risk?.let { r -> if (r.dangers.isNotEmpty() || r.limits.any { it.level == LimitLevel.DANGER && it.code == "daily_loss" }) item { RiskBanners(r.limits, r.dangers) } }
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("LIGNES · SIGNAL 1 JOUR", color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.weight(1f))
                        LiveBadge(model.live)
                    }
                }
                items(portfolio.lines, key = { it.holding.id }) { line ->
                    val stored = model.holdings.firstOrNull { it.id == line.holding.id }
                    LineRow(line, stored, line.holding.asset.symbol in usd.unconverted, model.radarDecision(line.holding.asset), dangerById[line.holding.id], onOpen = { open(line.holding.asset) }, onEdit = { form = stored ?: line.holding }) {
                        model.updateHoldings(model.holdings.filterNot { it.id == line.holding.id })
                    }
                }
                item { Caption("Touchez le crayon pour modifier une ligne, la corbeille pour la supprimer. Vos avoirs restent sur ce téléphone.") }
                risk?.let { r ->
                    item { LimitsCard(r.limits) }
                    item { StressCard(r.portfolio, r.stress, r.betas) }
                    item { SectorCard(r.portfolio.lines, model.client) }
                    item { WhatIfCard(r.portfolio, candles, model.client) }
                }
            }
        }
    }

    if (adding || form != null) {
        HoldingForm(model, form, form?.let { prices[it.asset.id] }) {
            adding = false
            form = null
        }
    }
}

@Composable
private fun Summary(model: AppModel, p: Portfolio) {
    Card {
        Caption("Valeur totale")
        Text(Format.money(p.total), style = mono(30.sp, FontWeight.Bold))
        p.gain?.let { g ->
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("${if (g >= 0) "+" else "−"}${Format.money(kotlin.math.abs(g))}", style = mono(14.sp), color = AltimColors.forChange(g))
                ChangeText(p.gainPercent)
                Caption("depuis l'achat")
            }
        }
        if (p.total > 0) {
            val crypto = Math.round(p.cryptoShare)
            Row(
                Modifier.fillMaxWidth().height(8.dp).clip(CircleShape).semantics { contentDescription = "Cryptos $crypto %, actions ${100 - crypto} %" },
                horizontalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                if (p.cryptoShare > 0) Box(Modifier.weight(p.cryptoShare.toFloat().coerceAtLeast(0.01f)).height(8.dp).background(AltimColors.allocCrypto))
                if (p.cryptoShare < 100) Box(Modifier.weight((100 - p.cryptoShare).toFloat().coerceAtLeast(0.01f)).height(8.dp).background(AltimColors.allocStock))
            }
            Row {
                Dot(AltimColors.allocCrypto, "Cryptos $crypto %", Modifier.weight(1f))
                Dot(AltimColors.allocStock, "Actions ${100 - crypto} %")
            }
        }
        p.warnings.forEach { Notice(it, Tone.WARN) }
        FxNote(model)
    }
}

@Composable
private fun Dot(color: androidx.compose.ui.graphics.Color, text: String, modifier: Modifier = Modifier) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(color))
        Text(text, fontSize = 12.sp)
    }
}

/** What the risk functions found for this visit (null until the prices are loaded). */
private class RiskView(
    val portfolio: RiskPortfolio,
    val betas: Map<String, BetaEstimate>,
    val stress: List<StressResult>,
    val limits: List<LimitCheck>,
    val dangers: List<Danger>,
)

@Composable
private fun LineRow(line: PortfolioLine, stored: Holding?, unconverted: Boolean, decision: ConfigSnapshot?, danger: Danger?, onOpen: () -> Unit, onEdit: () -> Unit, onDelete: () -> Unit) {
    val h = line.holding
    val shape = RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.surface.copy(alpha = 0.8f))
            .then(if (danger != null) Modifier.border(1.dp, AltimColors.sell.copy(alpha = 0.6f), shape) else Modifier)
            .clickable(onClick = onOpen).padding(start = 14.dp, top = 10.dp, bottom = 10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) {
                Text(h.asset.symbol, style = mono(15.sp, FontWeight.Bold))
                Text("${Format.quantity(h.quantity)} · ${Format.price(line.price)}", color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (unconverted) Caption("PRU non converti")
                Holdings.costNote(stored)?.let { Caption(it) }
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(line.value?.let { Format.money(it) } ?: "—", style = mono(14.sp))
                val gp = line.gainPercent
                if (gp != null) ChangeText(gp) else line.weight?.let { Text("${Math.round(it)} %", fontSize = 12.sp, color = AltimColors.textSecondary) }
            }
            // The asset's full decision when seen less than 12 h ago (Radar or asset page); nothing otherwise.
            decision?.let { DecisionBadge(it) }
            Column {
                IconButton(onClick = onEdit, modifier = Modifier.size(36.dp)) { Icon(Icons.Filled.Edit, contentDescription = "Modifier ${h.asset.symbol}", tint = AltimColors.violet, modifier = Modifier.size(18.dp)) }
                IconButton(onClick = onDelete, modifier = Modifier.size(36.dp)) { Icon(Icons.Filled.Delete, contentDescription = "Supprimer ${h.asset.symbol}", tint = AltimColors.sell, modifier = Modifier.size(18.dp)) }
            }
        }
        if (h.stop != null || danger != null) {
            Column(Modifier.padding(end = 14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                h.stop?.let { stop ->
                    val below = line.price?.takeIf { it > 0 }?.let { " (${Format.plain((it - stop) / it * 100, 1)} % sous le cours)" } ?: ""
                    Caption("Votre stop : ${Format.price(stop)}$below.")
                }
                danger?.let { DangerBlock(it) }
            }
        }
    }
}

/** Add or edit a line: asset (search), quantity, average purchase price. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HoldingForm(model: AppModel, holding: Holding?, lastPrice: Double?, close: () -> Unit) {
    var asset by remember { mutableStateOf(holding?.asset) }
    var query by remember { mutableStateOf("") }
    var results by remember { mutableStateOf<List<SearchItem>>(emptyList()) }
    var quantity by remember { mutableStateOf(holding?.let { Format.quantity(it.quantity) } ?: "") }
    // Amounts are typed in the display currency; a field left untouched keeps its saved value and currency (a dollar
    // cost basis is not silently re-based in euros). The journal and the engines get dollars.
    val cur = Money.displayCurrency()
    val avgInitial = remember { holding?.averagePrice?.let { Money.inputText(Holdings.shown(it, holding.costCurrency)) } ?: "" }
    val stopInitial = remember { holding?.stop?.let { Money.inputText(Holdings.shown(it, holding.stopCurrency)) } ?: "" }
    val usdLine = remember { holding?.let { Holdings.toUsd(listOf(it)).holdings.first() } }
    var average by remember { mutableStateOf(avgInitial) }
    var current by remember { mutableStateOf<Double?>(null) }
    var stopText by remember { mutableStateOf(stopInitial) }
    // A first entry of holdings is usually past purchases: not journaled unless asked; later additions and edits are.
    var journal by remember { mutableStateOf(holding != null || model.holdings.isNotEmpty()) }
    var note by remember { mutableStateOf("") }

    LaunchedEffect(query) {
        val q = query.trim()
        if (q.isEmpty()) {
            results = emptyList()
            return@LaunchedEffect
        }
        delay(250)
        results = runCatching { model.client?.search(q, 12) }.getOrNull() ?: emptyList()
    }
    LaunchedEffect(asset?.id) {
        current = asset?.let { a -> runCatching { model.client?.quotes(listOf(a))?.firstOrNull()?.price }.getOrNull() }
    }
    val q = Format.parse(quantity)
    val keepCost = holding != null && average == avgInitial
    val keepStop = holding != null && stopText == stopInitial
    val stop = if (stopText.isBlank()) null else Format.parse(stopText)
    val valid = asset != null && q != null && q > 0 && (average.isEmpty() || (Format.parse(average) ?: -1.0) > 0) && (stopText.isBlank() || (stop ?: -1.0) > 0)
    val avgNow = if (average.isEmpty()) null else Format.parse(average)
    // The same in dollars, for the journal (a kept euro cost without a rate: unknown).
    val avgUsd = if (keepCost) usdLine?.averagePrice else avgNow?.let(Money::fromDisplay)?.takeIf { it.isFinite() }
    val stopUsd = if (keepStop) usdLine?.stop else stop?.let(Money::fromDisplay)?.takeIf { it.isFinite() }
    // What the save records: an edit's purchase or sale, or the purchase of a new line (at its average cost, else the current price).
    val change = when {
        !valid -> null
        holding != null -> TradeJournal.holdingChange(holding.quantity, usdLine?.averagePrice, q!!, avgUsd, current ?: lastPrice)
        else -> (avgUsd ?: current)?.let { HoldingChange("buy", q!!, it, false) }
    }

    ModalBottomSheet(onDismissRequest = close, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = AltimColors.surface) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).imePadding().navigationBarsPadding(),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = close) { Text("Annuler", color = AltimColors.cyan) }
                Text(if (holding == null) "Ajouter un avoir" else "Modifier", fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, modifier = Modifier.weight(1f))
                TextButton(enabled = valid, onClick = {
                    val a = asset ?: return@TextButton
                    // What is saved: the typed amounts with their currency (untouched fields keep theirs).
                    val avg = if (keepCost) holding!!.averagePrice else avgNow
                    val avgCur = if (keepCost) holding!!.costCurrency else avg?.let { cur }
                    val savedStop = if (keepStop) holding!!.stop else stop
                    val stopCur = if (keepStop) holding!!.stopCurrency else savedStop?.let { cur }
                    val list = model.holdings.toMutableList()
                    val i = list.indexOfFirst { it.id == holding?.id }
                    val saved = if (i >= 0) Holding(holding!!.id, a, q!!, avg, savedStop, avgCur, stopCur).cleaned()
                    else Holding(asset = a, quantity = q!!, averagePrice = avg, stop = savedStop, costCurrency = avgCur, stopCurrency = stopCur).cleaned()
                    if (i >= 0) list[i] = saved else list += saved
                    model.updateHoldings(list)
                    if (journal && change != null) {
                        model.recordRealTrade(a, change.side, change.price, change.quantity, if (change.side == "buy") stopUsd else null, note, saved.id)
                    }
                    close()
                }) { Text("Enregistrer", color = if (valid) AltimColors.cyan else AltimColors.textSecondary, fontWeight = FontWeight.Bold) }
            }
            Caption("ACTIF")
            val a = asset
            if (a != null) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(a.symbol, style = mono(15.sp, FontWeight.Bold))
                        Caption(a.name)
                    }
                    if (holding == null) TextButton(onClick = { asset = null }) { Text("Changer", color = AltimColors.cyan) }
                }
            } else {
                FormField(query, "Rechercher : BTC, Apple, NVDA…", KeyboardType.Text) { query = it }
                results.forEach { item ->
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).clickable {
                            asset = item.asset
                            query = ""
                        }.padding(vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(item.symbol, style = mono(14.sp, FontWeight.Bold))
                        Text(item.name, color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                        Badge(item.kind.label, Tone.NEUTRAL)
                    }
                }
            }
            FormField(quantity, "Quantité (ex. 0,25)", KeyboardType.Decimal) { quantity = it }
            FormField(average, current?.let { "Prix moyen d'achat (actuel ${Format.price(it)})" } ?: "Prix moyen d'achat en ${cur.symbol} (facultatif)", KeyboardType.Decimal) { average = it }
            if (keepCost) Holdings.costNote(holding)?.let { Caption("$it Le modifier l'enregistre en ${cur.symbol}.") }
            Caption("Le prix moyen d'achat sert seulement à calculer votre gain ou perte. Rien n'est envoyé au serveur.")
            FormField(stopText, "Mon stop (${cur.symbol}, facultatif)", KeyboardType.Decimal) { stopText = it }
            Caption("Prix auquel vous comptez vendre pour limiter la perte. Altim vous alerte quand le cours s'en approche (moins d'une volatilité journalière) ou le casse. Aucun ordre n'est passé.")
            change?.let { c ->
                val text = if (holding == null) {
                    "Achats faits aujourd'hui : les inscrire au journal"
                } else {
                    "${if (c.side == "buy") "Achat" else "Vente"} de ${Format.quantity(c.quantity)} ${holding.asset.symbol} à ${Format.price(c.price)}${if (c.implied) " (déduit du nouveau PRU)" else " (cours actuel)"} : l'inscrire au journal"
                }
                JournalToggle(journal, { journal = it }, note, { note = it }, text)
            }
            Box(Modifier.height(24.dp))
        }
    }
}

@Composable
private fun FormField(value: String, placeholder: String, type: KeyboardType, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        placeholder = { Text(placeholder, color = AltimColors.textSecondary) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = type, autoCorrectEnabled = false),
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
    )
}
