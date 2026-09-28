package com.maxlestage.altim.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.BuyAlerts
import com.maxlestage.altim.kit.Format
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
            Caption("Toutes les 15 minutes, votre serveur vérifie le radar et vos avoirs : achetable si le signal 4 h dit ACHAT ou si le prix est dans une zone d'achat Fibonacci, sauf sources en désaccord, risque de choc ou zone cassée. Une notification seulement quand un actif devient achetable ou que la raison change. Conseil indicatif : Altim ne passe aucun ordre.")
            model.lastAlertCheck?.let { Caption("Dernière vérification : ${Format.date(it.toDouble(), time = true)} · ${model.lastBuyable} actif(s) achetable(s).") }
        }
        Card(title = "Sécurité") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Verrouiller par empreinte, visage ou code", modifier = Modifier.weight(1f), fontSize = 15.sp)
                Switch(model.biometricLock, { model.updateBiometricLock(it) }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
            }
            Caption("Demandé à l'ouverture et après 2 minutes en arrière-plan. Le mot de passe et la session sont chiffrés par une clé du Keystore Android propre à ce téléphone, et exclus des sauvegardes.")
        }
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
