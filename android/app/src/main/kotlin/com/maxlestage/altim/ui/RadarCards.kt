package com.maxlestage.altim.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.ConfigTransition
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Verdict

// Radar cards of web/src/webapp/Radar.tsx: the positions that became dangerous (measured on Mes avoirs) and the
// configuration changes of the decisions seen on this phone.

/** Last dangers measured on Mes avoirs, with the time: the Radar does not recompute them. */
@Composable
fun DangersNotice(state: PortfolioRisk.DangerState) {
    if (state.items.isEmpty()) return
    val title = if (state.items.size > 1) "Positions devenues dangereuses" else "Position devenue dangereuse"
    Notice(
        "⚠ $title dans vos avoirs\n" +
            state.items.joinToString("\n") { d -> "• ${d.symbol} : ${d.reasons.joinToString(" ") { it.text }}" } +
            "\nMesuré le ${Format.shortDateTime(state.at)} sur Mes avoirs (ouvrez-le pour actualiser).",
        Tone.BAD,
    )
}

private fun transitionColor(t: ConfigTransition): Color = when (t.to.verdict) {
    Verdict.BUY, Verdict.BUY_ZONE -> AltimColors.buy
    Verdict.SELL, Verdict.TRIM -> AltimColors.sell
    else -> AltimColors.cyan
}

/** "Changements de configuration · N": newest first, 5 shown then all on demand, cleared after a confirmation. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ConfigChangesCard(transitions: List<ConfigTransition>, open: (Asset) -> Unit, onClear: () -> Unit) {
    if (transitions.isEmpty()) return
    var showAll by rememberSaveable { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    Card(title = "Changements de configuration · ${transitions.size}") {
        (if (showAll) transitions else transitions.take(5)).forEach { t ->
            val c = transitionColor(t)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("↻", color = c, fontSize = 15.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, modifier = Modifier.widthIn(min = 18.dp))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        t.title,
                        fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = c,
                        modifier = Modifier.clickable(role = Role.Button, onClickLabel = "Ouvrir ${t.symbol}") { open(Asset(t.symbol, t.kind, t.name)) },
                    )
                    Caption(
                        "${Format.shortDateTime(t.at)} · niveau ${t.from.levelLabel} → ${t.to.levelLabel} · configuration précédente vue le ${Format.shortDateTime(t.since)}" +
                            if (t.personal) " · mode personnel" else "",
                    )
                    if (t.changes.isNotEmpty()) Text("Pourquoi le signal a changé : ${t.changes.joinToString(" ; ")}.", fontSize = 12.sp, color = Color.White.copy(alpha = 0.88f))
                    if (t.missing.isNotEmpty()) Text("Conditions manquantes : ${t.missing.joinToString(" ; ")}.", fontSize = 12.sp, color = Color.White.copy(alpha = 0.88f))
                    if (t.triggers.isNotEmpty()) Text("Ce qui changerait la décision : ${t.triggers.joinToString(" ; ")}.", fontSize = 12.sp, color = Color.White.copy(alpha = 0.88f))
                }
            }
        }
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (transitions.size > 5) TextButton(onClick = { showAll = !showAll }) { Text(if (showAll) "Voir moins" else "Voir les ${transitions.size}", color = AltimColors.cyan) }
            TextButton(onClick = { confirm = true }) { Text("Effacer", color = AltimColors.cyan) }
        }
        Caption(
            "Comparaison avec la dernière décision vue sur ce téléphone. Nouvelle analyse toutes les 15 minutes tant que le radar est ouvert, et à chaque ouverture d'une fiche. " +
                "50 derniers changements conservés ici uniquement.",
        )
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Effacer l'historique des changements ?") },
            confirmButton = {
                TextButton(onClick = {
                    confirm = false
                    onClear()
                }) { Text("Effacer", color = AltimColors.sell) }
            },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text("Annuler") } },
            containerColor = AltimColors.surface,
        )
    }
}
