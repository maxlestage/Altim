package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.Currency
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.background
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
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

/** Dollar amounts of the simulation in the display currency, cents when there are some (iOS PaperFormat.usd): "10 509,12 €", "10 000 €". */
private fun usd(v: Double?): String = if (v == null || !v.isFinite()) "—" else Money.moneyFmt(v, " ") { Format.plain(it, 2) }

/** "Décision à l'achat : ACHETER · confiance 61 % · 28 septembre 2026" (iOS PaperFormat.decisionLine). */
fun paperDecisionLine(d: PaperDecision?): String =
    if (d == null) "Ouverte sans décision affichée" else "Décision à l'achat : ${d.label} · confiance ${Math.round(d.confidence)} % · ${Format.date(d.asOf)}"

/** The whole Simulation part, from a state and prices (no server: also rendered by the tests). Same order and texts as iOS PaperView. */
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
            Card(glow = AltimColors.warning) {
                Text("Portefeuille simulé — aucun argent réel, aucun ordre passé", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.warning)
                Caption("Valeur simulée")
                Text(usd(v.equity), style = mono(30.sp, FontWeight.Bold), maxLines = 1, modifier = Modifier.semantics { contentDescription = "Valeur simulée : ${usd(v.equity)}" })
                KeyValue("Résultat", "${signedUsd(v.pnl)} (${Format.percent(v.pnlPct)})", pnlTone(v.pnl))
                KeyValue("Capital de départ", usd(state.startCapital))
                KeyValue("Liquidités", usd(v.cash))
                KeyValue("Positions (si vendues maintenant)", usd(v.positionsValue))
                Caption("Depuis le ${Format.date(state.startedAt)}.")
                if (Money.displayCurrency() == Currency.EUR) {
                    Caption("Portefeuille simulé tenu en $ comme les cours ; montants saisis en € convertis au taux du jour de la saisie, affichés au taux du jour.")
                }
                Money.note()?.let { Caption(it) }
                if (v.unpriced > 0) {
                    Notice(if (v.unpriced == 1) "1 position sans prix pour l'instant : comptée à son coût." else "${v.unpriced} positions sans prix pour l'instant : comptées à leur coût.", Tone.WARN)
                }
                if (resetOpen) {
                    ResetPanel(onDismiss = { resetOpen = false }) {
                        resetOpen = false
                        onReset(it)
                    }
                } else {
                    OutlinedButton(
                        onClick = { resetOpen = true },
                        modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Recommencer. Efface la simulation et repart d'un capital à choisir" },
                        border = BorderStroke(1.2.dp, AltimColors.violet),
                    ) { Text("Recommencer", color = Color(0xFFB9A3FF)) }
                }
            }
        }
        if (justClosed.isNotEmpty()) item { JustClosed(justClosed, onDismissClosed) }
        error?.let { item { Notice(it, Tone.BAD) } }

        item { ListTitle("POSITIONS OUVERTES") }
        if (v.lines.isEmpty()) {
            item { Caption("Aucune position ouverte. Sur la page d'un actif, le bouton « Simuler cet achat » de la carte Décision ouvre une position fictive.") }
        }
        items(v.lines, key = { "open-${it.id}" }) { line ->
            OpenPosition(line) {
                sellError = null
                selling = line
            }
        }

        item { ListTitle("JOURNAL DES VENTES SIMULÉES") }
        if (state.trades.isEmpty()) item { Caption("Aucune position clôturée pour l'instant.") }
        items(state.trades.reversed(), key = { "trade-${it.id}-${it.closedAt}" }) { TradeRow(it) }

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

private fun pnlTone(v: Double): Tone = if (v > 0) Tone.GOOD else if (v < 0) Tone.BAD else Tone.NEUTRAL

@Composable
private fun ListTitle(text: String) =
    Text(text, color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp).semantics { heading() })

@Composable
private fun JustClosed(trades: List<PaperTrade>, onDismiss: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        trades.forEach { t ->
            Notice(
                "Clôturée automatiquement : ${t.symbol}, ${t.reason.label} le ${Format.date(t.closedAt)} à ${Format.price(t.exit)} → ${signedUsd(t.pnl)} (${Format.percent(t.pnlPct)}).",
                if (t.pnl > 0) Tone.GOOD else Tone.WARN,
            )
        }
        OutlinedButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth(), border = BorderStroke(1.2.dp, AltimColors.cyan)) { Text("J'ai vu", color = AltimColors.cyan) }
    }
}

@Composable
private fun OpenPosition(line: OpenLine, onSell: () -> Unit) {
    Card(glow = AltimColors.violet) {
        Row(Modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) {
                Text(line.symbol, style = mono(16.sp, FontWeight.Bold))
                Text(line.name, color = AltimColors.textSecondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            line.decision?.let { Badge(it.label, verdictTone(it.verdict)) }
        }
        Caption(paperDecisionLine(line.decision))
        KeyValue("Entrée (glissement inclus)", Format.price(line.entry))
        KeyValue("Prix actuel", Format.price(line.price))
        KeyValue("Quantité", Format.quantity(line.quantity))
        KeyValue("Investi (frais inclus)", usd(line.invested))
        KeyValue("Valeur si vendue", line.value?.let { usd(it) } ?: "sans prix : au coût")
        val pnl = line.pnl
        val pct = line.pnlPct
        if (pnl != null && pct != null) KeyValue("Résultat", "${signedUsd(pnl)} (${Format.percent(pct)})", pnlTone(pnl))
        KeyValue("Stop", line.stop?.let { Format.price(it) } ?: "aucun", if (line.stop != null) Tone.BAD else null)
        KeyValue("Objectif", line.target?.let { Format.price(it) } ?: "aucun", if (line.target != null) Tone.GOOD else null)
        Text("Ouverte le ${Format.date(line.openedAt, time = true)}.", fontSize = 11.sp, color = AltimColors.textSecondary)
        OutlinedButton(
            onClick = onSell,
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Vendre ${line.symbol} (simulé). Vente fictive de toute la position, après confirmation" },
            border = BorderStroke(1.2.dp, AltimColors.sell),
        ) { Text("Vendre (simulé)", color = AltimColors.sell) }
    }
}

@Composable
private fun TradeRow(t: PaperTrade) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface.copy(alpha = 0.6f)).padding(12.dp).semantics(mergeDescendants = true) {},
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(t.symbol, style = mono(15.sp, FontWeight.Bold))
            Text(
                t.reason.label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold,
                color = when (t.reason) { PaperReason.STOP -> AltimColors.sell; PaperReason.TARGET -> AltimColors.buy; PaperReason.MANUAL -> AltimColors.cyan },
                modifier = Modifier.weight(1f),
            )
            Text(signedUsd(t.pnl), style = mono(14.sp), color = AltimColors.of(pnlTone(t.pnl)))
        }
        Text("${Format.price(t.entry)} → ${Format.price(t.exit)} · ${Format.percent(t.pnlPct)} · vendue le ${Format.date(t.closedAt)}", style = mono(12.sp), color = AltimColors.textSecondary)
        Text(paperDecisionLine(t.decision), fontSize = 11.sp, color = AltimColors.textSecondary)
    }
}

@Composable
private fun StatsCard(s: PaperStats) {
    Card(title = "Statistiques", glow = AltimColors.cyan) {
        if (s.trades == 0) {
            Text("Les statistiques apparaissent après la première vente (manuelle, stop ou objectif).", fontSize = 13.sp, color = AltimColors.textSecondary)
            return@Card
        }
        if (s.trades < FEW_TRADES) {
            Notice(
                "Seulement ${s.trades} position${if (s.trades > 1) "s" else ""} clôturée${if (s.trades > 1) "s" else ""} : en dessous d'une vingtaine, ces chiffres ne veulent pas dire grand-chose (le hasard pèse plus que la méthode).",
                Tone.WARN,
            )
        }
        KeyValue("Positions clôturées", "${s.trades}")
        KeyValue("Gagnantes", "${s.wins} (${Format.plain(s.winRate, 1)} %)")
        KeyValue("Gain moyen", s.avgWinPct?.let { Format.percent(it) } ?: "—", s.avgWinPct?.let { Tone.GOOD })
        KeyValue("Perte moyenne", s.avgLossPct?.let { Format.percent(it) } ?: "—", s.avgLossPct?.let { Tone.BAD })
        KeyValue("Profit factor", s.profitFactor?.let { Format.plain(it, 2) } ?: "— (aucune perte)")
        KeyValue("Résultat réalisé", signedUsd(s.realizedPnl), pnlTone(s.realizedPnl))
        KeyValue("Pire recul du capital", Format.percent(s.maxDrawdownPct), if (s.maxDrawdownPct < 0) Tone.BAD else null)
        KeyValue("Sorties", "objectif ${s.byReason.target} · stop ${s.byReason.stop} · manuelle ${s.byReason.manual}")
        s.best?.let { KeyValue("Meilleure", "${it.symbol} ${Format.percent(it.pnlPct)}", pnlTone(it.pnlPct)) }
        s.worst?.takeIf { s.trades > 1 }?.let { KeyValue("Pire", "${it.symbol} ${Format.percent(it.pnlPct)}", pnlTone(it.pnlPct)) }
        Text("Résultats par décision affichée à l'achat", fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = Color.White, modifier = Modifier.padding(top = 4.dp).semantics { heading() })
        VerdictTable(s)
        Caption("Profit factor = somme des gains ÷ somme des pertes. Pire recul = plus forte baisse du capital réalisé, vente après vente.")
    }
}

/** The table while its columns fit the width at the current font size, else one block per decision (iOS ViewThatFits). */
@Composable
private fun VerdictTable(s: PaperStats) {
    val blocks = androidx.compose.ui.platform.LocalDensity.current.fontScale > 1.3f
    if (blocks) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            s.byVerdict.forEach { r ->
                Column(Modifier.semantics(mergeDescendants = true) {}) {
                    Text(r.label, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
                    Text("${r.trades} position${if (r.trades > 1) "s" else ""} · ${Format.plain(r.winRate, 0)} % gagnantes · moyenne ${Format.percent(r.avgPnlPct)}", fontSize = 13.sp, color = AltimColors.textSecondary)
                }
            }
        }
        return
    }
    val weights = listOf(1.6f, 0.9f, 1.1f, 1.1f)
    Column(
        Modifier.clearAndSetSemantics {
            contentDescription = s.byVerdict.joinToString(" ; ") { r ->
                "${r.label} : ${r.trades} position${if (r.trades > 1) "s" else ""}, ${Format.plain(r.winRate, 0)} % gagnantes, résultat moyen ${Format.percent(r.avgPnlPct)}"
            }
        },
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(Modifier.fillMaxWidth()) {
            listOf("Décision", "Nombre", "Gagnantes", "Moyenne").forEachIndexed { i, h ->
                Text(h, color = AltimColors.textSecondary, fontSize = 11.sp, textAlign = if (i == 0) TextAlign.Start else TextAlign.End, modifier = Modifier.weight(weights[i]))
            }
        }
        s.byVerdict.forEach { r ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(r.label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.weight(weights[0]))
                Text("${r.trades}", style = mono(12.sp), textAlign = TextAlign.End, modifier = Modifier.weight(weights[1]))
                Text("${Format.plain(r.winRate, 0)} %", style = mono(12.sp), textAlign = TextAlign.End, modifier = Modifier.weight(weights[2]))
                Text(Format.percent(r.avgPnlPct), style = mono(12.sp), color = AltimColors.of(pnlTone(r.avgPnlPct)), textAlign = TextAlign.End, modifier = Modifier.weight(weights[3]))
            }
        }
    }
}

@Composable
private fun HowItWorks() {
    Card(title = "Comment c'est calculé", glow = AltimColors.violet) {
        listOf(
            "Frais de 0,1 % à l'achat et à la vente, comme le suivi des signaux.",
            "Glissement de 0,05 % contre vous à chaque ordre : achat un peu plus cher, vente un peu moins cher que le prix affiché.",
            "Stop et objectif vérifiés sur les bougies journalières à partir du lendemain de l'achat : la bougie du jour d'achat n'est pas utilisée (son plus bas peut être antérieur à l'achat).",
            "Une bougie qui touche à la fois le stop et l'objectif compte comme le stop (le pire cas : l'ordre dans la journée n'est pas connu).",
            "Ouverture sous le stop (écart) : vente au prix d'ouverture ; objectif : vente à l'objectif, jamais mieux que prévu.",
            "Valeur des positions = ce que rapporterait une vente maintenant (glissement et frais déduits) ; sans prix, au coût.",
            "Tout reste sur ce téléphone. Aucun argent réel, aucun ordre n'est jamais passé.",
        ).forEach { Text("• $it", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
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

/** "Recommencer" (iOS PaperResetSheet): the starting capital (10 000 in the display currency by default), then a confirmation. */
@Composable
private fun ResetPanel(onDismiss: () -> Unit, onConfirm: (Double) -> Unit) {
    var text by remember { mutableStateOf(Format.plain(Paper.DEFAULT_CAPITAL, 0)) }
    var confirming by remember { mutableStateOf(false) }
    val typed = Format.parse(text)?.takeIf { it > 0 }
    val capitalUsd = Money.fromDisplay(typed ?: Paper.DEFAULT_CAPITAL).takeIf { it.isFinite() && it > 0 } ?: Paper.DEFAULT_CAPITAL
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
        Text("Recommencer la simulation", fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.semantics { heading() })
        AmountField(text, "Capital de départ en ${Money.symbol()}") { text = it }
        Caption("Par défaut 10 000 ${Money.symbol()}. Les positions ouvertes et le journal simulés seront effacés. Aucun argent réel n'est en jeu.")
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onDismiss, modifier = Modifier.weight(1f)) { Text("Annuler", color = AltimColors.cyan) }
            OutlinedButton(
                onClick = { confirming = true },
                enabled = typed != null,
                modifier = Modifier.weight(1f),
                border = BorderStroke(1.2.dp, if (typed != null) AltimColors.violet else AltimColors.textSecondary),
            ) { Text("Recommencer", color = if (typed != null) Color(0xFFB9A3FF) else AltimColors.textSecondary) }
        }
    }
    if (confirming) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            title = { Text("Effacer la simulation ?") },
            text = { Text("Toutes les positions et ventes simulées seront effacées.") },
            confirmButton = {
                TextButton(onClick = {
                    confirming = false
                    onConfirm(capitalUsd)
                }) { Text("Recommencer avec ${usd(capitalUsd)}", color = AltimColors.sell) }
            },
            dismissButton = { TextButton(onClick = { confirming = false }) { Text("Annuler") } },
            containerColor = AltimColors.surface,
        )
    }
}

@Composable
private fun SellDialog(line: OpenLine, error: String?, onDismiss: () -> Unit, onConfirm: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Vendre (simulé) ?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    "Vente fictive de toute la position ${line.symbol} au prix actuel (${Format.price(line.price)}), moins le glissement de 0,05 % et les frais de 0,1 %. Aucun ordre réel n'est passé.",
                    fontSize = 14.sp,
                )
                error?.let { Notice(it, Tone.BAD) }
            }
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Vendre ${line.symbol} (simulé)", color = AltimColors.sell) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
        containerColor = AltimColors.surface,
    )
}

/** The form of the simulated purchase (iOS PaperBuySheet; also rendered by the tests). */
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
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onCancel) { Text("Annuler", color = AltimColors.cyan) }
            Text("Simuler cet achat", fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, modifier = Modifier.weight(1f).semantics { heading() })
            TextButton(onClick = onConfirm) { Text("Simuler", color = AltimColors.cyan, fontWeight = FontWeight.Bold) }
        }
        KeyValue("${asset.symbol} · ${asset.name}", Format.price(price))
        KeyValue("Décision affichée", "${decision.verdictLabel} · confiance ${Math.round(decision.confidence)} %")
        KeyValue("Liquidités simulées", usd(cash))
        Caption("Achat fictif dans le portefeuille simulé (Mes avoirs › Simulation) : aucun argent réel, aucun ordre passé.")
        if (against) {
            Notice("La décision actuelle est « ${decision.verdictLabel} » : vous simulez contre la décision. C'est permis ; le résultat sera classé sous « ${decision.verdictLabel} » dans les statistiques.", Tone.WARN)
        }
        val sym = Money.symbol()
        Text("Montant", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.textSecondary)
        AmountField(amount, "Montant en $sym (frais inclus)", onAmount)
        Caption("Frais de 0,1 % et glissement de 0,05 % déduits, comme pour un vrai ordre.")
        Text("Sortie automatique", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.textSecondary)
        AmountField(stop, "Stop en $sym (facultatif)", onStop)
        AmountField(target, "Objectif en $sym (facultatif)", onTarget)
        Caption("Pré-remplis avec le stop et l'objectif 1 du plan. Vérifiés sur les bougies journalières à partir du lendemain ; un stop au-dessus du prix d'achat ou un objectif en dessous est ignoré.")
        JournalNoteField(note, onNote)
        error?.let { Notice(it, Tone.BAD) }
        Box(Modifier.height(16.dp))
    }
}

/** Under this number of closed trades, the statistics say little (chance dominates). */
private const val FEW_TRADES = 20
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

/**
 * "Simuler cet achat" from the Décision card: amount (10 % of the simulated value by default, at most the cash),
 * stop and target of the plan prefilled; the decision shown is kept with the position.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SimulateSheet(model: AppModel, asset: Asset, decision: Decision, price: Double?, onDone: (String) -> Unit, onDismiss: () -> Unit) {
    val paper = model.paper
    val suggested = suggestedAmount(paper, paper.positions.associate { p -> p.key to model.live.price(p.asset)?.price })
    // The simulated ledger is kept in dollars like the prices; amounts are typed and shown in the display currency.
    var amount by remember { mutableStateOf(if (suggested > 0) Format.plain(floor(Money.toDisplay(suggested) * 100) / 100, 2) else "") }
    var stop by remember { mutableStateOf(decision.plan?.stop?.let { Format.plain(Money.toDisplay(it), 8) } ?: "") }
    var target by remember { mutableStateOf(decision.plan?.target1?.let { Format.plain(Money.toDisplay(it), 8) } ?: "") }
    var error by remember { mutableStateOf<String?>(null) }
    var note by remember { mutableStateOf("") }
    val against = decision.verdict != Verdict.BUY && decision.verdict != Verdict.BUY_ZONE

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = AltimColors.surface) {
        SimulateForm(
            asset = asset, decision = decision, price = price, cash = paper.cash, against = against,
            amount = amount, stop = stop, target = target, error = error, note = note,
            onAmount = { amount = it }, onStop = { stop = it }, onTarget = { target = it }, onNote = { note = it.take(1000) }, onCancel = onDismiss,
        ) {
            val a = Format.parse(amount)?.let(Money::fromDisplay)?.takeIf { it.isFinite() }
            val s = if (stop.isBlank()) null else Format.parse(stop)?.let(Money::fromDisplay)?.takeIf { it.isFinite() }
            val t = if (target.isBlank()) null else Format.parse(target)?.let(Money::fromDisplay)?.takeIf { it.isFinite() }
            error = when {
                a == null -> "Montant invalide."
                // Said in the display currency (the engine's own message counts in dollars).
                a > paper.cash + 1e-9 -> "Liquidités simulées insuffisantes (${usd(paper.cash)} disponibles)."
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
