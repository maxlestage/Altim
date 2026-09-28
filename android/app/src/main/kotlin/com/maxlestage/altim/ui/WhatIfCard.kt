package com.maxlestage.altim.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.FactorBeta
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.RiskPortfolio
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.WhatIf
import java.util.Locale
import kotlin.math.abs

// « Et si… ? » on Mes avoirs (web WhatIfCard.tsx): how the portfolio would move if one market factor (Nasdaq-100,
// S&P 500, Bitcoin) fell by a given shock, each line through its beta to that factor (one year of shared sessions).
// A line without a measurable beta is "non couverte", never guessed. Chips wrap, lines stack.

private fun usd(v: Double) = "${Format.plain(v, 0)} $"
private fun signedUsd(loss: Double) = "${if (loss > 0) "−" else if (loss < 0) "+" else ""}${usd(abs(loss))}"
private fun signedPct(v: Double) = "${if (v > 0) "+" else if (v < 0) "−" else ""}${Format.plain(abs(v), 1)} %"
private fun fixed2(v: Double) = String.format(Locale.ROOT, "%.2f", v)

/** [daily]: the daily candles already loaded (asset id); the factor's own candles are fetched when missing. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun WhatIfCard(portfolio: RiskPortfolio, daily: Map<String, List<Candle>>, client: AltimClient?, initialFactor: String = "qqq") {
    var factorKey by rememberSaveable { mutableStateOf(initialFactor) }
    var shock by rememberSaveable { mutableDoubleStateOf(-10.0) }
    var custom by rememberSaveable { mutableStateOf("") }
    var amountText by rememberSaveable { mutableStateOf("") }
    // Absent: not fetched yet; null: failed.
    val fetched = remember { mutableStateMapOf<String, List<Candle>?>() }
    val f = WhatIf.factor(factorKey)
    val own = daily[f.id]?.takeIf { it.isNotEmpty() }
    LaunchedEffect(f.id, own == null, client) {
        if (own != null || fetched.containsKey(f.id) || client == null) return@LaunchedEffect
        fetched[f.id] = runCatching { client.candles(f.asset, "1d").candles }.getOrNull()
    }
    val factorCandles = own ?: fetched[f.id]
    val loading = factorCandles == null && !fetched.containsKey(f.id) && client != null

    val customShock = Format.parse(custom.trim().removePrefix("−").removePrefix("-"))
    val effShock = if (custom.isNotBlank() && customShock != null && customShock > 0 && customShock <= 100) -customShock else shock
    val amount = Format.parse(amountText)
    val betas = remember(portfolio.lines.map { it.key }, daily, factorCandles) {
        val b = LinkedHashMap<String, FactorBeta>()
        if (!factorCandles.isNullOrEmpty()) for (l in portfolio.lines) if (l.key !in b) b[l.key] = WhatIf.factorBeta(daily[l.key].orEmpty(), factorCandles, WhatIf.BETA_DAYS)
        b
    }
    val r = WhatIf.whatIf(portfolio, f.key, effShock, betas, amount)
    val chipColors = FilterChipDefaults.filterChipColors(selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f), selectedLabelColor = AltimColors.cyan, labelColor = AltimColors.textSecondary)

    Card(title = "Et si… ?") {
        Caption("Comment le risque de ce portefeuille évolue si ${f.name} baisse de ${Format.plain(abs(effShock), 1)} % ? Chaque ligne bouge selon son bêta face à ce marché.")
        Caption("Marché")
        FlowRow(Modifier.semantics { contentDescription = "Marché" }, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            WhatIf.FACTORS.forEach { x -> FilterChip(selected = x.key == factorKey, onClick = { factorKey = x.key }, label = { Text(x.label) }, colors = chipColors) }
        }
        Caption("Choc")
        FlowRow(Modifier.semantics { contentDescription = "Choc" }, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            WhatIf.SHOCKS.forEach { s ->
                FilterChip(
                    selected = custom.isBlank() && s == shock,
                    onClick = {
                        shock = s
                        custom = ""
                    },
                    label = { Text(signedPct(s)) }, colors = chipColors,
                )
            }
        }
        Field(custom, "Autre baisse (%)", "ex. 15") { custom = it }
        Field(amountText, "Montant simulé (USD, facultatif)", usd(portfolio.total)) { amountText = it }
        if (r.scaled) Caption("Simulé sur ${usd(r.base)} répartis selon les poids actuels (liquidités comprises : ${usd(r.cash)}).")

        when {
            loading -> Caption("Chargement des cours de ${f.label}…")
            factorCandles.isNullOrEmpty() -> Notice("Cours journaliers de ${f.label} indisponibles : simulation non couverte pour l'instant.", Tone.WARN)
            else -> {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(2.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                    Text("Perte estimée", fontSize = 14.sp, color = Color.White)
                    Text(signedUsd(r.loss), style = mono(22.sp, FontWeight.Bold), color = if (r.loss > 0) AltimColors.sell else AltimColors.buy)
                    Caption("soit ${signedPct(-r.lossPercent)} de ${if (r.scaled) "ce montant" else "votre patrimoine"}")
                }
                r.worst?.let { w ->
                    Text(
                        buildAnnotatedString {
                            append("Ligne la plus touchée : ")
                            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(w.symbol) }
                            append(" (${signedPct(w.movePercent!!)}, ${signedUsd(w.loss!!)}).")
                        },
                        fontSize = 13.sp, color = Color.White,
                    )
                }
                r.lines.forEach { l ->
                    HorizontalDivider(color = Color.White.copy(alpha = 0.08f))
                    Column(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(l.symbol, style = mono(14.sp, FontWeight.Bold), color = Color.White, modifier = Modifier.weight(1f))
                            Text(
                                l.loss?.let { signedUsd(it) } ?: "non couvert", style = mono(13.sp),
                                color = when { l.loss == null -> AltimColors.textSecondary; l.loss!! > 0 -> AltimColors.sell; else -> AltimColors.buy },
                            )
                        }
                        val detail = when {
                            l.reference -> " · c'est ce marché lui-même (bêta 1)"
                            l.beta == null -> " · moins de ${PortfolioRisk.MIN_BETA_DAYS} jours communs avec ${f.label} : bêta non mesurable"
                            else -> " · bêta ${fixed2(l.beta!!)}${l.correlation?.let { ", corrélation ${fixed2(it)}" } ?: ""} · ${signedPct(l.movePercent!!)}" +
                                if (l.correlation != null && abs(l.correlation!!) < WhatIf.WEAK_CORRELATION) " · lien faible avec ${f.label} : ce bêta explique mal les mouvements de la ligne, résultat peu fiable" else ""
                        }
                        Caption("${usd(l.value)} · ${Format.plain(l.weight, 1)} %$detail")
                    }
                }
                if (r.uncovered.isNotEmpty()) Caption("Non couvert : ${r.uncovered.joinToString(", ")} (${usd(r.uncoveredValue)}) — laissé hors du total, jamais estimé.")
                Caption(
                    (if (r.minDays != null) "Bêta estimé sur ${r.minDays}${if (r.maxDays != r.minDays) " à ${r.maxDays}" else ""} jours de rendements journaliers communs" else "Aucun bêta mesuré") +
                        " ; hypothèse : choc instantané, relation stable — rarement vrai en crise (les corrélations montent quand tout baisse). Les liquidités ne bougent pas. Ce n'est pas une prévision.",
                )
            }
        }
    }
}

@Composable
private fun Field(value: String, label: String, placeholder: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        placeholder = { Text(placeholder, color = AltimColors.textSecondary) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, autoCorrectEnabled = false),
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan, focusedLabelColor = AltimColors.cyan),
    )
}
