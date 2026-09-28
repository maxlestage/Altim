package com.maxlestage.altim.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Brief
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.delay
import java.util.Locale
import kotlin.math.abs

/** "Point du jour": the day in a few lines for the radar and the holdings, refreshed every 5 minutes. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun BriefCard(model: AppModel, open: (Asset) -> Unit) {
    val context = LocalContext.current
    var brief by remember { mutableStateOf<Brief?>(null) }
    val assets = (model.watchlist + model.holdings.map { it.asset }).distinctBy { it.id }.take(20)
    LaunchedEffect(assets.joinToString { it.id }) {
        while (true) {
            val client = model.client ?: return@LaunchedEffect
            try {
                brief = client.brief(assets)
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (_: Exception) {
                // The rest of the radar still shows; the brief comes back at the next attempt.
            }
            delay(300_000)
        }
    }
    val b = brief ?: return
    Card(title = "Point du jour") {
        Text(b.headline, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, color = Color.White)
        b.market?.themes?.takeIf { it.isNotEmpty() }?.let { Caption("Sujets du moment : ${it.joinToString().lowercase()}.") }
        val moves = b.movers.filter { abs(it.change) >= 0.05 }.take(4)
        if (moves.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                moves.forEach { m ->
                    Text(
                        "${m.symbol} ${(if (m.change >= 0) "+" else "−") + String.format(Locale.FRANCE, "%.1f", abs(m.change)).removeSuffix(",0")} %",
                        fontSize = 12.sp,
                        color = AltimColors.forChange(m.change),
                        modifier = Modifier.clip(RoundedCornerShape(50)).background(Color.White.copy(alpha = 0.08f))
                            .clickable(role = Role.Button, onClickLabel = "Ouvrir ${m.symbol}") { open(m.asset) }
                            .padding(horizontal = 10.dp, vertical = 4.dp),
                    )
                }
            }
        }
        b.news.forEach { n ->
            Text(
                (if (n.alert) "ALERTE · " else "") + n.title + " · ${n.source}" + (if (n.alsoIn.isNotEmpty()) " +${n.alsoIn.size}" else ""),
                fontSize = 13.sp,
                color = if (n.alert) AltimColors.of(Tone.BAD) else Color.White.copy(alpha = 0.9f),
                modifier = Modifier.clickable(role = Role.Button, onClickLabel = "Lire l'article chez ${n.source}") {
                    n.safeUrl?.let { url ->
                        try {
                            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        } catch (_: ActivityNotFoundException) {
                        }
                    }
                },
            )
        }
        Caption("Achetable = signal 4 h à l'achat ou prix dans une zone d'achat Fibonacci, sans blocage (la règle des notifications). Variations depuis la dernière clôture journalière. Conseil indicatif : Altim ne passe aucun ordre.")
    }
}
