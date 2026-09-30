package com.maxlestage.altim.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.BotReport
import com.maxlestage.altim.kit.BotResult
import com.maxlestage.altim.kit.BotV3
import kotlinx.coroutines.delay

// « Bot Altim » on the Radar (web BotRadarCard.tsx): the report's headline (read from /api/bot, never written here), the
// forward test's counter and a link to the Bot screen. Loading, pending (first training) and error states; one compact card.

enum class BotRadarState { LOADING, PENDING, ERROR, READY }

/** Loads /api/bot (asked again every 15 s while the first training runs), then shows [BotRadarView]. */
@Composable
fun BotRadarCard(model: AppModel) {
    var report by remember { mutableStateOf<BotReport?>(null) }
    var state by remember { mutableStateOf(BotRadarState.LOADING) }
    LaunchedEffect(Unit) {
        while (true) {
            val client = model.client ?: return@LaunchedEffect
            try {
                when (val r = client.bot()) {
                    is BotResult.Ready -> {
                        report = r.report
                        state = BotRadarState.READY
                        return@LaunchedEffect
                    }
                    BotResult.Pending -> state = BotRadarState.PENDING
                }
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (_: Exception) {
                state = BotRadarState.ERROR
                return@LaunchedEffect
            }
            delay(15_000)
        }
    }
    BotRadarView(state, report)
}

/** The card itself (pure, rendered by the tests). */
@Composable
fun BotRadarView(state: BotRadarState, report: BotReport?) {
    val openBot = LocalOpenBot.current
    Card(title = "Bot Altim") {
        when (state) {
            BotRadarState.LOADING -> Caption("Chargement…")
            BotRadarState.PENDING -> Caption("Entraînement et test en cours sur le serveur (plusieurs minutes la première fois)…")
            BotRadarState.ERROR -> Caption("Résultats indisponibles pour le moment.")
            BotRadarState.READY -> if (report != null) {
                Text(BotV3.radarHeadline(report), fontSize = 13.sp, color = Color.White.copy(alpha = 0.92f))
                BotV3.radarCounter(report)?.let { Text(it, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White) }
            }
        }
        Text(
            "Voir le bot →",
            color = AltimColors.cyan, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
            modifier = Modifier.fillMaxWidth()
                .then(if (openBot != null) Modifier.clickable(role = Role.Button, onClick = openBot) else Modifier)
                .padding(vertical = 4.dp),
        )
    }
}
