package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.RiskLine
import com.maxlestage.altim.kit.SectorExposure
import com.maxlestage.altim.kit.SectorInsightLevel
import com.maxlestage.altim.kit.SectorItem
import com.maxlestage.altim.kit.SectorLine
import com.maxlestage.altim.kit.Sectors
import kotlin.coroutines.cancellation.CancellationException

// "Exposition sectorielle" of Mes avoirs (same texts as web/src/webapp/SectorCard.tsx): stocks by sector (Nasdaq / SEC),
// cryptos as their own block (no cash on Android). One thin single-colour bar per block, values in text colours,
// stacked rows: nothing wider than a 360 dp phone.

/** Single colour of the bars (same as the web card). */
private val SectorBar = Color(0xFF3987E5)

private fun pc(v: Double) = "${Format.plain(v, 1)} %"

/** Loads /api/sectors for the stocks held, then shows the exposure (every stock "Secteur inconnu" when it fails). */
@Composable
fun SectorCard(lines: List<RiskLine>, client: AltimClient?) {
    val stocks = lines.filter { it.kind == Kind.STOCK }.map { it.symbol }.distinct().sorted()
    val key = stocks.joinToString(",")
    // Items by symbol (null when the call failed) and the failure, for this list of stocks.
    var state by remember { mutableStateOf<Triple<String, Map<String, SectorItem>?, String?>?>(null) }
    LaunchedEffect(key, client) {
        state = if (stocks.isEmpty()) Triple(key, emptyMap(), null)
        else if (client == null) Triple(key, null, "hors connexion")
        else try {
            Triple(key, client.sectors(stocks).items.associateBy { it.symbol }, null)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Triple(key, null, e.message ?: "serveur injoignable")
        }
    }
    val s = state?.takeIf { it.first == key }
    val exposure = s?.let {
        Sectors.exposure(
            lines.map { l -> SectorLine(l.symbol, l.kind, l.value) }, 0.0, it.second,
            it.third?.let { e -> "classement indisponible ($e)" } ?: "classement sectoriel indisponible",
        )
    }
    SectorExposureCard(exposure)
}

/** The exposure once computed (null: still loading). */
@Composable
fun SectorExposureCard(e: SectorExposure?) {
    Card(title = "Exposition sectorielle") {
        if (e == null) Caption("Lecture des secteurs de vos actions…") else SectorBody(e)
    }
}

@Composable
private fun SectorBody(e: SectorExposure) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            e.blocks.forEach { b ->
                Column(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(b.label, fontSize = 14.sp, color = Color.White, modifier = Modifier.weight(1f))
                        Text(pc(b.weight), style = mono(14.sp), color = Color.White)
                    }
                    val barShape = RoundedCornerShape(topEnd = 3.dp, bottomEnd = 3.dp)
                    Box(Modifier.fillMaxWidth().height(6.dp).clip(barShape).background(Color.White.copy(alpha = 0.07f))) {
                        Box(
                            Modifier.fillMaxWidth((b.weight / 100).coerceIn(0.0, 1.0).toFloat()).widthIn(min = 2.dp).fillMaxHeight()
                                .clip(barShape).background(SectorBar),
                        )
                    }
                    val sub = b.symbols.joinToString(", ") +
                        if (b.stockWeight != null && e.stockValue < e.total) " · ${pc(b.stockWeight!!)} des actions" else ""
                    if (sub.isNotEmpty()) Caption(sub)
                }
            }
        }
        // The effective number of sectors is shown as a figure below.
        e.insights.filter { it.code != "sector_effective" }.forEach { i ->
            val warning = i.level == SectorInsightLevel.WARNING
            Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(if (warning) "⚠" else "ℹ", color = if (warning) AltimColors.warning else AltimColors.textSecondary, fontSize = 14.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, modifier = Modifier.widthIn(min = 18.dp))
                Text(Sectors.insightText(i), fontSize = 13.sp, color = Color.White.copy(alpha = 0.88f), modifier = Modifier.weight(1f))
            }
        }
        e.effectiveSectors?.let { eff ->
            Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Secteurs effectifs (actions classées)", fontSize = 13.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
                Text(Format.plain(eff, 1), fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
            }
        }
        if (e.unknown.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                e.unknown.forEach { u ->
                    Text(
                        buildAnnotatedString {
                            append("• ")
                            withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(u.symbol) }
                            append(" : ${u.reason}")
                        },
                        fontSize = 12.sp, color = Color.White.copy(alpha = 0.88f),
                    )
                }
            }
        }
        Caption(
            "Sources : ${Sectors.sourceLine(e.bySource).ifEmpty { "aucune action à classer" }}. Crypto : vos avoirs. " +
                "Les secteurs SEC (codes SIC) sont de grandes divisions, affichés à part de ceux du Nasdaq.",
        )
    }
}
