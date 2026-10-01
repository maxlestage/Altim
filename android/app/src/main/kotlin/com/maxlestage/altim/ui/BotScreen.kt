package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
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
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Bot
import com.maxlestage.altim.kit.BotAssetRow
import com.maxlestage.altim.kit.BotBucket
import com.maxlestage.altim.kit.BotCandidateStat
import com.maxlestage.altim.kit.BotGroupStat
import com.maxlestage.altim.kit.BotReport
import com.maxlestage.altim.kit.BotResult
import com.maxlestage.altim.kit.BotStatsLike
import com.maxlestage.altim.kit.BotV3
import com.maxlestage.altim.kit.BotView
import com.maxlestage.altim.kit.BotViews
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.ModelValidation
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.effectiveCounts
import com.maxlestage.altim.kit.effectiveNote
import kotlinx.coroutines.delay

// « Bot Altim » (web Bot.tsx): candidate models trained on long histories of the validation's basket and an extra
// universe (/api/bot), chosen at each retraining on an inner validation and tested walk-forward on periods they had not
// seen, saying ACHETER / ATTENDRE / VENDRE; today's view of the watched assets (/api/bot/views). v3 (BotV3Section.kt)
// first when present, then v2's selection as the reference. Stacked cards, wrapping chips, nothing wider than a 360 dp
// phone; the per-asset list keeps the basket's order; each candidate is shown alone for information only. v2 and v1
// answers still render (their later parts left out).

/** Opens « Bot Altim » above the current screen (null where it cannot be opened, e.g. in the tests). */
val LocalOpenBot = staticCompositionLocalOf<(() -> Unit)?> { null }

/** Colours of the calibration bars: predicted, observed. */
private val PredictedBar = AltimColors.cyan
private val ObservedBar = AltimColors.warning

/** Loads /api/bot (202 while the first training runs: asked again every 5 s), then the view of the watched assets. */
@Composable
fun BotScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit, onBack: () -> Unit) {
    var report by remember { mutableStateOf<BotReport?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var pending by remember { mutableStateOf(false) }
    var views by remember { mutableStateOf<BotViews?>(null) }
    var viewsError by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        val client = model.client ?: return@LaunchedEffect
        repeat(36) {
            try {
                when (val r = client.bot()) {
                    is BotResult.Ready -> {
                        report = r.report
                        pending = false
                        model.persistSession()
                        return@LaunchedEffect
                    }
                    BotResult.Pending -> {
                        pending = true
                        delay(5_000)
                    }
                }
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "Bot indisponible"
                return@LaunchedEffect
            }
        }
        error = "L'entraînement prend plus de temps que prévu. Revenez dans un instant."
    }
    val watchlist = model.watchlist
    val ready = report != null
    LaunchedEffect(ready, watchlist.joinToString(",") { it.id }) {
        if (!ready || watchlist.isEmpty()) return@LaunchedEffect
        val client = model.client ?: return@LaunchedEffect
        try {
            views = client.botViews(watchlist)
            viewsError = null
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            viewsError = e.message ?: "indisponible"
        }
    }
    BotAltimView(report, error, pending, modifier, watched = watchlist.isNotEmpty(), views = views, viewsError = viewsError, open = open, onBack = onBack)
}

/** The whole screen from its state (no server: also rendered by the tests). */
@Composable
fun BotAltimView(
    report: BotReport?,
    error: String?,
    pending: Boolean,
    modifier: Modifier = Modifier,
    watched: Boolean = false,
    views: BotViews? = null,
    viewsError: String? = null,
    open: (Asset) -> Unit = {},
    onBack: () -> Unit = {},
) {
    var featuresOpen by rememberSaveable { mutableStateOf(false) }
    Column(modifier.statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Retour") }
            Text("Bot Altim", fontSize = 20.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f).semantics { heading() })
        }
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxSize()) {
            item {
                Caption(
                    "Des modèles appris sur de longs historiques (actions depuis 1990, cryptos depuis leur cotation), qui disent ACHETER, ATTENDRE ou VENDRE à 20 et 60 jours. " +
                        "Jugés seulement sur des périodes qu'ils n'avaient pas vues, sur 34 actifs fixés d'avance, avec un seuil corrigé des essais multiples. Altim ne passe aucun ordre.",
                )
            }
            error?.let { item { Notice("⚠ $it", Tone.WARN) } }
            if (report == null && error == null) {
                item { Card { Caption(if (pending) "Téléchargement des historiques, entraînement et test en cours (plusieurs minutes la première fois)…" else "Chargement…"); Loading() } }
            }
            if (report != null) {
                val r = report
                val v3 = r.v3
                item { BotHeadCard(r) }
                // v3 first; v2's selection kept below as the reference.
                if (v3 != null) botV3Items(v3, r.timing)
                r.v4?.let { botV4Items(it) }
                if (v3 == null && r.changes.isNotEmpty()) item { Card(title = "Ce qui change avec la v2") { Bulleted(r.changes) } }
                if (watched) item { WatchedViewsCard(views, viewsError, open) }
                if (v3 != null) {
                    item { Label("Référence : sélection v2 à 20 jours") }
                    item { Caption(BotV3.v2ReferenceNote(v3)) }
                }
                item {
                    Card(title = "Comment il apprend et comment il est jugé") {
                        Bulleted(r.method)
                        Text(
                            "${if (featuresOpen) "▾" else "▸"} Les ${r.features.size} mesures lues à chaque clôture",
                            fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = AltimColors.cyan,
                            modifier = Modifier.fillMaxWidth().clickable(role = Role.Button) { featuresOpen = !featuresOpen }.padding(vertical = 6.dp),
                        )
                        if (featuresOpen) Bulleted(r.features.map { "${it.label} — ${it.help}" })
                    }
                }
                item { Label(if (v3 != null) "Sélection v2 : résultats hors échantillon" else "Résultats hors échantillon") }
                item { Caption("Modèle choisi à chaque réentraînement sur une validation interne (jamais sur le test), testé sur les actifs du panier.") }
                items(r.groups, key = { "group-${it.id}" }) { BotGroupCard(it) }
                val withCandidates = r.groups.filter { it.candidates.isNotEmpty() }
                if (withCandidates.isNotEmpty()) {
                    item { Label("Chaque modèle seul") }
                    item { Caption("À titre d'information, non utilisé pour choisir : ce qu'aurait donné chaque candidat retenu partout, sur les mêmes jours.") }
                    items(withCandidates, key = { "cands-${it.id}" }) { BotCandidatesCard(it) }
                }
                if (r.groups.any { it.holdout != null || it.extra != null }) {
                    item { Label("Dernière année et univers élargi") }
                    r.groups.forEach { g ->
                        g.holdout?.let { h -> item(key = "holdout-${g.id}") { BotSubResult(Bot.holdoutTitle(g), Bot.holdoutNote(h), h) } }
                        g.extra?.let { x -> item(key = "extra-${g.id}") { BotSubResult(Bot.extraTitle(g), Bot.extraNote(g), x) } }
                    }
                }
                item { Label("Calibration") }
                item { Caption("Quand le bot annonce une probabilité, la fréquence observée ensuite devrait être proche. Chaque ligne : jours de test dont la probabilité tombait dans la tranche.") }
                r.groups.forEach { g ->
                    item(key = "cal-${g.id}-up") { CalibrationCard("${g.label} · hausse", g.calibrationUp, g.brierSkillUp) }
                    item(key = "cal-${g.id}-down") { CalibrationCard("${g.label} · baisse", g.calibrationDown, g.brierSkillDown) }
                }
                item { Label("Actif par actif") }
                item { Caption("Dans l'ordre du panier, jamais classés par performance. « Aujourd'hui » : avis du modèle retenu, entraîné sur tout l'historique connu.") }
                items(r.assets, key = { "asset-${it.symbol}" }) { BotAssetRowView(it, open) }
                item {
                    Card(title = "Limites") {
                        Bulleted(r.limits)
                        // With v3 the computation (time and memory) is said in its own card.
                        Caption(Bot.sourceText(if (v3 != null) r.copy(timing = null) else r))
                        LocalOpenValidation.current?.let { openValidation ->
                            Text(
                                "Voir la validation du signal →",
                                color = AltimColors.cyan, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                                modifier = Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = openValidation).padding(vertical = 6.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** ACHETER / ATTENDRE / VENDRE in its colour, "pas d'avis" without an action. */
@Composable
fun ActionChip(action: String?) {
    val ui = Bot.actionUi(action)
    if (ui == null) Badge("pas d'avis", Tone.NEUTRAL) else Badge(ui.label, ui.tone)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BotHeadCard(r: BotReport) {
    Card {
        Text(r.headline, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Tile("Actifs testés", Bot.assetsTile(r))
            Tile("Jours testés", ModelValidation.fr(r.overall.labelled.toDouble(), 0))
            Tile("Achats / ventes", Bot.signalsTile(r))
        }
        Caption(Bot.parametersText(r))
        if (r.failures.isNotEmpty()) {
            Notice(Bot.failuresTitle(r.failures.size) + r.failures.joinToString("") { "\n• ${it.symbol} — ${it.error}" }, Tone.WARN)
        }
        Bot.extraFailuresText(r)?.let { Caption(it) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SideHead(action: String, verdict: String?, label: String?) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
        ActionChip(action)
        if (label != null) VerdictChip(verdict ?: "unproven", label)
    }
}

@Composable
private fun Body(text: String) = Text(text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))

@Composable
private fun Side(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.08f)))
        content()
    }
}

/** One group: its ACHETER, VENDRE and ATTENDRE results out of sample, each against a random day. */
@Composable
fun BotGroupCard(g: BotGroupStat) {
    val p = Bot::pct0
    val s = { v: Double? -> ModelValidation.signedPct(v) }
    Card(title = g.label) {
        Caption(Bot.groupSubtitle(g))
        Bot.dataLine(g)?.let { Caption(it) }
        Side {
            SideHead("buy", g.buy.verdict, g.buy.verdictLabel)
            Body(Bot.buyText(g.buy))
            g.buy.clustered?.let { Caption(Bot.clusteredText(it, g.buy.tStat)) }
            KeyValue("Achats gagnants / jours gagnants", "${p(g.buy.hitRate)} / ${p(g.buy.baselineHitRate)}")
            KeyValue("Achats cumulés / détention (médianes)", "${s(g.buy.medianBotReturn)} / ${s(g.buy.medianHoldReturn)}")
        }
        Side {
            SideHead("sell", g.sell.verdict, g.sell.verdictLabel)
            Body(Bot.sellText(g.sell))
            g.sell.clustered?.let { Caption(Bot.clusteredText(it, g.sell.tStat)) }
            KeyValue("Suivies d'une baisse / tous les jours", "${p(g.sell.fallRate)} / ${p(g.sell.baselineFallRate)}")
            KeyValue("Pire recul moyen ensuite / au hasard", "${s(g.sell.meanDrawdown)} / ${s(g.sell.baselineDrawdown)}")
            Bot.exitText(g.sell.exit)?.let { Body(it) }
        }
        Side {
            SideHead("wait", null, null)
            Body(Bot.waitText(g.wait))
        }
        if (g.selection.isNotEmpty()) {
            Side {
                Text("Modèle retenu à chaque réentraînement", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                Bot.selectionRuns(g.selection).forEach { x ->
                    Text(
                        buildAnnotatedString {
                            withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(Bot.runPeriod(x)) }
                            append(" ")
                            withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(Bot.runChoice(x)) }
                        },
                        fontSize = 13.sp,
                    )
                }
                Bot.liveModelText(g)?.let { Caption(it) }
            }
        }
    }
}

/** Each candidate's own out-of-sample result (for information, never used to choose): stacked rows, no table. */
@Composable
fun BotCandidatesCard(g: BotGroupStat) {
    Card(title = g.label) {
        g.candidates.forEach { c -> Side { CandidateRow(c) } }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CandidateRow(c: BotCandidateStat) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
        Text(c.label, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
        Badge(Bot.candidateChosenText(c), Tone.NEUTRAL)
    }
    if (c.description.isNotBlank()) Caption(c.description)
    listOf(Bot.candidateBuyRow(c), Bot.candidateSellRow(c), Bot.candidateSkillRow(c)).forEach { (k, v) -> KeyValue(k, v) }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        VerdictChip(c.buy.verdict ?: "unproven", Bot.prefixedVerdict("Achats", c.buy.verdictLabel))
        VerdictChip(c.sell.verdict ?: "unproven", Bot.prefixedVerdict("Ventes", c.sell.verdictLabel))
    }
}

/** The last 12 months, or the extra training assets: the same figures, shorter. */
@Composable
fun BotSubResult(title: String, note: String, st: BotStatsLike) {
    Card(title = title) {
        Caption(note)
        SideHead("buy", st.buy.verdict, st.buy.verdictLabel)
        Body(Bot.buyText(st.buy))
        SideHead("sell", st.sell.verdict, st.sell.verdictLabel)
        Body(Bot.sellText(st.sell))
        Bot.exitText(st.sell.exit)?.let { Caption(it) }
    }
}

/** Predicted vs realised per probability bucket: two thin bars per row, the numbers written next to them. */
@Composable
fun CalibrationCard(title: String, buckets: List<BotBucket>, skill: Double?) {
    Card(title = title) {
        Caption("Précision : ${Bot.skillText(skill)}.")
        Bot.calibrationRows(buckets).forEach { b ->
            Column(
                Modifier.fillMaxWidth().semantics(mergeDescendants = true) { contentDescription = "Prévu ${Bot.pct0(b.predicted)}, observé ${Bot.pct0(b.realised)}" },
                verticalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Color.White)) { append(b.label) }
                        withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" · ${ModelValidation.fr(b.rows.toDouble(), 0)} jours") }
                    },
                    fontSize = 13.sp,
                )
                Text(Bot.bucketText(b), fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
                ThinBar(b.predicted, PredictedBar)
                ThinBar(b.realised, ObservedBar)
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Key(PredictedBar)
            Caption("prévu ·")
            Key(ObservedBar)
            Caption("observé")
        }
    }
}

@Composable
private fun ThinBar(pct: Double?, color: Color) {
    val shape = RoundedCornerShape(3.dp)
    Box(Modifier.fillMaxWidth().height(5.dp).clip(shape).background(Color.White.copy(alpha = 0.05f))) {
        val f = ((pct ?: 0.0) / 100).coerceIn(0.0, 1.0).toFloat()
        if (f > 0f) Box(Modifier.fillMaxWidth(f).widthIn(min = 2.dp).fillMaxHeight().clip(shape).background(color))
    }
}

@Composable
private fun Key(color: Color) = Box(Modifier.width(14.dp).height(5.dp).clip(RoundedCornerShape(3.dp)).background(color))

/** One asset of the basket: symbol and name (opens the asset), class, today's action, then its test results. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun BotAssetRowView(a: BotAssetRow, open: (Asset) -> Unit) {
    val shape = RoundedCornerShape(16.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.surface.copy(alpha = 0.85f)).border(1.dp, Color.White.copy(alpha = 0.08f), shape)
            .clickable(role = Role.Button, onClickLabel = "Ouvrir ${a.symbol}") { open(a.asset) }.padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = AltimColors.cyan)) { append(a.symbol) }
                    if (a.name.isNotBlank()) withStyle(SpanStyle(color = AltimColors.textSecondary, fontSize = 12.sp)) { append(" ${a.name}") }
                },
                fontSize = 15.sp,
            )
            Badge(ModelValidation.classShort(a.`class`), Tone.NEUTRAL)
            ActionChip(a.now.action)
        }
        Text(Bot.todayText(a), fontSize = 13.sp, color = Color.White)
        V3AssetNowRow(a)
        V4AssetNowRow(a)
        Caption(Bot.assetTestText(a))
    }
}

/** Today's view of the watched assets (cached report only; nothing is trained here). */
@Composable
private fun WatchedViewsCard(views: BotViews?, error: String?, open: (Asset) -> Unit) {
    Card(title = "Vos actifs aujourd'hui") {
        if (error != null) Caption("Avis indisponibles ($error).")
        if (views == null && error == null) Caption("Chargement…")
        views?.views?.forEach { v -> ViewRow(v, open) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ViewRow(v: BotView, open: (Asset) -> Unit) {
    val kind = v.kind
    Column(
        Modifier.fillMaxWidth()
            .then(if (kind != null) Modifier.clickable(role = Role.Button, onClickLabel = "Ouvrir ${v.symbol}") { open(Asset(v.symbol, kind, v.symbol)) } else Modifier)
            .padding(vertical = 2.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Text(v.symbol, fontWeight = FontWeight.Bold, color = AltimColors.cyan, fontSize = 15.sp)
            ActionChip(v.action)
            Badge(if (v.effectiveCounts) "compte" else "ne compte pas", if (v.effectiveCounts) Tone.GOOD else Tone.NEUTRAL)
        }
        Caption(Bot.viewText(v))
        v.v3?.takeIf { it.available }?.let { V3SignalsRow(it.signals) }
        v.v4?.let { V4AvisBlock(it) }
    }
}

/**
 * « Bot Altim » in the decision (web DecisionCard.tsx `BotLine`): the learned model's action, its probabilities and
 * whether it counts in this decision, with the link to the « Bot Altim » screen and the report's date.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun BotLine(b: BotView) {
    val accent = when (b.tone) {
        "edge" -> AltimColors.buy
        "unproven" -> AltimColors.warning
        else -> AltimColors.textSecondary
    }
    val shape = RoundedCornerShape(12.dp)
    val openBot = LocalOpenBot.current
    val shown = b.available && Bot.actionUi(b.action) != null
    Row(
        Modifier.fillMaxWidth().height(IntrinsicSize.Min).clip(shape)
            .background(Color.White.copy(alpha = 0.03f))
            .border(1.dp, Color.White.copy(alpha = 0.12f), shape),
    ) {
        Box(Modifier.width(3.dp).fillMaxHeight().background(accent))
        Column(Modifier.weight(1f).padding(horizontal = 10.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text("Bot Altim", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White, modifier = Modifier.semantics { heading() })
                if (shown) {
                    ActionChip(b.action)
                    Badge(if (b.effectiveCounts) "compte" else "ne compte pas", Tone.NEUTRAL)
                }
            }
            Text(if (shown) Bot.probabilitiesText(b) else b.text, fontSize = 13.sp, color = Color.White.copy(alpha = 0.92f))
            // v3: today's action of the 4 headline configurations (they decide whether the bot counts).
            b.v3?.takeIf { it.available }?.let { V3SignalsRow(it.signals) }
            if (b.contributions.isNotEmpty()) Caption(Bot.contributionsText(b))
            val note = b.effectiveNote
            if (note.isNotBlank()) Text(note, fontSize = 13.sp, color = Color.White.copy(alpha = 0.92f))
            // v4: the selective bots today (« pas d'avis » unless a score is extreme).
            if (b.available) b.v4?.let { V4AvisBlock(it) }
            Text(
                buildAnnotatedString {
                    withStyle(SpanStyle(color = AltimColors.cyan, fontWeight = FontWeight.SemiBold)) { append("Voir le bot et ses résultats →") }
                    b.asOf?.let { withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(" · entraîné le ${Format.shortDateTime(it)}") } }
                },
                fontSize = 13.sp,
                modifier = Modifier.fillMaxWidth()
                    .then(if (openBot != null) Modifier.clickable(role = Role.Button, onClick = openBot) else Modifier)
                    .padding(vertical = 4.dp),
            )
        }
    }
}
