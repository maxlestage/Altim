package com.maxlestage.altim.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.edit
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.NewsItem
import com.maxlestage.altim.kit.NewsReport
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.delay

private val FILTERS = listOf("all" to "Tout", "actifs" to "Mes actifs", "monde" to "Monde", "marches" to "Marchés", "crypto" to "Crypto")

/** News tab: every feed in one place, stories told by several sources merged, the user's assets first. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NewsScreen(model: AppModel, modifier: Modifier, open: (Asset) -> Unit = {}) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("altim.news", Context.MODE_PRIVATE) }
    var report by remember { mutableStateOf<NewsReport?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var refresh by remember { mutableIntStateOf(0) }
    var filter by remember { mutableStateOf(prefs.getString("filter", "all") ?: "all") }
    var frenchOnly by remember { mutableStateOf(prefs.getBoolean("frenchOnly", false)) }
    // Articles or Agenda (remembered on this phone, like the web app).
    var view by remember { mutableStateOf(if (prefs.getString("view", null) == "agenda") "agenda" else "articles") }
    val chooseView = { v: String ->
        view = v
        prefs.edit { putString("view", v) }
    }

    val assets = (model.watchlist + model.holdings.map { it.asset }).distinctBy { it.id }.take(20)
    LaunchedEffect(refresh, assets.joinToString { it.id }, view) {
        if (view != "articles") return@LaunchedEffect
        while (true) {
            val client = model.client ?: return@LaunchedEffect
            try {
                report = client.news(assets)
                error = null
                model.persistSession()
            } catch (e: AltimException.Unauthorized) {
                model.sessionLost()
                return@LaunchedEffect
            } catch (e: Exception) {
                error = e.message ?: "Actualités indisponibles"
            }
            delay(300_000)
        }
    }

    val r = report
    val shown = r?.items.orEmpty().filter { (filter == "all" || it.category == filter) && (!frenchOnly || it.lang == "fr") }
    val upSources = r?.sources?.count { it.ok } ?: 0

    if (view == "agenda") {
        AgendaPane(model, modifier.statusBarsPadding()) { NewsHeader("Les événements à venir, chacun avec sa source.", view, chooseView) }
        return
    }
    PullToRefreshBox(isRefreshing = false, onRefresh = { refresh++ }, modifier = modifier.statusBarsPadding()) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item {
                NewsHeader(r?.let { "${it.items.size} articles de $upSources sources sur 48 h, mis à jour ${NewsItem.ago(it.asOf)}." } ?: "Chargement des sources…", view, chooseView)
            }
            error?.let { e -> item { ErrorBox(e) { refresh++ } } }
            if (r == null && error == null) item { Loading("Lecture d'une vingtaine de sources…") }

            r?.summary?.let { s -> item { NewsSummaryCard(s, open) } }

            if (r != null && filter == "all") {
                val top = r.topItems
                if (top.isNotEmpty()) {
                    item {
                        Card(title = "À la une", glow = AltimColors.cyan) {
                            Caption("Les sujets repris par plusieurs sources, et toute escalade grave (guerre, panique bancaire…).")
                            top.forEach { Story(it, featured = true) }
                        }
                    }
                }
                if (r.digest.total > 0) item { DigestCard(r.digest) }
            }

            item {
                FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FILTERS.forEach { (key, label) ->
                        FilterChip(
                            selected = filter == key,
                            onClick = { filter = key; prefs.edit { putString("filter", key) } },
                            label = { Text(label) },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f),
                                selectedLabelColor = AltimColors.cyan,
                                labelColor = AltimColors.textSecondary,
                            ),
                        )
                    }
                }
            }
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Articles en français seulement", fontSize = 14.sp, modifier = Modifier.weight(1f))
                    Switch(
                        checked = frenchOnly,
                        onCheckedChange = { frenchOnly = it; prefs.edit { putBoolean("frenchOnly", it) } },
                        colors = SwitchDefaults.colors(checkedTrackColor = AltimColors.cyan),
                    )
                }
            }
            if (r != null && shown.isEmpty()) item { Caption("Aucun article dans cette rubrique pour le moment.") }
            items(shown, key = { it.id }) { n ->
                Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(AltimColors.surface).padding(horizontal = 14.dp, vertical = 4.dp)) {
                    Story(n, featured = false)
                }
            }
            if (r != null) item { SourcesCard(r.sources) }
        }
    }
}

private val VIEWS = listOf("articles" to "Articles", "agenda" to "Agenda")

/** Title, what is shown, and the "Articles / Agenda" choice. */
@Composable
private fun NewsHeader(caption: String, view: String, onView: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Actualités", fontSize = 30.sp, fontWeight = FontWeight.Bold)
        Caption(caption)
        ChoiceRow(VIEWS, view, onView, description = "Vue")
    }
}

/** Opens the article at its source, in the browser (http/https links only). */
private fun openArticle(context: Context, n: NewsItem) {
    val url = n.safeUrl ?: return
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    } catch (_: ActivityNotFoundException) {
        // No browser installed: nothing to open.
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Story(n: NewsItem, featured: Boolean) {
    val context = LocalContext.current
    Column(
        Modifier.fillMaxWidth()
            .clickable(role = Role.Button, onClickLabel = "Lire l'article chez ${n.source}") { openArticle(context, n) }
            .padding(vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(n.title, fontWeight = FontWeight.SemiBold, fontSize = if (featured) 15.sp else 14.sp, color = Color.White)
        if (featured) n.summary?.let { Text(it, fontSize = 12.sp, color = AltimColors.textSecondary, maxLines = 3) }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(n.source, fontSize = 11.sp, color = Color.White.copy(alpha = 0.85f))
            Text(n.age(), fontSize = 11.sp, color = AltimColors.textSecondary)
            if (n.alsoIn.isNotEmpty()) Text("+${n.alsoIn.size} source${if (n.alsoIn.size > 1) "s" else ""}", fontSize = 11.sp, color = AltimColors.textSecondary)
            if (n.lang == "en") Text("EN", fontSize = 11.sp, color = AltimColors.textSecondary)
            when (n.tone) {
                "negative" -> Text("▼ négatif", fontSize = 11.sp, color = AltimColors.sell)
                "positive" -> Text("▲ positif", fontSize = 11.sp, color = AltimColors.buy)
            }
        }
        if (n.alert || n.assets.isNotEmpty() || n.themes.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (n.alert) Badge("ALERTE", Tone.BAD)
                n.assets.forEach { Chip(it.substringAfter(":"), Color.White) }
                n.themes.forEach { Chip(NewsItem.themeLabels[it] ?: it, AltimColors.textSecondary) }
            }
        }
    }
}

@Composable
private fun Chip(text: String, color: Color) {
    Text(
        text,
        fontSize = 11.sp,
        color = color,
        modifier = Modifier.clip(RoundedCornerShape(50)).background(Color.White.copy(alpha = 0.08f)).padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun DigestCard(d: NewsReport.Digest) {
    Card(title = "Ce qui domine (24 h)", glow = AltimColors.cyan) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            d.themes.forEach { Chip("${it.label} · ${it.count}", Color.White) }
        }
        val t = d.tone
        val label = "Ton des titres : ${t.negative} négatifs, ${t.neutral} neutres, ${t.positive} positifs"
        Row(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)).semantics { contentDescription = label }) {
            listOf(t.negative to AltimColors.sell, t.neutral to AltimColors.textSecondary.copy(alpha = 0.5f), t.positive to AltimColors.buy).forEach { (n, c) ->
                if (n > 0) Box(Modifier.weight(n.toFloat()).height(8.dp).background(c))
            }
        }
        Caption("$label (repérage par mots-clés, indicatif).")
    }
}

@Composable
private fun SourcesCard(sources: List<NewsReport.Source>) {
    var expanded by remember { mutableStateOf(false) }
    Card {
        TextButton(onClick = { expanded = !expanded }) {
            Text("Sources · ${sources.count { it.ok }}/${sources.size} en ligne ${if (expanded) "▲" else "▼"}", color = AltimColors.cyan)
        }
        if (expanded) {
            sources.forEach { s ->
                Caption(if (s.ok) "✔ ${s.name} · ${s.count}" else "✕ ${s.name} · ${s.error ?: "indisponible"}", if (s.ok) Color.White else AltimColors.textSecondary)
            }
            Caption("Les titres sont affichés tels que publiés (non traduits) ; les liens ouvrent l'article chez sa source.")
        }
    }
}
