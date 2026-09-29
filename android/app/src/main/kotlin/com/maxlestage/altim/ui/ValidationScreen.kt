package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
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
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.ModelValidation
import com.maxlestage.altim.kit.ModelValidation.NNBSP
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.ValAsset
import com.maxlestage.altim.kit.ValGroup
import com.maxlestage.altim.kit.ValRegimeGroup
import com.maxlestage.altim.kit.ValidationReport
import com.maxlestage.altim.kit.ValidationResult
import kotlinx.coroutines.delay

// « Validation du modèle » (web Validation.tsx): the signal's backtest (the decision's own track record, same costs) on
// a basket fixed in advance, pooled by asset class, by market regime and overall (/api/validation). Stacked cards,
// wrapping chips, nothing wider than a 360 dp phone; the per-asset list follows the basket's order by default, never
// ranked by performance.

/** Opens « Validation du modèle » above the current screen (null where it cannot be opened, e.g. in the tests). */
val LocalOpenValidation = staticCompositionLocalOf<(() -> Unit)?> { null }

/** Single colour of the "a battu la détention" bar (same as the sector bars). */
private val BeatBar = Color(0xFF3987E5)

/** Loads /api/validation (202 while the first computation runs: asked again every 5 s). */
@Composable
fun ValidationScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit, onBack: () -> Unit) {
    var report by remember { mutableStateOf<ValidationReport?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var pending by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        val client = model.client ?: return@LaunchedEffect
        repeat(36) {
            try {
                when (val r = client.validation()) {
                    is ValidationResult.Ready -> {
                        report = r.report
                        pending = false
                        model.persistSession()
                        return@LaunchedEffect
                    }
                    ValidationResult.Pending -> {
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
                error = e.message ?: "Validation indisponible"
                return@LaunchedEffect
            }
        }
        error = "Le calcul prend plus de temps que prévu. Revenez dans un instant."
    }
    ValidationView(report, error, pending, modifier, open = open, onBack = onBack)
}

/** The whole screen from its state (no server: also rendered by the tests). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ValidationView(
    report: ValidationReport?,
    error: String?,
    pending: Boolean,
    modifier: Modifier = Modifier,
    open: (Asset) -> Unit = {},
    onBack: () -> Unit = {},
) {
    var regimeGroup by rememberSaveable { mutableStateOf("all") }
    var sort by rememberSaveable { mutableStateOf(ModelValidation.Sort.CLASS) }
    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Retour") }
            Text("Validation du modèle", fontSize = 20.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f).semantics { heading() })
        }
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxSize()) {
            item {
                Caption(
                    "Le signal d'Altim rejoué jour par jour sur un panier d'actions, de bitcoin, d'ether et d'altcoins choisi à l'avance, avec les mêmes réglages et les mêmes coûts que " +
                        "l'historique de la carte Décision. Un test du passé, pas une promesse.",
                )
            }
            error?.let { item { Notice("⚠ $it", Tone.WARN) } }
            if (report == null && error == null) {
                item { Card { Caption(if (pending) "Calcul sur tout le panier en cours (environ 30 secondes la première fois)…" else "Chargement…"); Loading() } }
            }
            if (report != null) {
                val r = report
                val groups = listOf(r.overall) + r.classes
                val shownRegimes = groups.firstOrNull { it.id == regimeGroup } ?: r.overall
                item { HeadCard(r) }
                item { Label("Par classe d'actifs") }
                items(r.classes, key = { "class-${it.id}" }) { GroupCard(it, r.parameters.taxRatePct) }
                item { Label("Par régime de marché") }
                item {
                    Chips("Actifs pris en compte", groups.map { it.id to ModelValidation.groupChip(it) }, shownRegimes.id) { regimeGroup = it }
                }
                items(ModelValidation.regimesOf(shownRegimes), key = { "regime-${shownRegimes.id}-${it.regime}" }) { RegimeCard(it) }
                item { Caption("${r.parameters.regimeRule} Régime lu la veille, sans données futures ; « inconnu » tant que l'historique est trop court pour le classer.") }
                item { Label("Actif par actif") }
                item { Chips("Ordre des actifs", ModelValidation.Sort.entries.map { it to it.label }, sort) { sort = it } }
                items(ModelValidation.sortAssets(r.assets, sort), key = { "asset-${it.symbol}" }) { AssetRow(it, open) }
                item {
                    Card(title = "Protections contre les biais") {
                        Bulleted(r.protections)
                        Text(
                            buildAnnotatedString {
                                withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append("Hors échantillon ?") }
                                append(" ${r.outOfSample.note}")
                            },
                            fontSize = 13.sp, color = Color.White.copy(alpha = 0.88f),
                        )
                    }
                }
                item {
                    Card(title = "Biais et limites") {
                        Bulleted(r.limits)
                        Caption(ModelValidation.costsText(r))
                    }
                }
            }
        }
    }
}

@Composable
private fun Label(text: String) =
    Text(text, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = Color.White, modifier = Modifier.padding(top = 8.dp).semantics { heading() })

/** Wrapping filter chips, one of them chosen. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun <T> Chips(description: String, options: List<Pair<T, String>>, value: T, onChange: (T) -> Unit) {
    val colors = FilterChipDefaults.filterChipColors(selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f), selectedLabelColor = AltimColors.cyan, labelColor = AltimColors.textSecondary)
    FlowRow(Modifier.fillMaxWidth().semantics { contentDescription = description }, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        options.forEach { (k, l) -> FilterChip(selected = k == value, onClick = { onChange(k) }, label = { Text(l) }, colors = colors) }
    }
}

@Composable
private fun Bulleted(items: List<String>) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        items.forEach { t ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("•", fontSize = 13.sp, color = AltimColors.textSecondary)
                Text(t, fontSize = 13.sp, color = Color.White.copy(alpha = 0.88f), modifier = Modifier.weight(1f))
            }
        }
    }
}

/** Verdict of a group as a chip that wraps (long labels on a narrow phone). */
@Composable
private fun VerdictChip(verdict: String, label: String) {
    val c = AltimColors.of(ModelValidation.verdictTone(verdict))
    val shape = RoundedCornerShape(12.dp)
    Text(
        label,
        Modifier.clip(shape).background(c.copy(alpha = 0.14f)).border(1.dp, c.copy(alpha = 0.5f), shape).padding(horizontal = 10.dp, vertical = 4.dp),
        color = c, fontSize = 12.sp, fontWeight = FontWeight.Bold,
    )
}

/** Share of the assets where the signal beat buy-and-hold: one thin bar, the number written next to it. */
@Composable
fun BeatMeter(beat: Int, n: Int, share: Double?) {
    Column(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) { contentDescription = "A battu la détention sur $beat actifs sur $n" },
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        KeyValue("A battu la simple détention", ModelValidation.beatText(beat, n, share))
        val barShape = RoundedCornerShape(3.dp)
        Box(Modifier.fillMaxWidth().height(6.dp).clip(barShape).background(Color.White.copy(alpha = 0.07f))) {
            val f = ((share ?: 0.0) / 100).coerceIn(0.0, 1.0).toFloat()
            if (f > 0f) Box(Modifier.fillMaxWidth(f).widthIn(min = 2.dp).fillMaxHeight().clip(barShape).background(BeatBar))
        }
    }
}

private fun tone(v: Double?): Tone? = v?.let { if (it >= 0) Tone.GOOD else Tone.BAD }

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun HeadCard(r: ValidationReport) {
    Card {
        Text(r.headline, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Tile("Actifs testés", "${r.overall.assets} / ${r.basket.size}")
            Tile("Trades", "${r.overall.trades}")
            Tile("Historique", ModelValidation.historyTile(r))
        }
        BeatMeter(r.overall.beatHold, r.overall.assets, r.overall.beatShare)
        VerdictChip(r.overall.verdict, r.overall.verdictLabel)
        Caption("Tous les trades ensemble : ${ModelValidation.pooledText(r.overall)}")
        Caption(ModelValidation.periodText(r))
        if (r.failures.isNotEmpty()) {
            Notice(ModelValidation.failuresTitle(r.failures.size) + r.failures.joinToString("") { "\n• ${it.symbol} — ${it.error}" }, Tone.WARN)
        }
    }
}

@Composable
private fun Tile(label: String, value: String) {
    val shape = RoundedCornerShape(12.dp)
    Column(
        Modifier.clip(shape).background(Color.White.copy(alpha = 0.05f)).padding(horizontal = 10.dp, vertical = 6.dp).semantics(mergeDescendants = true) {},
    ) {
        Caption(label)
        Text(value, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = Color.White)
    }
}

@Composable
fun GroupCard(g: ValGroup, taxRate: Double) {
    val v = ModelValidation
    Card(title = g.label) {
        VerdictChip(g.verdict, g.verdictLabel)
        Caption(v.groupSubtitle(g))
        Text(v.pooledText(g), fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        BeatMeter(g.beatHold, g.assets, g.beatShare)
        KeyValue("Rendement médian du signal", v.signedPct(g.medianReturn), tone(g.medianReturn))
        KeyValue("Détention médiane", v.signedPct(g.medianHold), tone(g.medianHold))
        g.worstReturn?.let { KeyValue("Pire actif (${it.symbol})", v.signedPct(it.value), tone(it.value)) }
        KeyValue("Recul max médian (signal / détention)", "${v.signedPct(g.medianDrawdown)} / ${v.signedPct(g.medianHoldDrawdown)}")
        g.worstDrawdown?.let { KeyValue("Pire recul (${it.symbol})", v.signedPct(it.value)) }
        KeyValue("Sharpe / Sortino médians", "${v.plain(g.medianSharpe)} / ${v.plain(g.medianSortino)}")
        KeyValue("Multiple de R moyen", g.avgR?.let { "${v.plain(it)} R" } ?: "—")
        KeyValue("Temps investi médian", g.medianExposure?.let { "${v.plain(it, 0)}$NNBSP%" } ?: "—")
        KeyValue("Après impôt ${v.plain(taxRate, 0)}$NNBSP% (signal / détention)", "${v.signedPct(g.medianAfterTax)} / ${v.signedPct(g.medianHoldAfterTax)}")
    }
}

@Composable
fun RegimeCard(g: ValRegimeGroup) {
    Card(title = g.label) {
        VerdictChip(g.verdict, g.verdictLabel)
        Text(ModelValidation.pooledText(g), fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        Caption(ModelValidation.regimeDays(g))
        if (g.regime != "unknown") Text(ModelValidation.regimeDaysText(g), fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
    }
}

/** One asset: symbol and name (opens the asset), class and group, signal vs buy-and-hold, then the details. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AssetRow(a: ValAsset, open: (Asset) -> Unit) {
    val v = ModelValidation
    val shape = RoundedCornerShape(16.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.surface.copy(alpha = 0.85f)).border(1.dp, Color.White.copy(alpha = 0.08f), shape)
            .clickable(role = Role.Button, onClickLabel = "Ouvrir ${a.symbol}") { open(a.asset) }.padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = AltimColors.cyan)) { append(a.symbol) }
                    if (a.name.isNotBlank()) withStyle(SpanStyle(color = AltimColors.textSecondary, fontSize = 12.sp)) { append(" ${a.name}") }
                },
                fontSize = 15.sp,
            )
            Badge("${v.classShort(a.`class`)} · ${a.group}", Tone.NEUTRAL)
            if (a.lowSample) Badge("échantillon trop faible", Tone.WARN)
        }
        Text(
            buildAnnotatedString {
                append("Signal ")
                withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = AltimColors.forChange(a.totalReturn))) { append(v.signedPct(a.totalReturn)) }
                append(" · détention ")
                withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = AltimColors.forChange(a.buyAndHold))) { append(v.signedPct(a.buyAndHold)) }
                withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" (${if (a.beatHold) "mieux" else "moins bien"}, écart ${v.signedPct(a.gap)})") }
            },
            fontSize = 13.sp, color = Color.White,
        )
        Caption(v.assetDetails(a))
    }
}

/** Link to « Validation du modèle » (end of the signal's track record; Réglages has its own card). */
@Composable
fun ValidationLink(onClick: () -> Unit) {
    Text(
        "Validation du modèle : le même test sur 34 actifs →",
        color = AltimColors.cyan, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
        modifier = Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = onClick).padding(vertical = 6.dp),
    )
}
