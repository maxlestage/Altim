package com.maxlestage.altim.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.AskAnswer
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Tone
import com.maxlestage.altim.kit.Why
import com.maxlestage.altim.kit.WhyFactor
import com.maxlestage.altim.kit.WhyReport
import kotlinx.coroutines.launch

// « Pourquoi ça bouge ? » on the asset screen (web WhyCard.tsx): co-occurring observations from the server's cached
// sources, never presented as causes; a question to the server's AI model only when it says `askEnabled`.

/** Loads /api/why for this asset; the question goes to POST /api/ask. */
@Composable
fun WhyCard(model: AppModel, asset: Asset) {
    var state by remember(asset.id) { mutableStateOf<Loadable<WhyReport>>(Loadable.Loading) }
    var question by remember(asset.id) { mutableStateOf("") }
    var asking by remember(asset.id) { mutableStateOf(false) }
    var answer by remember(asset.id) { mutableStateOf<AskAnswer?>(null) }
    var askError by remember(asset.id) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(asset.id) {
        val client = model.client ?: return@LaunchedEffect
        state = try {
            Loadable.Loaded(client.why(asset))
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
            return@LaunchedEffect
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            Loadable.Failed(e.message ?: "Indisponible")
        }
    }
    WhyView(
        state, question, asking, answer, askError,
        onQuestion = { question = it.take(500) },
        onAsk = {
            val client = model.client
            if (client != null && Why.canAsk(question) && !asking) {
                asking = true
                askError = null
                answer = null
                scope.launch {
                    try {
                        answer = client.ask(asset, question.trim())
                    } catch (e: AltimException.Unauthorized) {
                        model.sessionLost()
                    } catch (e: kotlinx.coroutines.CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        askError = e.message ?: "Indisponible"
                    } finally {
                        asking = false
                    }
                }
            }
        },
    )
}

/** The card from its state (no server: also rendered by the tests). */
@Composable
fun WhyView(
    state: Loadable<WhyReport>,
    question: String = "",
    asking: Boolean = false,
    answer: AskAnswer? = null,
    askError: String? = null,
    onQuestion: (String) -> Unit = {},
    onAsk: () -> Unit = {},
) {
    Card(title = "Pourquoi ça bouge ?") {
        when (state) {
            is Loadable.Loading -> Loading()
            is Loadable.Failed -> Notice(state.message, Tone.WARN)
            is Loadable.Loaded -> {
                val r = state.value
                Text(r.summary, fontSize = 14.sp, color = Color.White)
                r.factors.forEach { FactorRow(it) }
                Why.notCoveredText(r)?.let { Caption(it) }
                Caption(r.disclaimer)
                if (r.askEnabled) {
                    OutlinedTextField(
                        value = question,
                        onValueChange = onQuestion,
                        label = { Text("Poser une question sur ces données") },
                        placeholder = { Text("ex. La baisse vient-elle du marché ou de l'actif ?", color = AltimColors.textSecondary) },
                        minLines = 2,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.fillMaxWidth(),
                        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, cursorColor = AltimColors.cyan, focusedLabelColor = AltimColors.cyan),
                    )
                    val can = Why.canAsk(question) && !asking
                    OutlinedButton(
                        onClick = onAsk, enabled = can, modifier = Modifier.fillMaxWidth(),
                        border = BorderStroke(1.2.dp, if (can) AltimColors.cyan else AltimColors.textSecondary),
                    ) { Text(if (asking) "Réponse en cours…" else "Demander", color = if (can) AltimColors.cyan else AltimColors.textSecondary) }
                    Caption(Why.ASK_NOTE)
                    askError?.let { Notice(it, Tone.WARN) }
                    answer?.let { a ->
                        Column(Modifier.semantics { liveRegion = LiveRegionMode.Polite }, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(a.answer, fontSize = 14.sp, color = Color.White)
                            Caption(Why.answerDataText(a))
                        }
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun FactorRow(f: WhyFactor) {
    val tone = when (f.certainty) { "observed" -> Tone.GOOD; "possibleCorrelation" -> Tone.NEUTRAL; else -> Tone.WARN }
    val c = AltimColors.of(tone)
    Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(Why.directionIcon(f.direction), color = c, fontSize = 15.sp, fontWeight = FontWeight.Bold, modifier = Modifier.width(18.dp).semantics { contentDescription = Why.directionWord(f.direction) })
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(f.label, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.White)
                Badge(f.certaintyLabel, tone)
            }
            Text(f.detail, fontSize = 13.sp, color = Color.White.copy(alpha = 0.9f))
            Caption("Sens ${Why.directionWord(f.direction)}, ampleur ${Why.magnitudeWord(f.magnitude)} · source : ${f.source}")
        }
    }
}
