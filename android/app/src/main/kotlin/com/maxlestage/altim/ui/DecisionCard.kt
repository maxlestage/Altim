package com.maxlestage.altim.ui

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
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.DecisionLevel
import com.maxlestage.altim.kit.FamilyStatus
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Fundamentals
import com.maxlestage.altim.kit.ScenarioKind
import com.maxlestage.altim.kit.StepState
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Uncertainty
import kotlin.math.abs

private const val NA = "non disponible"

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

/** 421 Md$, 3,2 M$, 950 $ (amounts); without [unit] for counts (supply). */
internal fun big(v: Double?, unit: String = "$"): String {
    if (v == null || !v.isFinite()) return NA
    val a = abs(v)
    val sep = if (unit.isEmpty()) "" else " "
    fun f(x: Double) = Format.plain(x, if (abs(x) < 100) 1 else 0)
    return when {
        a >= 1e9 -> "${f(v / 1e9)} Md$unit".trim()
        a >= 1e6 -> "${f(v / 1e6)} M$unit".trim()
        else -> "${Format.plain(v, 0)}$sep$unit".trim()
    }
}

private fun pct(v: Double?, digits: Int = 1) = v?.takeIf { it.isFinite() }?.let { "${Format.plain(it, digits)} %" } ?: NA
private fun signed(v: Double?, digits: Int = 1) = v?.takeIf { it.isFinite() }?.let { Format.percent(it, digits) } ?: NA
private fun ratio(v: Double?, digits: Int = 1) = v?.takeIf { it.isFinite() }?.let { Format.plain(it, digits) } ?: NA

/** The web card's `pct`: "—" when missing, + only with [sign], true minus, no trailing zeros. */
private fun pc(v: Double?, digits: Int = 1, sign: Boolean = false): String {
    if (v == null || !v.isFinite()) return "—"
    return "${if (v < 0) "−" else if (sign && v > 0) "+" else ""}${Format.plain(abs(v), digits)} %"
}

/** The web card's `num`: "—" when missing. */
private fun num(v: Double?, digits: Int = 2): String = if (v == null || !v.isFinite()) "—" else "${if (v < 0) "−" else ""}${Format.plain(abs(v), digits)}"

/** "+2,8 Md$" / "−271 M$". */
private fun signedUsd(v: Double?): String = if (v == null) "—" else "${if (v < 0) "−" else if (v > 0) "+" else ""}${Format.compactUsd(abs(v))}"

/** French flat tax (PFU) on the net gain, as in the sale tool: an assumption, not the user's own situation. */
private const val FLAT_TAX = 30.0

/** "Décision" card of the asset page: loading, error or the decision itself. */
@Composable
fun DecisionCard(state: Loadable<Decision>, held: Boolean, onSimulate: (() -> Unit)? = null, retry: () -> Unit) {
    when (state) {
        is Loadable.Loading -> Card(title = "Décision") {
            Caption(if (held) "Mode personnel : calcul avec votre prix d'achat et vos pondérations…" else "Mode informationnel…")
            Loading()
        }
        is Loadable.Failed -> Card(title = "Décision") { ErrorBox(state.message, retry) }
        is Loadable.Loaded -> DecisionView(state.value, onSimulate = onSimulate)
    }
}

/** The whole decision: verdict first, then the reasons, what would change it, and the details folded. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun DecisionView(d: Decision, expanded: Boolean = false, onSimulate: (() -> Unit)? = null) {
    val color = levelColor(d.level)
    Card(title = "Décision", glow = color) {
        ModeLine(d)
        // Verdict and level: always the words, the colour only repeats them.
        Column(
            Modifier.fillMaxWidth().semantics(mergeDescendants = true) {
                contentDescription = "Décision : ${d.verdictLabel}. Niveau : ${d.levelText}."
            },
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(d.verdictLabel, color = color, fontSize = 28.sp, fontWeight = FontWeight.Black, letterSpacing = 1.sp)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Dot(color, outlined = d.level == DecisionLevel.WAITING)
                Text(d.levelText, color = color, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                if (d.blocked) Badge("ACHAT BLOQUÉ", Tone.BAD)
            }
        }
        if (d.headline.isNotBlank()) Text(d.headline, fontSize = 14.sp, color = Color.White.copy(alpha = 0.92f))
        d.price?.let { Caption("Prix analysé : ${Format.price(it)} · ${Format.date(d.asOf, time = true)}") }

        Meter("Confiance du modèle", d.confidence, if (d.confidence >= 65) Tone.GOOD else if (d.confidence >= 40) Tone.WARN else Tone.BAD)
        if (d.confidenceText.isNotBlank()) Caption(d.confidenceText)

        if (d.families.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                d.families.forEach { FamilyLight(it) }
            }
        }

        d.exposure?.warning?.let { Notice(it, Tone.WARN) }

        d.plan?.let { PlanBlock(it) } ?: Caption("Pas de plan d'entrée : aucun niveau d'achat net pour l'instant.")

        // Paper trading: follow this decision with virtual money to see whether it holds (no order placed).
        onSimulate?.let {
            OutlinedButton(
                onClick = it,
                modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
                border = BorderStroke(1.2.dp, AltimColors.violet),
            ) { Text("Simuler cet achat", color = Color(0xFFB9A3FF)) }
            Caption("Sans argent réel : la position simulée apparaît dans Mes avoirs → Simulation.")
        }

        Bullets("Pourquoi attendre ?", d.whyWait)
        Conditions("Pour passer en ACHAT", d.toBuy)
        Conditions("Pour passer en VENTE", d.toSell)

        d.position?.let { p ->
            Section("Sorties progressives", "${p.exits.size} étape${if (p.exits.size > 1) "s" else ""}", expanded || p.exits.any { it.now }) {
                KeyValue("Votre prix d'achat moyen", Format.price(p.cost))
                p.pnlPct?.let { KeyValue("Plus ou moins-value", Format.percent(it, 1), if (it >= 0) Tone.GOOD else Tone.BAD) }
                if (p.advice.isNotBlank()) Text(p.advice, fontSize = 13.sp)
                p.exits.forEach { ExitRow(it) }
                Caption("Le reste de la position est conservé. Altim ne passe aucun ordre.")
            }
        }
        if (d.families.isNotEmpty()) {
            val available = d.families.count { it.status != FamilyStatus.UNAVAILABLE }
            Section("Familles d'indicateurs", "$available/${d.families.size} disponibles", expanded) { d.families.forEach { FamilyDetail(it) } }
        }
        if (d.vetoes.isNotEmpty()) {
            val active = d.vetoes.count { it.active }
            Section("Interdictions d'achat", if (active == 0) "aucune active" else "$active active${if (active > 1) "s" else ""}", expanded) {
                d.sortedVetoes.forEach { VetoRow(it) }
            }
        }
        d.setup?.takeIf { it.steps.isNotEmpty() }?.let { s ->
            Section("Setup : ${s.name}", "${s.met}/${s.total}", expanded) { s.steps.forEachIndexed { i, st -> StepRow(i + 1, st) } }
        }
        if (d.scenarios.isNotEmpty()) Section("Scénarios", "${d.scenarios.size}", expanded) { d.scenarios.forEach { ScenarioRow(it) } }
        if (d.pros.isNotEmpty() || d.cons.isNotEmpty()) {
            Section("Points favorables et défavorables", "${d.pros.size} / ${d.cons.size}", expanded) {
                Bullets("Points favorables", d.pros, "+", AltimColors.buy)
                Bullets("Points défavorables", d.cons, "−", AltimColors.sell)
            }
        }
        d.whyNot?.let { w ->
            Section("Pourquoi pas ?", "incertitude ${w.uncertainty.label}", expanded) {
                Caption("Ce qui pourrait rendre cette décision fausse, cherché exprès.")
                Bullets(null, w.risks)
                KeyValue("Incertitude", w.uncertainty.label, when (w.uncertainty) { Uncertainty.LOW -> Tone.GOOD; Uncertainty.MEDIUM -> Tone.WARN; Uncertainty.HIGH -> Tone.BAD })
                Bullets("Invalidation", w.invalidation)
            }
        }
        d.fundamentals?.let { f ->
            when (f) {
                is Fundamentals.Stock -> Section("Fondamentaux", f.period.ifBlank { "entreprise" }, expanded) { StockFundamentals(f) }
                is Fundamentals.Crypto -> Section("Fondamentaux", "jeton et réseau", expanded) { CryptoFundamentals(f) }
                is Fundamentals.Other -> Unit
            }
        }
        d.liquidity?.let { l ->
            Section("Liquidité", l.spreadPct?.let { "écart ${Format.plain(it, 4)} %" } ?: "", expanded) {
                KeyValue("Écart achat/vente", l.spreadPct?.let { "${Format.plain(it, 4)} %" } ?: NA)
                KeyValue("Échangé par jour (20 j)", big(l.dailyValue))
                KeyValue("Volume relatif", l.relativeVolume?.let { "× ${Format.plain(it, 1)}" } ?: NA)
                SourceLine(l.source)
            }
        }
        d.track?.let { t ->
            Section("Historique du signal", "${t.trades} trades", expanded) {
                Caption(t.period)
                KeyValue("Trades", "${t.trades}")
                KeyValue("Taux de réussite", pct(t.winRate))
                KeyValue("Gain moyen / perte moyenne", "${signed(t.avgWin)} / ${signed(t.avgLoss)}")
                KeyValue("Profit factor", ratio(t.profitFactor, 2))
                KeyValue("Sharpe · Sortino", "${ratio(t.sharpe, 2)} · ${ratio(t.sortino, 2)}")
                KeyValue("Pire recul (drawdown)", signed(t.maxDrawdown), Tone.BAD)
                KeyValue("Rendement du signal", signed(t.totalReturn), if (t.totalReturn >= 0) Tone.GOOD else Tone.BAD)
                KeyValue("Simple détention", signed(t.buyAndHold))
                KeyValue("Frais · glissement", "${pct(t.feesPct, 2)} · ${pct(t.slippagePct, 2)}")
                KeyValue("Pire série de pertes", "${t.losingStreak} trade${if (t.losingStreak > 1) "s" else ""}")
                if (t.note.isNotBlank()) Notice(t.note, if (t.totalReturn < t.buyAndHold) Tone.WARN else Tone.GOOD)
                if (t.hasDetails) TrackDetails(t)
            }
        }
        d.exposure?.let { e ->
            Section("Exposition du portefeuille", "${Format.plain(e.weight, 0)} % ${e.factor}", expanded) {
                KeyValue("Facteur de risque", e.factor)
                KeyValue("Part du portefeuille", pct(e.weight, 0), if (e.weight > 50) Tone.WARN else null)
                if (e.assets.isNotEmpty()) KeyValue("Lignes concernées", e.assets.joinToString(", "))
                KeyValue("Corrélation de cet actif", ratio(e.correlation, 2))
                Caption("Actifs corrélés à plus de 0,7 au facteur (rendements journaliers sur 90 jours).")
            }
        }
        if (d.sources.isNotEmpty()) {
            val ok = d.sources.count { it.ok }
            Section("Sources", "$ok/${d.sources.size} disponibles", expanded) {
                d.sources.forEach { s ->
                    Column {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(s.name, fontSize = 13.sp, modifier = Modifier.weight(1f))
                            Badge(if (s.ok) "OK" else "INDISPONIBLE", if (s.ok) Tone.GOOD else Tone.BAD)
                        }
                        if (s.detail.isNotBlank()) Caption(s.detail)
                    }
                }
            }
        }
        HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
        // Always visible, never folded.
        Caption(d.disclaimer.ifBlank { "Pas un conseil en investissement réglementé ; Altim ne passe aucun ordre." })
    }
}

@Composable
private fun ModeLine(d: Decision) {
    val text = if (d.isPersonal) {
        "Mode personnel : calculé avec votre prix d'achat et vos pondérations, transmis pour ce calcul et jamais conservés."
    } else {
        "Mode informationnel : données de marché et scénarios observés, pas une recommandation personnalisée."
    }
    // Lighter violet than the accent: readable on the dark card.
    Text(text, fontSize = 12.sp, color = if (d.isPersonal) Color(0xFFB9A3FF) else AltimColors.cyan)
}

@Composable
private fun Dot(color: Color, outlined: Boolean = false, size: Int = 10) {
    Box(
        Modifier.size(size.dp).clip(CircleShape)
            .background(if (outlined) Color.Transparent else color)
            .border(1.5.dp, color, CircleShape)
            .clearAndSetSemantics { },
    )
}

@Composable
private fun FamilyLight(f: Decision.Family) {
    val c = statusColor(f.status)
    Row(
        Modifier
            .clip(CircleShape)
            .background(c.copy(alpha = 0.10f))
            .border(1.dp, c.copy(alpha = 0.45f), CircleShape)
            .padding(horizontal = 8.dp, vertical = 4.dp)
            .semantics(mergeDescendants = true) { contentDescription = "${f.label} : ${f.status.label}" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Dot(c, outlined = f.status == FamilyStatus.UNAVAILABLE, size = 8)
        Text("${f.label} · ${f.status.label}", fontSize = 11.sp, color = Color.White.copy(alpha = 0.9f))
    }
}

@Composable
private fun SubTitle(text: String) = Text(text, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = Color.White, modifier = Modifier.semantics { heading() })

@Composable
private fun PlanBlock(p: Decision.Plan) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SubTitle("Plan")
        if (p.horizon.isNotBlank()) Caption(p.horizon)
        PlanRow("Zone d'achat", "${px(p.zoneFrom)} – ${px(p.zoneTo)}", null, AltimColors.cyan)
        PlanRow("Stop / invalidation", px(p.stop), Format.percent(-p.riskPct, 1), AltimColors.sell)
        PlanRow("Objectif 1", px(p.target1), Format.percent(p.reward1Pct, 1), AltimColors.buy)
        p.target2?.let { PlanRow("Objectif 2", px(it), p.reward2Pct?.let { r -> Format.percent(r, 1) }, AltimColors.buy) }
        PlanRow(
            "Gain/risque",
            Format.plain(p.riskReward, 1),
            "${if (p.acceptable) "suffisant" else "insuffisant"} (minimum ${Format.plain(p.minRiskReward, 1)})",
            if (p.acceptable) AltimColors.buy else AltimColors.sell,
        )
        Caption("Calculé depuis ${px(p.entry)} : gain jusqu'à l'objectif 1 divisé par la perte jusqu'au stop.")
    }
}

/** 78 400 $ above 1 000 $ (levels of a plan), else the usual price with cents. */
private fun px(v: Double): String = if (abs(v) >= 1000) "${Format.plain(v, 0)} $" else Format.price(v)

/** Label on the left, value and its detail on the right, stacked (no squeezed wrapping at 360 dp). */
@Composable
private fun PlanRow(label: String, value: String, detail: String?, color: Color) {
    Row(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(label, color = AltimColors.textSecondary, fontSize = 14.sp, modifier = Modifier.weight(1f))
        Column(horizontalAlignment = Alignment.End) {
            Text(value, style = mono(14.sp), color = color, textAlign = TextAlign.End)
            detail?.let { Text(it, fontSize = 12.sp, color = color.copy(alpha = 0.8f), textAlign = TextAlign.End) }
        }
    }
}

@Composable
private fun Bullets(title: String?, items: List<String>, mark: String = "•", markColor: Color = AltimColors.textSecondary) {
    if (items.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        title?.let { SubTitle(it) }
        items.forEach { t ->
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(mark, color = markColor, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                Text(t, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f), modifier = Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun Conditions(title: String, items: List<Decision.Condition>) {
    if (items.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SubTitle(title)
        items.forEach { c ->
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.Top) {
                Text("•", color = AltimColors.textSecondary, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                Column(Modifier.weight(1f)) {
                    Text(c.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
                    c.level?.let { Text("Niveau à surveiller : ${px(it)}", style = mono(12.sp), color = AltimColors.cyan) }
                }
            }
        }
    }
}

/** Folded part of the card: a title that opens it (button for TalkBack, with its state). */
@Composable
private fun ColumnScope.Section(title: String, summary: String, initiallyOpen: Boolean, content: @Composable ColumnScope.() -> Unit) {
    var open by rememberSaveable(title) { mutableStateOf(initiallyOpen) }
    HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
            .clickable(role = Role.Button, onClickLabel = if (open) "Replier" else "Déplier") { open = !open }
            .semantics(mergeDescendants = true) { stateDescription = if (open) "déplié" else "replié" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f).padding(vertical = 4.dp)) {
            Text(title, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
            if (summary.isNotBlank()) Text(summary, fontSize = 12.sp, color = AltimColors.textSecondary)
        }
        Icon(if (open) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, contentDescription = null, tint = AltimColors.cyan)
    }
    if (open) Column(verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
}

@Composable
private fun FamilyDetail(f: Decision.Family) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(f.label, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, modifier = Modifier.weight(1f))
            // Unavailable: the badge says it, no score.
            f.score?.let { Text("score ${if (it >= 0) "+" else "−"}${Math.round(abs(it))}", style = mono(12.sp), color = statusColor(f.status)) }
            Badge(f.status.label.uppercase(), when (f.status) { FamilyStatus.POSITIVE -> Tone.GOOD; FamilyStatus.NEGATIVE -> Tone.BAD; FamilyStatus.NEUTRAL -> Tone.WARN; FamilyStatus.UNAVAILABLE -> Tone.NEUTRAL })
        }
        if (f.summary.isNotBlank()) Text(f.summary, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        f.points.forEach { Text("• $it", fontSize = 12.sp, color = Color.White.copy(alpha = 0.8f)) }
        SourceLine(f.source)
    }
}

@Composable
private fun SourceLine(source: String) {
    if (source.isNotBlank() && source != "—") Caption("Source : $source")
}

@Composable
private fun VetoRow(v: Decision.Veto) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(v.label, fontSize = 13.sp, fontWeight = if (v.active) FontWeight.Bold else FontWeight.Normal, modifier = Modifier.weight(1f))
            when {
                v.active -> Badge("ACTIVE", Tone.BAD)
                !v.verifiable -> Badge("NON VÉRIFIABLE", Tone.WARN)
                else -> Badge("OK", Tone.GOOD)
            }
        }
        if (v.detail.isNotBlank()) Caption(v.detail)
    }
}

@Composable
private fun StepRow(n: Int, s: Decision.Step) {
    val (icon, tint) = when (s.state) {
        StepState.OK -> Icons.Filled.CheckCircle to AltimColors.buy
        StepState.NO -> Icons.Filled.Cancel to AltimColors.sell
        StepState.UNKNOWN -> Icons.Filled.QuestionMark to AltimColors.textSecondary
    }
    Row(
        Modifier.semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(icon, contentDescription = s.state.label, tint = tint, modifier = Modifier.size(18.dp))
        Column(Modifier.weight(1f)) {
            Text("$n. ${s.label}", fontSize = 13.sp)
            val detail = listOf(s.state.label, s.detail.takeIf { it.isNotBlank() && it != "—" }).filterNotNull().joinToString(" · ")
            Caption(detail)
        }
    }
}

@Composable
private fun ScenarioRow(s: Decision.Scenario) {
    val c = when (s.kind) { ScenarioKind.BULL -> AltimColors.buy; ScenarioKind.NEUTRAL -> AltimColors.warning; ScenarioKind.BEAR -> AltimColors.sell }
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(s.title.ifBlank { "Scénario ${s.kind.label}" }, color = c, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
        Text("Si ${s.condition.replaceFirstChar { it.lowercase() }} → ${s.consequence}", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        s.level?.let { Caption("Niveau à surveiller : ${px(it)}") }
    }
}

@Composable
private fun ExitRow(e: Decision.Exit) {
    val shape = RoundedCornerShape(12.dp)
    val c = if (e.now) AltimColors.warning else Color.White.copy(alpha = 0.25f)
    Column(
        Modifier.fillMaxWidth().clip(shape).border(1.dp, c, shape).padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Vendre ${Format.plain(e.share, 0)} % si ${e.trigger.replaceFirstChar { it.lowercase() }}", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            if (e.now) Badge("MAINTENANT", Tone.WARN)
        }
        Caption(
            listOfNotNull(
                "Sortie ${e.kind.label}",
                e.price?.let { "à ${px(it)}" },
                if (e.now) "condition déjà remplie" else null,
            ).joinToString(" · "),
        )
    }
}

@Composable
private fun StockFundamentals(f: Fundamentals.Stock) {
    Caption("TTM : les 12 derniers mois publiés.")
    if (f.periodEnd != null && f.filedAt != null) Caption("Comptes arrêtés au ${Format.nyDate(f.periodEnd!!)}, déposés à la SEC le ${Format.nyDate(f.filedAt!!)}.")
    f.sector?.let { Text("Secteur : ${it.label} · ${it.sicDescription} (code SIC ${it.sic})", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
    KeyValue("Chiffre d'affaires (TTM)", big(f.revenue))
    f.revenueGrowth?.let { KeyValue("Croissance du chiffre d'affaires", Format.percent(it, 1)) }
    KeyValue("Résultat net", big(f.netIncome))
    KeyValue("Bénéfice par action", f.eps?.let { Format.price(it) } ?: NA)
    f.epsGrowth?.let { KeyValue("Croissance du bénéfice par action", Format.percent(it, 1)) }
    KeyValue("Marge brute", pct(f.grossMargin))
    KeyValue("Marge opérationnelle", pct(f.operatingMargin))
    KeyValue("Marge nette", pct(f.netMargin))
    KeyValue("Flux de trésorerie libre (FCF)", big(f.freeCashFlow))
    KeyValue("Marge de FCF", pct(f.fcfMargin))
    KeyValue("Dette", big(f.debt))
    KeyValue("Trésorerie", big(f.cash))
    KeyValue("Dette nette", big(f.netDebt))
    KeyValue("ROE", pct(f.roe, 0))
    KeyValue("PER", ratio(f.per))
    KeyValue("PEG", ratio(f.peg, 2))
    KeyValue("EV/EBITDA", ratio(f.evEbitda))
    KeyValue("P/S (capitalisation ÷ ventes)", ratio(f.ps))
    KeyValue("P/B (capitalisation ÷ fonds propres)", ratio(f.pb))
    Stacked(
        "ROIC (rentabilité du capital investi)",
        f.roic?.let { "${pc(it, 1)} (impôt ${pc(f.roicTaxRate, 1)}${if (f.roicTaxStatutory) " : taux légal américain, taux effectif non calculable" else ", taux effectif"})" },
    )
    KeyValue("Rendement du dividende", pct(f.dividendYield, 2))
    KeyValue("Nombre d'actions (1 an)", signed(f.shareChange))
    KeyValue(
        "Prochains résultats",
        f.nextEarnings?.let { "${Format.date(it.date)}${if (it.estimated) " (date estimée)" else " (date annoncée)"}" } ?: NA,
    )
    if (f.surprises.isNotEmpty()) {
        SubTitle("Surprises sur le bénéfice")
        f.surprises.forEach { s ->
            KeyValue(s.quarter, "${Format.price(s.eps)} vs ${Format.price(s.consensus)} (${Format.percent(s.surprisePct, 1)})", if (s.surprisePct >= 0) Tone.GOOD else Tone.BAD)
        }
    }
    f.revisions?.let { r ->
        KeyValue("Révisions du consensus (1 mois)", "${Format.price(r.monthAgo)} → ${Format.price(r.now)} (${Format.percent(r.changePct, 1)})", if (r.changePct >= 0) Tone.GOOD else Tone.BAD)
    }
    f.valuationHistory?.let { h ->
        SubTitle("Valorisation par rapport à sa propre histoire")
        f.valuationVerdict?.let { Text("$it.", fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f)) }
        HistoryRows("PER", h.per)
        HistoryRows("P/S", h.ps)
        Caption("${h.method}. Source : ${h.source}.")
    }
    val c = f.peers
    if (c != null) {
        SubTitle("Comparaison sectorielle")
        if (f.sectorNote.isNotBlank()) Text(f.sectorNote, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        Bullets(
            null,
            c.peers.map { p ->
                "${p.name} (${p.symbol}) : PER ${num(p.per, 1)}, P/S ${num(p.ps, 1)}, marge opérationnelle ${pc(p.operatingMargin)}, chiffre d'affaires ${pc(p.revenueGrowth, 1, true)} sur un an"
            },
        )
        Caption("Cours du ${Format.nyDate(c.date)}. Source : ${c.source}.")
    } else if (f.sectorNote.isNotBlank()) Caption(f.sectorNote)
    if (f.guidance.isNotBlank()) Caption(f.guidance)
    SourceLine(f.source)
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

/**
 * More of the signal's track record (web TrackDetails.tsx): spread cost, expectancy, R multiples, results by market
 * regime, how the test avoids flattering itself, and the tax note.
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
        SubTitle("Selon le régime de marché")
        regimes.forEach { g ->
            val color = if (g.lowSample) AltimColors.textSecondary else if ((g.avgReturn ?: 0.0) >= 0) AltimColors.buy else AltimColors.warning
            val icon = when (g.regime) { "bull" -> "↗"; "bear" -> "↘"; "crisis" -> "⚠"; else -> "→" }
            Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(icon, color = color, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(g.label, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                    val figures = "${g.trades} trade${if (g.trades > 1) "s" else ""}" +
                        if (g.trades > 0) " · réussite ${pc(g.winRate, 0)} · moyenne ${pc(g.avgReturn, 1, true)}" else ""
                    Text(figures, fontSize = 12.sp, color = Color.White.copy(alpha = 0.85f))
                    if (g.lowSample) Badge("échantillon trop faible", Tone.NEUTRAL)
                }
            }
        }
        Caption(
            "Haussier : clôture au-dessus d'une moyenne 200 jours qui monte (sur 20 jours) ; baissier : sous une moyenne qui baisse ; crise : plus de 30 % sous le plus haut de l'année. " +
                "Régime lu à la date du signal, sans données futures.",
        )
    }
    Bullets("Comment ce test évite de se flatter", t.biasNotes)
    Caption(
        "Impôt (hypothèse : flat tax de ${Format.plain(FLAT_TAX, 0)} % sur le gain net, payée à la fin, pertes compensées) : rendement du signal après impôt ≈ ${pc(afterTax, 1, true)}. " +
            "Votre situation fiscale peut différer.",
    )
}

@Composable
private fun CryptoFundamentals(f: Fundamentals.Crypto) {
    KeyValue("Capitalisation", big(f.marketCap))
    KeyValue("Valorisation diluée (FDV)", big(f.fdv))
    KeyValue("Capitalisation / FDV", ratio(f.mcFdv, 2))
    KeyValue("Offre en circulation", pct(f.circulatingPct))
    if (f.circulatingSupply != null || f.maxSupply != null) {
        Caption("En circulation ${big(f.circulatingSupply, "")} · totale ${big(f.totalSupply, "")} · maximum ${big(f.maxSupply, "")}")
    }
    KeyValue("Valeur verrouillée (TVL)", big(f.tvl))
    KeyValue("Frais (30 j)", big(f.fees30d))
    KeyValue("Dominance du bitcoin", pct(f.btcDominance))
    KeyValue("Financement (8 h)", f.fundingRate?.let { "${Format.plain(it * 100, 4)} %" } ?: NA)
    KeyValue("Intérêt ouvert (OI)", big(f.openInterest))
    f.txPerDay?.let { KeyValue("Transactions par jour", Format.plain(it, 0)) }
    f.hashRate?.let { KeyValue("Taux de hachage", "${Format.plain(it / 1e18, 0)} EH/s") }
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text("Déblocages de jetons", color = AltimColors.textSecondary, fontSize = 14.sp)
        Text(f.unlocks.ifBlank { NA }, fontSize = 13.sp)
    }
    if (f.stablecoins != null || f.chainStablecoins != null) {
        SubTitle("Flux de stablecoins")
        StableRows(f.stablecoins, "tous réseaux")
        StableRows(f.chainStablecoins, "réseau ${f.chainStablecoins?.scope ?: ""}")
        Caption("Liquidité disponible sur le marché crypto. Source : ${(f.stablecoins ?: f.chainStablecoins)?.source}.")
    }
    if (f.devActivityKnown) {
        SubTitle("Activité de développement")
        val a = f.devActivity
        if (a != null) {
            Stacked("Commits sur 4 semaines", a.commits4w?.let { Format.count(it) })
            Stacked(
                "Lignes ajoutées / supprimées (4 semaines)",
                if (a.additions4w != null && a.deletions4w != null) "+${Format.count(a.additions4w)} / −${Format.count(a.deletions4w)}" else null,
            )
            Stacked("Pull requests intégrées (total)", a.pullRequestsMerged?.let { Format.count(it) })
            Stacked("Contributeurs", a.contributors?.let { Format.count(it) })
            Stacked("Étoiles", a.stars?.let { Format.count(it) })
            SourceLine(a.source)
        } else Caption("Non disponible (CoinGecko ne la publie plus et le dépôt GitHub du projet n'a pas répondu).")
    }
    if (f.notCovered.isNotBlank()) Caption("${f.notCovered}.")
    SourceLine(f.source)
}
