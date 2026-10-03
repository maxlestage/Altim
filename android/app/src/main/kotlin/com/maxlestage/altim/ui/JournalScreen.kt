package com.maxlestage.altim.ui

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.ClosedInfo
import com.maxlestage.altim.kit.EntryReview
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.GroupStat
import com.maxlestage.altim.kit.HorizonReview
import com.maxlestage.altim.kit.JournalProfile
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.TradeEntry
import com.maxlestage.altim.kit.TradeJournal
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs

// Journal (Mes avoirs → Journal, web Journal.tsx): every simulated purchase and every real purchase or sale saved in
// Mes avoirs, with the decision of that moment, reviewed at 3, 10 and 30 days on the following daily candles; the
// profile of the results by rating, plan respected and market regime. Stored on this phone only; stacked rows,
// wrapping chips, nothing scrolls sideways.

private fun fr(v: Double, d: Int = 1) = Format.plain(v, d)
private fun signed(v: Double, d: Int = 1) = "${if (v > 0) "+" else if (v < 0) "−" else ""}${fr(abs(v), d)}"
private fun rText(v: Double?) = if (v == null) "—" else "${signed(v, 2)} R"
private fun pctText(v: Double?) = if (v == null) "—" else "${signed(v)} %"
private val MACRO_LABEL = mapOf("calm" to "calme", "tense" to "tendu", "high" to "très tendu")
private fun stepIcon(s: String) = when (s) { "ok" -> "✓"; "no" -> "✕"; else -> "?" }

/** "1 juin 2026" (a candle day, UTC midnight) or "1 juin 2026 à 10:00" (a moment, Paris time). */
internal fun frDate(ms: Double, withTime: Boolean = false): String {
    val day = ms % 86_400_000.0 == 0.0
    val pattern = if (withTime && !day) "d MMMM yyyy 'à' HH:mm" else "d MMMM yyyy"
    return DateTimeFormatter.ofPattern(pattern, Locale.FRANCE).withZone(if (day) ZoneId.of("UTC") else ZoneId.of("Europe/Paris")).format(Instant.ofEpochMilli(ms.toLong()))
}

/** "Pourquoi j'entre" (optional), in the order sheets. */
@Composable
fun JournalNoteField(value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = { onChange(it.take(1000)) },
        label = { Text("Pourquoi j'entre (facultatif, pour le journal)") },
        placeholder = { Text("ex. rebond sur le support, objectif 1 visé", color = AltimColors.textSecondary) },
        minLines = 2,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan, focusedLabelColor = AltimColors.cyan),
    )
}

/** Checkbox "inscrire au journal" + the note, in « Mes avoirs ». */
@Composable
fun JournalToggle(checked: Boolean, onChange: (Boolean) -> Unit, note: String, onNote: (String) -> Unit, text: String) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Checkbox) { onChange(!checked) }
                .semantics(mergeDescendants = true) { toggleableState = ToggleableState(checked) },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(checked = checked, onCheckedChange = null, colors = CheckboxDefaults.colors(checkedColor = AltimColors.cyan, checkmarkColor = Color.Black))
            Text(text, fontSize = 14.sp, modifier = Modifier.padding(start = 8.dp).weight(1f))
        }
        if (checked) JournalNoteField(note, onNote)
    }
}

/** The Journal part of Mes avoirs: daily candles of every asset of the journal (once per opening), then the reviews. */
@Composable
fun JournalPane(model: AppModel, open: (Asset) -> Unit, modifier: Modifier = Modifier, onReal: (() -> Unit)? = null) {
    val entries = model.tradeJournal.entries
    // Absent: loading; null: failed (the review says so).
    val candles = remember { mutableStateMapOf<String, List<Candle>?>() }
    val assets = entries.map { it.asset }.distinctBy { it.id }
    LaunchedEffect(assets.map { it.id }.sorted().joinToString(",")) {
        val client = model.client ?: return@LaunchedEffect
        for (a in assets) {
            if (candles.containsKey(a.id) && candles[a.id] != null) continue
            candles[a.id] = try {
                client.candles(a, "1d").candles
            } catch (e: com.maxlestage.altim.kit.AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
        }
    }
    val closed = model.paper.trades.associate { it.id to ClosedInfo(it.closedAt, it.exit, it.reason.label) }
    JournalView(
        entries, model.tradeJournalError, candles.toMap(), closed, System.currentTimeMillis().toDouble(), modifier,
        open = open, onNote = model::setJournalNote, onDelete = model::deleteJournalEntry, onReal = onReal,
    )
}

/** The whole journal from its entries and candles (no server: also rendered by the tests). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun JournalView(
    entries: List<TradeEntry>,
    error: String?,
    candles: Map<String, List<Candle>?>,
    closed: Map<String, ClosedInfo>,
    now: Double,
    modifier: Modifier = Modifier,
    open: (Asset) -> Unit = {},
    onNote: (String, String) -> Unit = { _, _ -> },
    onDelete: (String) -> Unit = {},
    onReal: (() -> Unit)? = null,
) {
    var horizon by rememberSaveable { mutableIntStateOf(10) }
    val reviews = remember(entries, candles, closed, now) {
        entries.map { e -> TradeJournal.reviewEntry(e, candles[e.asset.id].orEmpty(), now, if (e.source == "paper" && e.refId != null) closed[e.refId] else null) }
    }
    val profile = remember(reviews, horizon) { TradeJournal.profile(reviews, horizon) }
    val newest = reviews.sortedByDescending { it.entry.createdAt }
    LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp), modifier = modifier) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Caption("Enregistré uniquement sur ce téléphone · ${entries.size} entrée${if (entries.size > 1) "s" else ""}")
                Caption(
                    "Chaque achat simulé et chaque achat ou vente réel enregistré dans « Mes avoirs » est noté ici avec la décision affichée à ce moment-là, puis revu " +
                        "automatiquement à 3, 10 et 30 jours sur les bougies journalières suivantes. Des faits, pas des jugements : un bon trade peut perdre, un mauvais peut gagner.",
                )
            }
        }
        error?.let { item { Notice(it, Tone.WARN) } }
        if (entries.isEmpty()) {
            item {
                Card(title = "Aucune entrée pour l'instant") {
                    Caption("Simulez un achat depuis la carte « Décision » d'un actif, ou enregistrez un achat ou une vente dans Mes avoirs : l'entrée apparaîtra ici.")
                    onReal?.let { TextButton(onClick = it) { Text("Mes avoirs réels", color = AltimColors.cyan, fontSize = 13.sp) } }
                }
            }
            return@LazyColumn
        }
        item { ProfileCard(profile, horizon) { horizon = it } }
        item { Text("ENTRÉES", color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp).semantics { heading() }) }
        items(newest, key = { it.entry.id }) { r ->
            EntryCard(r, if (candles.containsKey(r.entry.asset.id)) candles[r.entry.asset.id] else LOADING, open, onNote, onDelete)
        }
    }
}

/** Marker of candles still loading (distinct from null: failed). */
private val LOADING: List<Candle> = listOf(Candle(-1.0, 0.0, 0.0, 0.0, 0.0))

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ProfileCard(profile: JournalProfile, horizon: Int, onHorizon: (Int) -> Unit) {
    Card(title = "Votre profil") {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            TradeJournal.REVIEW_DAYS.forEach { d ->
                FilterChip(
                    selected = d == horizon, onClick = { onHorizon(d) }, label = { Text("À $d jours") },
                    colors = FilterChipDefaults.filterChipColors(selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f), selectedLabelColor = AltimColors.cyan, labelColor = AltimColors.textSecondary),
                )
            }
        }
        val p = profile
        Caption(
            "${p.reviewed} achat${if (p.reviewed > 1) "s" else ""} sur ${p.purchases} revu${if (p.reviewed > 1) "s" else ""} à $horizon jours" +
                (if (p.pending > 0) " · ${p.pending} pas encore à $horizon jours" else "") +
                ". Résultats en R (multiples du risque pris, entrée − stop) et en pire recul : un gain obtenu en prenant plus de risque ne compte pas davantage. " +
                "Sortie supposée au premier niveau touché (stop ou objectif 1), sinon au dernier cours.",
        )
        val all = p.all
        if (all != null) {
            GroupRows("Ensemble", listOf(all))
            GroupRows("Par note à l'entrée", p.byRating)
            GroupRows("Plan respecté ou non", p.byPlan)
            GroupRows("Par régime de marché", p.byRegime)
        } else {
            Caption("Aucun achat n'a encore $horizon jours d'historique après l'entrée.")
        }
    }
}

/** Figures two by two (label above, value below): stacked, never a wide table. */
@Composable
private fun Figures(items: List<Triple<String, String, Color>>) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        items.chunked(2).forEach { row ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                row.forEach { (label, value, color) ->
                    Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}) {
                        Text(label, fontSize = 11.sp, color = AltimColors.textSecondary)
                        Text(value, style = mono(13.sp), color = color)
                    }
                }
                if (row.size == 1) Column(Modifier.weight(1f)) {}
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GroupRows(title: String, groups: List<GroupStat>) {
    if (groups.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White, modifier = Modifier.semantics { heading() })
        groups.forEach { g ->
            HorizontalDivider(color = Color.White.copy(alpha = 0.08f))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(g.label, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                Caption("${g.n} achat${if (g.n > 1) "s" else ""}${if (g.withStop < g.n) " (${g.withStop} avec stop)" else ""}")
                if (g.lowSample) Badge("échantillon trop faible (< ${TradeJournal.MIN_SAMPLE})", Tone.NEUTRAL)
            }
            val avg = g.avgR ?: 0.0
            Figures(
                listOf(
                    Triple("Résultat moyen", rText(g.avgR), if (avg > 0) AltimColors.buy else if (avg < 0) AltimColors.sell else Color.White),
                    Triple("Médiane", rText(g.medianR), Color.White),
                    Triple("Positifs", g.winRate?.let { "${fr(it, 0)} %" } ?: "—", Color.White),
                    Triple("Recul moyen", if (g.avgMaeR != null) rText(g.avgMaeR) else pctText(g.avgMaePct), Color.White),
                    Triple("Pire recul", rText(g.worstMaeR), Color.White),
                    Triple("Variation moy.", pctText(g.avgReturnPct), Color.White),
                ),
            )
        }
    }
}

private fun horizonText(h: HorizonReview, buy: Boolean): String = when (h.status) {
    "pending" -> "${h.days} j : disponible le ${frDate(h.availableAt)}."
    "noData" -> "${h.days} j : pas de bougie journalière après l'entrée dans l'historique disponible."
    else -> {
        val first = when (h.first) {
            "stop" -> "stop touché en ${h.firstDays} j"
            "target1" -> "objectif 1 atteint en ${h.firstDays} j${if (h.target2) ", puis objectif 2" else ""}"
            else -> "ni stop ni objectif"
        }
        "${h.days} j : ${pctText(h.returnPct)} au dernier cours · plus haut ${pctText(h.mfePct)}${h.mfeR?.let { " (${rText(it)})" } ?: ""} · plus bas ${pctText(h.maePct)}" +
            (h.maeR?.let { " (${rText(it)})" } ?: "") +
            if (buy) " · $first${h.resultR?.let { " · résultat selon le plan ${rText(it)}" } ?: ""}" else ""
    }
}

@Composable
private fun Chip(text: String) {
    Text(
        text, fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f),
        modifier = Modifier.clip(RoundedCornerShape(50)).border(1.dp, Color.White.copy(alpha = 0.15f), RoundedCornerShape(50)).padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

@Composable
private fun Bullet(mark: String, text: String, color: Color) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(mark, color = color, fontSize = 13.sp, fontWeight = FontWeight.Bold)
        Text(text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f))
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EntryCard(r: EntryReview, loaded: List<Candle>?, open: (Asset) -> Unit, onNote: (String, String) -> Unit, onDelete: (String) -> Unit) {
    val e = r.entry
    val d = e.decision
    val buy = e.buy
    val m = e.market
    var editing by remember(e.id) { mutableStateOf(false) }
    var note by remember(e.id, e.note) { mutableStateOf(e.note) }
    var details by rememberSaveable(e.id) { mutableStateOf(false) }
    var confirm by remember(e.id) { mutableStateOf(false) }
    val glow = when (r.coherent) { true -> AltimColors.buy; false -> AltimColors.warning; null -> AltimColors.cyan }
    Card(glow = glow) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f).clickable(role = Role.Button, onClickLabel = "Ouvrir ${e.symbol}") { open(e.asset) }) {
                Text("${if (buy) "Achat" else "Vente"} · ${e.name.ifEmpty { e.symbol }}", fontWeight = FontWeight.Bold, fontSize = 15.sp, color = Color.White)
                Text("${e.symbol} · ${frDate(e.createdAt, true)}", style = mono(12.sp), color = AltimColors.textSecondary)
            }
            Badge(if (e.source == "paper") "simulé" else "réel", if (e.source == "paper") Tone.GOOD else Tone.NEUTRAL)
        }
        val figures = mutableListOf(Triple("Prix", Format.price(e.price), Color.White))
        if (buy) {
            figures += Triple("Stop${if (r.levels.stopSource == "plan") " (plan)" else ""}", r.levels.stop?.let { Format.price(it) } ?: "aucun", Color.White)
            figures += Triple("Objectifs${if (r.levels.targetsSource == "plan") " (plan)" else ""}", if (r.levels.targets.isNotEmpty()) r.levels.targets.joinToString(" · ") { Format.price(it) } else "aucun", Color.White)
            figures += Triple("Gain visé", rText(r.planR), Color.White)
        }
        Figures(figures)
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(color = AltimColors.textSecondary)) { append("Signal utilisé : ") }
                append(e.signal)
            },
            fontSize = 13.sp, color = Color.White,
        )

        TextButton(onClick = { details = !details }) { Text("Pourquoi, et le contexte de marché ${if (details) "▲" else "▼"}", color = AltimColors.cyan, fontSize = 14.sp) }
        if (details) {
            if (d != null) {
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(d.ratingLabel ?: d.label) }
                        append(" · confiance ${Math.round(d.confidence)}/100${d.score?.let { " · score ${Math.round(it)}/100" } ?: ""} · décision du ${frDate(d.asOf, true)}")
                    },
                    fontSize = 13.sp, color = Color.White,
                )
                if (d.headline.isNotBlank()) Text(d.headline, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
                if (d.degraded) Notice("Signal dégradé : ${d.degradedHeadline ?: ""}", Tone.WARN)
                if (d.vetoes.isNotEmpty()) Notice("Interdictions d'achat actives : ${d.vetoes.joinToString(", ")}", Tone.BAD)
                d.pros.forEach { Bullet("＋", it, AltimColors.buy) }
                d.cons.forEach { Bullet("－", it, AltimColors.sell) }
                Caption("Configuration « ${d.setup.name} » : ${d.setup.met}/${d.setup.total}")
                if (d.setup.steps.isNotEmpty()) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        d.setup.steps.forEach { s -> Chip("${stepIcon(s.state)} ${s.label}") }
                    }
                }
                d.plan?.let { p ->
                    Caption(
                        "Plan : zone ${Format.price(minOf(p.zoneFrom, p.zoneTo))} – ${Format.price(maxOf(p.zoneFrom, p.zoneTo))}, stop ${Format.price(p.stop)}, objectif 1 ${Format.price(p.target1)}" +
                            (p.target2?.let { ", objectif 2 ${Format.price(it)}" } ?: "") + ", rapport gain / risque ${fr(p.riskReward)}.",
                    )
                }
            } else {
                Caption("Aucune décision chargée pour cet actif à ce moment-là.")
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Chip("Régime : ${m.regimeLabel ?: "inconnu"}")
                Chip("Stress macro : ${m.macroScore?.let { "${Math.round(it)}/100 (${MACRO_LABEL[m.macroLevel ?: ""] ?: m.macroLevel})" } ?: "inconnu"}")
                Chip("ATR : ${m.atrPct?.let { "${fr(it, 2)} %/jour" } ?: "inconnu"}")
                Chip("Volume relatif : ${m.relativeVolume?.let { "${fr(it, 2)}×" } ?: "inconnu"}")
                Chip("Événements à 7 j : ${m.events ?: "inconnu"}")
            }
        }

        if (editing) {
            JournalNoteField(note) { note = it }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = {
                    onNote(e.id, note)
                    editing = false
                }) { Text("Enregistrer", color = AltimColors.cyan) }
                TextButton(onClick = {
                    note = e.note
                    editing = false
                }) { Text("Annuler", color = AltimColors.textSecondary) }
            }
        } else {
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(if (buy) "Pourquoi je suis entré : " else "Pourquoi j'ai vendu : ") }
                    if (e.note.isNotEmpty()) append(e.note) else withStyle(SpanStyle(color = AltimColors.textSecondary, fontStyle = FontStyle.Italic)) { append("pas de note") }
                },
                fontSize = 13.sp, color = Color.White,
            )
            TextButton(onClick = { editing = true }) { Text(if (e.note.isNotEmpty()) "Modifier" else "Ajouter", color = AltimColors.cyan) }
        }

        HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
        Text("Revue automatique", fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Color.White, modifier = Modifier.semantics { heading() })
        if (loaded == null) Notice("Cours journaliers indisponibles pour l'instant : revue non calculée.", Tone.WARN)
        if (loaded === LOADING) Caption("Chargement des cours…")
        r.horizons.forEach { h -> Bullet("•", horizonText(h, buy), AltimColors.textSecondary) }
        r.closed?.let { c ->
            Text(
                "Clôturée le ${frDate(c.at)} (${c.reason}) à ${Format.price(c.price)}${r.realizedR?.let { " : ${rText(it)} réalisé" } ?: ""}.",
                fontSize = 13.sp, color = Color.White,
            )
        }
        if (r.worked.isNotEmpty()) {
            Text("Qu'est-ce qui a fonctionné ?", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
            r.worked.forEach { Bullet("✔", it, AltimColors.buy) }
        }
        if (r.failed.isNotEmpty()) {
            Text("Qu'est-ce qui n'a pas fonctionné ?", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
            r.failed.forEach { Bullet("⚠", it, AltimColors.warning) }
        }
        Text("Le signal était-il cohérent avec les données disponibles à ce moment-là ?", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(when (r.coherent) { null -> "Non vérifiable"; true -> "Oui"; false -> "Non" }) }
                if (r.coherent == false) append(" : au moins un point ci-dessous ne l'était pas.")
            },
            fontSize = 13.sp, color = Color.White,
        )
        r.coherence.forEach { c ->
            Row(Modifier.semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    when (c.ok) { null -> "?"; true -> "✓"; false -> "✕" }, fontSize = 13.sp, fontWeight = FontWeight.Bold,
                    color = when (c.ok) { null -> AltimColors.textSecondary; true -> AltimColors.buy; false -> AltimColors.warning },
                )
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(c.label) }
                        append(" — ${c.detail}")
                    },
                    fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f),
                )
            }
        }
        TextButton(onClick = { confirm = true }) { Text("Supprimer", color = AltimColors.sell) }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Supprimer cette entrée du journal (${e.name.ifEmpty { e.symbol }}) ?") },
            confirmButton = {
                TextButton(onClick = {
                    confirm = false
                    onDelete(e.id)
                }) { Text("Supprimer", color = AltimColors.sell) }
            },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text("Annuler") } },
            containerColor = AltimColors.surface,
        )
    }
}
