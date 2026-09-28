package com.maxlestage.altim.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.kit.AlertJournal
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.JournalEntry
import com.maxlestage.altim.kit.PriceTarget
import com.maxlestage.altim.kit.Tone

/**
 * Alerts tab: what can be bought right now (same rule as the notifications), the price alerts chosen by the user,
 * and the journal of the notifications received with what each one gave since (measured, without fees).
 */
@Composable
fun AlertsScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit) {
    var alerts by remember { mutableStateOf<List<BuyAlert>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var refresh by remember { mutableIntStateOf(0) }
    val quotes = remember { mutableStateMapOf<String, Double>() }
    val context = LocalContext.current

    val tracked = (model.watchlist + model.holdings.map { it.asset } + model.priceTargets.map { it.asset } + model.journal.map { it.asset }).distinctBy { it.id }
    LaunchedEffect(refresh, tracked.joinToString { it.id }) {
        val client = model.client ?: return@LaunchedEffect
        try {
            alerts = client.alerts((model.watchlist + model.holdings.map { it.asset }).distinctBy { it.id })
            error = null
            client.quotes(tracked).forEach { quotes["${it.kind.raw}:${it.symbol}"] = it.price }
            model.persistSession()
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: Exception) {
            error = e.message
        }
    }
    // Live price first, consensus quote otherwise.
    fun price(a: Asset): Double? = model.live.price(a)?.price ?: quotes[a.id]

    PullToRefreshBox(isRefreshing = false, onRefresh = { refresh++ }, modifier = modifier.statusBarsPadding()) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item { Text("Alertes", fontSize = 30.sp, fontWeight = FontWeight.Bold) }
            if (!model.alertsEnabled) {
                item {
                    Notice("Notifications d'achat désactivées : activez-les dans Réglages pour être prévenu même app fermée.", Tone.WARN)
                }
            }

            item { SectionTitle("Achetables maintenant") }
            error?.let { e -> item { ErrorBox(e) { refresh++ } } }
            val buyable = alerts?.filter { it.buy }
            when {
                alerts == null && error == null -> item { Loading("Vérification du radar et des avoirs…") }
                buyable.isNullOrEmpty() && error == null -> item { Caption("Rien d'achetable pour l'instant selon la règle d'Altim (signal 4 h ou zone d'achat, sans blocage).") }
                else -> items(buyable.orEmpty(), key = { "buy:" + it.id }) { a -> BuyRow(a) { open(a.asset) } }
            }

            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    SectionTitle("Alertes de prix")
                    androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
                }
            }
            if (model.priceTargets.isEmpty()) {
                item { Caption("Aucune alerte de prix. Sur la fiche d'un actif, bouton « Alerte de prix » : « préviens-moi si BTC passe sous 80 000 $ ».") }
            }
            items(model.priceTargets, key = { "t:" + it.id }) { t -> TargetRow(t, price(t.asset), onOpen = { open(t.asset) }, onRearm = { model.rearmTarget(context, t.id) }) { model.removeTarget(context, t.id) } }

            item { SectionTitle("Journal des alertes") }
            val summary = AlertJournal.summary(model.journal, model.journal.associate { it.asset.id to (price(it.asset) ?: Double.NaN) }.filterValues { it.isFinite() })
            summary?.let { s ->
                item {
                    Card(glow = if (s.average >= 0) AltimColors.buy else AltimColors.sell) {
                        Text("Depuis leur envoi, ${s.up} alerte(s) d'achat sur ${s.count} sont en hausse (${Math.round(s.upShare)} %), variation moyenne ${Format.percent(s.average, 1)}.", fontSize = 14.sp)
                        Caption("Mesure simple depuis chaque alerte, sans frais ni règle de sortie, sur vos propres alertes : une indication, pas une preuve. Les alertes de moins d'une heure et vos alertes de prix n'y sont pas comptées.")
                    }
                }
            }
            if (model.journal.isEmpty()) item { Caption("Les notifications reçues s'afficheront ici, avec ce qu'elles ont donné depuis.") }
            items(model.journal.take(100), key = { "j:" + it.id }) { e -> JournalRow(e, price(e.asset)) { open(e.asset) } }
            if (model.journal.isNotEmpty()) item { TextButton(onClick = { model.clearJournal() }) { Text("Vider le journal", color = AltimColors.sell) } }
        }
    }
}

@Composable
private fun BuyRow(a: BuyAlert, onClick: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface).clickable(onClick = onClick).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(a.symbol, style = mono(15.sp, FontWeight.Bold), modifier = Modifier.weight(1f))
            Text(Format.price(a.price), style = mono(14.sp))
            androidx.compose.foundation.layout.Spacer(Modifier.size(8.dp))
            Badge(if (a.strong) "ACHAT CONSEILLÉ" else "ACHAT POSSIBLE", Tone.GOOD)
        }
        (a.reasons + a.cautions).forEach { Text(it, fontSize = 12.sp, color = Color.White.copy(alpha = 0.85f)) }
    }
}

@Composable
private fun TargetRow(t: PriceTarget, price: Double?, onOpen: () -> Unit, onRearm: () -> Unit, onDelete: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface).clickable(onClick = onOpen).padding(start = 14.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text("${t.asset.symbol} · ${t.label.lowercase()}", fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            val status = t.triggered?.let { "Atteinte le ${Format.date(it.toDouble(), time = true)}" }
                ?: price?.let { p -> "Prix actuel ${Format.price(p)} · encore ${Format.percent((t.price / p - 1) * 100, 1)}" }
                ?: "En attente"
            Text(status, fontSize = 12.sp, color = if (t.triggered != null) AltimColors.buy else AltimColors.textSecondary)
        }
        if (t.triggered != null) IconButton(onClick = onRearm) { Icon(Icons.Filled.Replay, contentDescription = "Réarmer l'alerte ${t.asset.symbol}", tint = AltimColors.cyan) }
        IconButton(onClick = onDelete) { Icon(Icons.Filled.Delete, contentDescription = "Supprimer l'alerte ${t.asset.symbol}", tint = AltimColors.sell) }
    }
}

@Composable
private fun JournalRow(e: JournalEntry, price: Double?, onClick: () -> Unit) {
    val change = e.change(price)
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface.copy(alpha = 0.7f)).clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(e.title, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${Format.date(e.date.toDouble(), time = true)} · à ${Format.price(e.price)}", fontSize = 11.sp, color = AltimColors.textSecondary)
        }
        if (e.source == JournalEntry.Source.TARGET) Badge("PRIX", Tone.NEUTRAL) else Badge(if (e.source == JournalEntry.Source.STRONG_BUY) "CONSEILLÉ" else "POSSIBLE", Tone.GOOD)
        ChangeText(change)
    }
}

/** "Alerte de prix" card of an asset page: above / below, threshold prefilled with the current price. */
@Composable
fun PriceTargetCard(model: AppModel, asset: Asset, current: Double?, onClose: () -> Unit) {
    val context = LocalContext.current
    var above by remember { mutableStateOf(false) }
    var text by remember { mutableStateOf(current?.let { Format.plain(it, if (it >= 1) 2 else 6) } ?: "") }
    var denied by remember { mutableStateOf(false) }
    val value = Format.parse(text)
    // The threshold must be on the right side of the current price, otherwise it would fire at once.
    val sideOk = value != null && value > 0 && (current == null || (if (above) value > current else value < current))
    fun save() {
        model.addTarget(context, PriceTarget(asset = asset, above = above, price = value!!))
        onClose()
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) save() else denied = true
    }
    Card(title = "Alerte de prix · ${asset.symbol}", glow = AltimColors.cyan) {
        current?.let { Caption("Prix actuel : ${Format.price(it)}") }
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            listOf(false to "Passe sous", true to "Passe au-dessus").forEachIndexed { i, (v, label) ->
                SegmentedButton(
                    selected = above == v,
                    onClick = { above = v },
                    shape = SegmentedButtonDefaults.itemShape(i, 2),
                    colors = SegmentedButtonDefaults.colors(activeContainerColor = AltimColors.cyan.copy(alpha = 0.2f), activeContentColor = AltimColors.cyan),
                ) { Text(label) }
            }
        }
        OutlinedTextField(
            value = text,
            onValueChange = { text = it },
            suffix = { Text("$") },
            singleLine = true,
            textStyle = mono(18.sp),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
            modifier = Modifier.fillMaxWidth(),
            colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
        )
        if (value != null && !sideOk) Caption(if (above) "Choisissez un prix au-dessus du prix actuel." else "Choisissez un prix en dessous du prix actuel.", AltimColors.warning)
        if (denied) Caption("Notifications refusées : autorisez-les dans Paramètres Android → Applications → Altim.", AltimColors.warning)
        Caption("Vérifiée toutes les 15 minutes avec les alertes d'achat, même app fermée ; une seule notification, puis vous pouvez la réarmer.")
        Row {
            TextButton(onClick = onClose) { Text("Annuler") }
            androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
            TextButton(enabled = sideOk, onClick = {
                if (Build.VERSION.SDK_INT >= 33 && !BuyAlerts.canNotify(context)) permission.launch(Manifest.permission.POST_NOTIFICATIONS) else save()
            }) { Text("Créer l'alerte", color = if (sideOk) AltimColors.cyan else AltimColors.textSecondary, fontWeight = FontWeight.Bold) }
        }
    }
}
