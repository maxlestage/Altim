package com.maxlestage.altim.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.kit.Format
import com.maxlestage.altim.kit.RiskSettings
import com.maxlestage.altim.kit.ScoreWeights
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.launch

@Composable
fun SettingsScreen(model: AppModel, modifier: Modifier, onBack: (() -> Unit)? = null) {
    var confirmLogout by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var denied by remember { mutableStateOf(false) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        denied = !granted
        if (granted) model.updateAlerts(context, enabled = true)
    }
    val newsPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        denied = !granted
        if (granted) model.updateNewsAlerts(context, true)
    }
    // Configuration changes / dangerous positions: the switch turned on once the permission is granted.
    var pendingSwitch by remember { mutableStateOf<String?>(null) }
    val watchPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        denied = !granted
        if (granted) when (pendingSwitch) {
            "config" -> model.updateConfigAlerts(context, true)
            "dangers" -> model.updateDangerAlerts(context, true)
        }
        pendingSwitch = null
    }
    fun switchOn(which: String, on: Boolean, update: (Boolean) -> Unit) {
        if (on && android.os.Build.VERSION.SDK_INT >= 33 && !BuyAlerts.canNotify(context)) {
            pendingSwitch = which
            watchPermission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
        } else update(on)
    }
    val version = remember { runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull() ?: "—" }

    Column(
        modifier.statusBarsPadding().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            onBack?.let { IconButton(onClick = it) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Retour") } }
            Text("Réglages", fontSize = 30.sp, fontWeight = FontWeight.Bold)
        }
        Card(title = "Serveur") {
            KeyValue("Adresse", model.serverUrl?.host ?: "—")
            KeyValue("Identifiant", model.user ?: "accès ouvert")
            TextButton(onClick = { confirmLogout = true }) { Text("Se déconnecter", color = AltimColors.sell, fontWeight = FontWeight.Bold) }
        }
        Card(title = "Notifications d'achat") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Me prévenir quand je peux acheter", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.alertsEnabled, { on ->
                    if (on && android.os.Build.VERSION.SDK_INT >= 33 && !BuyAlerts.canNotify(context)) permission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                    else model.updateAlerts(context, enabled = on)
                }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Seulement les achats conseillés (signal + zone)", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.alertsStrongOnly, { model.updateAlerts(context, strongOnly = it) }, enabled = model.alertsEnabled, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            if (denied) Notice("Notifications refusées : autorisez-les dans Paramètres Android → Applications → Altim → Notifications.", Tone.WARN)
            Caption("Toutes les 15 minutes, votre serveur vérifie le radar et vos avoirs : achetable seulement si la décision complète de l'actif dit ACHETER ou ZONE D'ACHAT (signal 4 h ou zone Fibonacci, sans source en désaccord, choc ni zone cassée). Une notification seulement quand un actif devient achetable ou que la raison change. Conseil indicatif : Altim ne passe aucun ordre.")
            model.lastAlertCheck?.let { Caption("Dernière vérification : ${Format.date(it.toDouble(), time = true)} · ${model.lastBuyable} actif(s) achetable(s).") }
        }
        Card(title = "Alertes actualité") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Me prévenir des actualités importantes", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.newsAlertsEnabled, { on ->
                    if (on && android.os.Build.VERSION.SDK_INT >= 33 && !BuyAlerts.canNotify(context)) newsPermission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                    else model.updateNewsAlerts(context, on)
                }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            Caption("Toutes les 15 minutes : une escalade grave (guerre déclarée, invasion, panique bancaire…) reprise par au moins 2 sources, ou un sujet sur un actif de votre radar ou de vos avoirs repris par au moins 3 sources, dans les 6 dernières heures. Un même sujet raconté par plusieurs médias ne prévient qu'une fois.")
        }
        Card(title = "Surveillance en arrière-plan") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Changements de configuration", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.configAlertsEnabled, { on -> switchOn("config", on) { model.updateConfigAlerts(context, it) } }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            Caption("Toutes les 15 minutes environ (selon Android), la décision des 20 premiers actifs du radar est relue (marché seul, 2 à la fois) et comparée à la dernière vue sur ce téléphone. Une seule notification regroupe les nouveaux changements (ATTENDRE → ZONE D'ACHAT…), avec les conditions manquantes et ce qui a changé ; un même changement ne prévient qu'une fois.")
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Positions dangereuses", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.dangerAlertsEnabled, { on -> switchOn("dangers", on) { model.updateDangerAlerts(context, it) } }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            Caption("Même vérification sur Mes avoirs : stop cassé, cours à moins d'une volatilité journalière (ATR) de votre stop, ou perte latente au-delà de votre risque accepté par idée. Prévenu quand une ligne entre dans cet état ou pour une nouvelle raison, pas à chaque vérification. Conseil indicatif : Altim ne passe aucun ordre.")
        }
        RiskCard(model)
        ScoreWeightsCard(model)
        Card(title = "Sécurité") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Verrouiller par empreinte, visage ou code", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.biometricLock, { model.updateBiometricLock(it) }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            Caption("Demandé à l'ouverture et après 2 minutes en arrière-plan. Le mot de passe et la session sont chiffrés par une clé du Keystore Android propre à ce téléphone, et exclus des sauvegardes.")
        }
        LocalOpenValidation.current?.let { openValidation ->
            Card(title = "Validation du modèle") {
                Caption("Le signal testé sur 34 actions, cryptos et ETF choisis à l'avance, par classe d'actifs et par régime de marché, avec ses biais et limites.")
                TextButton(onClick = openValidation) { Text("Voir la validation", color = AltimColors.cyan) }
            }
        }
        GlossaryCard()
        Card(title = "Données") {
            KeyValue("Sources de prix", "40 (23 crypto, 17 actions)")
            KeyValue("Prix en direct", "7 bourses crypto, actions toutes les 5 s")
            Caption("Chaque prix est la médiane des sources qui s'accordent ; une source qui s'écarte est écartée. Les calculs (signaux, zones de Fibonacci, garde-fou, sélection) sont faits sur votre serveur, comme sur le site.")
        }
        Card(title = "À savoir") {
            Caption("Altim est un conseiller : il ne passe aucun ordre et n'a accès à aucun de vos comptes. Les signaux sont des probabilités mesurées sur l'historique, jamais des certitudes ; investir comporte un risque de perte en capital.")
            KeyValue("Version", version)
        }
    }

    if (confirmLogout) {
        AlertDialog(
            onDismissRequest = { confirmLogout = false },
            title = { Text("Se déconnecter ?") },
            text = { Text("Le mot de passe enregistré sur ce téléphone sera effacé. Vos avoirs et votre radar restent.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmLogout = false
                    scope.launch { model.logout() }
                }) { Text("Se déconnecter", color = AltimColors.sell) }
            },
            dismissButton = { TextButton(onClick = { confirmLogout = false }) { Text("Annuler") } },
            containerColor = AltimColors.surface,
        )
    }
}

/** One setting of "Prudence des conseils": its bounds and step, as on the web app. */
private class RiskField(val label: String, val min: Double, val max: Double, val step: Double, val unit: String, val get: (RiskSettings) -> Double, val set: (RiskSettings, Double) -> RiskSettings)

private val RISK_FIELDS = listOf(
    RiskField("Risque accepté par idée", 0.25, 5.0, 0.25, " %", { it.riskPerTradePercent }, { r, v -> r.copy(riskPerTradePercent = v) }),
    RiskField("Taille max d'une ligne", 5.0, 100.0, 5.0, " %", { it.maxPositionPercent }, { r, v -> r.copy(maxPositionPercent = v) }),
    RiskField("Perte max du jour", 0.5, 10.0, 0.5, " %", { it.dailyLossLimitPercent }, { r, v -> r.copy(dailyLossLimitPercent = v) }),
    RiskField("Part crypto max", 0.0, 100.0, 5.0, " %", { it.maxCryptoPercent }, { r, v -> r.copy(maxCryptoPercent = v) }),
)

/** The user's limits, checked in Mes avoirs (Vos limites de risque, positions devenues dangereuses). */
@Composable
private fun RiskCard(model: AppModel) {
    Card(title = "Prudence des conseils") {
        Caption("Ces réglages servent aux contrôles de Mes avoirs : la part de votre patrimoine qu'une ligne peut perdre si son stop est touché, la taille maximale d'une ligne, la perte du jour et la part des cryptos.")
        RISK_FIELDS.forEach { f ->
            val v = f.get(model.risk)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(f.label, fontSize = 15.sp, modifier = Modifier.weight(1f))
                IconButton(onClick = { model.updateRisk(f.set(model.risk, maxOf(f.min, round2(v - f.step)))) }, enabled = v > f.min) {
                    Icon(Icons.Filled.Remove, contentDescription = "Diminuer ${f.label}", tint = AltimColors.cyan)
                }
                Text("${Format.plain(v, 2)}${f.unit}", style = mono(14.sp), textAlign = TextAlign.Center, modifier = Modifier.widthIn(min = 64.dp))
                IconButton(onClick = { model.updateRisk(f.set(model.risk, minOf(f.max, round2(v + f.step)))) }, enabled = v < f.max) {
                    Icon(Icons.Filled.Add, contentDescription = "Augmenter ${f.label}", tint = AltimColors.cyan)
                }
            }
        }
        Caption("Règle professionnelle : ne jamais risquer plus de 1 à 2 % de son patrimoine sur une seule idée.")
        Caption(
            "Perte max du jour : si votre patrimoine a déjà perdu ce pourcentage depuis la clôture de la veille, Mes avoirs vous conseille de ne plus ouvrir de position aujourd'hui. " +
                "Part crypto max : au-delà, Mes avoirs signale une surexposition aux cryptos, qui peuvent perdre 50 % ou plus ensemble (60 % par défaut ; 10 à 30 % est plus courant pour un patrimoine prudent).",
        )
        TextButton(onClick = { model.updateRisk(RiskSettings.DEFAULT) }) { Text("Valeurs recommandées", color = AltimColors.cyan) }
    }
}

private fun round2(v: Double) = Math.round(v * 100) / 100.0

/** Weights of the composite score of the Décision card (sent as w= when not the defaults; the verdict never changes). */
@Composable
private fun ScoreWeightsCard(model: AppModel) {
    val w = model.scoreWeights
    Card(title = "Score composite") {
        Caption("Poids de chaque famille dans le score de −100 à +100 de la carte Décision. Seuls les facteurs mesurés comptent : leurs poids sont ramenés à 100 %. Le verdict, lui, ne change pas.")
        ScoreWeights.FACTORS.forEach { f ->
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(f.label, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                        Caption(f.hint)
                    }
                    Text("${w[f.key]} %", style = mono(14.sp))
                }
                Slider(
                    value = w[f.key].toFloat(),
                    onValueChange = { model.updateScoreWeights(w.with(f.key, Math.round(it))) },
                    valueRange = 0f..100f,
                    modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Poids ${f.label}" },
                    colors = SliderDefaults.colors(thumbColor = AltimColors.cyan, activeTrackColor = AltimColors.cyan),
                )
            }
        }
        Caption("Total : ${w.total} (ramené à 100 %).${if (w.total == 0) " Tous à 0 : les poids par défaut sont utilisés." else ""}")
        TextButton(onClick = { model.updateScoreWeights(ScoreWeights.DEFAULT) }) { Text("Poids par défaut (32 / 18 / 20 / 10 / 10 / 10)", color = AltimColors.cyan) }
    }
}
