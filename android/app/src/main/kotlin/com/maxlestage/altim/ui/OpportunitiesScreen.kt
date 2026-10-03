package com.maxlestage.altim.ui

import android.content.Context
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.edit
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.OppFilters
import com.maxlestage.altim.kit.OppItem
import com.maxlestage.altim.kit.OppSaved
import com.maxlestage.altim.kit.Opportunities
import com.maxlestage.altim.kit.OpportunitiesResult
import com.maxlestage.altim.kit.OpportunityReport
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.delay
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

// "Opportunités du moment" (Sélection → Opportunités, web Opportunities.tsx): the selection's universe scanned by
// category on closed daily sessions, with the measured reason for each asset; category chips with counts and the
// filters (market cap or rank, liquidity, volatility) wrap, the market and filters are kept on this phone.

private const val KEY = "opportunities.v1"

/** Loads /api/opportunities (202 while the first scan runs: asked again every 5 s). */
@Composable
fun OpportunitiesScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim", Context.MODE_PRIVATE) }
    var saved by remember { mutableStateOf(Opportunities.parseSaved(prefs.getString(KEY, null))) }
    val update = { s: OppSaved ->
        saved = s
        prefs.edit { putString(KEY, Opportunities.encodeSaved(s)) }
    }
    var report by remember { mutableStateOf<OpportunityReport?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var pending by remember { mutableStateOf(false) }
    LaunchedEffect(saved.market) {
        val client = model.client ?: return@LaunchedEffect
        report = null
        error = null
        pending = false
        repeat(36) {
            try {
                when (val r = client.opportunities(saved.market)) {
                    is OpportunitiesResult.Ready -> {
                        report = r.report
                        pending = false
                        model.persistSession()
                        return@LaunchedEffect
                    }
                    OpportunitiesResult.Pending -> {
                        pending = true
                        delay(5_000)
                    }
                }
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "Indisponible"
                return@LaunchedEffect
            }
        }
        error = "Le calcul prend plus de temps que prévu. Revenez dans un instant."
    }
    OpportunitiesView(saved, report, error, pending, modifier, onSaved = update, open = open, onBack = onBack)
}

/** The whole screen from its state (no server: also rendered by the tests). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun OpportunitiesView(
    saved: OppSaved,
    report: OpportunityReport?,
    error: String?,
    pending: Boolean,
    modifier: Modifier = Modifier,
    onSaved: (OppSaved) -> Unit = {},
    open: (Asset) -> Unit = {},
    onBack: () -> Unit = {},
) {
    val market = saved.market
    val filters = saved.filters
    fun setFilters(f: OppFilters) = onSaved(saved.copy(filters = f))
    val shown = report?.let { Opportunities.filterItems(it.items, filters) }.orEmpty()
    val counts = report?.let { Opportunities.countByCategory(it.items, filters) }
    val limited = report?.categories?.filter { it.id in filters.categories && (it.note != null || it.error != null) }.orEmpty()
    val chipColors = FilterChipDefaults.filterChipColors(selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f), selectedLabelColor = AltimColors.cyan, labelColor = AltimColors.textSecondary)

    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Retour") }
            Text("Opportunités du moment", fontSize = 20.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f).semantics { heading() })
        }
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxSize()) {
            item { Caption("Ce qui bouge de façon notable aujourd'hui, catégorie par catégorie, avec la raison mesurée. Des pistes à examiner, pas des ordres d'achat.") }
            item {
                ChoiceRow(listOf(Kind.STOCK to "Actions", Kind.CRYPTO to "Cryptos"), market, { onSaved(saved.copy(market = it)) }, description = "Marché")
            }
            item {
                Card(title = "Catégories") {
                    FlowRow(Modifier.semantics { contentDescription = "Catégories" }, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Opportunities.CATEGORIES.forEach { c ->
                            FilterChip(
                                selected = c in filters.categories,
                                onClick = { setFilters(Opportunities.toggle(filters, c)) },
                                label = { Text("${Opportunities.short(c)}${counts?.let { " · ${it[c] ?: 0}" } ?: ""}") },
                                colors = chipColors,
                            )
                        }
                    }
                    if (market == Kind.STOCK) {
                        Choice("Capitalisation", Opportunities.CAPS, filters.minCap) { setFilters(filters.copy(minCap = it)) }
                    } else {
                        Choice("Rang (capitalisation)", Opportunities.RANKS, filters.maxRank) { setFilters(filters.copy(maxRank = it)) }
                    }
                    Choice("Liquidité (volume échangé)", Opportunities.LIQ, filters.minLiquidity) { setFilters(filters.copy(minLiquidity = it)) }
                    Choice("Volatilité max (ATR)", Opportunities.VOL, filters.maxVolatility) { setFilters(filters.copy(maxVolatility = it)) }
                    limited.forEach { c ->
                        val text = buildAnnotatedString {
                            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(Opportunities.short(c.id)) }
                            append(" : ${c.error ?: c.note}")
                        }
                        if (c.error != null) Notice(text.text, Tone.WARN) else Text(text, fontSize = 12.sp, color = AltimColors.textSecondary)
                    }
                }
            }
            error?.let { item { Notice(it, Tone.WARN) } }
            if (report == null && error == null) {
                item { Card { Caption(if (pending) Opportunities.pendingText(market) else "Chargement…"); Loading() } }
            }
            if (report != null) {
                item {
                    Text(
                        "RÉSULTATS · ${shown.size} SUR ${report.scanned} ANALYSÉ${if (report.scanned > 1) "S" else ""}",
                        color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp).semantics { heading() },
                    )
                }
                if (shown.isEmpty()) item { Caption("Aucun actif ne remplit ces critères en ce moment.") }
                items(shown, key = { it.symbol }) { i -> OppCard(i, market, open) }
                item { RulesCard(report) }
                item {
                    val at = DateTimeFormatter.ofPattern("HH:mm:ss", Locale.FRANCE).withZone(ZoneId.of("Europe/Paris")).format(Instant.ofEpochMilli(report.asOf.toLong()))
                    Caption("Scan calculé à $at. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.")
                }
            }
        }
    }
}

/** One filter: its label, then its choices as wrapping chips (no drop-down wider than the screen). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun <T> Choice(label: String, options: List<Pair<String, T?>>, value: T?, onChange: (T?) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Caption(label)
        FlowRow(Modifier.semantics { contentDescription = label }, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            options.forEach { (l, v) ->
                FilterChip(
                    selected = v == value, onClick = { onChange(v) }, label = { Text(l) },
                    colors = FilterChipDefaults.filterChipColors(selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f), selectedLabelColor = AltimColors.cyan, labelColor = AltimColors.textSecondary),
                )
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun OppCard(i: OppItem, market: Kind, open: (Asset) -> Unit) {
    val asset = Asset(i.symbol, market, i.name.ifEmpty { i.symbol })
    Card {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(Modifier.weight(1f).clickable(role = Role.Button, onClickLabel = "Ouvrir ${i.symbol}") { open(asset) }) {
                Text(i.name.ifEmpty { i.symbol }, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = Color.White)
                Caption("${i.symbol} · ${if (market == Kind.CRYPTO) (i.rank?.let { "rang $it" } ?: "crypto") else i.sector}")
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(Format.price(i.price), style = mono(14.sp, FontWeight.Bold), color = Color.White)
                i.change1d?.let { Text(Opportunities.change(it), style = mono(12.sp), color = if (it >= 0) AltimColors.buy else AltimColors.sell) }
            }
        }
        i.hits.forEach { h ->
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Badge(Opportunities.short(h.category), Tone.NEUTRAL)
                Text(h.reason, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
            }
        }
        val m = Opportunities.metrics(i)
        if (m.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) { m.forEach { Caption(it) } }
        }
        OutlinedButton(onClick = { open(asset) }, border = BorderStroke(1.2.dp, AltimColors.cyan)) { Text("Voir la fiche", color = AltimColors.cyan) }
    }
}

@Composable
private fun RulesCard(r: OpportunityReport) {
    var open by rememberSaveable { mutableStateOf(false) }
    Card {
        TextButton(onClick = { open = !open }) { Text("Règles, sources et limites ${if (open) "▲" else "▼"}", color = AltimColors.cyan) }
        if (open) {
            r.categories.forEach { c ->
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(c.label) }
                        append(" (${c.analyzed} analysé${if (c.analyzed > 1) "s" else ""}) : ${c.rule}")
                        c.note?.let { withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" $it") } }
                        c.error?.let { withStyle(SpanStyle(color = AltimColors.sell)) { append(" $it") } }
                    },
                    fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f),
                )
            }
            r.notCovered.forEach { n ->
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("Non couvert — ${n.label}") }
                        append(" : ${n.reason}.")
                    },
                    fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f),
                )
            }
            Caption("${r.universe}. Sources : ${r.source}. Séances closes uniquement.")
        }
    }
}
