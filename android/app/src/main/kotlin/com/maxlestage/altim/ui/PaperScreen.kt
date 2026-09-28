package com.maxlestage.altim.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
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
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.OpenLine
import com.maxlestage.altim.kit.OpenOrder
import com.maxlestage.altim.kit.Paper
import com.maxlestage.altim.kit.PaperDecision
import com.maxlestage.altim.kit.PaperReason
import com.maxlestage.altim.kit.PaperState
import com.maxlestage.altim.kit.PaperStats
import com.maxlestage.altim.kit.PaperTrade
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Verdict
import com.maxlestage.altim.kit.paperStats
import com.maxlestage.altim.kit.valuation
import java.util.UUID
import kotlin.math.abs
import kotlin.math.floor

/** Under this number of closed trades, the statistics say little (chance dominates). */
private const val FEW_TRADES = 20

/** 1 234,56 $ (amounts of the simulation, cents kept: the fees are small), 0,50 $ under a dollar. */
private fun usd(v: Double?): String =
    if (v != null && v.isFinite() && abs(v) < 1) String.format(java.util.Locale.FRANCE, "%.2f $", v) else Format.price(v)
private fun signedUsd(v: Double): String = "${if (v >= 0) "+" else "−"}${usd(abs(v))}"

/** Suggested amount of a simulated purchase: 10 % of the simulated value, at most the cash (cents rounded down). */
fun suggestedAmount(paper: PaperState, prices: Map<String, Double?>): Double =
    floor(minOf(valuation(paper, prices).equity * 0.1, paper.cash) * 100).coerceAtLeast(0.0) / 100

private fun verdictTone(verdict: String): Tone = when (verdict) {
    "buy", "buyZone" -> Tone.GOOD
    "trim", "sell" -> Tone.BAD
    else -> Tone.NEUTRAL
}

private fun reasonTone(r: PaperReason): Tone = when (r) {
    PaperReason.TARGET -> Tone.GOOD
    PaperReason.STOP -> Tone.BAD
    PaperReason.MANUAL -> Tone.NEUTRAL
}

/**
 * "Mes avoirs → Simulation": prices of the simulated positions (live, else the consensus quote), then the
 * automatic exits (daily candles, each time the part is shown and when the app comes back to the foreground).
 */
@Composable
fun PaperPane(model: AppModel, modifier: Modifier = Modifier) {
    val quotes = remember { mutableStateMapOf<String, Double>() }
    var error by remember { mutableStateOf<String?>(null) }
    val paper = model.paper
    val ids = paper.positions.joinToString { it.id }

    LaunchedEffect(ids, model.resumeCount) {
        val client = model.client ?: return@LaunchedEffect
        val assets = paper.positions.map { it.asset }.distinctBy { it.id }
        if (assets.isEmpty()) return@LaunchedEffect
        try {
            client.quotes(assets).forEach { quotes["${it.kind.raw}:${it.symbol}"] = it.price }
            error = null
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
            return@LaunchedEffect
        } catch (e: Exception) {
            error = e.message
        }
        try {
            model.checkPaperExits()
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (_: Exception) {
            // Checked again next time (the candles of each asset are fetched independently).
        }
    }

    val prices: Map<String, Double?> = quotes.toMutableMap<String, Double?>().apply {
        paper.positions.forEach { p -> model.live.price(p.asset)?.let { put(p.key, it.price) } }
    }
    PaperView(
        state = paper,
        prices = prices,
        justClosed = model.paperJustClosed,
        error = error,
        modifier = modifier,
        onReset = model::paperReset,
        onSell = { line -> line.price?.let { model.paperSell(line.id, it) } ?: "Prix indisponible." },
        onDismissClosed = { model.paperJustClosed = emptyList() },
    )
}

/** The whole Simulation part, from a state and prices (no server: also rendered by the tests). */
@Composable
fun PaperView(
    state: PaperState,
    prices: Map<String, Double?>,
    justClosed: List<PaperTrade>,
    error: String?,
    modifier: Modifier = Modifier,
    onReset: (Double) -> Unit,
    onSell: (OpenLine) -> String?,
    onDismissClosed: () -> Unit,
) {
    val v = valuation(state, prices)
    val stats = paperStats(state)
    var resetOpen by remember { mutableStateOf(false) }
    var selling by remember { mutableStateOf<OpenLine?>(null) }
    var sellError by remember { mutableStateOf<String?>(null) }

    LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp), modifier = modifier) {
        item {
            Card(title = "Portefeuille simulé", glow = AltimColors.violet) {
                Notice("Portefeuille simulé — aucun argent réel, aucun ordre passé.", Tone.NEUTRAL)
                Caption("Valeur si tout était vendu maintenant")
                Text(usd(v.equity), style = mono(28.sp, FontWeight.Bold), modifier = Modifier.semantics { contentDescription = "Valeur simulée ${usd(v.equity)}" })
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(signedUsd(v.pnl), style = mono(14.sp), color = AltimColors.forChange(v.pnl))
                    ChangeText(v.pnlPct)
                    Caption("depuis le départ")
                }
                KeyValue("Capital de départ", usd(state.startCapital))
                KeyValue("Liquidités", usd(v.cash))
                KeyValue("Positions ouvertes", usd(v.positionsValue))
                Caption("Simulation commencée le ${Format.date(state.startedAt)}.")
                if (v.unpriced > 0) {
                    Caption(if (v.unpriced > 1) "${v.unpriced} positions sans prix pour l'instant : comptées à leur coût." else "1 position sans prix pour l'instant : comptée à son coût.")
                }
                if (resetOpen) {
                    ResetPanel(state.startCapital, onDismiss = { resetOpen = false }) {
                        resetOpen = false
                        onReset(it)
                    }
                } else {
                    OutlinedButton(onClick = { resetOpen = true }, modifier = Modifier.fillMaxWidth(), border = BorderStroke(1.2.dp, AltimColors.cyan)) {
                        Text("Recommencer", color = AltimColors.cyan)
                    }
                }
            }
        }
        if (justClosed.isNotEmpty()) item { JustClosed(justClosed, onDismissClosed) }
        error?.let { item { Notice(it, Tone.BAD) } }

        item { ListTitle("POSITIONS OUVERTES · ${v.lines.size}") }
        if (v.lines.isEmpty()) {
            item { Caption("Aucune position simulée. Sur la fiche d'un actif, « Simuler cet achat » sous la décision ouvre une position ici.") }
        }
        items(v.lines, key = { "open-${it.id}" }) { line ->
            OpenPosition(line) {
                sellError = null
                selling = line
            }
        }

        item { ListTitle("JOURNAL DES VENTES · ${state.trades.size}") }
        if (state.trades.isEmpty()) item { Caption("Aucune vente pour l'instant.") }
        items(state.trades.sortedByDescending { it.closedAt }, key = { "trade-${it.id}-${it.closedAt}" }) { TradeRow(it) }

        item { StatsCard(stats) }
        item { HowItWorks() }
    }

    selling?.let { line ->
        SellDialog(line, sellError, onDismiss = { selling = null }) {
            val e = onSell(line)
            if (e == null) selling = null else sellError = e
        }
    }
}

@Composable
private fun ListTitle(text: String) =
    Text(text, color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp).semantics { heading() })

@Composable
private fun JustClosed(trades: List<PaperTrade>, onDismiss: () -> Unit) {
    Card(title = "Fermé automatiquement", glow = AltimColors.warning) {
        trades.forEach { t ->
            Notice(
                "${t.symbol} : ${t.reason.label} le ${Format.date(t.closedAt)}, vendu ${usd(t.exit)} · ${signedUsd(t.pnl)} (${Format.percent(t.pnlPct)})",
                if (t.pnl > 0) Tone.GOOD else Tone.WARN,
            )
        }
        TextButton(onClick = onDismiss) { Text("J'ai vu", color = AltimColors.cyan) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun DecisionLine(d: PaperDecision?) {
    if (d == null) {
        Caption("Ouverte sans décision affichée.")
        return
    }
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
        itemVerticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.semantics(mergeDescendants = true) {
            contentDescription = "Décision à l'achat : ${d.label}, confiance ${Math.round(d.confidence)} sur 100, du ${Format.date(d.asOf)}"
        },
    ) {
        Caption("Décision à l'achat")
        Badge(d.label, verdictTone(d.verdict))
        Caption("confiance ${Math.round(d.confidence)}/100 · ${Format.date(d.asOf)}")
    }
}

@Composable
private fun OpenPosition(line: OpenLine, onSell: () -> Unit) {
    Card {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) {
                Text(line.symbol, style = mono(16.sp, FontWeight.Bold))
                Text(line.name, color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(line.value?.let { usd(it) } ?: "—", style = mono(15.sp))
                val pnl = line.pnl
                if (pnl != null) {
                    Text("${signedUsd(pnl)} · ${Format.percent(line.pnlPct)}", style = mono(12.sp), color = AltimColors.forChange(pnl), textAlign = TextAlign.End)
                } else {
                    Caption("prix indisponible")
                }
            }
        }
        DecisionLine(line.decision)
        KeyValue("Achetée le", Format.date(line.openedAt))
        KeyValue("Prix d'entrée", usd(line.entry))
        KeyValue("Prix actuel", usd(line.price))
        KeyValue("Quantité", Format.quantity(line.quantity))
        KeyValue("Investi", usd(line.invested))
        KeyValue("Stop", line.stop?.let { usd(it) } ?: "aucun", if (line.stop != null) Tone.BAD else null)
        KeyValue("Objectif", line.target?.let { usd(it) } ?: "aucun", if (line.target != null) Tone.GOOD else null)
        OutlinedButton(
            onClick = onSell,
            enabled = line.price != null,
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Vendre ${line.symbol} (simulé)" },
            border = BorderStroke(1.2.dp, if (line.price != null) AltimColors.sell else AltimColors.textSecondary),
        ) { Text("Vendre (simulé)", color = if (line.price != null) AltimColors.sell else AltimColors.textSecondary) }
    }
}

@Composable
private fun TradeRow(t: PaperTrade) {
    Column(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(t.symbol, style = mono(14.sp, FontWeight.Bold))
            Badge(t.reason.label.uppercase(), reasonTone(t.reason))
            Box(Modifier.weight(1f))
            Text(signedUsd(t.pnl), style = mono(13.sp), color = AltimColors.forChange(t.pnl))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Caption("${usd(t.entry)} → ${usd(t.exit)}")
            Box(Modifier.weight(1f))
            ChangeText(t.pnlPct)
        }
        Caption("Du ${Format.date(t.openedAt)} au ${Format.date(t.closedAt)} · décision à l'achat : ${t.decision?.label ?: "aucune"}")
        HorizontalDivider(color = Color.White.copy(alpha = 0.08f), modifier = Modifier.padding(top = 4.dp))
    }
}

@Composable
private fun StatsCard(s: PaperStats) {
    Card(title = "Statistiques") {
        if (s.trades < FEW_TRADES) {
            Notice(
                "${s.trades} vente${if (s.trades > 1) "s" else ""} seulement : sous une vingtaine de trades, ces chiffres ne veulent pas dire grand-chose (le hasard domine).",
                Tone.WARN,
            )
        }
        KeyValue("Ventes", "${s.trades}")
        KeyValue("Taux de réussite", if (s.trades > 0) "${Format.plain(s.winRate, 1)} %" else "—")
        KeyValue("Gain moyen", Format.percent(s.avgWinPct), s.avgWinPct?.let { Tone.GOOD })
        KeyValue("Perte moyenne", Format.percent(s.avgLossPct), s.avgLossPct?.let { Tone.BAD })
        KeyValue("Profit factor", s.profitFactor?.let { Format.plain(it, 2) } ?: "—")
        KeyValue("Résultat réalisé", signedUsd(s.realizedPnl), if (s.realizedPnl >= 0) Tone.GOOD else Tone.BAD)
        s.best?.let { KeyValue("Meilleure vente", "${it.symbol} ${Format.percent(it.pnlPct)}", Tone.GOOD) }
        s.worst?.takeIf { s.trades > 1 }?.let { KeyValue("Pire vente", "${it.symbol} ${Format.percent(it.pnlPct)}", if (it.pnl > 0) Tone.GOOD else Tone.BAD) }
        KeyValue("Pire recul du capital réalisé", Format.percent(s.maxDrawdownPct), if (s.maxDrawdownPct < 0) Tone.BAD else null)
        KeyValue("Objectif atteint", "${s.byReason.target}")
        KeyValue("Stop touché", "${s.byReason.stop}")
        KeyValue("Vente manuelle", "${s.byReason.manual}")
        if (s.byVerdict.isNotEmpty()) {
            HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
            Text("Résultats par décision affichée à l'achat", fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.semantics { heading() })
            VerdictTable(s)
            Caption("Pour voir quelles décisions ont vraiment marché : résultat moyen de chaque vente, frais compris.")
        }
    }
}

@Composable
private fun VerdictTable(s: PaperStats) {
    val weights = listOf(1.6f, 0.9f, 1f, 1.1f)
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth()) {
            listOf("Décision", "Ventes", "Réussite", "Moyenne").forEachIndexed { i, h ->
                Text(h, color = AltimColors.textSecondary, fontSize = 11.sp, textAlign = if (i == 0) TextAlign.Start else TextAlign.End, modifier = Modifier.weight(weights[i]))
            }
        }
        s.byVerdict.forEach { v ->
            Row(
                Modifier.fillMaxWidth().semantics(mergeDescendants = true) {
                    contentDescription = "${v.label} : ${v.trades} vente${if (v.trades > 1) "s" else ""}, ${Format.plain(v.winRate, 0)} % de réussite, moyenne ${Format.percent(v.avgPnlPct)}"
                },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(v.label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.of(verdictTone(v.verdict)).takeIf { v.verdict != "none" && v.verdict != "wait" } ?: Color.White, modifier = Modifier.weight(weights[0]))
                Text("${v.trades}", style = mono(12.sp), textAlign = TextAlign.End, modifier = Modifier.weight(weights[1]))
                Text("${Format.plain(v.winRate, 0)} %", style = mono(12.sp), textAlign = TextAlign.End, modifier = Modifier.weight(weights[2]))
                Text(Format.percent(v.avgPnlPct, 1), style = mono(12.sp), color = AltimColors.forChange(v.avgPnlPct), textAlign = TextAlign.End, modifier = Modifier.weight(weights[3]))
            }
        }
    }
}

@Composable
private fun HowItWorks() {
    Card(title = "Comment c'est calculé") {
        listOf(
            "Chaque achat et chaque vente paient 0,1 % de frais et 0,05 % de glissement (le prix obtenu est un peu moins bon que le prix affiché).",
            "Un stop ou un objectif n'est vérifié que sur les bougies journalières qui suivent le jour de l'achat : le plus bas du jour de l'achat a pu avoir lieu avant.",
            "Si une même journée touche le stop et l'objectif, c'est le stop qui compte (le pire cas : l'ordre dans la journée est inconnu).",
            "Une ouverture sous le stop est vendue au prix d'ouverture ; un objectif est vendu à l'objectif, jamais mieux.",
            "La valeur des positions ouvertes est celle d'une vente maintenant, frais et glissement déduits.",
            "Tout reste sur ce téléphone. Aucun ordre n'est passé, nulle part.",
        ).forEach { t ->
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("•", color = AltimColors.textSecondary, fontSize = 13.sp)
                Text(t, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun AmountField(value: String, label: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, autoCorrectEnabled = false),
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan, focusedLabelColor = AltimColors.cyan),
    )
}

/** Confirmation of "Recommencer", inside the card: the new starting capital (10 000 $ by default). */
@Composable
private fun ResetPanel(current: Double, onDismiss: () -> Unit, onConfirm: (Double) -> Unit) {
    var text by remember { mutableStateOf(Format.plain(Paper.DEFAULT_CAPITAL, 2)) }
    val capital = Format.parse(text)?.takeIf { it > 0 }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
        Text("Recommencer la simulation ?", fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.semantics { heading() })
        Text("Les positions et le journal simulés seront effacés (capital de départ actuel : ${usd(current)}).", fontSize = 13.sp)
        AmountField(text, "Capital de départ en $") { text = it }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onDismiss, modifier = Modifier.weight(1f)) { Text("Annuler", color = AltimColors.cyan) }
            OutlinedButton(
                onClick = { capital?.let(onConfirm) },
                enabled = capital != null,
                modifier = Modifier.weight(1f),
                border = BorderStroke(1.2.dp, if (capital != null) AltimColors.sell else AltimColors.textSecondary),
            ) { Text("Tout effacer", color = if (capital != null) AltimColors.sell else AltimColors.textSecondary) }
        }
    }
}

@Composable
private fun SellDialog(line: OpenLine, error: String?, onDismiss: () -> Unit, onConfirm: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Vendre ${line.symbol} (simulé) ?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    "Vente simulée au prix actuel ${usd(line.price)}, moins 0,05 % de glissement et 0,1 % de frais : environ ${usd(line.value)} " +
                        "(${line.pnl?.let { signedUsd(it) } ?: "—"}). Aucun ordre réel n'est passé.",
                    fontSize = 14.sp,
                )
                error?.let { Notice(it, Tone.BAD) }
            }
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Vendre (simulé)", color = AltimColors.sell) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
        containerColor = AltimColors.surface,
    )
}

/**
 * "Simuler cet achat" from the Décision card: amount (10 % of the simulated value by default, at most the cash),
 * stop and target of the plan prefilled; the decision shown is kept with the position.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SimulateSheet(model: AppModel, asset: Asset, decision: Decision, price: Double?, onDone: (String) -> Unit, onDismiss: () -> Unit) {
    val paper = model.paper
    val suggested = suggestedAmount(paper, paper.positions.associate { p -> p.key to model.live.price(p.asset)?.price })
    var amount by remember { mutableStateOf(if (suggested > 0) Format.plain(suggested, 2) else "") }
    var stop by remember { mutableStateOf(decision.plan?.stop?.let { Format.plain(it, 8) } ?: "") }
    var target by remember { mutableStateOf(decision.plan?.target1?.let { Format.plain(it, 8) } ?: "") }
    var error by remember { mutableStateOf<String?>(null) }
    var note by remember { mutableStateOf("") }
    val against = decision.verdict != Verdict.BUY && decision.verdict != Verdict.BUY_ZONE

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = AltimColors.surface) {
        SimulateForm(
            asset = asset, decision = decision, price = price, cash = paper.cash, against = against,
            amount = amount, stop = stop, target = target, error = error, note = note,
            onAmount = { amount = it }, onStop = { stop = it }, onTarget = { target = it }, onNote = { note = it.take(1000) }, onCancel = onDismiss,
        ) {
            val a = Format.parse(amount)
            val s = if (stop.isBlank()) null else Format.parse(stop)
            val t = if (target.isBlank()) null else Format.parse(target)
            error = when {
                a == null -> "Montant invalide."
                stop.isNotBlank() && s == null -> "Stop invalide."
                target.isNotBlank() && t == null -> "Objectif invalide."
                else -> {
                    val snapshot = PaperDecision(decision.verdict.serialName, decision.verdictLabel, decision.confidence, decision.asOf)
                    model.paperBuy(OpenOrder(UUID.randomUUID().toString(), asset.symbol, asset.kind, asset.name, price ?: Double.NaN, a, s, t, snapshot), decision, note)
                }
            }
            if (error == null) onDone("Achat simulé de ${usd(a)} de ${asset.symbol} enregistré : Mes avoirs → Simulation. Inscrit au journal.")
        }
    }
}

/** The form of the simulated purchase (also rendered by the tests). */
@Composable
fun SimulateForm(
    asset: Asset,
    decision: Decision,
    price: Double?,
    cash: Double,
    against: Boolean,
    amount: String,
    stop: String,
    target: String,
    error: String?,
    onAmount: (String) -> Unit,
    onStop: (String) -> Unit,
    onTarget: (String) -> Unit,
    note: String = "",
    onNote: (String) -> Unit = {},
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).imePadding().navigationBarsPadding(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Simuler l'achat de ${asset.symbol}", fontWeight = FontWeight.Bold, fontSize = 18.sp, modifier = Modifier.semantics { heading() })
        Caption("Portefeuille simulé — aucun argent réel, aucun ordre passé.")
        KeyValue("Prix actuel", usd(price))
        KeyValue("Liquidités simulées", usd(cash))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Caption("Décision affichée")
            Badge(decision.verdictLabel, verdictTone(decision.verdict.serialName))
            Caption("confiance ${Math.round(decision.confidence)}/100")
        }
        if (against) {
            Notice("La décision affichée est « ${decision.verdictLabel} » : vous simulez contre la décision.", Tone.WARN)
        }
        AmountField(amount, "Montant en $ (frais compris)", onAmount)
        AmountField(stop, "Stop en $ (facultatif)", onStop)
        AmountField(target, "Objectif en $ (facultatif)", onTarget)
        Caption("Stop et objectif pré-remplis depuis le plan de la décision. Vérifiés chaque jour sur les bougies journalières, à partir du lendemain.")
        // The engine ignores a stop above the fill price and a target below it: said before, not discovered after.
        val entry = price?.let { it * (1 + Paper.SLIPPAGE) }
        val s = Format.parse(stop)
        val t = Format.parse(target)
        if (entry != null && s != null && s >= entry) Notice("Stop au-dessus du prix d'achat : il sera ignoré.", Tone.WARN)
        if (entry != null && t != null && t <= entry) Notice("Objectif sous le prix d'achat : il sera ignoré.", Tone.WARN)
        JournalNoteField(note, onNote)
        error?.let { Notice(it, Tone.BAD) }
        NeonButton("SIMULER L'ACHAT", enabled = price != null, onClick = onConfirm)
        TextButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) { Text("Annuler", color = AltimColors.cyan) }
        Box(Modifier.height(16.dp))
    }
}

/** "buy", "buyZone"… (the server's value, kept with the position). */
val Verdict.serialName: String
    get() = when (this) {
        Verdict.BUY -> "buy"
        Verdict.BUY_ZONE -> "buyZone"
        Verdict.WAIT -> "wait"
        Verdict.NO_POSITION -> "noPosition"
        Verdict.TRIM -> "trim"
        Verdict.SELL -> "sell"
    }
