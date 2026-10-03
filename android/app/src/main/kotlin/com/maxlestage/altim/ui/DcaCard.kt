package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.Currency
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.DcaResult
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Tone
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs

private fun signed(v: Double) = (if (v >= 0) "+" else "−") + String.format(Locale.FRANCE, "%.1f", abs(v)).removeSuffix(",0") + " %"
/** A dollar amount in the display currency, whole ("1 000 €"). */
private fun dollars(v: Double) = Format.amount(v, 0)
private val LONG_DAY = DateTimeFormatter.ofPattern("d MMM yyyy", Locale.FRANCE).withZone(ZoneOffset.UTC)

/** "If I had invested 100 € every month": regular purchases replayed on the real daily closes of the asset. */
/** One purchase repeats nothing: its "next purchase" is far beyond any period. */
private const val ONCE = 100_000

/**
 * "If I had invested 1 000 €": one purchase by default (not everybody wants to spend every month), regular
 * purchases as an option, replayed on the real daily closes of the asset.
 */
@Composable
fun DcaCard(model: AppModel, asset: Asset) {
    var amountText by rememberSaveable { mutableStateOf("1000") }
    var every by rememberSaveable { mutableIntStateOf(ONCE) }
    val once = every == ONCE
    var days by rememberSaveable { mutableIntStateOf(365) }
    var closes by remember(asset.id) { mutableStateOf<List<Pair<Long, Double>>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(asset.id, days) {
        val client = model.client ?: return@LaunchedEffect
        closes = null
        try {
            closes = client.history(listOf(asset), if (days == 730) 730 else 365).byId[asset.id].orEmpty()
            error = null
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: Exception) {
            error = e.message ?: "Historique indisponible"
        }
    }
    // Typed in the display currency, replayed in dollars on the dollar closes.
    val amount = Money.fromDisplay(Format.parse(amountText) ?: 0.0).takeIf { it.isFinite() } ?: 0.0
    val r = closes?.let { if (amount > 0) DcaResult.simulate(it, amount, every, days) else null }

    Card(title = "Si j'avais investi") {
        OutlinedTextField(
            value = amountText,
            onValueChange = { amountText = it },
            label = { Text(if (once) "Montant investi" else "Montant par achat") },
            suffix = { Text(Money.symbol()) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
            modifier = Modifier.fillMaxWidth(),
            colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
        )
        Choice(listOf(ONCE to "Une fois", 7 to "Chaque semaine", 30 to "Chaque mois"), every) { every = it }
        Choice(listOf(182 to "6 mois", 365 to "1 an", 730 to "2 ans"), days) { days = it }
        when {
            error != null -> Notice(error!!, Tone.BAD)
            closes == null -> Loading("Chargement de l'historique…")
            r == null -> Caption(if (amount > 0) "Pas assez d'historique pour ${asset.symbol} sur cette période." else "Indiquez un montant.")
            else -> {
                val firstDay = LONG_DAY.format(Instant.ofEpochMilli(r.first))
                KeyValue(
                    if (once) "${dollars(r.invested)} investis le $firstDay" else "${r.buys} achats · ${dollars(r.invested)} investis",
                    "${dollars(r.value)} (${signed(r.gain)})",
                    if (r.gain >= 0) Tone.GOOD else Tone.BAD,
                )
                DcaChart(r)
                if (!once) KeyValue("Tout investi le $firstDay", "${dollars(r.lumpValue)} (${signed(r.lumpGain)})", if (r.lumpGain >= 0) Tone.GOOD else Tone.BAD)
                KeyValue(if (once) "Prix d'achat" else "Prix moyen payé", Format.price(r.averagePrice))
                KeyValue("Prix à la dernière clôture", Format.price(r.lastPrice))
                if (Money.displayCurrency() == Currency.EUR) Caption("Rejoué en $ sur les cours en dollars, puis converti au taux du jour : l'effet de change passé (EUR/USD) n'est pas compté.")
                Caption(
                    when {
                        once -> "Un seul achat, à la clôture de ce jour-là."
                        r.lumpGain > r.gain -> "Sur cette période, tout acheter le premier jour a mieux rendu : le prix a surtout monté."
                        r.lumpGain < r.gain -> "Sur cette période, étaler les achats a mieux rendu : ils ont profité des baisses."
                        else -> "Sur cette période, les deux façons d'investir reviennent au même."
                    } + " Rejoué sur les vraies clôtures journalières, sans frais ni impôts ; le passé ne dit pas ce qui arrivera.",
                )
            }
        }
    }
}

@Composable
private fun Choice(options: List<Pair<Int, String>>, selected: Int, onSelect: (Int) -> Unit) {
    ChoiceRow(options, selected, onSelect, fontSize = 13.sp)
}

@Composable
private fun DcaChart(r: DcaResult) {
    val max = r.path.maxOf { maxOf(it.second, it.third) }.takeIf { it > 0 } ?: 1.0
    Canvas(Modifier.fillMaxWidth().height(110.dp).semantics { contentDescription = "${dollars(r.invested)} investis, valeur ${dollars(r.value)}" }) {
        fun x(i: Int) = size.width * i / (r.path.size - 1)
        fun y(v: Double) = size.height * (1 - (v / max).toFloat())
        fun line(sel: (Triple<Long, Double, Double>) -> Double, color: Color, width: Float, dashed: Boolean) {
            val p = Path()
            r.path.forEachIndexed { i, s -> if (i == 0) p.moveTo(x(i), y(sel(s))) else p.lineTo(x(i), y(sel(s))) }
            drawPath(p, color, style = Stroke(width = width, cap = StrokeCap.Round, join = StrokeJoin.Round, pathEffect = if (dashed) PathEffect.dashPathEffect(floatArrayOf(10f, 8f)) else null))
        }
        line({ it.second }, Color.White.copy(alpha = 0.45f), 1.5.dp.toPx(), true)
        line({ it.third }, Color(0xFF3987E5), 2.dp.toPx(), false)
    }
    Caption("Trait plein : valeur · pointillés : somme investie")
}
