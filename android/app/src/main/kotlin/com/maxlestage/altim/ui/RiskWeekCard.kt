package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.Calendar
import com.maxlestage.altim.kit.Tone

// "Calendrier de risque · 7 jours" at the top of the Agenda (web Agenda.tsx `RiskWeek`): one stacked row per day,
// weekends included, from its own request (the user's stocks and the largest companies together, `top=1`).

private fun riskColor(d: Calendar.RiskDay): Color = when {
    d.unknown -> AltimColors.textSecondary
    d.level == Calendar.RiskLevel.HIGH -> AltimColors.sell
    d.level == Calendar.RiskLevel.MEDIUM -> AltimColors.orange
    else -> AltimColors.buy
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun RiskWeekCard(days: List<Calendar.RiskDay>?, error: String?) {
    Card(title = "Calendrier de risque · 7 jours") {
        error?.let { Notice(it, Tone.WARN) }
        if (days == null && error == null) Caption("Chargement…")
        days?.forEach { d ->
            val c = riskColor(d)
            val shape = RoundedCornerShape(10.dp)
            val label = if (d.unknown) "Risque non évalué" else d.level.label
            Column(
                Modifier.fillMaxWidth().clip(shape).background(c.copy(alpha = 0.06f)).border(1.dp, c.copy(alpha = 0.35f), shape).padding(horizontal = 10.dp, vertical = 8.dp)
                    .semantics(mergeDescendants = true) {},
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                    Text(d.label, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = Color.White)
                    if (d.weekend) Badge("week-end", Tone.NEUTRAL)
                    Text(if (d.unknown) "⚪" else d.level.icon, fontSize = 13.sp, modifier = Modifier.semantics { contentDescription = label })
                    Text(Calendar.riskText(d), fontSize = 13.sp, color = if (d.level == Calendar.RiskLevel.LOW) AltimColors.textSecondary else Color.White)
                }
                if (d.incomplete && !d.unknown) Caption("Sources incomplètes ce jour-là.")
            }
        }
        var rule by rememberSaveable { mutableStateOf(false) }
        TextButton(onClick = { rule = !rule }) { Text("Règle ${if (rule) "▲" else "▼"}", color = AltimColors.cyan, fontSize = 13.sp) }
        if (rule) Caption(Calendar.RISK_RULE)
    }
}
