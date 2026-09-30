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
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.BotAssetRow
import com.maxlestage.altim.kit.BotTiming
import com.maxlestage.altim.kit.BotV3
import com.maxlestage.altim.kit.BotV3Config
import com.maxlestage.altim.kit.BotV3Group
import com.maxlestage.altim.kit.BotV3Horizon
import com.maxlestage.altim.kit.BotV3Report
import com.maxlestage.altim.kit.BotV3SideStats
import com.maxlestage.altim.kit.BotV3Signal
import com.maxlestage.altim.kit.BotV3VolManaged
import com.maxlestage.altim.kit.ModelValidation
import com.maxlestage.altim.kit.Tone

// « Bot Altim » v3 on the Bot screen (web BotV3.tsx): what changed, the pre-registration date, the corrected threshold
// (K tests), the 4 headline results (2 families × horizons 20 / 60) with raw and required t, the forward test, each
// configuration alone for information, the volatility-managed trend vs holding, and the time / memory of the
// computation. Stacked cards and rows (no table), nothing wider than a 360 dp phone.

/** The v3 section's items, placed first on the Bot screen (v2 kept below as the reference). */
fun LazyListScope.botV3Items(v3: BotV3Report, timing: BotTiming?) {
    val req = v3.tRequired
    item(key = "v3-head") { V3HeadCard(v3) }
    item(key = "v3-results") { Label("Résultats v3 hors échantillon") }
    item(key = "v3-results-note") { Caption(BotV3.resultsNote(v3)) }
    items(v3.groups, key = { "v3-group-${it.id}" }) { V3GroupCard(it, req) }
    item(key = "v3-forward") { Label(BotV3.forwardTitle(v3)) }
    item(key = "v3-forward-card") { V3ForwardCard(v3) }
    item(key = "v3-configs") { Label("Chaque configuration seule") }
    item(key = "v3-configs-note") { Caption("À titre d'information, non utilisé pour choisir : chaque candidat retenu partout, et la sélection v2 refaite sur les nouvelles données.") }
    v3.groups.forEach { g ->
        g.horizons.forEach { h -> item(key = "v3-configs-${g.id}-${h.horizon}") { V3ConfigsCard("${g.label} · ${h.horizon} jours", h.configs, req) } }
    }
    if (v3.groups.any { it.volManaged != null }) {
        item(key = "v3-vol") { Label("Tendance à volatilité gérée") }
        item(key = "v3-vol-note") {
            Caption(
                "Règle publiée, sans apprentissage : investi seulement au-dessus de la moyenne 200 jours, exposition réduite quand la volatilité dépasse sa médiane passée ; " +
                    "comparée à la simple détention, sur les mêmes jours de test.",
            )
        }
        v3.groups.forEach { g -> g.volManaged?.let { v -> item(key = "v3-vol-${g.id}") { V3VolCard(g.label, v) } } }
    }
    item(key = "v3-method") { V3MethodCard(v3, timing) }
}

/** A foldable part of a card (the web's <details>). */
@Composable
private fun Fold(title: String, content: @Composable () -> Unit) {
    var open by rememberSaveable(title) { mutableStateOf(false) }
    Text(
        "${if (open) "▾" else "▸"} $title",
        fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.cyan,
        modifier = Modifier.fillMaxWidth().clickable(role = Role.Button) { open = !open }.padding(vertical = 6.dp),
    )
    if (open) content()
}

@Composable
private fun V3Body(text: String) = Text(text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))

@Composable
private fun V3Chip(s: BotV3SideStats, req: Double) = VerdictChip(s.verdict ?: "unproven", BotV3.verdictLabel(s, req))

@Composable
private fun V3Rule() = Box(Modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.08f)))

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun V3HeadCard(v3: BotV3Report) {
    Card(title = BotV3.title(v3)) {
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Tile("Seuil corrigé", "t ≥ ${ModelValidation.plain(v3.tRequired, 2)}")
            Tile("Tests comptés (v1 à v3)", "${v3.k.total}")
            Tile("Test sur l'avenir", BotV3.signalsWord(BotV3.forwardSignals(v3)))
        }
        Caption(BotV3.protocolText(v3))
        if (v3.afterPrereg.isNotEmpty()) {
            Notice("Modifié après le pré-enregistrement :" + v3.afterPrereg.joinToString("") { "\n• $it" }, Tone.WARN)
        }
        Fold("Ce qui change avec la v3") { Bulleted(v3.changes) }
    }
}

/** One group: per horizon, its 2 headline configurations (the family chosen at each retraining). */
@Composable
fun V3GroupCard(g: BotV3Group, req: Double) {
    Card(title = g.label) {
        Caption(BotV3.groupSubtitle(g))
        g.horizons.forEach { h ->
            V3Rule()
            HorizonHead(h)
            h.configs.filter { it.headline }.forEach { c -> HeadlineConfig(c, req, BotV3.choiceRuns(h.selection, peers = c.family == "peers")) }
        }
    }
}

@Composable
private fun HorizonHead(h: BotV3Horizon) {
    Text(
        buildAnnotatedString {
            withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(BotV3.horizonTitle(h)) }
            withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" · ${BotV3.horizonSubtitle(h)}") }
        },
        fontSize = 13.sp,
    )
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SideChipRow(head: String, s: BotV3SideStats, req: Double) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
        Caption(head)
        V3Chip(s, req)
    }
}

@Composable
private fun HeadlineConfig(c: BotV3Config, req: Double, runs: List<BotV3.ChoiceRun>) {
    val m = c.main
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(BotV3.familyLabel(c.family), fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
        V3Body(BotV3.sideText(c.family, true, m.buy, req))
        SideChipRow(BotV3.sideHead(true, m), m.buy, req)
        m.buyVsMean?.let { Control(it, req, BotV3.proven(m, true)) }
        V3Body(BotV3.sideText(c.family, false, m.sell, req))
        SideChipRow(BotV3.sideHead(false, m), m.sell, req)
        m.sellVsMean?.let { Control(it, req, BotV3.proven(m, false)) }
        listOf(true, false).forEach { buy -> BotV3.pendingText(c, buy, req)?.let { Notice(it, Tone.WARN) } }
        if (m.holdAssets > 0) KeyValue("Achats cumulés / détention (médianes)", BotV3.holdText(m))
        Fold("Modèle retenu à chaque réentraînement") { Caption(BotV3.runsText(runs)) }
    }
}

/** Family « peers » against the group's equal-weight mean (control added after the first real run), and what counts. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Control(s: BotV3SideStats, req: Double, proven: Boolean) {
    Column(Modifier.fillMaxWidth().padding(start = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Caption(BotV3.controlText(s, req))
            V3Chip(s, req)
        }
        Text(BotV3.controlVerdict(proven), fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
    }
}

@Composable
private fun V3ForwardCard(v3: BotV3Report) {
    Card {
        V3Body(v3.forwardHeadline)
        Caption(BotV3.forwardNote(v3))
        v3.groups.forEach { g ->
            BotV3.headlineConfigs(g).forEach { h ->
                V3Rule()
                Text(BotV3.forwardRowTitle(g, h), fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
                Caption(BotV3.forwardText(h.c.forward, v3.tRequired))
            }
        }
    }
}

/** Every configuration of a group and horizon (headline included), compact rows. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun V3ConfigsCard(title: String, configs: List<BotV3Config>, req: Double) {
    Card(title = title) {
        configs.forEach { c ->
            V3Rule()
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(c.label, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                if (c.headline) Badge("principal", Tone.NEUTRAL)
                if (c.candidate != null) Badge(BotV3.chosenText(c), Tone.NEUTRAL)
            }
            listOfNotNull(BotV3.configBuyRow(c, req), BotV3.configSellRow(c, req), BotV3.configMeanRow(c)).forEach { (k, v) -> KeyValue(k, v) }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                V3Chip(c.main.buy, req)
                V3Chip(c.main.sell, req)
            }
        }
    }
}

@Composable
fun V3VolCard(label: String, v: BotV3VolManaged) {
    Card(title = label) {
        Caption(BotV3.volSubtitle(v))
        BotV3.volRows(v).forEach { r -> KeyValue(r.label, "gérée ${r.managed} · détention ${r.hold}") }
        V3Body(v.text)
    }
}

@Composable
private fun V3MethodCard(v3: BotV3Report, timing: BotTiming?) {
    Card(title = "Comment la v3 est jugée") {
        Bulleted(v3.method)
        Fold("Les ${v3.candidates.size} candidats") { Bulleted(v3.candidates.map(BotV3::candidateLine)) }
        Fold("Limites de la v3") { Bulleted(v3.limits) }
        BotV3.computeLine(v3, timing)?.let { Caption(it) }
    }
}

/** "v3 : hausse/baisse 20 j [ATTENDRE] 60 j [ATTENDRE] · entre pairs 20 j […] 60 j […]" of an asset of the basket. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun V3AssetNowRow(a: BotAssetRow) {
    val n = a.v3 ?: return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
        Caption("v3 : hausse/baisse 20 j")
        ActionChip(n.absolute20)
        Caption("60 j")
        ActionChip(n.absolute60)
        Caption("· entre pairs 20 j")
        ActionChip(n.peers20)
        Caption("60 j")
        ActionChip(n.peers60)
    }
}

/** "v3 : hausse/baisse 20 j [chip] · entre pairs 20 j [chip] …" of a view (watched asset or decision). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun V3SignalsRow(signals: List<BotV3Signal>) {
    if (signals.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
        Caption("v3 :")
        signals.forEach { s ->
            Caption(BotV3.signalLabel(s))
            ActionChip(s.action)
        }
    }
}
