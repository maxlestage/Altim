package com.maxlestage.altim.ui

import com.maxlestage.altim.kit.Money
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Anomaly
import com.maxlestage.altim.kit.AnomalyReport
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Derivatives
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Opportunities
import com.maxlestage.altim.kit.Tone
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

// "Détection d'anomalies" on the asset screen (web AnomaliesCard.tsx): unusual readings (volume, price/volume, z-score;
// OKX derivatives for a crypto) with their threshold and what they may mean, the measures within the normal, and what
// no free source covers. Stacked rows only.

private val PARIS = ZoneId.of("Europe/Paris")
private fun moment(t: Double) = DateTimeFormatter.ofPattern("d MMM 'à' HH:mm", Locale.FRANCE).withZone(PARIS).format(Instant.ofEpochMilli(t.toLong()))
private fun day(t: Double) = DateTimeFormatter.ofPattern("dd/MM/yyyy", Locale.FRANCE).withZone(PARIS).format(Instant.ofEpochMilli(t.toLong()))

@Composable
fun AnomaliesCard(model: AppModel, asset: Asset) {
    var state by remember(asset.id) { mutableStateOf<Loadable<AnomalyReport>>(Loadable.Loading) }
    LaunchedEffect(asset.id) {
        val client = model.client ?: return@LaunchedEffect
        state = try {
            Loadable.Loaded(client.anomalies(asset))
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
            return@LaunchedEffect
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            Loadable.Failed(e.message ?: "Indisponible")
        }
    }
    AnomaliesView(state)
}

/** The card from its state (no server: also rendered by the tests). */
@Composable
fun AnomaliesView(state: Loadable<AnomalyReport>) {
    Card(title = "Détection d'anomalies") {
        when (state) {
            is Loadable.Loading -> Loading()
            is Loadable.Failed -> Notice(state.message, Tone.WARN)
            is Loadable.Loaded -> {
                val r = state.value
                if (r.anomalies.isEmpty()) {
                    Text("Rien d'inhabituel sur les mesures ci-dessous${r.session?.let { " (séance du ${day(it)})" } ?: ""}.", fontSize = 13.sp, color = Color.White)
                } else {
                    r.anomalies.forEach { AnomalyBox(it) }
                }
                r.errors.forEach { Notice(it, Tone.WARN) }
                if (r.normal.isNotEmpty()) {
                    var open by rememberSaveable(r.symbol) { mutableStateOf(false) }
                    TextButton(onClick = { open = !open }) { Text("Mesures dans la normale · ${r.normal.size} ${if (open) "▲" else "▼"}", color = AltimColors.cyan, fontSize = 13.sp) }
                    if (open) {
                        r.normal.forEach { a ->
                            Column {
                                Text(a.title, fontSize = 13.sp, color = Color.White)
                                Text(a.measured, style = mono(12.sp), color = AltimColors.textSecondary)
                            }
                        }
                    }
                }
                r.derivatives?.let { DerivativesBlock(it) }
                Caption("Une anomalie est un écart mesuré, pas une prévision : son sens reste à confirmer.${if (r.source.isNotBlank()) " ${r.source}." else ""}")
            }
        }
    }
}

@Composable
private fun AnomalyBox(a: Anomaly) {
    val c = if (a.severity == "high") AltimColors.sell else AltimColors.warning
    val shape = RoundedCornerShape(12.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(c.copy(alpha = 0.07f)).border(1.dp, c.copy(alpha = 0.45f), shape).padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text("⚠️ ${a.title}", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = Color.White)
        Text(a.measured, style = mono(12.sp), color = c)
        Text(a.meaning, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
        Caption("Source : ${a.source}")
    }
}

@Composable
private fun DerivativesBlock(d: Derivatives) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("DÉRIVÉS ET GROS MOUVEMENTS", color = AltimColors.textSecondary, fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp).semantics { heading() })
        d.errors.forEach { Notice(it, Tone.WARN) }
        d.liquidations?.let { l ->
            val share = Opportunities.longShare(l)
            DerivRow("Liquidations ${if (l.complete) "24 h" else "${Format.plain(l.hours, 1)} h (lecture partielle)"}", Opportunities.compactUsd(l.longUsd + l.shortUsd))
            DerivRow("Acheteurs liquidés (${l.longCount})", Opportunities.compactUsd(l.longUsd) + (share?.let { " · ${Math.round(it)} %" } ?: ""), Tone.BAD)
            DerivRow("Vendeurs liquidés (${l.shortCount})", Opportunities.compactUsd(l.shortUsd), Tone.GOOD)
            l.largest?.let { g -> DerivRow("Plus grosse", "${Opportunities.compactUsd(g.usd)} · ${if (g.long) "acheteur" else "vendeur"} à ${Money.moneyFmt(g.price, " ") { Format.plain(it, 2) }} · ${moment(g.time)}") }
            if (l.scope.isNotBlank()) Caption(l.scope)
        }
        d.openInterest?.let { o ->
            DerivRow(
                "Open interest",
                Opportunities.compactUsd(o.usd) + (o.change24h?.let { " · ${Opportunities.signed(it)} 24 h" } ?: "") + (o.change7d?.let { " · ${Opportunities.signed(it)} 7 j" } ?: ""),
            )
        }
        d.funding?.let { f ->
            DerivRow(
                "Funding (dernier règlement${f.periodHours?.takeIf { it > 0 }?.let { ", toutes les ${Format.plain(it, 0)} h" } ?: ""})",
                "${Opportunities.signed(f.rate, 4)} · habituel ${Opportunities.signed(f.p5, 4)} à ${Opportunities.signed(f.p95, 4)}",
            )
        }
        d.longShort?.let { s ->
            DerivRow("Ratio comptes acheteurs / vendeurs", "${Format.plain(s.ratio, 2)} · habituel ${Format.plain(s.p5, 2)} à ${Format.plain(s.p95, 2)}")
        }
        Caption("Source : ${d.source} ; plages habituelles = 5 à 95 % des valeurs récentes (funding : ≈ 100 derniers règlements ; ratio : 30 j).")
        d.notCovered.forEach { n ->
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("•", color = AltimColors.textSecondary, fontSize = 12.sp)
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("Non couvert — ${n.label}") }
                        append(" : ${n.reason}.")
                    },
                    fontSize = 12.sp, color = Color.White.copy(alpha = 0.85f), modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

/** Label above, value below: the long derivative readings stay readable at 360 dp. */
@Composable
private fun DerivRow(label: String, value: String, tone: Tone? = null) {
    Column(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(1.dp)) {
        Text(label, fontSize = 13.sp, color = AltimColors.textSecondary)
        Text(value, style = mono(13.sp), color = tone?.let { AltimColors.of(it) } ?: Color.White)
    }
}
