package com.maxlestage.altim.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.edit
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Calendar
import com.maxlestage.altim.kit.CalendarEvent
import com.maxlestage.altim.kit.CalendarReport
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.delay
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import java.time.LocalDate

// Agenda (Actu → Agenda, web/src/webapp/Agenda.tsx): the coming days' economic releases, central bank decisions,
// earnings, dividends, splits and IPOs, each with its source; what no free source covers is listed at the bottom.
// Chips wrap (FlowRow), rows stack: nothing scrolls sideways.

/** Everything the agenda shows, and what the user can change. */
class AgendaUi(
    val report: CalendarReport?,
    val error: String?,
    val loading: Boolean,
    val days: Int,
    val filter: String,
    val mine: Boolean,
    /** Stock symbols of the radar and holdings. */
    val stocks: List<String>,
    val today: LocalDate = Calendar.today(),
    val onDays: (Int) -> Unit = {},
    val onFilter: (String) -> Unit = {},
    val onMine: () -> Unit = {},
)

/** The Agenda view of the News tab: loads /api/calendar, every 15 minutes while open. */
@Composable
fun AgendaPane(model: AppModel, modifier: Modifier, header: @Composable () -> Unit) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim.news", Context.MODE_PRIVATE) }
    var report by remember { mutableStateOf<CalendarReport?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(true) }
    var refresh by remember { mutableIntStateOf(0) }
    var days by remember { mutableIntStateOf(14) }
    var filter by remember { mutableStateOf(prefs.getString("agendaFilter", "all")?.takeIf { f -> Calendar.FILTERS.any { it.first == f } } ?: "all") }
    var mine by remember { mutableStateOf(prefs.getBoolean("agendaMine", false)) }

    val stocks = Calendar.stockSymbols(model.watchlist + model.holdings.map { it.asset })
    // "Mes actifs": the server gives these stocks' events (even small companies); without any stock, the whole
    // calendar is filtered on the phone.
    val asked = if (mine && stocks.isNotEmpty()) stocks else null
    LaunchedEffect(days, asked?.joinToString(","), refresh) {
        while (true) {
            val client = model.client ?: return@LaunchedEffect
            loading = true
            try {
                report = client.calendar(days, asked)
                error = null
                model.persistSession()
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "Agenda indisponible"
            } finally {
                loading = false
            }
            delay(900_000)
        }
    }

    // Risk of the next 7 days: its own request, with the user's stocks and the largest companies together (top=1),
    // whatever the list's filters.
    val held = Calendar.stockSymbols(model.holdings.map { it.asset })
    val watched = Calendar.stockSymbols(model.watchlist)
    val riskStocks = (held + watched).distinct().take(50)
    var risk by remember { mutableStateOf<CalendarReport?>(null) }
    var riskError by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(riskStocks.joinToString(","), refresh) {
        val client = model.client ?: return@LaunchedEffect
        try {
            risk = client.calendar(7, riskStocks.ifEmpty { null }, top = true)
            riskError = null
        } catch (e: AltimException.Unauthorized) {
            model.sessionLost()
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            riskError = e.message ?: "Calendrier indisponible"
        }
    }
    val riskDays = risk?.let { r -> Calendar.riskDays(r.events, r.from, held, watched, 7, Calendar.failedDays(r)) }

    val ui = AgendaUi(
        report, error, loading, days, filter, mine, stocks,
        onDays = { days = it },
        onFilter = { f ->
            filter = f
            prefs.edit { putString("agendaFilter", f) }
        },
        onMine = {
            mine = !mine
            prefs.edit { putBoolean("agendaMine", mine) }
        },
    )
    PullToRefreshBox(isRefreshing = false, onRefresh = { refresh++ }, modifier = modifier) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item { header() }
            item { RiskWeekCard(riskDays, riskError) }
            agendaItems(ui)
        }
    }
}

/** The agenda's rows (settings card, days with their events, what is not covered), for a LazyColumn. */
@OptIn(ExperimentalLayoutApi::class)
fun LazyListScope.agendaItems(ui: AgendaUi) {
    item {
        Card(title = "Agenda") {
            Caption("Publications économiques majeures, décisions des banques centrales, résultats, dividendes, splits et introductions en bourse. Heures de Paris ; chiffres tels que publiés par la source.")
            ChoiceRow(Calendar.DAYS, ui.days, ui.onDays, description = "Période", fontSize = 13.sp)
            FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Calendar.FILTERS.forEach { (key, label) -> AgendaChip(label, ui.filter == key) { ui.onFilter(key) } }
                AgendaChip("${if (ui.mine) "✓ " else ""}Mes actifs", ui.mine) { ui.onMine() }
            }
            if (ui.mine) {
                val n = ui.stocks.size
                Caption(
                    if (n > 0) "Résultats, dividendes et splits de vos $n action${if (n > 1) "s" else ""} (radar et avoirs), plus l'économie et les banques centrales, qui concernent tous les actifs, cryptos compris."
                    else "Aucune action dans votre radar ni vos avoirs : seules l'économie et les banques centrales, qui concernent aussi les cryptos, sont affichées.",
                )
            }
        }
    }
    val r = ui.report
    ui.error?.let { e -> item { Notice(e, Tone.WARN) } }
    if (r == null && ui.error == null) item { Loading("Chargement de l'agenda…") }
    if (r != null && ui.loading) item { Caption("Mise à jour…") }
    r?.let { Calendar.failedSources(it) }?.let { item { Notice(it, Tone.WARN) } }
    if (r == null) return
    val groups = Calendar.groupByDay(Calendar.filterEvents(r.events, ui.filter, if (ui.mine) ui.stocks else null))
    if (groups.isEmpty()) item { Caption("Aucun événement de ce type sur la période.") }
    groups.forEach { g ->
        item(key = "day:${g.day}") {
            Text(
                Calendar.dayLabel(g.day, ui.today).uppercase(),
                color = AltimColors.textSecondary, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp,
                modifier = Modifier.padding(top = 4.dp).semantics { heading() },
            )
        }
        g.events.forEachIndexed { i, e -> item(key = "ev:${g.day}:$i:${e.kind}:${e.title}") { AgendaEventRow(e) } }
    }
    item { NotCoveredCard(r) }
}

@Composable
private fun AgendaChip(label: String, selected: Boolean, onClick: () -> Unit) {
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(label) },
        colors = FilterChipDefaults.filterChipColors(
            selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f),
            selectedLabelColor = AltimColors.cyan,
            labelColor = AltimColors.textSecondary,
        ),
    )
}

/** Opens the source's page in the browser (http/https links only). */
private fun openLink(context: Context, url: String) {
    val safe = url.toHttpUrlOrNull()?.toString() ?: return
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(safe)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    } catch (_: ActivityNotFoundException) {
        // No browser installed: nothing to open.
    }
}

@Composable
private fun Tag(text: String, color: Color = AltimColors.textSecondary) {
    Text(text, fontSize = 11.sp, color = color, modifier = Modifier.clip(RoundedCornerShape(50)).background(Color.White.copy(alpha = 0.08f)).padding(horizontal = 8.dp, vertical = 2.dp))
}

/** One event (Agenda, and the decision card's "Agenda (7 jours)" with [timeLabel] = "Demain · 14:30"). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AgendaEventRow(e: CalendarEvent, timeLabel: String? = null) {
    val context = LocalContext.current
    val shape = RoundedCornerShape(14.dp)
    val importance = if (e.high) "Importance haute" else "Importance moyenne"
    Column(
        Modifier.fillMaxWidth().clip(shape).background(AltimColors.surface)
            .border(1.dp, if (e.high) AltimColors.sell.copy(alpha = 0.35f) else Color.White.copy(alpha = 0.06f), shape)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Time on its own line: the title keeps the whole width ("avant l'ouverture" would squeeze it at 360 dp).
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(if (e.high) AltimColors.sell else AltimColors.warning).semantics { contentDescription = importance })
            Text(timeLabel ?: e.time ?: "Journée", style = mono(12.sp), color = AltimColors.textSecondary)
        }
        Text(e.title, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = Color.White)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Tag(e.categoryLabel)
            e.country?.let { Tag(it) }
            e.symbol?.let { Tag(it, Color.White) }
            e.originalName?.takeIf { it != e.title }?.let { Tag(it) }
        }
        if (e.actual != null || e.consensus != null || e.previous != null) {
            val earnings = e.kind == "earnings"
            FlowRow(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                e.actual?.let { Text("Publié $it", fontSize = 12.sp, color = Color.White) }
                e.consensus?.let { Text("${if (earnings) "Consensus" else "Attendu"} $it", fontSize = 12.sp, color = Color.White) }
                e.previous?.let { Text(if (earnings) it else "Précédent $it", fontSize = 12.sp, color = if (earnings) AltimColors.textSecondary else Color.White) }
            }
        }
        e.detail?.let { Caption(it) }
        e.note?.let { Text(it, fontSize = 12.sp, color = AltimColors.warning) }
        Text(
            "Source : ${e.source}",
            fontSize = 12.sp, color = AltimColors.cyan,
            modifier = Modifier.clickable(role = Role.Button, onClickLabel = "Ouvrir la source") { openLink(context, e.url) },
        )
    }
}

@Composable
private fun NotCoveredCard(r: CalendarReport) {
    var sources by remember { mutableStateOf(false) }
    Card(title = "Non couvert") {
        r.notCovered.forEach { Text("• $it", fontSize = 12.sp, color = Color.White.copy(alpha = 0.88f)) }
        Caption("« · » sépare plusieurs séries publiées sous le même nom par la source (souvent la variation sur un mois et sur un an), dans l'ordre de la source.")
        TextButton(onClick = { sources = !sources }) {
            Text("Sources · ${r.sources.count { it.ok }}/${r.sources.size} en ligne ${if (sources) "▲" else "▼"}", color = AltimColors.cyan)
        }
        if (sources) {
            r.sources.forEach { s ->
                Caption(
                    if (s.ok) "✔ ${s.name}" else "✕ ${s.name} · ${s.error ?: "indisponible"}${if (s.failed.isNotEmpty()) " (${s.failed.joinToString(", ")})" else ""}",
                    if (s.ok) Color.White else AltimColors.textSecondary,
                )
            }
        }
    }
}
