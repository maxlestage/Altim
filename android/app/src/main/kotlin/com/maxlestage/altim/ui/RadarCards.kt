package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Report
import androidx.compose.material3.Icon
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.semantics
import com.maxlestage.altim.kit.Danger
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
    DangerNotice(state.items, state.at)
}

/**
 * Positions that became dangerous (stop broken or close, loss beyond the risk per idea). On the Radar ([measuredAt]):
 * with the reasons and when it was measured; on Mes avoirs: the advice (the detail is on each line).
 */
@Composable
fun DangerNotice(dangers: List<Danger>, measuredAt: Double? = null) {
    val shape = androidx.compose.foundation.shape.RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.sell.copy(alpha = 0.1f)).border(1.dp, AltimColors.sell.copy(alpha = 0.4f), shape).padding(12.dp)
            .semantics(mergeDescendants = true) {},
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(Icons.Filled.Report, contentDescription = null, tint = AltimColors.sell, modifier = Modifier.size(18.dp))
            Text(
                "${if (dangers.size > 1) "Positions devenues dangereuses" else "Position devenue dangereuse"}${if (measuredAt == null) "" else " dans vos avoirs"} : ${dangers.joinToString(", ") { it.symbol }}",
                fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White,
            )
        }
        if (measuredAt != null) {
            dangers.forEach { d -> Text("${d.symbol} : ${d.reasons.joinToString(" ") { it.text }}", fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f)) }
            Text("Mesuré le ${Format.date(measuredAt, time = true)} sur Mes avoirs (ouvrez-le pour actualiser).", fontSize = 11.sp, color = AltimColors.textSecondary)
        } else {
            Caption("Détail sur chaque ligne ci-dessous. Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque).")
        }
    }
}

private fun transitionColor(t: ConfigTransition): Color = when (t.to.verdict) {
    Verdict.BUY, Verdict.BUY_ZONE -> AltimColors.buy
    Verdict.SELL, Verdict.TRIM -> AltimColors.sell
    else -> AltimColors.cyan
}

/** "Changements de configuration · N": newest first, 5 shown then all on demand, cleared after a confirmation. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ConfigChangesCard(transitions: List<ConfigTransition>, open: (Asset) -> Unit, backgroundAlerts: Boolean = false, onClear: () -> Unit) {
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
                        "${Format.date(t.at, time = true)} · niveau ${t.from.levelLabel} → ${t.to.levelLabel} · configuration précédente vue le ${Format.date(t.since, time = true)}" +
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
            "Comparaison avec la dernière décision vue sur ce téléphone. Nouvelle analyse toutes les 15 minutes tant que le radar est ouvert, à chaque ouverture d'une fiche" +
                (if (backgroundAlerts) ", et en arrière-plan quand Android le permet (notification « Changements de configuration »)" else "") +
                ". 50 derniers changements conservés ici uniquement.",
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
