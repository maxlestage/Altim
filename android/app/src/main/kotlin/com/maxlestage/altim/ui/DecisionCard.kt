package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Money
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.filled.AccountCircle
import androidx.compose.material.icons.filled.ArrowCircleDown
import androidx.compose.material.icons.filled.ArrowCircleUp
import androidx.compose.material.icons.filled.CheckCircleOutline
import androidx.compose.material.icons.filled.HelpOutline
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.RemoveCircle
import androidx.compose.material.icons.filled.Report
import com.maxlestage.altim.kit.JsFormat
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.QuestionMark
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.ActionZone
import com.maxlestage.altim.kit.ActionZones
import com.maxlestage.altim.kit.Bias
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.ConfigTransition
import com.maxlestage.altim.kit.CounterArgument
import com.maxlestage.altim.kit.Guidance
import com.maxlestage.altim.kit.NoTrade
import com.maxlestage.altim.kit.Calendar
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.MarketRegime
import com.maxlestage.altim.kit.ModelEvidence
import com.maxlestage.altim.kit.signedScore
import com.maxlestage.altim.kit.DecisionLevel
import com.maxlestage.altim.kit.FamilyStatus
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Fundamentals
import com.maxlestage.altim.kit.ScenarioKind
import com.maxlestage.altim.kit.StepState
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Uncertainty
import kotlin.math.abs

/** "Décision" card of the asset page: loading, error or the decision itself (iOS AssetDetailView.decisionCard). */
@Composable
fun DecisionCard(state: Loadable<Decision>, held: Boolean, change: ConfigTransition? = null, onSimulate: (() -> Unit)? = null, retry: () -> Unit) {
    when (state) {
        is Loadable.Loading -> Card(title = "Décision") { Loading() }
        is Loadable.Failed -> Card(title = "Décision") { ErrorBox(state.message, retry) }
        is Loadable.Loaded -> DecisionView(state.value, onSimulate = onSimulate, change = change)
    }
}

/** "Objectif 1 atteint" → "objectif 1 atteint" (kept as is when it starts with an acronym such as "VIX"). */
internal fun lowerFirst(s: String): String {
    if (s.length < 2) return s
    return if (s[1].isUpperCase()) s else s.replaceFirstChar { it.lowercase() }
}

/**
 * Decision on one asset (GET /api/decision), in the iPhone's order: verdict and level, what changed, confidence, the
 * model's proof, the bot, the evidence families, the mode, the market regime, the composite score, the entry plan, the
 * action zones, what would change the decision, the position, then the details folded (iOS DecisionCard.swift). The
 * level is always written out, never shown by its colour alone.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun DecisionView(d: Decision, expanded: Boolean = false, onSimulate: (() -> Unit)? = null, change: ConfigTransition? = null) {
    val rating = d.rating
    // The colour of the headline: the rating's level when there is one.
    val headLevel = rating?.level ?: d.level
    Card(title = "Décision", glow = levelColor(headLevel)) {
        // Verdict: the rating (or the plan's verdict), the level, why, the warnings and the headline.
        Column(
            Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (rating != null) {
                Text("${rating.emoji} ${d.ratingLabel.ifBlank { rating.label }}", color = levelColor(rating.level), fontSize = 24.sp, fontWeight = FontWeight.Black)
            } else {
                Text(d.verdictLabel, color = levelColor(d.level), fontSize = 24.sp, fontWeight = FontWeight.Black)
            }
            Text("${d.level.emoji} ${d.levelText}", fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
            if (rating != null) {
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(color = Color.White.copy(alpha = 0.9f))) { append("Verdict du plan : ") }
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(d.verdictLabel) }
                        withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" · la note résume verdict, niveau et confiance") }
                    },
                    fontSize = 13.sp,
                )
            }
            d.ratingReason?.takeIf { it.isNotBlank() }?.let { Text(it, fontSize = 13.sp, color = AltimColors.textSecondary) }
            d.degraded?.takeIf { it.active }?.let { DegradedBanner(it) }
            d.noTrade?.takeIf { it.active }?.let { NoTradeBanner(it) }
            if (d.headline.isNotBlank()) Text(d.headline, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
            if (d.blocked) Notice("Achat interdit pour l'instant : " + d.vetoes.filter { it.active }.joinToString(", ") { it.label } + ".", Tone.BAD)
        }
        change?.let { ChangeBlock(it) }
        Meter("Confiance", d.confidence, if (d.confidence >= 65) Tone.GOOD else if (d.confidence >= 40) Tone.WARN else Tone.BAD)
        if (d.confidenceText.isNotBlank()) Caption(d.confidenceText)
        d.modelEvidence?.let { EvidenceLine(it) }
        d.bot?.let { BotLine(it) }
        if (d.families.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                d.families.forEach { FamilyLight(it) }
            }
        }
        ModeLine(d)
        d.marketRegime?.let { RegimeLine(it) }
        d.score?.let { ScoreBlock(it) }
        d.plan?.let { PlanBlock(it, d) }
        d.actionZones?.let { ActionLadder(it) }
        if (d.whyWait.isNotEmpty()) {
            SubTitle("Pourquoi attendre ?")
            Bullets(d.whyWait)
        }
        Conditions("Pour passer en ACHAT", d.toBuy)
        Conditions("Pour passer en VENTE", d.toSell)
        d.counterArgument?.let { CounterBlock(it) }
        d.position?.let { PositionBlock(it) }
        d.exposure?.let { ExposureBlock(it) }
        Details(d, expanded)
        // Paper trading: follow this decision with virtual money (no real money, no order placed).
        onSimulate?.let {
            OutlinedButton(
                onClick = it,
                modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)
                    .semantics { contentDescription = "Simuler cet achat. Achat fictif dans le portefeuille simulé : aucun argent réel, aucun ordre passé" },
                border = BorderStroke(1.2.dp, AltimColors.violet),
            ) { Text("Simuler cet achat", color = Color(0xFFB9A3FF)) }
        }
        Text("Calculé le ${Format.date(d.asOf, time = true)}.", fontSize = 11.sp, color = AltimColors.textSecondary)
        if (d.disclaimer.isNotBlank()) Caption(d.disclaimer)
    }
}

/** "⚠️ Signal dégradé — …": the server's headline as sent (it names the actual cause), then its reasons. */
@Composable
private fun DegradedBanner(g: Decision.Degraded) {
    val shape = RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.sell.copy(alpha = 0.12f)).border(1.dp, AltimColors.sell.copy(alpha = 0.5f), shape).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(g.headline, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
        g.reasons.forEach { Text("• $it", fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f)) }
    }
}

@Composable
private fun ModeLine(d: Decision) {
    Row(Modifier.semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Icon(if (d.isPersonal) Icons.Filled.AccountCircle else Icons.Filled.Info, contentDescription = null, tint = AltimColors.cyan, modifier = Modifier.size(16.dp))
        Caption(
            if (d.isPersonal) {
                "Mode personnel : votre prix d'achat moyen et la part de chaque ligne dans votre portefeuille (en %, jamais les quantités ni les montants) sont transmis pour ce calcul et jamais conservés."
            } else {
                "Mode informationnel : données de marché uniquement, sans vos avoirs."
            },
        )
    }
}

private fun statusIcon(s: FamilyStatus) = when (s) {
    FamilyStatus.POSITIVE -> Icons.Filled.ArrowCircleUp
    FamilyStatus.NEUTRAL -> Icons.Filled.RemoveCircle
    FamilyStatus.NEGATIVE -> Icons.Filled.ArrowCircleDown
    FamilyStatus.UNAVAILABLE -> Icons.Filled.HelpOutline
}

/** One family of evidence: its status written out, an icon that differs by status, then the colour. */
@Composable
private fun FamilyLight(f: Decision.Family) {
    val c = statusColor(f.status)
    Row(
        Modifier.widthIn(min = 120.dp).clearAndSetSemantics {
            contentDescription = "${f.label} : ${f.status.label}" + (f.score?.let { ", score ${Math.round(it)} sur une échelle de −100 à +100" } ?: "")
        },
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(statusIcon(f.status), contentDescription = null, tint = c, modifier = Modifier.size(16.dp))
        Column {
            Text(f.label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
            Text(f.status.label, fontSize = 11.sp, color = c)
        }
    }
}

@Composable
private fun SubTitle(text: String) = Text(text, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, color = Color.White, modifier = Modifier.padding(top = 4.dp).semantics { heading() })

@Composable
private fun Bullets(items: List<String>, mark: String = "•") {
    items.forEach { Text("$mark $it", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
}

@Composable
private fun PlanBlock(p: Decision.Plan, d: Decision) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SubTitle("Plan · ${p.horizon}")
        KeyValue("Zone d'achat", "${Format.price(p.zoneFrom)} – ${Format.price(p.zoneTo)}")
        KeyValue("Stop / invalidation", "${Format.price(p.stop)} (${Format.percent(-abs(p.riskPct), 1)})", Tone.BAD)
        KeyValue("Objectif 1", "${Format.price(p.target1)} (${Format.percent(p.reward1Pct, 1)})", Tone.GOOD)
        p.target2?.let { t2 -> KeyValue("Objectif 2", Format.price(t2) + (p.reward2Pct?.let { " (${Format.percent(it, 1)})" } ?: ""), Tone.GOOD) }
        val t3 = p.target3
        if (t3 != null) KeyValue("Objectif 3", Format.price(t3) + (p.reward3Pct?.let { " (${Format.percent(it, 1)})" } ?: ""), Tone.GOOD)
        else if (d.rating != null) KeyValue("Objectif 3", "aucun")
        KeyValue("Gain/risque", "${Format.plain(p.riskReward, 1)} (minimum ${Format.plain(p.minRiskReward, 1)})", if (p.acceptable) Tone.GOOD else Tone.BAD)
        Caption("Calculé depuis ${Format.price(p.entry)} : " + if (p.acceptable) "rapport suffisant." else "rapport insuffisant, pas d'entrée à ce prix.")
        d.horizon?.let { h ->
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(color = AltimColors.textSecondary)) { append("Horizon : ") }
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(h.label) }
                    withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" — ${h.detail}") }
                },
                fontSize = 12.sp,
            )
        }
        p.target3Source?.takeIf { it.isNotBlank() }?.let { Caption("Objectif 3 : ${it.replaceFirstChar { c -> c.lowercase() }}.") }
    }
}

@Composable
private fun Conditions(title: String, items: List<Decision.Condition>) {
    if (items.isEmpty()) return
    SubTitle(title)
    items.forEach { c -> Text("• ${c.text}" + (c.level?.let { " — niveau ${Format.price(it)}" } ?: ""), fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
}

/** Personal mode: the user's position and the staged exits. */
@Composable
private fun PositionBlock(p: Decision.Position) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SubTitle("Votre position")
        KeyValue("Prix d'achat moyen", Format.price(p.cost))
        p.pnlPct?.let { KeyValue("Plus ou moins-value", Format.percent(it, 1), if (it >= 0) Tone.GOOD else Tone.BAD) }
        if (p.advice.isNotBlank()) Text(p.advice, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        if (p.exits.isNotEmpty()) {
            Text("Sorties progressives", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.padding(top = 2.dp))
            p.exits.forEach { ExitRow(it) }
        }
    }
}

@Composable
private fun ExitRow(e: Decision.Exit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(Color.White.copy(alpha = if (e.now) 0.08f else 0.04f)).padding(8.dp)
            .semantics(mergeDescendants = true) {},
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Vendre ${Format.plain(e.share, 0)} % si ${lowerFirst(e.trigger)}", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.weight(1f))
            if (e.now) Badge("MAINTENANT", Tone.WARN)
        }
        Text(e.kind.label + (e.price?.let { " · ${Format.price(it)}" } ?: ""), style = mono(12.sp), color = AltimColors.textSecondary)
    }
}

@Composable
private fun ExposureBlock(e: Decision.Exposure) {
    e.warning?.let { Notice(it, Tone.WARN) }
    Caption(
        "Exposition au facteur ${e.factor} : ${Format.plain(e.weight, 0)} % du portefeuille (${e.assets.joinToString(", ")})" +
            (e.correlation?.let { " ; corrélation de cet actif : ${Format.plain(it, 2)}" } ?: "") + ".",
    )
}

/** Folded part of the card (iOS DisclosureGroup): a title that opens it (button for TalkBack, with its state). */
@Composable
private fun ColumnScope.Section(title: String, initiallyOpen: Boolean, content: @Composable ColumnScope.() -> Unit) {
    var open by rememberSaveable(title) { mutableStateOf(initiallyOpen) }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
            .clickable(role = Role.Button, onClickLabel = if (open) "Replier" else "Déplier") { open = !open }
            .semantics(mergeDescendants = true) { stateDescription = if (open) "déplié" else "replié" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = Color.White, modifier = Modifier.weight(1f))
        Icon(if (open) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, contentDescription = null, tint = Color.White)
    }
    if (open) Column(Modifier.padding(top = 2.dp), verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
}

private const val NA = "non disponible"

@Composable
private fun SourceLine(s: String) = Text("Source : " + if (s.isBlank() || s == "—") NA else s, fontSize = 11.sp, color = AltimColors.textSecondary)

@Composable
private fun ColumnScope.Details(d: Decision, expanded: Boolean) {
    d.noTrade?.let { n -> Section("Quand ne pas trader · ${Guidance.noTradeBadge(n)}", expanded) { NoTradeList(n) } }
    val available = d.families.count { it.status != FamilyStatus.UNAVAILABLE }
    Section("Familles · $available/${d.families.size} disponibles", expanded) { d.families.forEach { FamilyDetail(it) } }
    d.structure?.let { st ->
        Section("Structure technique · ${st.score?.let { "direction ${signedScore(it)}" } ?: NA}", expanded) { StructureList(st) }
    }
    if (d.eventsKnown) {
        val ev = d.events
        val badge = if (ev == null) "non vérifié" else if (ev.isEmpty()) "rien de majeur" else "${ev.size} événement${if (ev.size > 1) "s" else ""}"
        Section("Agenda (7 jours) · $badge", expanded) {
            when {
                ev == null -> Caption("Calendrier indisponible ou incomplet : les annonces à venir n'ont pas pu être vérifiées.")
                ev.isEmpty() -> Caption("Aucune annonce majeure (banques centrales, inflation, emploi, PIB)${if (d.kind == Kind.STOCK) ", ni résultats, dividende ou split" else ""} dans les 7 jours.")
                else -> ev.forEach { e -> AgendaEventRow(e, "${Calendar.dayLabel(e.day)}${e.time?.let { " · $it" } ?: ""}") }
            }
        }
    }
    Section("Interdictions d'achat · ${d.vetoes.count { it.active }} active(s)", expanded) { d.sortedVetoes.forEach { VetoRow(it) } }
    val setup = d.setup ?: Decision.Setup()
    Section("Setup : ${setup.name} · ${setup.met}/${setup.total} étapes", expanded) { setup.steps.forEachIndexed { i, st -> StepRow(i + 1, st) } }
    if (d.scenarios.isNotEmpty()) {
        Section("Scénarios" + (d.unfolding?.let { " · en cours : ${it.kind.label.lowercase()}" } ?: ""), expanded) {
            d.unfolding?.let { Text(it.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
            d.scenarios.forEach { ScenarioRow(it) }
        }
    }
    if (d.pros.isNotEmpty() || d.cons.isNotEmpty()) {
        Section("Points favorables / défavorables", expanded) {
            if (d.pros.isNotEmpty()) Text("Favorables", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.buy)
            Bullets(d.pros, "+")
            if (d.cons.isNotEmpty()) Text("Défavorables", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.sell, modifier = Modifier.padding(top = 4.dp))
            Bullets(d.cons, "−")
        }
    }
    val w = d.whyNot ?: Decision.WhyNot()
    Section("Pourquoi pas ?", expanded) {
        Caption("Ce qui pourrait rendre cette décision fausse · incertitude ${w.uncertainty.label}")
        Bullets(w.risks)
        if (w.invalidation.isNotEmpty()) {
            Text("Invalidation", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.padding(top = 4.dp))
            Bullets(w.invalidation)
        }
    }
    d.fundamentals?.let { f ->
        Section("Fondamentaux", expanded) {
            when (f) {
                is Fundamentals.Stock -> StockFundamentals(f)
                is Fundamentals.Crypto -> CryptoFundamentals(f)
                is Fundamentals.Other -> Text("Fondamentaux non disponibles dans cette version de l'app.", fontSize = 13.sp, color = AltimColors.textSecondary)
            }
        }
    }
    d.liquidity?.let { l ->
        Section("Liquidité", expanded) {
            KeyValue("Écart achat/vente", l.spreadPct?.let { "${Format.plain(it, 4)} %" } ?: NA)
            KeyValue("Échangé par jour (20 j)", l.dailyValue?.let { Format.large(it) } ?: NA)
            KeyValue("Volume relatif", l.relativeVolume?.let { Format.plain(it, 1) } ?: NA)
            SourceLine(l.source)
        }
    }
    d.track?.let { t -> Section("Historique du signal", expanded) { TrackView(t) } }
    if (d.sources.isNotEmpty()) {
        Section("Sources · ${d.sources.count { it.ok }}/${d.sources.size} en ligne", expanded) {
            d.sources.forEach { s ->
                Text(if (s.ok) "✔ ${s.name} · ${s.detail}" else "✕ ${s.name} · ${s.detail}", fontSize = 12.sp, color = if (s.ok) Color.White else AltimColors.textSecondary)
            }
        }
    }
}

/** Content of the "Quand ne pas trader" section. */
@Composable
private fun NoTradeList(n: NoTrade) {
    if (n.reasons.isEmpty()) {
        Text("Aucune raison mesurée de s'abstenir maintenant, ce qui ne garantit rien pour la suite.", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
    } else {
        n.reasons.forEach { r ->
            Text(
                buildAnnotatedString {
                    append("• ")
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(r.label) }
                    append(" — ${r.detail}")
                },
                fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f),
            )
        }
    }
    if (n.unchecked.isNotEmpty()) Caption("Non vérifié faute de données : ${n.unchecked.joinToString(" ; ")}.")
    Caption("Volatilité, liquidité, écart achat/vente, résultats (avant et 1 à 2 séances après), annonces, marché sans direction, signal faible ou dégradé, séance de Wall Street (actions). N'interdit rien : signale un mauvais moment.")
}

/** Compact banner near the top: the headline and the reasons' names (the detail is in its section). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun NoTradeBanner(n: NoTrade) {
    val shape = RoundedCornerShape(14.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.warning.copy(alpha = 0.1f)).border(1.dp, AltimColors.warning.copy(alpha = 0.45f), shape).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(n.headline, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp),
            modifier = Modifier.semantics { contentDescription = "Raisons : " + n.reasons.joinToString(", ") { it.label } },
        ) {
            n.reasons.forEach { r ->
                Text(
                    r.label, fontSize = 12.sp, color = AltimColors.warning,
                    modifier = Modifier.clip(CircleShape).background(Color.White.copy(alpha = 0.06f)).padding(horizontal = 10.dp, vertical = 4.dp),
                )
            }
        }
    }
}

@Composable
private fun FamilyDetail(f: Decision.Family) {
    Column(Modifier.semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(f.label, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = Color.White, modifier = Modifier.weight(1f))
            val score = f.score?.let { "${f.status.label} · ${if (it >= 0) "+" else "−"}${Math.round(abs(it))}" } ?: "${f.status.label} · $NA"
            Text(score, style = mono(12.sp, FontWeight.SemiBold), color = statusColor(f.status), textAlign = TextAlign.End)
        }
        if (f.summary.isNotBlank()) Text(f.summary, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        Bullets(f.points)
        SourceLine(f.source)
    }
}

@Composable
private fun VetoRow(v: Decision.Veto) {
    val (state, tone, icon) = when {
        !v.verifiable -> Triple("non vérifiable", Tone.NEUTRAL, Icons.Filled.HelpOutline)
        v.active -> Triple("ACTIVE", Tone.BAD, Icons.Filled.Report)
        else -> Triple("non active", Tone.GOOD, Icons.Filled.CheckCircleOutline)
    }
    Column(Modifier.semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(icon, contentDescription = null, tint = AltimColors.of(tone), modifier = Modifier.size(16.dp))
            Text(v.label, fontSize = 13.sp, fontWeight = if (v.active) FontWeight.SemiBold else FontWeight.Normal, color = Color.White, modifier = Modifier.weight(1f))
            Text(state, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.of(tone))
        }
        if (v.detail.isNotBlank()) Caption(v.detail)
    }
}

@Composable
private fun StepRow(n: Int, s: Decision.Step) {
    val (icon, text, tone) = when (s.state) {
        StepState.OK -> Triple(Icons.Filled.CheckCircle, "fait", Tone.GOOD)
        StepState.NO -> Triple(Icons.Filled.Cancel, "pas encore", Tone.BAD)
        StepState.UNKNOWN -> Triple(Icons.Filled.HelpOutline, "inconnu", Tone.NEUTRAL)
    }
    Column(Modifier.semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(icon, contentDescription = null, tint = AltimColors.of(tone), modifier = Modifier.size(16.dp))
            Text("$n. ${s.label}", fontSize = 13.sp, color = Color.White, modifier = Modifier.weight(1f))
            Text(text, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.of(tone))
        }
        if (s.detail.isNotBlank() && s.detail != "—") Caption(s.detail)
    }
}

private fun scenarioIcon(k: ScenarioKind) = when (k) { ScenarioKind.BULL -> "↗"; ScenarioKind.BEAR -> "↘"; ScenarioKind.NEUTRAL -> "→" }

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ScenarioRow(s: Decision.Scenario) {
    val c = when (s.kind) { ScenarioKind.BULL -> AltimColors.buy; ScenarioKind.NEUTRAL -> AltimColors.warning; ScenarioKind.BEAR -> AltimColors.sell }
    val shape = RoundedCornerShape(10.dp)
    Column(
        Modifier.fillMaxWidth()
            .then(if (s.unfolding) Modifier.clip(shape).background(AltimColors.cyan.copy(alpha = 0.05f)).border(1.dp, AltimColors.cyan, shape).padding(8.dp) else Modifier)
            .semantics(mergeDescendants = true) { if (s.unfolding) stateDescription = "scénario en cours" },
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Text("${scenarioIcon(s.kind)} ${s.title}", color = c, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            Guidance.scenarioCount(s)?.let { Text(it, style = mono(12.sp), color = if (s.unfolding) AltimColors.cyan else AltimColors.textSecondary) }
        }
        Text("Si ${lowerFirst(s.condition)} → ${s.consequence}", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        s.level?.let { Text("Niveau à surveiller : ${Format.price(it)}", style = mono(12.sp), color = AltimColors.textSecondary) }
        s.conditions.forEach { k ->
            val tint = when (k.state) { "met" -> AltimColors.buy; "unmet" -> AltimColors.orange; else -> AltimColors.textSecondary }
            HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
            Column(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text("${Guidance.checkIcon(k.state)} ${Guidance.checkLabel(k.state)}", fontSize = 12.sp, color = tint, fontWeight = FontWeight.SemiBold)
                Text(k.text, fontSize = 12.sp, color = Color.White)
                if (k.detail.isNotBlank()) Text(k.detail, fontSize = 11.sp, color = AltimColors.textSecondary)
            }
        }
    }
}

private fun pctNa(v: Double?, digits: Int = 1) = v?.takeIf { it.isFinite() }?.let { "${Format.plain(it, digits)} %" } ?: NA
private fun signedNa(v: Double?) = v?.takeIf { it.isFinite() }?.let { Format.percent(it, 1) } ?: NA
private fun ratioNa(v: Double?, digits: Int = 1) = v?.takeIf { it.isFinite() }?.let { Format.plain(it, digits) } ?: NA
private fun amountNa(v: Double?) = v?.takeIf { it.isFinite() }?.let { Format.large(it) } ?: NA
private fun unitsNa(v: Double?) = v?.takeIf { it.isFinite() }?.let { Format.large(it, "") } ?: NA

/** Title of a block inside a section (valuation history, peers, stablecoins, developer activity). */
@Composable
private fun Block(title: String) = Text(title, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.padding(top = 6.dp))

@Composable
private fun Note(text: String, strong: Boolean = false) = Text(text, fontSize = 12.sp, color = if (strong) Color.White.copy(alpha = 0.9f) else AltimColors.textSecondary)

@Composable
private fun StockFundamentals(s: Fundamentals.Stock) {
    Caption(s.period)
    if (s.periodEnd != null && s.filedAt != null) Note("Comptes arrêtés au ${Format.nyDate(s.periodEnd!!)}, déposés à la SEC le ${Format.nyDate(s.filedAt!!)}.")
    s.sector?.let { Text("Secteur : ${it.label} · ${it.sicDescription} (code SIC ${it.sic})", fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f)) }
    KeyValue("Chiffre d'affaires", "${amountNa(s.revenue)} (${signedNa(s.revenueGrowth)})")
    KeyValue("Bénéfice net", amountNa(s.netIncome))
    KeyValue("Bénéfice par action", "${s.eps?.let { Format.price(it) } ?: NA} (${signedNa(s.epsGrowth)})")
    KeyValue("Marge brute · opérationnelle · nette", "${pctNa(s.grossMargin)} · ${pctNa(s.operatingMargin)} · ${pctNa(s.netMargin)}")
    KeyValue("Flux de trésorerie libre", "${amountNa(s.freeCashFlow)} (marge ${pctNa(s.fcfMargin)})")
    KeyValue("Dette · trésorerie", "${amountNa(s.debt)} · ${amountNa(s.cash)}")
    KeyValue("Dette nette", amountNa(s.netDebt))
    KeyValue("Rentabilité des capitaux propres", pctNa(s.roe))
    KeyValue("PER · PEG · EV/EBITDA", "${ratioNa(s.per)} · ${ratioNa(s.peg, 2)} · ${ratioNa(s.evEbitda)}")
    if (s.guidance.isNotBlank() || s.ps != null || s.pb != null || s.roic != null) {
        KeyValue("P/S (capitalisation ÷ ventes)", ratioNa(s.ps))
        KeyValue("P/B (capitalisation ÷ fonds propres)", ratioNa(s.pb))
        KeyValue(
            "ROIC (rentabilité du capital investi)",
            s.roic?.let { "${pc(it, 1)} (impôt ${pc(s.roicTaxRate, 1)}${if (s.roicTaxStatutory) " : taux légal américain, taux effectif non calculable" else ", taux effectif"})" } ?: NA,
        )
    }
    KeyValue("Rendement du dividende", pctNa(s.dividendYield, 2))
    KeyValue("Nombre d'actions sur 1 an", signedNa(s.shareChange))
    KeyValue("Prochains résultats", s.nextEarnings?.let { "${Format.date(it.date)}${if (it.estimated) " (date estimée)" else " (date annoncée)"}" } ?: NA)
    if (s.surprises.isNotEmpty()) {
        Text("Résultats publiés vs attendus", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.padding(top = 2.dp))
        s.surprises.forEach { x ->
            Text("${x.quarter} : BPA ${Format.price(x.eps)} contre ${Format.price(x.consensus)} attendu (${Format.percent(x.surprisePct, 1)})", style = mono(12.sp), color = Color.White)
        }
    }
    s.revisions?.let { r ->
        Text("Révisions : BPA attendu de l'année ${Format.price(r.monthAgo)} il y a un mois, ${Format.price(r.now)} aujourd'hui (${Format.percent(r.changePct, 1)}).", style = mono(12.sp), color = Color.White)
    }
    s.valuationHistory?.let { h ->
        Block("Valorisation par rapport à sa propre histoire")
        s.valuationVerdict?.let { Note("$it.", strong = true) }
        HistoryRows("PER", h.per)
        HistoryRows("P/S", h.ps)
        Note("${h.method}. Source : ${h.source}.")
    }
    val c = s.peers
    if (c != null) {
        Block("Comparaison sectorielle")
        Note(s.sectorNote, strong = true)
        c.peers.forEach { p ->
            Text(
                "• ${p.name} (${p.symbol}) : PER ${num(p.per, 1)}, P/S ${num(p.ps, 1)}, marge opérationnelle ${pc(p.operatingMargin)}, chiffre d'affaires ${pc(p.revenueGrowth, 1, true)} sur un an",
                fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f),
            )
        }
        Note("Cours du ${Format.nyDate(c.date)}. Source : ${c.source}.")
    } else if (s.sectorNote.isNotBlank()) {
        Caption(s.sectorNote)
    }
    if (s.guidance.isNotBlank()) Note(s.guidance)
    SourceLine(s.source)
}

@Composable
private fun CryptoFundamentals(c: Fundamentals.Crypto) {
    KeyValue("Capitalisation", amountNa(c.marketCap))
    KeyValue("Valeur totale diluée (FDV)", amountNa(c.fdv))
    KeyValue("Capitalisation / FDV", ratioNa(c.mcFdv, 2))
    KeyValue("Offre en circulation", "${unitsNa(c.circulatingSupply)} (${pctNa(c.circulatingPct)} du maximum)")
    KeyValue("Offre totale · maximale", "${unitsNa(c.totalSupply)} · ${unitsNa(c.maxSupply)}")
    KeyValue("Valeur bloquée (TVL)", amountNa(c.tvl))
    KeyValue("Frais sur 30 jours", amountNa(c.fees30d))
    KeyValue("Dominance du bitcoin", pctNa(c.btcDominance))
    KeyValue("Financement (funding)", c.fundingRate?.let { "${Format.percent(it * 100, 4)} par 8 h" } ?: NA)
    KeyValue("Positions ouvertes (OI)", amountNa(c.openInterest))
    c.txPerDay?.let { KeyValue("Transactions par jour", Format.large(it, "")) }
    c.hashRate?.let { KeyValue("Taux de hachage", "${Format.plain(it / 1e18, 0)} EH/s") }
    Caption("Déblocages de jetons : ${c.unlocks}")
    if (c.stablecoins != null || c.chainStablecoins != null) {
        Block("Flux de stablecoins")
        StableRows(c.stablecoins, "tous réseaux")
        StableRows(c.chainStablecoins, "réseau ${c.chainStablecoins?.scope ?: ""}")
        Note("Liquidité disponible sur le marché crypto. Source : ${(c.stablecoins ?: c.chainStablecoins)?.source ?: NA}.")
    }
    if (c.devActivityKnown) {
        Block("Activité de développement")
        val dev = c.devActivity
        if (dev != null) {
            KeyValue("Commits sur 4 semaines", dev.commits4w?.let { Format.count(it) } ?: NA)
            KeyValue(
                "Lignes ajoutées / supprimées (4 semaines)",
                if (dev.additions4w != null && dev.deletions4w != null) "+${Format.count(dev.additions4w)} / −${Format.count(dev.deletions4w)}" else NA,
            )
            KeyValue("Pull requests intégrées (total)", dev.pullRequestsMerged?.let { Format.count(it) } ?: NA)
            KeyValue("Contributeurs", dev.contributors?.let { Format.count(it) } ?: NA)
            KeyValue("Étoiles", dev.stars?.let { Format.count(it) } ?: NA)
            SourceLine(dev.source)
        } else {
            Note("Non disponible (CoinGecko ne la publie plus et le dépôt GitHub du projet n'a pas répondu).")
        }
    }
    if (c.notCovered.isNotBlank()) Note("${c.notCovered}.")
    SourceLine(c.source)
}

@Composable
private fun TrackView(t: Decision.Track) {
    Caption(t.period)
    KeyValue("Trades", "${t.trades}")
    KeyValue("Gagnants", pctNa(t.winRate))
    KeyValue("Gain moyen · perte moyenne", "${signedNa(t.avgWin)} · ${signedNa(t.avgLoss)}")
    KeyValue("Profit factor", ratioNa(t.profitFactor, 2))
    KeyValue("Sharpe · Sortino", "${ratioNa(t.sharpe, 2)} · ${ratioNa(t.sortino, 2)}")
    KeyValue("Pire recul (drawdown)", Format.percent(t.maxDrawdown, 1), Tone.BAD)
    KeyValue("Rendement du signal", Format.percent(t.totalReturn, 1), if (t.totalReturn >= 0) Tone.GOOD else Tone.BAD)
    KeyValue("Simple détention", Format.percent(t.buyAndHold, 1))
    KeyValue("Frais · glissement par ordre", "${pctNa(t.feesPct, 2)} · ${pctNa(t.slippagePct, 2)}")
    KeyValue("Pire série de pertes", "${t.losingStreak} trade${if (t.losingStreak > 1) "s" else ""}")
    if (t.note.isNotBlank()) Caption(t.note)
    if (t.hasDetails) TrackDetails(t)
    LocalOpenValidation.current?.let { ValidationLink(it) }
}

/**
 * More of the signal's track record (iOS TrackDetails.swift): spread cost, expectancy, R multiples, results by market
 * regime, how the test avoids flattering itself, and the tax assumption.
 */
@Composable
private fun TrackDetails(t: Decision.Track) {
    val afterTax = if (t.totalReturn > 0) t.totalReturn * (1 - FLAT_TAX / 100) else t.totalReturn
    KeyValue("Espérance par trade (coûts inclus)", pc(t.expectancy, 2, true), if ((t.expectancy ?: 0.0) >= 0) Tone.GOOD else Tone.BAD)
    KeyValue("Multiple de R moyen (gain ÷ risque jusqu'au stop)", t.avgR?.let { "${num(it, 2)} R" } ?: "—")
    t.spreadPct?.let { KeyValue("Écart achat/vente ${if (t.spreadMeasured) "mesuré" else "supposé"}", pc(it, 3)) }
    if (t.spreadNote.isNotBlank()) Caption(t.spreadNote)
    val regimes = t.regimes.orEmpty()
    if (regimes.isNotEmpty()) {
        Text("Selon le régime de marché", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.padding(top = 4.dp))
        regimes.forEach { g ->
            val tone = if (g.lowSample) Tone.NEUTRAL else if ((g.avgReturn ?: 0.0) >= 0) Tone.GOOD else Tone.WARN
            val icon = when (g.regime) { "bull" -> "↗"; "bear" -> "↘"; "crisis" -> "⚠"; else -> "→" }
            var detail = "${g.trades} trade${if (g.trades > 1) "s" else ""}"
            if (g.trades > 0) detail += " · réussite ${pc(g.winRate, 0)} · moyenne ${pc(g.avgReturn, 1, true)}"
            if (g.lowSample) detail += " · échantillon trop faible"
            val shape = RoundedCornerShape(12.dp)
            Row(
                Modifier.fillMaxWidth().clip(shape).background(AltimColors.of(tone).copy(alpha = 0.08f)).padding(10.dp).semantics(mergeDescendants = true) {},
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(icon, color = AltimColors.of(tone), fontSize = 14.sp, fontWeight = FontWeight.Bold)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(g.label, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
                    Text(detail, fontSize = 13.sp, color = Color.White.copy(alpha = 0.85f))
                }
            }
        }
        Caption("Haussier : clôture au-dessus d'une moyenne 200 jours qui monte (sur 20 jours) ; baissier : sous une moyenne qui baisse ; crise : plus de 30 % sous le plus haut de l'année. Régime lu à la date du signal, sans données futures.")
    }
    if (t.biasNotes.isNotEmpty()) {
        Text("Comment ce test évite de se flatter", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = Color.White, modifier = Modifier.padding(top = 4.dp))
        t.biasNotes.forEach { Text("• $it", fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f)) }
    }
    Caption(
        "Impôt (hypothèse : flat tax de ${Format.plain(FLAT_TAX, 0)} % sur le gain net, payée à la fin, pertes compensées) : rendement du signal après impôt ≈ ${pc(afterTax, 1, true)}. " +
            "Votre situation fiscale peut différer.",
    )
}

private fun levelColor(l: DecisionLevel): Color = when (l) {
    DecisionLevel.STRONG -> AltimColors.buy
    DecisionLevel.MODERATE -> AltimColors.warning
    DecisionLevel.WAITING -> Color.White.copy(alpha = 0.85f)
    DecisionLevel.HIGH_RISK -> AltimColors.orange
    DecisionLevel.EXIT -> AltimColors.sell
}

private fun statusColor(s: FamilyStatus): Color = when (s) {
    FamilyStatus.POSITIVE -> AltimColors.buy
    FamilyStatus.NEUTRAL -> AltimColors.warning
    FamilyStatus.NEGATIVE -> AltimColors.sell
    FamilyStatus.UNAVAILABLE -> AltimColors.textSecondary
}

/** The web card's `pct`: "—" when missing, + only with [sign], true minus, no trailing zeros. */
private fun pc(v: Double?, digits: Int = 1, sign: Boolean = false): String {
    if (v == null || !v.isFinite()) return "—"
    return "${if (v < 0) "−" else if (sign && v > 0) "+" else ""}${JsFormat.fr(abs(v), digits)} %"
}

/** The web card's `num`: "—" when missing. */
private fun num(v: Double?, digits: Int = 2): String = if (v == null || !v.isFinite()) "—" else "${if (v < 0) "−" else ""}${JsFormat.fr(abs(v), digits)}"

/** "+2,8 Md€" / "−271 M€". */
private fun signedUsd(v: Double?): String = if (v == null) "—" else "${if (v < 0) "−" else if (v > 0) "+" else ""}${Format.compactUsd(abs(v))}"

/** French flat tax (PFU) on the net gain, as in the sale tool: an assumption, not the user's own situation. */
private const val FLAT_TAX = 30.0

/**
 * « Preuve du modèle » (web DecisionCard.tsx `EvidenceLine`): the cross-asset validation of the signal on this asset's
 * class, next to the confidence, with the link to the « Validation du modèle » screen and the report's date.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun EvidenceLine(e: ModelEvidence) {
    val accent = when (e.tone) {
        "edge" -> AltimColors.buy
        "weak" -> AltimColors.orange
        "unproven" -> AltimColors.warning
        else -> AltimColors.textSecondary
    }
    val shape = RoundedCornerShape(12.dp)
    val openValidation = LocalOpenValidation.current
    Row(
        Modifier.fillMaxWidth().height(IntrinsicSize.Min).clip(shape)
            .background(Color.White.copy(alpha = 0.03f))
            .border(1.dp, Color.White.copy(alpha = 0.12f), shape),
    ) {
        Box(Modifier.width(3.dp).fillMaxHeight().background(accent))
        Column(Modifier.weight(1f).padding(horizontal = 10.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text("Preuve du modèle", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White, modifier = Modifier.semantics { heading() })
                val beat = e.beatHold
                if (e.available && !beat.isNullOrBlank()) Badge("bat la détention : $beat", Tone.NEUTRAL)
            }
            if (e.text.isNotBlank()) Text(e.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.92f))
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(color = AltimColors.cyan, fontWeight = FontWeight.SemiBold)) { append("Voir la validation du modèle →") }
                    e.asOf?.let { withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" · calculée le ${Format.shortDateTime(it)}") } }
                },
                fontSize = 13.sp,
                modifier = Modifier.fillMaxWidth()
                    .then(if (openValidation != null) Modifier.clickable(role = Role.Button, onClick = openValidation) else Modifier)
                    .padding(vertical = 4.dp),
            )
        }
    }
}

// ---------- Guidance: when not to trade, action zones, counter-argument, why the signal changed ----------

private fun zoneColor(kind: String): Color = when (kind) {
    "invalidation" -> Color(0xFFFF6B82)
    "exit" -> AltimColors.orange
    "buy" -> AltimColors.buy
    "profit" -> AltimColors.cyan
    else -> Color(0xFFC7D0E6)
}

/** Vertical ladder, highest price at the top; bands in the kind's colour, the price marked where it sits. */
@Composable
fun ActionLadder(z: ActionZones) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SubTitle("Zones d'action")
        Text(z.hereText, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
        Column(Modifier.fillMaxWidth().semantics { contentDescription = "Zones d'action, du prix le plus haut au plus bas" }) {
            Guidance.ladder(z).forEach { row ->
                when (row) {
                    is Guidance.LadderRow.Marker -> Text(
                        row.text, style = mono(12.sp, FontWeight.Bold), color = AltimColors.warning,
                        modifier = Modifier.fillMaxWidth().border(1.dp, AltimColors.warning.copy(alpha = 0.6f), RoundedCornerShape(6.dp)).padding(horizontal = 8.dp, vertical = 4.dp),
                    )
                    is Guidance.LadderRow.Zone -> ZoneRung(row.zone, row.here, z.price)
                }
            }
        }
    }
}

@Composable
private fun ZoneRung(zone: ActionZone, here: Boolean, price: Double) {
    val c = zoneColor(zone.kind)
    Row(
        Modifier.fillMaxWidth().height(IntrinsicSize.Min)
            .then(if (here) Modifier.clip(RoundedCornerShape(topEnd = 10.dp, bottomEnd = 10.dp)).background(Color.White.copy(alpha = 0.05f)) else Modifier)
            .padding(vertical = 6.dp)
            .semantics(mergeDescendants = true) { if (here) stateDescription = "vous êtes ici" },
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // Band: a bar for a zone, a thin line for the single invalidation level.
        Box(Modifier.width(6.dp).fillMaxHeight(), contentAlignment = Alignment.Center) {
            Box(
                Modifier.width(6.dp).then(if (zone.kind == "invalidation") Modifier.height(2.dp) else Modifier.fillMaxHeight())
                    .clip(RoundedCornerShape(3.dp)).background(c.copy(alpha = 0.8f)),
            )
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(zone.label, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = c)
            Text(Guidance.zoneRange(zone), style = mono(12.sp), color = Color.White)
            if (here) Text("◀ vous êtes ici · ${Guidance.usd(price)}", style = mono(12.sp, FontWeight.Bold), color = AltimColors.warning)
            if (zone.note.isNotBlank()) Caption(zone.note)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CounterBlock(c: CounterArgument) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SubTitle("Contre-argument")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(buildAnnotatedString { append("🟢 Raisons favorables : "); withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("${c.favourable}") } }, fontSize = 14.sp, color = Color.White)
            Text(buildAnnotatedString { append("🔴 Raisons défavorables : "); withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("${c.unfavourable}") } }, fontSize = 14.sp, color = Color.White)
        }
        if (c.familiesText.isNotBlank()) Caption(c.familiesText)
        if (c.invalidators.isNotEmpty()) {
            Text("Points qui pourraient invalider le scénario", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
            c.invalidators.forEach { i ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("•", color = AltimColors.textSecondary, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                    Text(i.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f))
                }
            }
        }
    }
}

/** "Pourquoi le signal a changé depuis le 28/09 à 14:02": what the measurements say changed. */
@Composable
fun ChangeBlock(t: ConfigTransition) {
    val shape = RoundedCornerShape(12.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.cyan.copy(alpha = 0.04f)).border(1.dp, AltimColors.cyan.copy(alpha = 0.35f), shape)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        SubTitle("Pourquoi le signal a changé depuis le ${Format.shortDateTime(t.since)}")
        Text(ConfigChanges.changeSummary(t), fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        if (t.changes.isNotEmpty()) {
            t.changes.forEach { line ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("•", color = AltimColors.textSecondary, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                    Text(line, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f))
                }
            }
        } else {
            Caption("Mesures de la décision précédente non enregistrées (vue avant cette version) : changement non détaillé.")
        }
    }
}

/** Label above, value below: long values stay readable at 360 dp. "—" or null: "non disponible". */
@Composable
private fun Stacked(label: String, value: String?) {
    val missing = value == null || value == "—"
    Column(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(label, color = AltimColors.textSecondary, fontSize = 14.sp)
        Text(if (missing) NA else value!!, style = mono(13.sp), color = if (missing) AltimColors.textSecondary else Color.White)
    }
}

/** Two rows per ratio: today vs median and range, then where today stands in the window. */
@Composable
private fun HistoryRows(name: String, r: Fundamentals.Stock.RatioHistory?) {
    if (r == null) return
    Stacked("$name sur la période", "${num(r.current, 1)} aujourd'hui · médiane ${num(r.median, 1)} · de ${num(r.min, 1)} à ${num(r.max, 1)}")
    Stacked("Centile du $name", "plus haut que ${num(r.percentile, 0)} % des ${Format.count(r.days.toDouble())} jours (${Format.nyDate(r.from)} – ${Format.nyDate(r.to)})")
}

@Composable
private fun StableRows(s: Fundamentals.Crypto.StablecoinFlows?, label: String) {
    if (s == null) return
    Stacked("Stablecoins ($label)", "${Format.compactUsd(s.total)} au ${Format.nyDate(s.date)}")
    Stacked("… sur 7 jours", if (s.change7d != null) "${signedUsd(s.change7d)} (${pc(s.change7dPct, 2, true)})" else null)
    Stacked("… sur 30 jours", if (s.change30d != null) "${signedUsd(s.change30d)} (${pc(s.change30dPct, 2, true)})" else null)
}

// ---------- Market regime, composite score and technical structure (web DecisionCard.tsx) ----------

@Composable
private fun RegimeLine(r: MarketRegime) {
    Text(
        buildAnnotatedString {
            withStyle(SpanStyle(color = AltimColors.textSecondary)) { append("Régime de marché : ") }
            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("${r.kind.emoji} ${r.kind.label}") }
            if (r.reasons.isNotEmpty()) withStyle(SpanStyle(color = AltimColors.textSecondary, fontSize = 12.sp)) { append(" — ${r.reasons.joinToString(" · ")}") }
        },
        fontSize = 13.sp, color = Color.White,
    )
}

/** Score −100 … +100 on one thin axis centred on 0. */
@Composable
private fun ScoreBar(v: Double, label: String) {
    val w = (minOf(100.0, abs(v)) / 100).toFloat()
    val c = if (v >= 0) AltimColors.buy else AltimColors.sell
    Row(
        Modifier.fillMaxWidth().height(4.dp).clip(CircleShape).background(Color.White.copy(alpha = 0.08f))
            .clearAndSetSemantics { contentDescription = "$label : ${signedScore(v)} sur une échelle de −100 à +100" },
    ) {
        Box(Modifier.weight(1f).height(4.dp)) {
            if (v < 0 && w > 0f) Box(Modifier.align(Alignment.CenterEnd).fillMaxWidth(w).height(4.dp).background(c))
        }
        Box(Modifier.weight(1f).height(4.dp)) {
            if (v > 0 && w > 0f) Box(Modifier.align(Alignment.CenterStart).fillMaxWidth(w).height(4.dp).background(c))
        }
    }
}

@Composable
private fun ScoreBlock(s: Decision.CompositeScore) {
    val v = s.value
    if (v == null) {
        Caption(s.text)
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        KeyValue("Score composite", "${signedScore(v)} · ${s.label}", if (v >= 20) Tone.GOOD else if (v <= -20) Tone.BAD else null)
        ScoreBar(v, "Score composite")
        s.factors.forEach { f ->
            Column(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        buildAnnotatedString {
                            append(f.label)
                            withStyle(SpanStyle(color = AltimColors.textSecondary, fontSize = 11.sp)) {
                                append(if (f.value == null) " non mesuré" else " poids ${num(f.applied, 1)} %")
                            }
                        },
                        fontSize = 13.sp, color = Color.White, modifier = Modifier.weight(1f),
                    )
                    Text(f.value?.let { signedScore(it) } ?: "—", style = mono(12.sp), color = Color.White)
                }
                f.value?.let { ScoreBar(it, f.label) }
            }
        }
        Caption(s.text)
    }
}

private fun pts(v: Double) = "${if (v > 0) "+" else if (v < 0) "−" else ""}${num(abs(v), 1)} pts"
private fun usdPx(v: Double) = Format.price(v)

private class StructureRow(val name: String, val bias: Bias? = null, val reading: String, val extra: String? = null, val list: List<String> = emptyList())

/** Every item of the structure, measured or not (a missing one says why rather than disappearing). */
@Composable
private fun StructureList(st: Decision.Structure) {
    val na = "Non disponible : historique trop court"
    val noVolume = "Non disponible : volume absent ou historique trop court"
    val rows = buildList {
        add(
            st.ichimoku?.let {
                StructureRow(
                    "Ichimoku (9, 26, 52)", it.bias, it.reading,
                    "Tenkan ${usdPx(it.tenkan)} · Kijun ${usdPx(it.kijun)} · nuage ${usdPx(minOf(it.senkouA, it.senkouB))} – ${usdPx(maxOf(it.senkouA, it.senkouB))}",
                )
            } ?: StructureRow("Ichimoku (9, 26, 52)", reading = na),
        )
        add(st.supertrend?.let { StructureRow("Supertrend (ATR 10 × 3)", it.bias, it.reading) } ?: StructureRow("Supertrend (ATR 10 × 3)", reading = na))
        add(
            st.donchian?.let { StructureRow("Canal de Donchian (20)", it.bias, it.reading, "Haut ${usdPx(it.upper)} · milieu ${usdPx(it.mid)} · bas ${usdPx(it.lower)}") }
                ?: StructureRow("Canal de Donchian (20)", reading = na),
        )
        add(st.vwap?.let { StructureRow("VWAP glissant (${it.bars} bougies)", it.bias, it.reading) } ?: StructureRow("VWAP glissant", reading = noVolume))
        add(
            st.volumeProfile?.let { StructureRow("Profil de volume (approximation)", it.bias, it.reading, it.note) }
                ?: StructureRow("Profil de volume (approximation)", reading = noVolume),
        )
        add(
            st.pivots?.let {
                StructureRow(
                    "Points pivots (dernière séance)", null, it.reading,
                    "S2 ${usdPx(it.s2)} · S1 ${usdPx(it.s1)} · P ${usdPx(it.pivot)} · R1 ${usdPx(it.r1)} · R2 ${usdPx(it.r2)}",
                )
            } ?: StructureRow("Points pivots", reading = na),
        )
        add(
            StructureRow(
                "Supports et résistances", null, st.levelsReading,
                list = st.levels.take(8).map { l -> "${if (l.kind == "support") "Support" else "Résistance"} ${usdPx(l.price)} · ${l.touches} contacts · ${pc(l.distancePct, 1, true)}" },
            ),
        )
        add(st.breakout?.let { StructureRow("Cassure", it.bias, it.reading) } ?: StructureRow("Cassure", reading = na))
        add(
            st.marketStructure?.let { StructureRow("Structure (sommets et creux)", it.bias, it.reading) }
                ?: StructureRow("Structure (sommets et creux)", reading = "Non disponible : pas assez de sommets et de creux"),
        )
        st.relative.forEach { r ->
            add(
                StructureRow(
                    "Force relative contre ${r.benchmark}", r.bias, r.reading,
                    list = r.periods.map { x -> "${x.label} : ${pc(x.assetPct, 1, true)} contre ${pc(x.benchmarkPct, 1, true)} (${pts(x.diff)})" },
                ),
            )
        }
        if (st.relative.isEmpty()) add(StructureRow("Force relative", reading = st.relativeNote ?: "Non disponible"))
    }
    Caption("${st.timeframe}.${st.score?.let { " Direction d'ensemble : ${signedScore(it)}/100." } ?: ""}")
    rows.forEach { r ->
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(r.name, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, modifier = Modifier.weight(1f))
                r.bias?.let { b ->
                    val c = when (b) { Bias.BULLISH -> AltimColors.buy; Bias.BEARISH -> AltimColors.sell; Bias.NEUTRAL -> AltimColors.warning }
                    Text("${b.arrow} ${b.label}", fontSize = 12.sp, color = c, fontWeight = FontWeight.SemiBold)
                }
            }
            Text(r.reading, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
            r.extra?.let { Caption(it) }
            r.list.forEach { Text("• $it", fontSize = 12.sp, color = Color.White.copy(alpha = 0.85f)) }
        }
    }
}
