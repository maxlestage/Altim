package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.BotAssetRow
import com.maxlestage.altim.kit.BotV4Bot
import com.maxlestage.altim.kit.BotV4Report
import com.maxlestage.altim.kit.BotV4Texts
import com.maxlestage.altim.kit.BotV4View
import com.maxlestage.altim.kit.ModelValidation
import com.maxlestage.altim.kit.Tone

// « Bots sélectifs » (v4) on the Bot screen and in the decision's bot line (Yew `app/bot/v4.rs`): each bot's avis
// today (or « pas d'avis »), its measured precision with the Wilson interval, its signals per year and its status.
// Stacked cards and wrapping rows, nothing wider than a 360 dp phone.

/** The « Bots sélectifs » items, after v3 on the Bot screen. */
fun LazyListScope.botV4Items(v4: BotV4Report) {
    item(key = "v4-label") { Label(BotV4Texts.SECTION_TITLE) }
    item(key = "v4-head") { V4HeadCard(v4) }
    BotV4Texts.cards(v4).forEach { (title, bots) -> item(key = "v4-card-$title") { V4BotsCard(title, bots, v4.tRequired) } }
    item(key = "v4-forward") {
        Card(title = BotV4Texts.FORWARD_TITLE) {
            V4Body(v4.forwardHeadline)
            Caption("Un bot précis sur le passé ne compte dans les décisions qu'après ${v4.parameters.minSignals} signaux sur l'avenir qui le confirment.")
        }
    }
    item(key = "v4-method") {
        Card(title = BotV4Texts.METHOD_TITLE) {
            Bulleted(v4.method)
            V4Fold("Limites") { Bulleted(v4.limits) }
        }
    }
}

@Composable
private fun V4Fold(title: String, content: @Composable () -> Unit) {
    var open by rememberSaveable(title) { mutableStateOf(false) }
    Text(
        "${if (open) "▾" else "▸"} $title",
        fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.cyan,
        modifier = Modifier.fillMaxWidth().clickable(role = Role.Button) { open = !open }.padding(vertical = 6.dp),
    )
    if (open) content()
}

@Composable
private fun V4Body(text: String) = Text(text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))

@Composable
private fun V4Rule() = Box(Modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.08f)))

private fun statusTone(status: String): Tone = when (status) {
    "proven" -> Tone.GOOD
    "contradicted" -> Tone.WARN
    else -> Tone.NEUTRAL
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun V4HeadCard(v4: BotV4Report) {
    Card(title = BotV4Texts.title(v4)) {
        V4Body(v4.headline)
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Tile("Seuil corrigé", "t ≥ ${ModelValidation.plain(v4.tRequired, 2)}")
            Tile("Tests comptés (v1 à v4)", "${v4.k.total}")
            Tile("Bots précis sur le passé", BotV4Texts.preciseCount(v4))
        }
        Caption(BotV4Texts.INTRO)
        if (v4.afterPrereg.isNotEmpty()) {
            Notice("Modifié après le pré-enregistrement :" + v4.afterPrereg.joinToString("") { "\n• $it" }, Tone.WARN)
        }
    }
}

/** Four bots of one group and horizon. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun V4BotsCard(title: String, bots: List<BotV4Bot>, req: Double) {
    Card(title = title) {
        bots.forEach { b ->
            val s = b.main
            V4Rule()
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(BotV4Texts.sideLabel(b.side), fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                Badge(BotV4Texts.statusLabel(b.status), statusTone(b.status))
            }
            Caption("Avis du jour : ${BotV4Texts.todayText(b)}")
            KeyValue("Précision mesurée", BotV4Texts.precisionShort(s))
            KeyValue("Hasard (référence)", BotV4Texts.pct(s.reference))
            KeyValue("Signaux", BotV4Texts.rhythm(s))
            KeyValue("t par jour", BotV4Texts.tText(s, req))
            V4Fold("Détails · ${b.id}") {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Caption("Réussite : ${b.hit}.")
                    Caption("${b.models}.")
                    Caption(b.text)
                }
            }
        }
    }
}

/** "Bots sélectifs : Hausse 20 j, …" of an asset of the basket (nothing before v4). */
@Composable
fun V4AssetNowRow(a: BotAssetRow) {
    val n = a.v4 ?: return
    Caption("Bots sélectifs : " + if (n.bots.isEmpty()) "pas d'avis" else n.bots.joinToString(", ") { BotV4Texts.shortLabel(it) })
}

/** The group's selective bots today in a view (decision line, watched asset): those speaking, and whether they count. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun V4AvisBlock(v: BotV4View) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (!v.available) {
            Caption(v.note)
            return@Column
        }
        Text("Bots sélectifs : ${BotV4Texts.viewHead(v)}", fontSize = 13.sp, color = Color.White.copy(alpha = 0.92f))
        v.speaking.forEach { s ->
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(s.label, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
                Badge(BotV4Texts.statusLabel(s.status), statusTone(s.status))
            }
            Caption(BotV4Texts.signalText(s))
        }
        Caption(v.note)
    }
}
