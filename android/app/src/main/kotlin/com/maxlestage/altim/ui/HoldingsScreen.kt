package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.Portfolio
import com.maxlestage.altim.kit.PortfolioLine
import com.maxlestage.altim.kit.RadarRow
import com.maxlestage.altim.kit.SearchItem
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.delay

/**
 * Portfolio: value at live prices, gain / loss, concentration, and the daily signal of each line.
 * The lines stay on this phone; only the symbols are sent to the server to get prices and signals.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HoldingsScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit) {
    val quotes = remember { mutableStateMapOf<String, Double>() }
    val signals = remember { mutableStateMapOf<String, RadarRow>() }
    var error by remember { mutableStateOf<String?>(null) }
    var form by remember { mutableStateOf<Holding?>(null) }
    var adding by remember { mutableStateOf(false) }
    var refresh by remember { mutableIntStateOf(0) }

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
        runCatching { client.radar(assets, "1d") }.getOrNull()?.forEach { signals[it.id] = it }
    }

    // Live price when available, otherwise the consensus quote.
    val prices = quotes.toMutableMap().apply { model.holdings.forEach { h -> model.live.price(h.asset)?.let { put(h.asset.id, it.price) } } }
    val portfolio = Portfolio(model.holdings, prices)

    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Mes avoirs", fontSize = 30.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            IconButton(onClick = { adding = true }) { Icon(Icons.Filled.Add, contentDescription = "Ajouter un avoir", tint = AltimColors.cyan) }
        }
        PullToRefreshBox(isRefreshing = false, onRefresh = { refresh++ }) {
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
                item { Summary(portfolio) }
                error?.let { item { Notice(it, Tone.BAD) } }
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("LIGNES · SIGNAL 1 JOUR", color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.weight(1f))
                        LiveBadge(model.live)
                    }
                }
                items(portfolio.lines, key = { it.holding.id }) { line ->
                    LineRow(line, signals[line.holding.asset.id], onOpen = { open(line.holding.asset) }, onEdit = { form = line.holding }) {
                        model.updateHoldings(model.holdings.filterNot { it.id == line.holding.id })
                    }
                }
                item { Caption("Touchez le crayon pour modifier une ligne, la corbeille pour la supprimer. Vos avoirs restent sur ce téléphone.") }
            }
        }
    }

    if (adding || form != null) {
        HoldingForm(model, form) {
            adding = false
            form = null
        }
    }
}

@Composable
private fun Summary(p: Portfolio) {
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
    }
}

@Composable
private fun Dot(color: androidx.compose.ui.graphics.Color, text: String, modifier: Modifier = Modifier) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(color))
        Text(text, fontSize = 12.sp)
    }
}

@Composable
private fun LineRow(line: PortfolioLine, signal: RadarRow?, onOpen: () -> Unit, onEdit: () -> Unit, onDelete: () -> Unit) {
    val h = line.holding
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface.copy(alpha = 0.8f)).clickable(onClick = onOpen).padding(start = 14.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(h.asset.symbol, style = mono(15.sp, FontWeight.Bold))
            Text("${Format.quantity(h.quantity)} · ${Format.price(line.price)}", color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(line.value?.let { Format.money(it) } ?: "—", style = mono(14.sp))
            val gp = line.gainPercent
            if (gp != null) ChangeText(gp) else line.weight?.let { Text("${Math.round(it)} %", fontSize = 12.sp, color = AltimColors.textSecondary) }
        }
        signal?.signal?.let { ActionBadge(it.action) }
        Column {
            IconButton(onClick = onEdit, modifier = Modifier.size(36.dp)) { Icon(Icons.Filled.Edit, contentDescription = "Modifier ${h.asset.symbol}", tint = AltimColors.violet, modifier = Modifier.size(18.dp)) }
            IconButton(onClick = onDelete, modifier = Modifier.size(36.dp)) { Icon(Icons.Filled.Delete, contentDescription = "Supprimer ${h.asset.symbol}", tint = AltimColors.sell, modifier = Modifier.size(18.dp)) }
        }
    }
}

/** Add or edit a line: asset (search), quantity, average purchase price. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HoldingForm(model: AppModel, holding: Holding?, close: () -> Unit) {
    var asset by remember { mutableStateOf(holding?.asset) }
    var query by remember { mutableStateOf("") }
    var results by remember { mutableStateOf<List<SearchItem>>(emptyList()) }
    var quantity by remember { mutableStateOf(holding?.let { Format.quantity(it.quantity) } ?: "") }
    var average by remember { mutableStateOf(holding?.averagePrice?.let { Format.quantity(it) } ?: "") }
    var current by remember { mutableStateOf<Double?>(null) }

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
    val valid = asset != null && q != null && q > 0 && (average.isEmpty() || (Format.parse(average) ?: -1.0) > 0)

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
                    val avg = if (average.isEmpty()) null else Format.parse(average)
                    val list = model.holdings.toMutableList()
                    val i = list.indexOfFirst { it.id == holding?.id }
                    if (i >= 0) list[i] = Holding(holding!!.id, a, q!!, avg) else list += Holding(asset = a, quantity = q!!, averagePrice = avg)
                    model.updateHoldings(list)
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
            FormField(average, current?.let { "Prix moyen d'achat (actuel ${Format.price(it)})" } ?: "Prix moyen d'achat en $ (facultatif)", KeyboardType.Decimal) { average = it }
            Caption("Le prix moyen d'achat sert seulement à calculer votre gain ou perte. Rien n'est envoyé au serveur.")
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
