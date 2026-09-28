package com.maxlestage.altim.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.NewsSummary
import com.maxlestage.altim.kit.StorySummary
import com.maxlestage.altim.kit.Tone
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

// "Résumé" of the News tab (web News.tsx `Summary`): the day's important events from /api/news `summary`, each with its
// potential impact (by rule or measured), the assets concerned, the sources' consensus and the calculation. Chips wrap.

private fun openUrl(context: Context, url: String) {
    val safe = url.toHttpUrlOrNull()?.toString() ?: return
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, safe.toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    } catch (_: ActivityNotFoundException) {
        // No browser installed: nothing to open.
    }
}

private fun impactTone(impact: String) = when (impact) { "high" -> Tone.BAD; "medium" -> Tone.WARN; else -> Tone.NEUTRAL }

/** A "Détails" part opened on demand (the web's <details>). */
@Composable
private fun Details(title: String, content: @Composable () -> Unit) {
    var open by rememberSaveable(title) { mutableStateOf(false) }
    TextButton(onClick = { open = !open }, modifier = Modifier.heightIn(min = 40.dp)) {
        Text("$title ${if (open) "▲" else "▼"}", color = AltimColors.cyan, fontSize = 13.sp)
    }
    if (open) Column(verticalArrangement = Arrangement.spacedBy(6.dp)) { content() }
}

@Composable
fun NewsSummaryCard(list: List<StorySummary>, open: (Asset) -> Unit) {
    val head = NewsSummary.heading(list)
    Card(title = head.title, glow = AltimColors.cyan) {
        head.others?.let { Text(it, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
        Caption("Sujets des dernières 24 h repris par plusieurs sources indépendantes, escalades graves, ou articles citant vos actifs sur un sujet sensible. Impact potentiel indicatif, pas un signal.")
        list.forEach { SummaryEvent(it, open) }
        Details("Comment l'impact est estimé") {
            Caption(
                "Par règle : sources indépendantes, un même titre repris mot pour mot comptant pour une (2–3 : +1, 4 et plus : +2), thème (escalade grave +2 ; banques centrales, régulation ou piratage +1), " +
                    "cite un de vos actifs (+1). 0–1 point : faible, 2–3 : moyen, 4 et plus : important.",
            )
            Caption(
                "Mesuré : si les bougies horaires d'un actif cité sont déjà en mémoire sur le serveur, la plus forte variation depuis la clôture précédant la " +
                    "publication (action : 1 % moyen, 3 % important ; crypto : 2 % et 5 %). Une variation après un article ne prouve pas qu'il en est la cause.",
            )
            Caption("Consensus : ton des titres de chaque source (repérage par mots-clés) ; divergent dès qu'un titre est négatif et un autre positif.")
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SummaryEvent(s: StorySummary, open: (Asset) -> Unit) {
    val context = LocalContext.current
    val shape = RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.surface)
            .border(1.dp, if (s.alert) AltimColors.sell.copy(alpha = 0.6f) else Color.White.copy(alpha = 0.06f), shape)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            s.title, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = Color.White,
            modifier = Modifier.clickable(role = Role.Button, onClickLabel = "Lire l'article chez ${s.source}") { openUrl(context, s.link) },
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Badge("Impact potentiel ${NewsSummary.impactLabel(s.impact)}", impactTone(s.impact))
            Badge(NewsSummary.basisLabel(s), Tone.NEUTRAL)
        }
        if (s.assets.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Caption("Actifs concernés")
                s.assets.forEach { id ->
                    val a = NewsSummary.assetLink(id)
                    val asset = NewsSummary.asset(id)
                    Text(
                        a.symbol, fontSize = 12.sp, color = AltimColors.cyan, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clip(RoundedCornerShape(50)).background(AltimColors.cyan.copy(alpha = 0.1f))
                            .then(if (asset != null) Modifier.clickable(role = Role.Button, onClickLabel = "Ouvrir ${a.symbol}") { open(asset) } else Modifier)
                            .padding(horizontal = 10.dp, vertical = 4.dp),
                    )
                }
            }
        }
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(color = AltimColors.textSecondary)) { append("Sources ") }
                append(NewsSummary.consensusText(s))
            },
            fontSize = 12.sp, color = Color.White,
        )
        s.moves.forEach { m ->
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(color = if (m.changePct >= 0) AltimColors.buy else AltimColors.sell)) { append(NewsSummary.moveText(m)) }
                    withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" (clôtures horaires, pas forcément dues à cette actualité)") }
                },
                fontSize = 12.sp,
            )
        }
        s.technical.forEach { t ->
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(color = AltimColors.textSecondary)) {
                        append("Impact sur le signal technique${if (s.technical.size > 1) " (${NewsSummary.assetLink(t.asset).symbol})" else ""} : ")
                    }
                    append(t.text)
                },
                fontSize = 12.sp, color = Color.White,
            )
        }
        Details("Sources et calcul") {
            s.links.forEach { l ->
                val (mark, c) = when (l.tone) { "negative" -> "▼" to AltimColors.sell; "positive" -> "▲" to AltimColors.buy; else -> "·" to AltimColors.textSecondary }
                Row(
                    Modifier.fillMaxWidth().clickable(role = Role.Button, onClickLabel = "Lire l'article chez ${l.source}") { openUrl(context, l.link) }
                        .semantics(mergeDescendants = true) { contentDescription = "${l.source}, ton ${l.tone} : ${l.title}" },
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(mark, color = c, fontSize = 12.sp)
                    Text(
                        buildAnnotatedString {
                            withStyle(SpanStyle(color = AltimColors.cyan)) { append(l.source) }
                            withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" · ${l.title}") }
                        },
                        fontSize = 12.sp, modifier = Modifier.weight(1f),
                    )
                }
            }
            Caption(NewsSummary.ruleText(s))
        }
    }
}
