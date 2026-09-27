package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Candidate
import com.maxlestage.altim.kit.Criterion
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.Horizon
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.SelectionReport
import com.maxlestage.altim.kit.SelectionResult
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Validation
import kotlinx.coroutines.delay
import kotlin.math.floor

/**
 * Which stocks or cryptos to buy, for 8 holding durations: ranked by what was measured to work on the past,
 * each finalist checked, with an entry plan, a stop, a target and an amount.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SelectionScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit) {
    var report by remember { mutableStateOf<SelectionReport?>(null) }
    var pending by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var refresh by remember { mutableIntStateOf(0) }
    var budgetText by remember { mutableStateOf(if (model.budget > 0) Format.plain(model.budget, 0) else "") }
    val market = model.selectionMarket
    val horizon = model.selectionHorizon

    LaunchedEffect(market, horizon, refresh) {
        val client = model.client ?: return@LaunchedEffect
        report = null
        error = null
        pending = false
        // The first computation takes ≈ 30 s: the server answers "pending", we come back every 8 s (at most 3 min).
        repeat(24) {
            try {
                when (val r = client.selection(horizon, market)) {
                    is SelectionResult.Ready -> {
                        report = r.report
                        pending = false
                        model.selectionAssets = r.report.buy.map { Asset(it.symbol, r.report.market, it.name) }
                        model.persistSession()
                        return@LaunchedEffect
                    }
                    SelectionResult.Pending -> {
                        pending = true
                        delay(8_000)
                    }
                }
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message
                return@LaunchedEffect
            }
        }
        error = "Le calcul prend plus de temps que prévu. Tirez vers le bas pour réessayer."
    }
    DisposableEffect(Unit) { onDispose { model.selectionAssets = emptyList() } }

    PullToRefreshBox(isRefreshing = false, onRefresh = { refresh++ }, modifier = modifier.statusBarsPadding()) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text("Sélection", fontSize = 30.sp, fontWeight = FontWeight.Bold)
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                listOf(Kind.STOCK to "Actions", Kind.CRYPTO to "Cryptos").forEachIndexed { i, (k, label) ->
                    SegmentedButton(
                        selected = market == k,
                        onClick = { model.updateSelection(market = k) },
                        shape = SegmentedButtonDefaults.itemShape(i, 2),
                        colors = SegmentedButtonDefaults.colors(activeContainerColor = AltimColors.cyan.copy(alpha = 0.2f), activeContentColor = AltimColors.cyan),
                    ) { Text(label) }
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Caption("Durée de détention")
                FlowRow(maxItemsInEachRow = 4, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Horizon.entries.forEach { h ->
                        val on = h == horizon
                        Box(
                            Modifier.weight(1f).heightIn(min = 40.dp).clip(RoundedCornerShape(10.dp))
                                .background(if (on) AltimColors.cyan else Color.White.copy(alpha = 0.07f))
                                .clickable(role = Role.Tab) { model.updateSelection(horizon = h) }
                                .semantics { selected = on },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(h.label, color = if (on) Color.Black else Color.White, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                        }
                    }
                }
            }

            error?.let { ErrorBox(it) { refresh++ } }
            val r = report
            if (r != null) {
                Card(title = "Comment la sélection est faite") {
                    Text("Classement : ${r.rankText}", fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    Text(r.evidence, fontSize = 13.sp, color = Color.White.copy(alpha = 0.85f))
                    Caption("${r.scanned} ${if (r.market == Kind.CRYPTO) "cryptos" else "actions"} analysées. Chaque finaliste est vérifié : prix recoupés sur plusieurs sources, garde-fou marché, tendance de fond. Plan pour une détention de ${r.holdText} : entrée (zone d'achat Fibonacci), stop selon la volatilité, objectif à 2 fois le risque.")
                }
                if (r.marketClosed) Notice("Bourse de New York fermée : ce classement vient de la dernière séance ; il changera à la réouverture.")
                r.validation?.let { ValidationCard(it, r) }
                Card(title = "Budget à investir") {
                    OutlinedTextField(
                        value = budgetText,
                        onValueChange = {
                            budgetText = it
                            model.updateBudget(Format.parse(it) ?: 0.0)
                        },
                        placeholder = { Text("10 000", color = AltimColors.textSecondary) },
                        suffix = { Text("$") },
                        singleLine = true,
                        textStyle = mono(18.sp),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                        modifier = Modifier.fillMaxWidth(),
                        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
                    )
                    Caption("Réparti pour que chaque ligne risque la même somme si son stop est touché (une valeur volatile reçoit moins), sans dépasser 20 % du budget par ligne.")
                }
                val amounts = r.allocate(model.budget)
                SectionTitle("À acheter · ${r.buy.size}")
                r.buy.forEach { c -> PickCard(model, c, r, amounts[c.symbol], open) }
                if (r.watch.isNotEmpty()) {
                    SectionTitle("À surveiller · ${r.watch.size}")
                    r.watch.forEach { c ->
                        Card(glow = AltimColors.warning, modifier = Modifier.clickable { open(Asset(c.symbol, r.market, c.name)) }) {
                            Text(buildAnnotatedString {
                                withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("${c.name} ") }
                                withStyle(SpanStyle(color = AltimColors.textSecondary)) { append(c.symbol) }
                            })
                            c.reason?.let { Text("⚠ $it", color = AltimColors.warning, fontSize = 13.sp) }
                        }
                    }
                }
                if (r.setAside.isNotEmpty()) {
                    var show by remember(r.asOf) { mutableStateOf(false) }
                    Card {
                        TextButton(onClick = { show = !show }) { Text("Écartées faute de données fiables · ${r.setAside.size}", color = AltimColors.cyan) }
                        if (show) r.setAside.forEach { Caption("${it.symbol} : ${it.reason}") }
                    }
                }
                Caption("Sélection calculée le ${Format.date(r.asOf, time = true)}, prix en direct. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.")
            } else if (error == null) {
                Loading(
                    if (pending) "Analyse des ${if (market == Kind.CRYPTO) "120 cryptos" else "150 actions"} en cours (environ 30 secondes la première fois)…"
                    else "Chargement de la sélection…",
                )
            }
        }
    }
}

@Composable
private fun ValidationCard(v: Validation, report: SelectionReport) {
    Card(title = "Ce que cette méthode aurait donné", glow = if (v.edge == "clear") AltimColors.buy else AltimColors.warning) {
        when (v.edge) {
            "none" -> Notice("Pas d'avance mesurée pour ${report.horizon.label}. Une fois les frais payés (${Format.plain(v.cost)} % l'aller-retour), ce classement n'a pas fait mieux que de choisir au hasard. Il est affiché à titre indicatif : ne misez pas dessus.", Tone.BAD)
            "weak" -> Notice("Avance faible et irrégulière. En moyenne la sélection a fait mieux, mais seulement environ une fois sur deux : quelques très bons choix tirent la moyenne. Une durée plus longue est plus fiable.", Tone.WARN)
        }
        var s = "Rejouée ${v.periods} fois"
        v.from?.let { s += " depuis le ${Format.date(it)}" }
        s += " (sélection de ${v.topN}, gardée ${report.holdText}) : ${Format.percent(v.top, 1)} en moyenne pour la sélection contre ${Format.percent(v.universe, 1)} pour l'ensemble des ${report.scanned} ${if (report.market == Kind.CRYPTO) "cryptos" else "actions"}"
        v.benchmark?.let { s += " et ${Format.percent(it, 1)} pour le simple achat de Bitcoin" }
        s += " ; la sélection a fait mieux que l'ensemble ${Math.round(v.beatRate)} % du temps."
        Text(s, fontSize = 13.sp)
        if (report.market == Kind.CRYPTO && v.top < 0) {
            Notice("Sur cette période, la sélection a perdu moins que les autres cryptos, mais elle a quand même perdu : quand presque toutes les cryptos baissent, bien choisir limite la casse sans l'éviter.")
        }
        Caption(
            when {
                report.horizon.isIntraday -> "Durées courtes : rejouées sur ${if (report.market == Kind.CRYPTO) "quelques jours à 3 semaines" else "60 jours"} seulement ; à ces échelles les prix sont surtout du bruit et les frais pèsent lourd. Ce n'est pas une garantie."
                report.market == Kind.CRYPTO -> "Limites honnêtes : l'historique ne couvre qu'environ 2 ans et demi (les plateformes gardent 1 000 jours), et la liste est celle des cryptos qui existent encore aujourd'hui, ce qui embellit les chiffres. Les cryptos restent très risquées. Ce n'est pas une garantie."
                else -> "Limite honnête : la liste est celle des plus grandes sociétés d'aujourd'hui, qui ont par définition réussi, ce qui gonfle ces chiffres. Entre fin 2021 et 2023, la force relative n'a presque rien apporté ; l'essentiel de l'avance vient de 2023–2026. Ce n'est pas une garantie."
            },
        )
    }
}

@Composable
private fun PickCard(model: AppModel, c: Candidate, report: SelectionReport, amount: Double?, open: (Asset) -> Unit) {
    val asset = Asset(c.symbol, report.market, c.name)
    val price = model.live.price(asset)?.price ?: c.price
    val plan = c.plan
    val buyAt = plan?.limit ?: price
    var expanded by remember(c.symbol) { mutableStateOf(c.rank <= 3) }
    val rankLabel = when (report.rankBy) {
        Criterion.SIGNAL -> "signal"
        Criterion.MOMENTUM -> if (report.rankRule == "reversal") "rebond" else "force"
        Criterion.RISK -> "calme"
        Criterion.TREND -> "tendance"
        Criterion.ZONE -> "zone"
    }
    val rankScore = Math.round(if (report.rankRule == "reversal") 100 - (c.scores["momentum"] ?: 0.0) else (c.scores[report.rankBy.raw] ?: 0.0))

    Card(glow = if (c.rank <= 3) AltimColors.cyan else AltimColors.violet) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
            Box(Modifier.size(30.dp).clip(CircleShape).background(AltimColors.cyan), contentAlignment = Alignment.Center) {
                Text("${c.rank}", color = Color.Black, style = mono(16.sp, FontWeight.Bold))
            }
            Column(Modifier.weight(1f)) {
                Text(c.name, fontWeight = FontWeight.Bold, fontSize = 15.sp, maxLines = 2)
                Caption("${c.symbol} · ${c.sector}")
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(Format.price(price), style = mono(14.sp))
                Text("$rankLabel $rankScore/100", fontSize = 11.sp, color = AltimColors.textSecondary)
            }
        }
        if (plan != null) {
            KeyValue("Entrée", plan.limit?.let { "ordre limite ${Format.price(it)}" } ?: "maintenant ≈ ${Format.price(price)}")
            if (plan.limit != null) KeyValue("ou tout de suite", "≈ ${Format.price(price)}")
            KeyValue("Stop", "${Format.price(plan.stop)} (${Format.percent((plan.stop / buyAt - 1) * 100, 1)})", Tone.BAD)
            KeyValue("Objectif", "${Format.price(plan.target)} (${Format.percent((plan.target / buyAt - 1) * 100, 1)})", Tone.GOOD)
            if (amount != null && amount > 0) {
                // Whole shares for a stock; cryptos are divisible.
                val qty = if (buyAt > 0) (if (report.market == Kind.STOCK) floor(amount / buyAt) else amount / buyAt) else 0.0
                KeyValue("Montant suggéré", Format.money(amount), Tone.NEUTRAL)
                Caption(
                    if (qty > 0) {
                        val units = if (report.market == Kind.STOCK) "${qty.toInt()} action${if (qty > 1) "s" else ""}" else "${Format.quantity(qty)} ${c.symbol}"
                        "$units · perte max ≈ ${Format.money(qty * (buyAt - plan.stop))} au stop"
                    } else "moins d'une action : fractionnée chez votre courtier",
                )
            }
        }
        TextButton(onClick = { expanded = !expanded }) {
            Text(if (expanded) "Masquer le détail" else "Pourquoi celle-ci ? Le détail", color = AltimColors.cyan, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        if (expanded) {
            report.orderedCriteria.forEach { k ->
                val score = c.scores[k.raw] ?: 0.0
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(report.criteria[k.raw] ?: k.raw, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = if (k == report.rankBy) AltimColors.cyan else Color.White)
                        Text(report.roles[k.raw] ?: "", fontSize = 11.sp, color = AltimColors.textSecondary, modifier = Modifier.weight(1f))
                        Text("${Math.round(score)}", style = mono(12.sp))
                    }
                    Bar(score, if (score >= 70) AltimColors.buy else if (score >= 40) AltimColors.warning else AltimColors.sell, 5.dp)
                    c.why[k.raw]?.let { Text(it, fontSize = 11.sp, color = AltimColors.textSecondary) }
                }
            }
            c.checks.forEach { x ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(if (x.ok) Icons.Filled.CheckCircle else Icons.Filled.Warning, contentDescription = null, tint = if (x.ok) AltimColors.buy else AltimColors.warning, modifier = Modifier.size(16.dp))
                    Text(buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("${x.label} : ") }
                        append(x.detail)
                    }, fontSize = 12.sp)
                }
            }
            c.track?.let { t ->
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(Icons.Filled.Info, contentDescription = null, tint = AltimColors.cyan, modifier = Modifier.size(16.dp))
                    Text(buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("Signaux d'Altim sur ce titre : ") }
                        append("${t.trades} achats passés, ${Math.round(t.winRate)} % gagnants, ${Format.percent(t.avgReturn, 1)} en moyenne")
                    }, fontSize = 12.sp)
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = { open(asset) }) { Text("Voir la fiche complète", color = AltimColors.cyan, fontWeight = FontWeight.Bold, fontSize = 13.sp) }
                androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
                if (!model.isWatched(asset)) TextButton(onClick = { model.watch(asset) }) { Text("+ Ajouter au radar", color = AltimColors.cyan, fontSize = 13.sp) }
            }
        }
    }
}
