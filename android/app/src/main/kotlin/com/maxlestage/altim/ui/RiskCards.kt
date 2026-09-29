package com.maxlestage.altim.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.BetaEstimate
import com.maxlestage.altim.kit.Danger
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.LimitCheck
import com.maxlestage.altim.kit.LimitLevel
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.RiskPortfolio
import com.maxlestage.altim.kit.StressResult
import java.util.Locale
import kotlin.math.abs

// Risk cards of Mes avoirs (same texts as web/src/webapp/RiskCards.tsx): the portfolio against the user's limits, and
// market shocks passed through each line's beta. Stacked rows, no table: nothing wider than a 360 dp phone.

private fun usd(v: Double) = Format.amount(abs(v), 0)
private fun pc(v: Double) = "${Format.plain(abs(v), 1)} %"

private fun limitIcon(l: LimitLevel) = when (l) {
    LimitLevel.DANGER -> "⛔"
    LimitLevel.WARNING -> "⚠"
    LimitLevel.OK -> "✔"
    LimitLevel.NA -> "?"
}

private fun limitColor(l: LimitLevel) = when (l) {
    LimitLevel.DANGER -> AltimColors.sell
    LimitLevel.WARNING -> AltimColors.warning
    LimitLevel.OK -> AltimColors.buy
    LimitLevel.NA -> AltimColors.textSecondary
}

/** One line of a card: icon (with its word for TalkBack), bold title, then the detail. */
@Composable
private fun InsightRow(icon: String, color: Color, title: String, detail: String, extra: String? = null) {
    Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(icon, color = color, fontSize = 15.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, modifier = Modifier.widthIn(min = 18.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = Color.White)
            Text(detail, fontSize = 13.sp, color = Color.White.copy(alpha = 0.88f))
            extra?.let { Caption(it) }
        }
    }
}

/** The portfolio against the user's own limits (Réglages). */
@Composable
fun LimitsCard(checks: List<LimitCheck>) {
    if (checks.isEmpty()) return
    Card(title = "Vos limites de risque") {
        checks.forEach { c -> InsightRow(limitIcon(c.level), limitColor(c.level), c.label, c.detail) }
        Caption(
            "Limites réglables dans Réglages → Prudence des conseils. Hypothèse : stop que vous avez saisi, sinon stop de protection à 2 × la volatilité " +
                "journalière ; variation du jour mesurée depuis la clôture de la veille (journée UTC pour les cryptos, dernière séance pour les actions).",
        )
    }
}

/** Market shocks passed through each line's beta: loss in $ and %, worst line, what the cash cushions. */
@Composable
fun StressCard(p: RiskPortfolio, results: List<StressResult>, betas: Map<String, BetaEstimate>) {
    val assets = p.lines.distinctBy { it.key }
    val estimated = assets.filter { betas[it.key]?.estimated == true }
    val reference = assets.filter { betas[it.key]?.reference == true }
    val fallback = assets.filter { betas[it.key]?.estimated != true && betas[it.key]?.reference != true }
    val days = estimated.map { betas.getValue(it.key).days }
    Card(title = "Scénarios de crise") {
        Caption(
            "Ce que perdrait votre portefeuille si les marchés chutaient d'un coup : chaque ligne bouge selon son bêta face à sa référence " +
                "(${PortfolioRisk.benchmarkLabel(Kind.CRYPTO)} pour les cryptos, ${PortfolioRisk.benchmarkLabel(Kind.STOCK)} pour les actions)." +
                if (p.cash > 0) " Vos liquidités (${usd(p.cash)}) ne bougent pas." else "",
        )
        results.forEach { r ->
            val color = if (r.lossPercent >= 10) AltimColors.sell else if (r.loss > 0) AltimColors.warning else AltimColors.buy
            val cushion = if (p.cash > 0 && r.loss > 0) " (${pc(r.investedLossPercent)} de vos placements : les liquidités amortissent ${pc(r.investedLossPercent - r.lossPercent)})" else ""
            InsightRow(
                if (r.loss > 0) "↘" else "→", color, r.scenario.label,
                "${if (r.loss >= 0) "Perte" else "Gain"} ≈ ${if (r.loss > 0) "−" else "+"}${usd(r.loss)}, soit ${pc(r.lossPercent)} du patrimoine$cushion.",
                r.worst?.let { w -> "Ligne la plus touchée : ${w.symbol} (${if (w.movePercent > 0) "+" else "−"}${pc(w.movePercent)}, −${usd(w.loss)})" },
            )
        }
        val notes = buildString {
            append("Hypothèse : choc instantané, bêta constant. ")
            if (estimated.isNotEmpty()) {
                val lo = days.min()
                val hi = days.max()
                append("Bêta estimé sur $lo${if (hi != lo) " à $hi" else ""} jours de rendements journaliers (")
                append(estimated.joinToString(", ") { "${it.symbol} ${String.format(Locale.ROOT, "%.2f", betas.getValue(it.key).beta)}" })
                append("). ")
            }
            if (reference.isNotEmpty()) append("${reference.joinToString(", ") { it.symbol }} : référence elle-même, bêta 1. ")
            if (fallback.isNotEmpty()) {
                append("Historique trop court (moins de ${PortfolioRisk.MIN_BETA_DAYS} jours communs) ou indisponible pour ${fallback.joinToString(", ") { it.symbol }} : bêta 1 retenu. ")
            }
            append("Une vraie crise peut aller plus loin : les corrélations montent quand tout baisse.")
        }
        Caption(notes)
    }
}

/** Banner above the lines: the daily loss limit reached, and the positions that became dangerous. */
@Composable
fun RiskBanners(checks: List<LimitCheck>, dangers: List<Danger>) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (checks.any { it.code == "daily_loss" && it.level == LimitLevel.DANGER }) {
            Notice("⛔ ${PortfolioRisk.DAILY_LOSS_REACHED}", com.maxlestage.altim.kit.Tone.BAD)
        }
        if (dangers.isNotEmpty()) {
            Notice(
                "${if (dangers.size > 1) "Positions devenues dangereuses" else "Position devenue dangereuse"} : ${dangers.joinToString(", ") { it.symbol }}\n" +
                    "Détail sur chaque ligne ci-dessous. Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque).",
                com.maxlestage.altim.kit.Tone.BAD,
            )
        }
    }
}

/** Reasons shown under a line that became dangerous. */
@Composable
fun DangerBlock(d: Danger) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text("⚠ Position devenue dangereuse", color = AltimColors.sell, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        d.reasons.forEach { Text("• ${it.text}", fontSize = 12.sp, color = Color.White.copy(alpha = 0.9f)) }
    }
}
