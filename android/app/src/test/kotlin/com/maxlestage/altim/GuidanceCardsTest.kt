package com.maxlestage.altim

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import com.github.takahirom.roborazzi.captureRoboImage
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.data.SecretStore
import com.maxlestage.altim.data.SecureStore
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.AskAnswer
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Calendar
import com.maxlestage.altim.kit.CalendarReport
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.ClosedInfo
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.ConfigState
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.DecisionLevel
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.NewTradeEntry
import com.maxlestage.altim.kit.NoTrade
import com.maxlestage.altim.kit.NoTradeReason
import com.maxlestage.altim.kit.OpenOrder
import com.maxlestage.altim.kit.RiskPortfolio
import com.maxlestage.altim.kit.Strategies
import com.maxlestage.altim.kit.StrategiesReport
import com.maxlestage.altim.kit.StorySummary
import com.maxlestage.altim.kit.TradeJournal
import com.maxlestage.altim.kit.Verdict
import com.maxlestage.altim.kit.WhyReport
import com.maxlestage.altim.ui.AltimTheme
import com.maxlestage.altim.ui.AppBackground
import com.maxlestage.altim.ui.DecisionView
import com.maxlestage.altim.ui.JournalView
import com.maxlestage.altim.ui.Loadable
import com.maxlestage.altim.ui.NewsSummaryCard
import com.maxlestage.altim.ui.RiskWeekCard
import com.maxlestage.altim.ui.StrategiesView
import com.maxlestage.altim.ui.WhatIfCard
import com.maxlestage.altim.ui.WhyView
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.time.LocalDate
import java.time.ZoneOffset

private class GuidanceMemory : SecretStore {
    val values = mutableMapOf<SecureStore.Key, String>()
    override fun set(k: SecureStore.Key, value: String?) {
        if (value.isNullOrEmpty()) values.remove(k) else values[k] = value
    }
    override fun get(k: SecureStore.Key) = values[k]
    override fun clear() = values.clear()
}

/**
 * The cards of the news summary, the 7-day risk calendar, the strategy comparator, the decision's guidance, the journal,
 * « Et si… ? » and « Pourquoi ça bouge ? », rendered from the shared samples (no server) on a narrow 360 dp phone, with
 * the no-overflow check. Screenshots in app/build/screens/guidance-*.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w360dp-h4000dp-xhdpi", application = android.app.Application::class)
class GuidanceCardsTest {
    @get:Rule val compose = createComposeRule()

    private fun fixture(name: String) = File("../kit/src/test/resources/fixtures/$name").readText()
    private fun has(text: String) = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun fr(text: String) = text.replace(Regex("(?<=\\d) (?=\\d)"), " ")
    private fun expect(vararg texts: String) = texts.map(::fr).forEach { assertTrue("absent : $it", has(it)) }

    /** Nothing wider than the screen: no horizontal scroll at 360 dp. */
    private fun fitsWidth() {
        val width = compose.onRoot().fetchSemanticsNode().boundsInRoot.width
        fun walk(n: SemanticsNode) {
            assertTrue("dépasse à droite : ${n.config} (${n.boundsInRoot.right} > $width)", n.boundsInRoot.right <= width + 1f)
            n.children.forEach(::walk)
        }
        walk(compose.onRoot(useUnmergedTree = true).fetchSemanticsNode())
    }

    /** Content shown: set once per test, then replaced (the activity takes one setContent only). */
    private val shown = androidx.compose.runtime.mutableStateOf<(@androidx.compose.runtime.Composable () -> Unit)?>(null)

    private fun screen(content: @androidx.compose.runtime.Composable () -> Unit) {
        val first = shown.value == null
        shown.value = content
        if (first) {
            compose.setContent {
                AltimTheme {
                    AppBackground {
                        androidx.compose.runtime.key(shown.value) {
                            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) { shown.value?.invoke() }
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun click(text: String) {
        compose.onNode(hasText(text, substring = true), useUnmergedTree = true).performClick()
        compose.waitForIdle()
    }

    @Test fun newsSummary() {
        val list = AltimJson.decodeFromJsonElement(ListSerializer(StorySummary.serializer()), AltimJson.parseToJsonElement(fixture("news-summary-sample.json")).jsonObject.getValue("summary"))
        var opened: Asset? = null
        screen { NewsSummaryCard(list) { opened = it } }
        expect(
            "Aucun événement important aujourd'hui", "4 sujets repris par plusieurs sources, à impact faible.", "Impact potentiel indicatif, pas un signal.",
            "Impact potentiel faible", "impact mesuré", "impact estimé par règle", "Actifs concernés", "Convergent · 2 sources · ton des titres : 2 positifs",
            "depuis la publication", "(clôtures horaires, pas forcément dues à cette actualité)", "Sources et calcul", "Comment l'impact est estimé",
        )
        compose.onAllNodes(hasText("NVDA"), useUnmergedTree = true)[0].performClick()
        assertEquals(Asset("NVDA", Kind.STOCK, "NVDA"), opened)
        compose.onAllNodes(hasText("Sources et calcul", substring = true), useUnmergedTree = true)[0].performClick()
        click("Comment l'impact est estimé")
        expect("Règle : ", "par règle ; retenu : ", "Par règle : sources indépendantes", "Consensus : ton des titres de chaque source")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-news-summary.png")
    }

    @Test fun riskWeek() {
        val report = AltimJson.decodeFromString(CalendarReport.serializer(), fixture("calendar-sample.json"))
        val days = Calendar.riskDays(report.events, "2026-09-28", listOf("KDP"), listOf("MU"), 7, listOf("2026-10-03"))
        screen {
            RiskWeekCard(days, null)
            RiskWeekCard(null, "Calendrier indisponible")
        }
        expect(
            "Calendrier de risque · 7 jours", "Lun. 28 sept.", "Dividende KDP", "Résultats CCL · Prise de parole de la présidence de la BCE", "Résultats MU",
            "PIB · Inflation PCE", "week-end", "Sources incomplètes : risque non évalué", "Aucun événement majeur", "Règle", "Calendrier indisponible",
        )
        compose.onAllNodes(hasText("Règle", substring = true), useUnmergedTree = true)[0].performClick()
        compose.waitForIdle()
        expect("🔴 décision de taux d'une banque centrale")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-risk-week.png")
    }

    private val aapl = AltimJson.decodeFromString(StrategiesReport.serializer(), File("../kit/src/test/resources/fixtures/strategies-aapl.json").readText())
    private val btc = AltimJson.decodeFromString(StrategiesReport.serializer(), File("../kit/src/test/resources/fixtures/strategies-btc.json").readText())

    @Test fun strategiesChartListAndMetrics() {
        screen { StrategiesView("AAPL", Loadable.Loaded(aapl)) }
        expect(
            "Comparer les stratégies", "l'historique de AAPL", "Période testée", "juil. 2022 – sept. 2026", "valeur de 100 investis", "Tendance", "Cassure",
            "Moyenne", "DCA", "Conserver", "Suivi de tendance", "Voir en liste", "Paramètres fixes", "Rendement total", "Pire recul", "Sharpe / Sortino",
            "Gain sur les sommes versées", "Rendement annuel (TRI)", "Achats", "non pertinent", "tout investir au départ", "échantillon trop faible (6 trades)",
            "Comment ce test évite de se flatter", "aucune optimisation",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-strategies-chart.png")
        click("Voir en liste")
        expect("Voir le graphique", "15 juil. 2022")
        assertTrue(!has("valeur de 100 investis"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-strategies-list.png")
    }

    @Test fun strategiesAllCurvesOnACrypto() {
        screen { StrategiesView("BTC", Loadable.Loaded(btc), initialSelection = Strategies.ORDER) }
        expect("Plus de 4 courbes : touchez le graphique pour lire les valeurs, ou passez en liste.", "Non applicable", "pas de bénéfices")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-strategies-all.png")
        // Unselecting everything: nothing to draw.
        screen { StrategiesView("BTC", Loadable.Loaded(btc), initialSelection = listOf("value")) }
        expect("Choisissez au moins une stratégie disponible.")
        screen { StrategiesView("BTC", Loadable.Failed("historique journalier trop court")) }
        expect("historique journalier trop court")
    }

    private fun guidance(): Decision = AltimJson.decodeFromString(Decision.serializer(), fixture("decision-guidance.json"))

    @Test @Config(qualifiers = "w360dp-h12000dp-xhdpi")
    fun decisionGuidance() {
        val d = guidance()
        val before = d.copy(verdict = Verdict.BUY_ZONE, label = "ZONE D'ACHAT", level = DecisionLevel.MODERATE, levelLabel = "Signal modéré", snapshot = d.snapshot!!.copy(composite = 30.0, relativeVolume = 1.4))
        val at = { h: Int, m: Int -> LocalDate.of(2026, 9, 28).atTime(h, m).toInstant(ZoneOffset.UTC).toEpochMilli().toDouble() }
        val s1 = ConfigChanges.apply(ConfigState(), before, false, at(12, 2)).state
        val t = ConfigChanges.apply(s1, d, false, at(16, 0)).transition!!
        screen { DecisionView(d, expanded = true, change = t) }
        expect(
            "Pourquoi le signal a changé depuis le 28/09 à 14:02", "ZONE D'ACHAT → ATTENDRE", "Score composite +30 → +41", "Volume en baisse (1,4× → 0,7× la moyenne)",
            "Zones d'action", "Vous êtes ici : zone d'attente", "Zone de prise de bénéfices", "Zone d'attente", "◀ vous êtes ici · 341,04 $",
            "Niveau d'invalidation", "Zone de sortie", "Contre-argument", "🟢 Raisons favorables : ", "Points qui pourraient invalider le scénario",
            "Cassure de la résistance 344,95", "Quand ne pas trader", "rien à signaler", "Aucune raison mesurée de s'abstenir maintenant",
            "Non vérifié faute de données", "en cours : neutre", "Scénario neutre en cours : 1 condition sur 2 remplie.", "1/2 conditions · en cours",
            "Clôture au-dessus de 345,34", "✓ remplie", "✕ non remplie",
        )
        assertTrue(!has("Pas le moment de trader"))
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-decision.png")
    }

    @Test fun decisionNoTradeBannerAndOlderAnswer() {
        val d = guidance().copy(
            noTrade = NoTrade(
                true, "🕰️ Pas le moment de trader : marché sans direction, marché fermé.",
                listOf(NoTradeReason("trendless", "Marché sans direction", "ADX 14 (sous 20) et sommets et creux sans direction"), NoTradeReason("marketClosed", "Marché fermé", "Hors séance régulière de Wall Street")),
                emptyList(),
            ),
        )
        screen { DecisionView(d, expanded = false) }
        expect("🕰️ Pas le moment de trader : marché sans direction, marché fermé.", "Marché sans direction", "Marché fermé", "2 raisons")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-notrade.png")
        click("Quand ne pas trader")
        expect("Marché sans direction — ADX 14")
        // An older answer: none of the guidance blocks.
        val old = AltimJson.decodeFromString(Decision.serializer(), fixture("decision-btc.json"))
        screen { DecisionView(old, expanded = true) }
        assertTrue(!has("Pas le moment de trader"))
        assertTrue(!has("Zones d'action"))
        assertTrue(!has("Contre-argument"))
        assertTrue(!has("Quand ne pas trader"))
    }

    private val t0 = LocalDate.of(2026, 6, 1).atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli().toDouble()

    @Test @Config(qualifiers = "w360dp-h9000dp-xhdpi")
    fun journal() {
        val at = t0 + 10 * 3_600_000
        val d = guidance().copy(asOf = at - 3_600_000)
        val paper = TradeJournal.createEntry(NewTradeEntry("p", at, "paper", "buy", "AAPL", Kind.STOCK, "Apple", 300.0, quantity = 2.0, amount = 600.0, stop = 285.0, targets = listOf(330.0), note = "Rebond sur le support", refId = "pos1", decision = d))
        val sale = TradeJournal.createEntry(NewTradeEntry("s", at + 1000, "real", "sell", "AAPL", Kind.STOCK, "Apple", 300.0, quantity = 1.0, decision = d))
        val bare = TradeJournal.createEntry(NewTradeEntry("b", at + 2000, "real", "buy", "BTC", Kind.CRYPTO, "Bitcoin", 100_000.0, quantity = 0.01))
        val c = (0 until 40).map { i -> val p = 300.0 + i; Candle(t0 + i * TradeJournal.DAY, p - 1, p + 4, p - 3, p, 1e6) }
        val entries = TradeJournal.patchEntry(TradeJournal.addEntry(TradeJournal.addEntry(TradeJournal.addEntry(TradeJournal.empty(), paper), sale), bare), "p", market = com.maxlestage.altim.kit.JournalMarket(macroScore = 31.0, macroLevel = "tense", atrPct = 1.84)).entries
        screen {
            JournalView(
                entries, null, mapOf("stock:AAPL" to c, "crypto:BTC" to null), mapOf("pos1" to ClosedInfo(t0 + 12 * TradeJournal.DAY, 312.0, "vente manuelle")),
                t0 + 45 * TradeJournal.DAY, Modifier.fillMaxSize(),
            )
        }
        expect(
            "Enregistré uniquement sur ce téléphone · 3 entrées", "Des faits, pas des jugements", "Votre profil", "À 3 jours", "À 10 jours", "À 30 jours",
            "Résultats en R", "Ensemble", "Par note à l'entrée", "Plan respecté ou non", "Par régime de marché", "échantillon trop faible (< 5)", "Résultat moyen",
            "Achat · Apple", "Vente · Apple", "simulé", "réel", "Stop", "Objectifs", "Gain visé", "Signal utilisé : ",
            "Pourquoi je suis entré : Rebond sur le support", "Pourquoi j'ai vendu : ", "pas de note", "Revue automatique", "objectif 1 atteint",
            "Clôturée le", "(vente manuelle)", "Qu'est-ce qui a fonctionné ?", "Le signal était-il cohérent avec les données disponibles à ce moment-là ?",
            "Achat conforme à la décision", "Cours journaliers indisponibles pour l'instant : revue non calculée.", "Aucune décision chargée pour cet actif à ce moment-là",
            "Supprimer",
        )
        // The simulated purchase is the oldest entry: the last card.
        compose.onAllNodes(hasText("Pourquoi, et le contexte de marché", substring = true), useUnmergedTree = true)[2].performClick()
        compose.waitForIdle()
        expect("Stress macro : 31/100 (tendu)", "ATR : 1,84 %/jour", "Configuration « ", "Événements à 7 j : ")
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-journal.png")
        screen { JournalView(emptyList(), "Le journal enregistré sur ce téléphone est illisible : il a été ignoré.", emptyMap(), emptyMap(), t0) }
        expect("Aucune entrée pour l'instant", "illisible")
    }

    /** The journal is written by a simulated purchase and kept after a restart; an unreadable one is ignored with a message. */
    @Test fun journalStorage() {
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        app.getSharedPreferences("altim", Context.MODE_PRIVATE).edit().clear().putString(TradeJournal.KEY, "{oops").commit()
        val m = AppModel(app, GuidanceMemory())
        assertTrue(m.tradeJournalError!!.contains("illisible"))
        val d = guidance()
        assertEquals(null, m.paperBuy(OpenOrder("x", "AAPL", Kind.STOCK, "Apple", 300.0, 1_000.0, 280.0, 330.0), d, "rebond"))
        val e = m.tradeJournal.entries.single()
        assertEquals("paper", e.source)
        assertEquals("x", e.refId)
        assertEquals("rebond", e.note)
        assertEquals(listOf(330.0), e.targets)
        assertTrue(e.signal.startsWith("ATTENDRE"))
        assertEquals(null, m.tradeJournalError)
        // The unreadable text is kept aside.
        assertEquals("{oops", app.getSharedPreferences("altim", Context.MODE_PRIVATE).getString("${TradeJournal.KEY}.invalid", null))
        m.recordRealTrade(Asset("BTC", Kind.CRYPTO, "Bitcoin"), "sell", 100_000.0, 0.5)
        val again = AppModel(app, GuidanceMemory())
        assertEquals(listOf("paper", "real"), again.tradeJournal.entries.map { it.source })
        again.deleteJournalEntry("x".let { again.tradeJournal.entries[0].id })
        assertEquals(1, AppModel(app, GuidanceMemory()).tradeJournal.entries.size)
    }

    @Test fun whatIf() {
        val days = (0 until 300).map { t0 + it * TradeJournal.DAY }
        var q = 100.0
        var n = 100.0
        var x = 11L
        val qqq = mutableListOf<Candle>()
        val nvda = mutableListOf<Candle>()
        days.forEach { t ->
            x = (x * 16807) % 2147483647
            val r = (x.toDouble() / 2147483647 - 0.5) * 0.04
            q *= 1 + r
            n *= 1 + 1.8 * r
            qqq += Candle(t, q, q, q, q, 1.0)
            nvda += Candle(t, n, n, n, n, 1.0)
        }
        val holdings = listOf(
            Holding("1", Asset("NVDA", Kind.STOCK, "Nvidia"), 10.0, 100.0),
            Holding("2", Asset("QQQ", Kind.STOCK, "Invesco QQQ"), 5.0, 100.0),
            Holding("3", Asset("DOGE", Kind.CRYPTO, "Dogecoin"), 10_000.0, 0.1),
        )
        val p = RiskPortfolio.of(holdings, 0.0, mapOf("stock:NVDA" to n, "stock:QQQ" to q, "crypto:DOGE" to 0.12), emptyMap())
        screen { WhatIfCard(p, mapOf("stock:NVDA" to nvda, "stock:QQQ" to qqq), null) }
        expect(
            "Et si… ?", "si le Nasdaq-100 baisse de 10 %", "Nasdaq-100 (QQQ)", "S&P 500 (SPY)", "Bitcoin", "−5 %", "−50 %", "Autre baisse (%)", "Montant simulé ($, facultatif)",
            "Perte estimée", "de votre patrimoine", "Ligne la plus touchée : NVDA", "bêta 1.80, corrélation 1.00", "c'est ce marché lui-même (bêta 1)",
            "non couvert", "moins de 30 jours communs avec Nasdaq-100 (QQQ) : bêta non mesurable", "Non couvert : DOGE", "laissé hors du total, jamais estimé.",
            "Bêta estimé sur 250 jours de rendements journaliers communs", "Ce n'est pas une prévision.",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-whatif.png")
        click("−30 %")
        expect("si le Nasdaq-100 baisse de 30 %")
        // Bitcoin: its candles are not loaded and there is no server here.
        click("Bitcoin")
        expect("Cours journaliers de Bitcoin indisponibles : simulation non couverte pour l'instant.")
        fitsWidth()
    }

    @Test fun why() {
        val r = AltimJson.decodeFromString(WhyReport.serializer(), fixture("why-sample.json"))
        val answer = AskAnswer("La baisse suit en partie le marché (S&P 500 −1,2 %) ; le reste n'est pas expliqué par ces données.", "m", "q", AltimJson.parseToJsonElement("""{"derniereDecision":null}"""), "d")
        screen { WhyView(Loadable.Loaded(r), question = "La baisse vient-elle du marché ?", answer = answer) }
        expect(
            "Pourquoi ça bouge ?", "NVDA −3,4 % aujourd'hui", "Variation du cours", "observé", "corrélation possible", "non vérifiable",
            "Sens baissier, ampleur fort · source : ", "Non couvert : financement des contrats perpétuels", "pas des causes prouvées",
            "Poser une question sur ces données", "Demander", "Réponse d'un modèle d'IA (Claude, Anthropic)", "La baisse suit en partie le marché",
            "Données utilisées : les observations ci-dessus.",
        )
        fitsWidth()
        compose.onRoot().captureRoboImage("build/screens/guidance-why.png")
        // Without a key on the server: no question box.
        screen { WhyView(Loadable.Loaded(r.copy(askEnabled = false))) }
        assertTrue(!has("Poser une question"))
    }
}
