package com.maxlestage.altim.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AddCircleOutline
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.RemoveCircle
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.background
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.DecisionDigests
import com.maxlestage.altim.kit.DecisionOpportunity
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.MacroInfo
import com.maxlestage.altim.kit.RadarRow
import com.maxlestage.altim.kit.SearchItem
import com.maxlestage.altim.kit.Timeframe
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

/** The decisions of the radar are re-read at most this often (they are heavier than the signals). */
private const val DECISION_EVERY = 15 * 60_000.0

/**
 * Watch list: live price, full decision (the technical signal of the chosen timeframe as one input), reliability of the
 * data, macro context.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RadarScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit, onSettings: () -> Unit = {}) {
    val rows = remember { mutableStateMapOf<String, RadarRow>() }
    var macro by remember { mutableStateOf<MacroInfo?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    var results by remember { mutableStateOf<List<SearchItem>>(emptyList()) }
    var editing by remember { mutableStateOf(false) }
    // "Mon ordre", "Variation" or "Décision", kept on this phone (iOS radar.sort).
    val context = androidx.compose.ui.platform.LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim", android.content.Context.MODE_PRIVATE) }
    var sort by remember { mutableStateOf(prefs.getString("radar.sort", "mine")?.takeIf { it in setOf("mine", "change", "decision") } ?: "mine") }
    val scope = rememberCoroutineScope()
    val timeframe = Timeframe.radar(model.timeframe)

    suspend fun load() {
        val client = model.client ?: return
        if (model.watchlist.isEmpty()) return
        loading = true
        try {
            val r = client.radar(model.watchlist, timeframe.raw)
            rows.clear()
            r.forEach { rows[it.id] = it }
            error = null
            model.persistSession()
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: Exception) {
            error = e.message
        } finally {
            loading = false
        }
        macro = runCatching { client.macro() }.getOrNull()
    }

    LaunchedEffect(model.watchlist.joinToString { it.id }, timeframe) { load() }
    // Decisions of the watched assets (market data only), re-read at most every 15 minutes while the Radar is open:
    // each one goes through the configuration diff (ConfigChanges). Two at a time: the server fetches fundamentals and
    // order books for each one; a decision that fails is simply compared at the next pass.
    LaunchedEffect(model.watchlist.joinToString { it.id }, model.client) {
        val client = model.client ?: return@LaunchedEffect
        while (true) {
            val now = System.currentTimeMillis().toDouble()
            val due = model.watchlist.take(20).filter { a -> model.lastDecisionCheck(a)?.let { now - it >= DECISION_EVERY } ?: true }
            for (pair in due.chunked(2)) {
                coroutineScope {
                    pair.map { a -> async { runCatching { client.decision(a) }.getOrNull() } }.awaitAll()
                }.filterNotNull().forEach { model.recordDecision(it, personal = false) }
            }
            delay(DECISION_EVERY.toLong())
        }
    }
    LaunchedEffect(query) {
        val q = query.trim()
        if (q.isEmpty()) {
            results = emptyList()
            return@LaunchedEffect
        }
        delay(250)
        results = runCatching { model.client?.search(q) }.getOrNull() ?: emptyList()
    }

    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Radar", fontSize = 30.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            TextButton(onClick = { editing = !editing }) { Text(if (editing) "OK" else "Modifier", color = AltimColors.cyan) }
            IconButton(onClick = onSettings) { Icon(Icons.Filled.Settings, contentDescription = "Réglages", tint = AltimColors.cyan) }
        }
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            placeholder = { Text("Ajouter : BTC, Apple, NVDA…", color = AltimColors.textSecondary) },
            leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
            trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Filled.Close, contentDescription = "Effacer") } },
            singleLine = true,
            shape = RoundedCornerShape(14.dp),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
        )
        if (query.isNotEmpty()) {
            LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (results.isEmpty()) item { Caption("Aucun résultat") }
                items(results, key = { it.id }) { item ->
                    SearchRow(item, model.isWatched(item.asset)) {
                        model.watch(item.asset)
                        query = ""
                    }
                }
            }
            return@Column
        }
        PullToRefreshBox(isRefreshing = loading && rows.isNotEmpty(), onRefresh = { scope.launch { load() } }) {
            LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                item { BriefCard(model, open) }
                item { BotRadarCard(model) }
                macro?.takeIf { it.level != "calm" }?.let { m -> item { MacroBanner(m) } }
                error?.let { e -> item { ErrorBox(e) { scope.launch { load() } } } }
                model.dangers?.takeIf { it.items.isNotEmpty() }?.let { d -> item { DangersNotice(d) } }
                if (model.configChanges.transitions.isNotEmpty()) {
                    item { ConfigChangesCard(model.configChanges.transitions, open, model.configAlertsEnabled, onClear = { model.clearTransitions() }) }
                }
                val now = System.currentTimeMillis().toDouble()
                // Assets whose full decision is ACHETER or ZONE D'ACHAT (not the technical signal alone).
                val opportunities = DecisionDigests.opportunities(model.watchlist, model.decisionDigests, now)
                if (opportunities.isNotEmpty()) item { OpportunitiesCard(opportunities, open) }
                item { TimeframeChoice(timeframe) { model.updateTimeframe(it) } }
                if (model.watchlist.size > 1) item {
                    SortChoice(sort) {
                        sort = it
                        prefs.edit().putString("radar.sort", it).apply()
                    }
                }
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Caption("DÉCISION · PRIX MÉDIAN DE 40 SOURCES", Modifier.weight(1f))
                        LiveBadge(model.live)
                    }
                }
                if (loading && rows.isEmpty()) item { Loading("Analyse des marchés…") }
                val shown = when (sort) {
                    // Largest move first, whatever its direction; no price last.
                    "change" -> DecisionDigests.sortByChange(model.watchlist) { a -> model.live.price(a)?.change ?: rows[a.id]?.change }
                    // "Décision": the full decision's rating (buy side first), then its confidence.
                    "decision" -> DecisionDigests.sortByDecision(model.watchlist, model.decisionDigests, now)
                    else -> model.watchlist
                }
                items(shown, key = { it.id }) { asset ->
                    val index = model.watchlist.indexOf(asset)
                    val dismiss = rememberSwipeToDismissBoxState()
                    LaunchedEffect(dismiss.currentValue) {
                        if (dismiss.currentValue == SwipeToDismissBoxValue.EndToStart) model.unwatch(asset)
                    }
                    SwipeToDismissBox(
                        state = dismiss,
                        enableDismissFromStartToEnd = false,
                        backgroundContent = {
                            if (dismiss.dismissDirection == SwipeToDismissBoxValue.EndToStart) Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.sell.copy(alpha = 0.3f)).padding(16.dp), horizontalArrangement = Arrangement.End) {
                                Text("Retirer", color = Color.White, fontWeight = FontWeight.Bold)
                            }
                        },
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (editing) {
                                IconButton(onClick = { model.unwatch(asset) }) {
                                    Icon(Icons.Filled.RemoveCircle, contentDescription = "Retirer ${asset.symbol} du radar", tint = AltimColors.sell)
                                }
                            }
                            RadarRowView(model, asset, rows[asset.id], timeframe, Modifier.weight(1f).clickable(enabled = !editing) { open(asset) })
                            // Reordering only in "Mon ordre" (another sort would move the rows back).
                            if (editing && sort == "mine") {
                                Column {
                                    IconButton(enabled = index > 0, onClick = { model.updateWatchlist(model.watchlist.toMutableList().apply { add(index - 1, removeAt(index)) }) }) {
                                        Icon(Icons.Filled.KeyboardArrowUp, contentDescription = "Monter ${asset.symbol}")
                                    }
                                    IconButton(enabled = index < model.watchlist.lastIndex, onClick = { model.updateWatchlist(model.watchlist.toMutableList().apply { add(index + 1, removeAt(index)) }) }) {
                                        Icon(Icons.Filled.KeyboardArrowDown, contentDescription = "Descendre ${asset.symbol}")
                                    }
                                }
                            }
                        }
                    }
                }
                item {
                    Caption("Le verdict est la décision complète de l'actif (la même que sur sa page), relue toutes les 15 minutes ; la tendance technique ${timeframe.label} (unité choisie ci-dessus) n'en est qu'un indice. Une probabilité mesurée sur l'historique, jamais une certitude. Glissez vers la gauche pour retirer un actif.")
                }
                if (model.watchlist.size >= 2) item { CompareCard(model) }
            }
        }
    }
}

@Composable
private fun Caption(text: String, modifier: Modifier) = Text(text, color = AltimColors.textSecondary, fontSize = 12.sp, modifier = modifier)

/** Timeframe of the technical line (4 h, 1 j, 4 j, 1 sem.), saved and shared with the asset page. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TimeframeChoice(selected: Timeframe, onChange: (Timeframe) -> Unit) {
    ChoiceRow(Timeframe.RADAR.map { it to it.label }, selected, onChange, description = "Unité de temps de l'analyse technique")
}

/** "Trier le radar": the user's order, the largest move, or the full decision. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SortChoice(sort: String, onChange: (String) -> Unit) {
    val options = listOf("mine" to "Mon ordre", "change" to "Variation", "decision" to "Décision")
    ChoiceRow(options, sort, onChange, description = "Trier le radar")
}

/** The watched assets whose full decision says ACHETER or ZONE D'ACHAT, most confident first. */
@Composable
private fun OpportunitiesCard(items: List<DecisionOpportunity>, open: (Asset) -> Unit) {
    Card(title = "Opportunités détectées") {
        items.forEach { o ->
            val a = o.asset
            val d = o.decision
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable { open(a) },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(a.name, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Text("confiance ${Math.round(d.confidence)}", color = AltimColors.textSecondary, fontSize = 12.sp)
                DecisionBadge(d)
            }
        }
    }
}

@Composable
private fun SearchRow(item: SearchItem, watched: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface).clickable(onClick = onClick).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(item.symbol, style = mono(15.sp))
            Text(item.name, color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Badge(if (item.etf == true) "ETF" else item.kind.label, Tone.NEUTRAL)
        Icon(if (watched) Icons.Filled.CheckCircle else Icons.Filled.AddCircleOutline, contentDescription = if (watched) "Déjà au radar" else "Ajouter", tint = AltimColors.cyan)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun RadarRowView(model: AppModel, asset: Asset, row: RadarRow?, timeframe: Timeframe = Timeframe.STANDARD, modifier: Modifier = Modifier) {
    val tick = model.live.price(asset)
    val decision = model.radarDecision(asset)
    Column(
        modifier.clip(RoundedCornerShape(14.dp)).background(AltimColors.surface).padding(horizontal = 14.dp, vertical = 12.dp)
            .semantics(mergeDescendants = true) { contentDescription = "${asset.name}, ${Format.price(tick?.price ?: row?.price)}" },
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Symbol, sparkline, price: the symbol and the price share the width (a long price wraps, never runs off a
        // 320 dp screen); the sparkline steps aside when the font is enlarged.
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(Modifier.weight(1f)) {
                Text(asset.symbol, style = mono(16.sp, FontWeight.Bold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(asset.name, color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (LocalDensity.current.fontScale <= 1.3f) row?.sparkline?.takeIf { it.size > 2 }?.let { Sparkline(it, Modifier.width(56.dp).height(26.dp)) }
            Column(Modifier.weight(1f, fill = false), horizontalAlignment = Alignment.End) {
                Text(Format.price(tick?.price ?: row?.price), style = mono(14.sp), textAlign = TextAlign.End)
                ChangeText(tick?.change ?: row?.change)
            }
        }
        // The verdict is the full decision's; the technical signal is only one of its inputs (line below). The chips
        // wrap rather than push the row wider than the screen.
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            DecisionBadge(decision)
            val s = row?.signal
            if (s != null) TechnicalText(s.action, timeframe.label)
            else if (row?.error != null) Text("signal technique indisponible", color = AltimColors.textSecondary, fontSize = 11.sp, maxLines = 1)
            row?.reliability?.takeIf { it.level != "high" }?.let { Badge(if (it.level == "medium") "FIAB. MOY." else "FIAB. FAIBLE", it.tone) }
        }
        // Why the chip says ATTENDRE: the decision's short reason, on its own line (web DecisionNote).
        DecisionNote(decision)
    }
}

/** "Régime de marché : Risk-on — Stress macro 10/100 (calme) · …" (/api/macro and the zones' macro context). */
@Composable
fun RegimeText(r: com.maxlestage.altim.kit.MarketRegime) {
    Text(
        androidx.compose.ui.text.buildAnnotatedString {
            pushStyle(androidx.compose.ui.text.SpanStyle(color = AltimColors.textSecondary))
            append("Régime de marché : ")
            pop()
            pushStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.Bold))
            append(r.label.ifBlank { r.kind.label })
            pop()
            if (r.reasons.isNotEmpty()) {
                pushStyle(androidx.compose.ui.text.SpanStyle(color = AltimColors.textSecondary))
                append(" — ${r.reasons.joinToString(" · ")}")
                pop()
            }
        },
        fontSize = 12.sp, color = Color.White,
    )
}

@Composable
fun MacroBanner(macro: MacroInfo) {
    val c = AltimColors.of(macro.tone)
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(c.copy(alpha = 0.1f)).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Contexte macro : ${macro.levelLabel}", fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.weight(1f))
            Badge("${Math.round(macro.score)}/100", macro.tone)
        }
        macro.regime?.let { RegimeText(it) }
        macro.factors.take(3).forEach { Text("• ${it.text}", fontSize = 12.sp, color = Color.White.copy(alpha = 0.85f)) }
        Caption("Une zone d'achat peut céder si la situation mondiale se dégrade (guerre, crise, taux) : tailles réduites conseillées.")
    }
    Spacer(Modifier.size(4.dp))
}
