package com.maxlestage.altim.ui

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Lightbulb
import androidx.compose.material.icons.filled.NotificationsActive
import androidx.compose.material.icons.filled.Newspaper
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material.icons.filled.MonitorHeart
import androidx.compose.material.icons.filled.Radar
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.WarningAmber
import androidx.compose.material.icons.filled.Work
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.Asset

@Composable
fun Root(model: AppModel) {
    AppBackground {
        when {
            !model.acceptedDisclaimer -> DisclaimerScreen(model)
            model.phase == AppModel.Phase.SETUP -> LoginScreen(model)
            model.phase == AppModel.Phase.LOCKED -> LockScreen(model)
            else -> MainTabs(model)
        }
    }
    // Live prices of everything visible: watch list, holdings, Sélection picks, simulated positions and the open asset.
    var resumed by remember { mutableStateOf(false) }
    LifecycleResumeEffect(Unit) {
        resumed = true
        onPauseOrDispose { resumed = false }
    }
    val followed = model.watchlist + model.holdings.map { it.asset } + model.paper.positions.map { it.asset } + model.selectionAssets +
        listOfNotNull(model.focus, model.activityAsset)
    val key = "${model.phase}|$resumed|" + followed.map { it.id }.toSortedSet().joinToString(",")
    LaunchedEffect(key) {
        if (model.phase != AppModel.Phase.READY || !resumed) model.live.stop()
        else model.live.follow(followed, model.client, onRenewed = { model.persistSession() }) { model.sessionLost() }
    }
    // The live following (iOS Live Activity) moves with the live price while the app runs (at most every 5 s).
    val tracked = model.activityAsset?.let { model.live.price(it) }
    LaunchedEffect(tracked) { tracked?.let { model.liveTrackTick(it) } }
    // EUR/USD rate of the display: read now, then every 10 minutes while the app is in front.
    LaunchedEffect(model.phase, resumed, model.client) {
        if (model.phase != AppModel.Phase.READY || !resumed) return@LaunchedEffect
        while (true) {
            try {
                model.refreshFx()
            } catch (_: com.maxlestage.altim.kit.AltimException.Unauthorized) {
                // The screens' own calls handle the expired session.
            }
            kotlinx.coroutines.delay(com.maxlestage.altim.kit.Fx.REFRESH_MS)
        }
    }
}

/** Screens opened above a tab and its assets (from Réglages or a Décision card). */
private enum class Overlay { VALIDATION, BOT }

private enum class Tab(val label: String, val icon: ImageVector) {
    RADAR("Radar", Icons.Filled.Radar),
    SELECTION("Sélection", Icons.AutoMirrored.Filled.List),
    HOLDINGS("Mes avoirs", Icons.Filled.Work),
    ALERTS("Alertes", Icons.Filled.NotificationsActive),
    NEWS("Actu", Icons.Filled.Newspaper),
}

/** Each tab keeps its own stack of opened assets (like one NavigationStack per tab on iPhone). */
@Composable
fun MainTabs(model: AppModel) {
    var tab by rememberSaveable { mutableStateOf(Tab.RADAR) }
    val stacks = remember { Tab.entries.associateWith { mutableStateListOf<Asset>() } }
    val stack: SnapshotStateList<Asset> = stacks.getValue(tab)
    var settingsOpen by rememberSaveable { mutableStateOf(false) }
    // « Validation du modèle » or « Bot Altim », opened from Réglages or from an asset's Décision card: the size of the
    // tab's asset stack when it was opened (-1: closed). Assets opened from it go above it, back returns to it.
    var overlayAt by rememberSaveable { mutableIntStateOf(-1) }
    var overlay by rememberSaveable { mutableStateOf(Overlay.VALIDATION) }
    val overlayOpen = overlayAt >= 0
    val money = model.displayCurrency
    /** Opens [kind] above the current screen (a no-op when it is already the open one; the other one is replaced). */
    val openOverlay: (Overlay) -> Unit = { kind ->
        if (!overlayOpen || overlay != kind) {
            overlay = kind
            overlayAt = stack.size
        }
    }
    val open: (Asset) -> Unit = {
        if (!overlayOpen) settingsOpen = false
        stack.add(it)
    }
    BackHandler(enabled = settingsOpen && !overlayOpen) { settingsOpen = false }
    BackHandler(enabled = stack.isNotEmpty() && !settingsOpen && !overlayOpen) { stack.removeAt(stack.lastIndex) }
    BackHandler(enabled = overlayOpen) {
        if (stack.size > overlayAt) stack.removeAt(stack.lastIndex) else overlayAt = -1
    }
    // The stack shrank under the screen (tab re-tapped): the screen closes with it.
    LaunchedEffect(stack.size) { if (overlayAt > stack.size) overlayAt = -1 }
    LaunchedEffect(stack.lastOrNull()) { model.focus = stack.lastOrNull() }
    // Tapped notification: open the asset on top of the Radar.
    LaunchedEffect(model.pendingOpen) {
        val a = model.pendingOpen ?: return@LaunchedEffect
        overlayAt = -1
        tab = Tab.RADAR
        stacks.getValue(Tab.RADAR).add(a)
        model.pendingOpen = null
    }
    // Tapped news notification: the Actu tab.
    LaunchedEffect(model.pendingNews) {
        if (!model.pendingNews) return@LaunchedEffect
        settingsOpen = false
        overlayAt = -1
        tab = Tab.NEWS
        model.pendingNews = false
    }

    Scaffold(
        containerColor = Color.Transparent,
        bottomBar = {
            Column {
            model.offlineSince?.let { OfflineBanner(it) }
            NavigationBar(containerColor = AltimColors.surface) {
                Tab.entries.forEach { t ->
                    NavigationBarItem(
                        selected = t == tab,
                        onClick = {
                            settingsOpen = false
                            overlayAt = -1
                            if (t == tab) stack.clear() else tab = t
                        },
                        icon = { Icon(t.icon, contentDescription = null) },
                        label = { Text(t.label, fontSize = 11.sp) },
                        colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = AltimColors.cyan,
                            selectedTextColor = AltimColors.cyan,
                            indicatorColor = AltimColors.cyan.copy(alpha = 0.12f),
                            unselectedIconColor = AltimColors.textSecondary,
                            unselectedTextColor = AltimColors.textSecondary,
                        ),
                    )
                }
            }
            }
        },
    ) { padding ->
        val m = Modifier.padding(padding).fillMaxSize()
        val covered = stack.isNotEmpty()
        val blockTouches = m.pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent(PointerEventPass.Final).changes.forEach { it.consume() } } }
        CompositionLocalProvider(LocalOpenValidation provides { openOverlay(Overlay.VALIDATION) }, LocalOpenBot provides { openOverlay(Overlay.BOT) }) {
        // The tab stays composed under the open asset: coming back keeps its list, scroll and loaded data.
        // While covered, it is hidden from TalkBack and receives no touch.
        Box(if (covered || settingsOpen || overlayOpen) Modifier.clearAndSetSemantics { } else Modifier) {
            // The display currency changed (Réglages, or the first rate read): the screens redraw their amounts and
            // ask the server again (its texts follow the currency).
            key(money) {
            when (tab) {
            Tab.RADAR -> RadarScreen(model, m, open, onSettings = { settingsOpen = true })
            Tab.SELECTION -> SelectionScreen(model, m, open)
            Tab.HOLDINGS -> HoldingsScreen(model, m, open)
            Tab.ALERTS -> AlertsScreen(model, m, open)
            Tab.NEWS -> NewsScreen(model, m, open)
            }
            }
        }
        // The asset below the validation screen (or the only one when it is closed).
        stack.lastOrNull()?.takeIf { !overlayOpen || stack.size <= overlayAt }?.let { top ->
            Box(if (overlayOpen || settingsOpen) Modifier.clearAndSetSemantics { } else Modifier) {
                AppBackground(blockTouches) {
                    key(top.id, stack.size, money) { AssetDetailScreen(model, top, Modifier.fillMaxSize(), onBack = { stack.removeAt(stack.lastIndex) }) }
                }
            }
        }
        // Réglages, opened from the gear of the Radar, above the tab and its assets.
        if (settingsOpen) {
            Box(if (overlayOpen) Modifier.clearAndSetSemantics { } else Modifier) {
                AppBackground(blockTouches) {
                    SettingsScreen(model, Modifier.fillMaxSize(), onBack = { settingsOpen = false })
                }
            }
        }
        // « Validation du modèle » or « Bot Altim », kept composed (data and scroll) under the assets opened from it.
        if (overlayOpen) {
            val coveredByAsset = stack.size > overlayAt
            Box(if (coveredByAsset) Modifier.clearAndSetSemantics { } else Modifier) {
                AppBackground(blockTouches) {
                    key(money) {
                    when (overlay) {
                        Overlay.VALIDATION -> ValidationScreen(model, Modifier.fillMaxSize(), open, onBack = { overlayAt = -1 })
                        Overlay.BOT -> BotScreen(model, Modifier.fillMaxSize(), open, onBack = { overlayAt = -1 })
                    }
                    }
                }
            }
            stack.lastOrNull()?.takeIf { coveredByAsset }?.let { top ->
                AppBackground(blockTouches) {
                    key(top.id, stack.size, money) { AssetDetailScreen(model, top, Modifier.fillMaxSize(), onBack = { stack.removeAt(stack.lastIndex) }) }
                }
            }
        }
        }
    }
}

/** Mandatory warning at first launch. */
@Composable
fun DisclaimerScreen(model: AppModel) {
    var understood by remember { mutableStateOf(false) }
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(24.dp),
    ) {
        Text("ALTIM", style = TextStyle(brush = AltimColors.accentGradient, fontSize = 56.sp, fontWeight = FontWeight.Black), modifier = Modifier.padding(top = 32.dp))
        Text("Avant de commencer", fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Card(glow = AltimColors.warning) {
            Point(Icons.Filled.MonitorHeart, "Les signaux sont des probabilités calculées sur l'historique, jamais des certitudes. Aucun outil ne garantit un gain.")
            Point(Icons.Filled.WarningAmber, "Investir en crypto-actifs et en actions comporte un risque de perte totale du capital investi.")
            Point(Icons.Filled.Lightbulb, "Altim est un conseiller : il ne passe aucun ordre et ne demande aucun accès à vos comptes. Vous décidez, chez votre courtier habituel.")
            Point(Icons.Filled.Security, "Vos avoirs restent sur ce téléphone, jamais envoyés au serveur. Les analyses viennent de votre serveur Altim privé (40 sources de prix).")
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("J'ai compris que je reste seul responsable de mes décisions d'investissement.", fontSize = 14.sp, modifier = Modifier.weight(1f))
            Spacer(Modifier.size(12.dp))
            Switch(understood, { understood = it }, colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan))
        }
        NeonButton("ENTRER", enabled = understood) { model.acceptDisclaimer() }
    }
}

@Composable
private fun Point(icon: ImageVector, text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(icon, contentDescription = null, tint = AltimColors.cyan, modifier = Modifier.size(22.dp))
        Text(text, fontSize = 14.sp, color = Color.White.copy(alpha = 0.85f))
    }
}

/** Fingerprint, face or the phone's screen lock (PIN, pattern), like Face ID / passcode on iPhone. */
@Composable
fun LockScreen(model: AppModel) {
    val activity = LocalActivity.current as FragmentActivity
    val prompt = {
        val keyguard = activity.getSystemService(android.app.KeyguardManager::class.java)
        // Opens without asking only when the phone has no screen lock at all (nothing to check against). Any other
        // unavailability (sensor busy, temporary error) keeps the app locked and shows the prompt, which falls back
        // to the PIN / pattern.
        if (keyguard?.isDeviceSecure != true) {
            model.unlocked()
        } else {
            BiometricPrompt(
                activity,
                ContextCompat.getMainExecutor(activity),
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = model.unlocked()
                },
            ).authenticate(
                BiometricPrompt.PromptInfo.Builder()
                    .setTitle("Déverrouiller Altim")
                    .setAllowedAuthenticators(BIOMETRIC_WEAK or DEVICE_CREDENTIAL)
                    .build(),
            )
        }
    }
    LaunchedEffect(Unit) { prompt() }
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(24.dp, Alignment.CenterVertically),
    ) {
        Icon(Icons.Outlined.Lock, contentDescription = null, tint = AltimColors.cyan, modifier = Modifier.size(64.dp))
        Text("Altim est verrouillé", fontSize = 24.sp, fontWeight = FontWeight.Black)
        Text(
            "L'empreinte, le visage ou le verrouillage de l'écran protège vos avoirs et votre accès.",
            color = AltimColors.textSecondary, textAlign = TextAlign.Center, fontSize = 14.sp,
        )
        NeonButton("Déverrouiller", modifier = Modifier.fillMaxWidth()) { prompt() }
    }
}

/** Shown above the tabs while the server cannot be reached: the screens show the last data received, dated. */
@Composable
fun OfflineBanner(since: Long) {
    Row(
        Modifier.fillMaxWidth().background(AltimColors.warning.copy(alpha = 0.18f)).padding(horizontal = 16.dp, vertical = 8.dp)
            .semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(Icons.Filled.CloudOff, contentDescription = null, tint = AltimColors.warning, modifier = Modifier.size(18.dp))
        Text(
            "Hors ligne : données du ${com.maxlestage.altim.kit.Format.date(since.toDouble(), time = true)}. Reconnexion automatique.",
            fontSize = 12.sp,
            color = Color.White,
        )
    }
}
