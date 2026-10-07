package com.maxlestage.altim.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.maxlestage.altim.BuildConfig
import com.maxlestage.altim.kit.AccessMode
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.AlertJournal
import com.maxlestage.altim.kit.AlertTracker
import com.maxlestage.altim.kit.JournalEntry
import com.maxlestage.altim.kit.PriceTarget
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.ChangeNoticeState
import com.maxlestage.altim.kit.ChangeNotices
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.DecisionDigest
import com.maxlestage.altim.kit.DecisionDigests
import com.maxlestage.altim.kit.VerdictPush
import com.maxlestage.altim.kit.LiveTrack
import com.maxlestage.altim.kit.LiveTrackState
import com.maxlestage.altim.kit.LocalNotice
import com.maxlestage.altim.kit.RiskPortfolio
import com.maxlestage.altim.kit.ConfigTransition
import com.maxlestage.altim.kit.Credentials
import com.maxlestage.altim.kit.Danger
import com.maxlestage.altim.kit.Candle
import com.maxlestage.altim.kit.FxRate
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.RiskSettings
import com.maxlestage.altim.kit.ScoreWeights
import com.maxlestage.altim.kit.FileResponseCache
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.Holdings
import com.maxlestage.altim.kit.UsdHoldings
import com.maxlestage.altim.kit.Currency
import com.maxlestage.altim.kit.Fx
import com.maxlestage.altim.kit.Money
import com.maxlestage.altim.kit.Horizon
import com.maxlestage.altim.kit.Kind
import com.maxlestage.altim.kit.NewsAlertTracker
import com.maxlestage.altim.kit.NewsItem
import com.maxlestage.altim.kit.OpenOrder
import com.maxlestage.altim.kit.Paper
import com.maxlestage.altim.kit.PaperResult
import com.maxlestage.altim.kit.PaperState
import com.maxlestage.altim.kit.PaperTrade
import com.maxlestage.altim.kit.checkExits
import com.maxlestage.altim.kit.closePosition
import com.maxlestage.altim.kit.decodePaper
import com.maxlestage.altim.kit.encodePaper
import com.maxlestage.altim.kit.newPaper
import com.maxlestage.altim.kit.openPosition
import com.maxlestage.altim.kit.JournalMarket
import com.maxlestage.altim.kit.NewTradeEntry
import com.maxlestage.altim.kit.TradeEntry
import com.maxlestage.altim.kit.TradeJournal
import com.maxlestage.altim.kit.TradeJournalState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import java.util.UUID
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.HttpUrl

/** App state: server access, lock, watch list and holdings (stored on the phone only). Same as the iPhone AppModel. */
class AppModel(context: Context, private val secure: SecretStore = SecureStore(context)) {
    companion object {
        /** Watched assets whose decision the background check re-reads (as many as the Radar). */
        const val MAX_BACKGROUND_DECISIONS = 20
        /** A decision compared less than 15 minutes ago (Radar open, asset page, previous check) is not fetched again. */
        const val DECISION_EVERY = 15 * 60_000.0
        /** The background check re-reads decisions for about 20 seconds at most; the rest waits for the next check. */
        const val CONFIG_CHECK_BUDGET_MS = 20_000L
        /** An answer older than this (served from the offline cache) is not compared: it would tell a false change. */
        const val CONFIG_MAX_ANSWER_AGE = 3_600_000.0

        /** A new simulation starts with 10 000 in the display currency, kept in dollars like the prices. */
        fun startCapital(currency: Currency, fx: FxRate?): Double =
            if (currency == Currency.EUR && fx != null && fx.rate > 0) Paper.DEFAULT_CAPITAL / fx.rate else Paper.DEFAULT_CAPITAL
    }

    private val appContext = context.applicationContext
    enum class Phase { SETUP, LOCKED, READY }

    private val prefs = context.getSharedPreferences("altim", Context.MODE_PRIVATE)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val live = LivePrices(scope)

    var phase by mutableStateOf(Phase.SETUP)
        private set
    var client by mutableStateOf<AltimClient?>(null)
        private set
    var serverUrl by mutableStateOf<HttpUrl?>(null)
        private set
    var user by mutableStateOf<String?>(null)
        private set

    var acceptedDisclaimer by mutableStateOf(prefs.getBoolean("acceptedDisclaimer", false))
        private set
    var biometricLock by mutableStateOf(prefs.getBoolean("biometricLock", true))
        private set
    var watchlist by mutableStateOf(load(ListSerializer(Asset.serializer()), "watchlist") ?: Asset.defaults)
        private set
    /** Holdings exactly as saved (cost and stop in the currency they were typed in): the editing forms. */
    var holdings by mutableStateOf(load(ListSerializer(Holding.serializer()), "holdings")?.map { it.cleaned() } ?: emptyList())
        private set

    /** Display currency of every amount (Réglages → Devise d'affichage): euros by default, dollars on request. */
    var currency by mutableStateOf(Currency.of(prefs.getString("currency", "EUR")))
        private set
    /** Last EUR/USD rate (/api/fx, kept 7 days on the phone), null when none is known: amounts then stay in dollars. */
    var fx by mutableStateOf(Fx.saved(prefs.getString(Fx.KEY, null)))
        private set
    /** Why the last rate read failed (shown in Réglages), null when it worked. */
    var fxError by mutableStateOf<String?>(null)
        private set

    /** Currency actually shown: euros only when chosen and a rate is known (read by the screens to redraw on a change). */
    val displayCurrency: Currency get() = if (currency == Currency.EUR && fx != null) Currency.EUR else Currency.USD

    /** Holdings in dollars at the current rate (engines, server, decision), recomputed when the holdings or the rate change. */
    val usdHoldings: UsdHoldings
        get() {
            // Read so that a composable using it redraws when the currency or the rate changes.
            currency
            fx
            return Holdings.toUsd(holdings)
        }
    /** Limits of Réglages → Prudence des conseils, checked in Mes avoirs (same defaults as the web app). */
    var risk by mutableStateOf(load(RiskSettings.serializer(), "risk")?.sanitized() ?: RiskSettings.DEFAULT)
        private set
    /** Weights of the decision's composite score (Réglages → Score composite), sent only when not the defaults. */
    var scoreWeights by mutableStateOf(ScoreWeights.parse(prefs.getString("scoreWeights", null)))
        private set
    /** Configuration changes of the decisions seen on this phone (validated when read back; damaged = empty). */
    var configChanges by mutableStateOf(ConfigChanges.parse(prefs.getString(ConfigChanges.KEY, null)))
        private set
    /** The last decision seen for each asset (its page, the Radar's re-reading, the background check): the Radar's verdict. */
    var decisionDigests by mutableStateOf(DecisionDigests.parse(prefs.getString(DecisionDigests.KEY, null)))
        private set
    /** What the background notifications already told: no second notification of the same change or danger. */
    var changeNotices by mutableStateOf(ChangeNotices.parse(prefs.getString(ChangeNotices.KEY, null)))
        private set
    /** Last "positions devenues dangereuses" measured on Mes avoirs, shown on the Radar with their time. */
    var dangers by mutableStateOf(PortfolioRisk.parseDangers(prefs.getString("dangers.v1", null)))
        private set
    /**
     * Budget of the Sélection tab and the currency it was typed in (null = not set). Older versions saved a bare number
     * of dollars ("budget").
     */
    var budget by mutableStateOf(
        Money.parseBudget(prefs.getString("budget.v2", null))
            ?: prefs.getFloat("budget", 0f).toDouble().takeIf { it > 0 }?.let { Money.Typed(it, Currency.USD) },
    )
        private set
    var selectionMarket by mutableStateOf(Kind.of(prefs.getString("selectionMarket", null)) ?: Kind.STOCK)
        private set
    var selectionHorizon by mutableStateOf(Horizon.of(prefs.getString("selectionHorizon", null)) ?: Horizon.MO1)
        private set
    /** Timeframe of the technical line (Radar) and of the asset chart, saved; 4 h by default. */
    var timeframe by mutableStateOf(com.maxlestage.altim.kit.Timeframe.saved(prefs.getString(com.maxlestage.altim.kit.Timeframe.KEY, null)))
        private set

    /** Buy notifications (Réglages): on by default once allowed; "strong only" = signal and zone together. */
    var alertsEnabled by mutableStateOf(prefs.getBoolean("alertsEnabled", false))
        private set
    var alertsStrongOnly by mutableStateOf(prefs.getBoolean("alertsStrongOnly", false))
        private set
    /** News alerts (Réglages): serious escalation, or a story about one of my assets told by at least 3 sources. */
    var newsAlertsEnabled by mutableStateOf(prefs.getBoolean("newsAlertsEnabled", false))
        private set
    /**
     * Configuration changes of the Radar and positions that became dangerous, notified in the background (Réglages).
     * Never set yet: on only if the buy notifications already are (written once, so it no longer follows them).
     */
    var configAlertsEnabled by mutableStateOf(initialSwitch("configAlertsEnabled"))
        private set
    var dangerAlertsEnabled by mutableStateOf(initialSwitch("dangerAlertsEnabled"))
        private set
    /** Time of the last background check and what it found (shown in Réglages). */
    var lastAlertCheck by mutableStateOf(prefs.getLong("lastAlertCheck", 0L).takeIf { it > 0 })
        private set
    /** Last alerts computed by a check (buyable or not): Réglages, the home screen widget and the live following. */
    var lastAlerts by mutableStateOf(load(ListSerializer(BuyAlert.serializer()), "lastAlerts") ?: emptyList())
        private set
    /** Price alerts chosen by the user ("sous 80 000 $") and the journal of the notifications received. */
    var priceTargets by mutableStateOf(load(ListSerializer(PriceTarget.serializer()), "priceTargets") ?: emptyList())
        private set
    var journal by mutableStateOf(load(ListSerializer(JournalEntry.serializer()), "journal") ?: emptyList())
        private set

    /**
     * Simulation (paper trading): virtual portfolio, no real money, no order placed. Stored on this phone like the
     * holdings; a damaged saved state is ignored (a fresh 10 000 $ simulation starts instead).
     */
    var paper by mutableStateOf(
        decodePaper(prefs.getString("paper", null))
            ?: newPaper(startCapital(Currency.of(prefs.getString("currency", "EUR")), Fx.saved(prefs.getString(Fx.KEY, null))), System.currentTimeMillis().toDouble()),
    )
        private set
    /**
     * Automatic trading journal (simulated purchases, real purchases and sales of Mes avoirs), kept on this phone only.
     * Unreadable saved data is ignored (kept aside as "tradeJournal.v1.invalid" at the next write), with a message.
     */
    private val savedJournal = TradeJournal.parseSaved(prefs.getString(TradeJournal.KEY, null))
    var tradeJournal by mutableStateOf(savedJournal.state)
        private set
    var tradeJournalError by mutableStateOf(savedJournal.error)
        private set
    /** Last decisions received in this session (memory only): the one "of the moment" for a journal entry. */
    private val recentDecisions = mutableMapOf<String, Pair<Double, Decision>>()

    /** Positions just closed by their stop or target (shown once in the Simulation part). */
    var paperJustClosed by mutableStateOf<List<PaperTrade>>(emptyList())
    /** Incremented each time the app comes back to the foreground (the Simulation part checks its exits again). */
    var resumeCount by mutableIntStateOf(0)
        private set

    /** Asset to open, from a tapped notification. */
    var pendingOpen by mutableStateOf<Asset?>(null)
    /** A news notification was tapped: the Actu tab opens. */
    var pendingNews by mutableStateOf(false)

    /**
     * Live following of one asset outside the app (iOS Live Activity): an ongoing notification with its live price and
     * Altim's buy verdict. Offered on the asset pages when on (Réglages, on by default).
     */
    var liveActivityEnabled by mutableStateOf(prefs.getBoolean("liveActivityEnabled", true))
        private set
    /** State of the running live following (null: none), kept across launches like a Live Activity. */
    var liveTrack by mutableStateOf(LiveTrack.parse(prefs.getString(LiveTrack.KEY, null)))
        private set
    /** Asset of the running live following (its live price is followed to update it). */
    val activityAsset: Asset? get() = liveTrack?.asset
    private var lastTrackPush = 0L

    /** Offline mode: date of the data shown when the server cannot be reached (null = online). */
    var offlineSince by mutableStateOf<Long?>(null)
        private set
    private val responseCache = FileResponseCache(java.io.File(context.cacheDir, "api"))

    /** Client with the offline cache: the last good answers are shown, dated, when the network or the server fails. */
    private fun makeClient(url: HttpUrl, creds: Credentials?, cookie: String?) =
        AltimClient(url, creds, cookie, cache = responseCache, onStatus = { offlineSince = it })

    /** Asset open on screen (its live price is followed too). */
    var focus by mutableStateOf<Asset?>(null)
    /** Picks shown in the Sélection tab (live prices too). */
    var selectionAssets by mutableStateOf<List<Asset>>(emptyList())

    private var lastBackground: Long? = null

    /** The app is in the foreground (ProcessLifecycleOwner): the live socket is open only then. */
    var foreground by mutableStateOf(false)
        private set

    init {
        Money.set(currency, fx)
        restore()
        // What the live socket pushes while the app is open.
        live.onAlerts = { items, at -> applyPushedAlerts(items, at) }
        live.onVerdict = { applyPushedVerdict(it) }
        live.onFx = { rate ->
            if (fx?.rate != rate) scope.launch { runCatching { refreshFx() } }
        }
        // Process-wide: a rotation or a system dialog does not close the socket, going to the background does.
        runCatching {
            androidx.lifecycle.ProcessLifecycleOwner.get().lifecycle.addObserver(object : androidx.lifecycle.DefaultLifecycleObserver {
                override fun onStart(owner: androidx.lifecycle.LifecycleOwner) {
                    foreground = true
                }

                override fun onStop(owner: androidx.lifecycle.LifecycleOwner) {
                    foreground = false
                }
            })
        }
    }

    /**
     * Alerts pushed by the live socket while the app is open. The same tracker as the background check decides what to
     * notify, so a situation is notified once whichever found it first; merged into the last alerts (a socket covers
     * the followed assets only), then the widget and the live following are refreshed at once.
     */
    fun applyPushedAlerts(items: List<BuyAlert>, checkedAt: Long) {
        if (items.isEmpty()) return
        val tracker = prefs.getString("alertTracker", null)?.let { runCatching { AltimJson.decodeFromString(AlertTracker.serializer(), it) }.getOrNull() } ?: AlertTracker()
        val (next, fresh) = tracker.newAlerts(items, alertsStrongOnly)
        val byId = lastAlerts.associateBy { it.id }.toMutableMap()
        items.forEach { byId[it.id] = it }
        lastAlerts = byId.values.sortedWith(compareBy<BuyAlert> { if (it.buy) 0 else 1 }.thenBy { it.symbol })
        lastAlertCheck = checkedAt
        prefs.edit().putString("alertTracker", AltimJson.encodeToString(AlertTracker.serializer(), next))
            .putString("lastAlerts", AltimJson.encodeToString(ListSerializer(BuyAlert.serializer()), lastAlerts))
            .putLong("lastAlertCheck", checkedAt)
            .apply()
        BuyWidget.refresh(appContext)
        refreshLiveTrack()
        if (!alertsEnabled || fresh.isEmpty()) return
        val now = System.currentTimeMillis()
        val entries = fresh.mapNotNull { a ->
            a.price?.let { JournalEntry(asset = a.asset, source = if (a.strong) JournalEntry.Source.STRONG_BUY else JournalEntry.Source.BUY, title = a.title, price = it, date = now) }
        }
        if (entries.isNotEmpty()) {
            journal = AlertJournal.add(entries, journal)
            save(ListSerializer(JournalEntry.serializer()), "journal", journal)
        }
        BuyAlerts.postAll(appContext, fresh)
    }

    /** Verdicts already re-read after a push (one reading per pushed change). */
    private val pushedVerdicts = mutableSetOf<String>()

    /**
     * A Radar verdict pushed by the live socket that differs from the last decision seen: that decision is re-read at
     * once (configuration diff, chip, note) instead of at the next 15-minute round. An asset never read yet, or seen
     * last in personal mode, is left to the Radar's own reading.
     */
    fun applyPushedVerdict(v: VerdictPush) {
        val c = client ?: return
        val old = decisionDigests[DecisionDigests.key(v.kind, v.symbol)] ?: return
        if (old.personal) return
        val pushed = runCatching { AltimJson.decodeFromString(com.maxlestage.altim.kit.Verdict.serializer(), "\"${v.verdict}\"") }.getOrNull()
        if (pushed == old.verdict && (old.note ?: "") == (v.chipNote ?: "")) return
        if (!pushedVerdicts.add("${v.key}|${v.verdict}|${v.chipNote ?: ""}")) return
        val asset = watchlist.firstOrNull { it.id == v.key } ?: Asset(v.symbol, v.kind, v.symbol)
        scope.launch {
            val d = try {
                c.decision(asset)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
            d?.let { recordDecision(it, personal = false) }
        }
    }

    // ---------- Settings ----------

    fun acceptDisclaimer() {
        acceptedDisclaimer = true
        prefs.edit().putBoolean("acceptedDisclaimer", true).apply()
    }

    fun updateBiometricLock(on: Boolean) {
        biometricLock = on
        prefs.edit().putBoolean("biometricLock", on).apply()
    }

    /** The budget as typed, with its currency (a rate arriving later never re-reads it in another one). */
    fun updateBudget(v: Money.Typed?) {
        budget = v?.takeIf { it.amount > 0 && it.amount.isFinite() }
        val edit = prefs.edit().remove("budget")
        budget?.let { edit.putString("budget.v2", Money.encodeBudget(it)) } ?: edit.remove("budget.v2")
        edit.apply()
    }

    /** Réglages → Devise d'affichage. */
    fun updateCurrency(c: Currency) {
        currency = c
        prefs.edit().putString("currency", c.name).apply()
        applyMoney()
        if (c == Currency.EUR && fx == null) scope.launch { runCatching { refreshFx() } }
    }

    /** Every formatter (kit Money / Format) reads the currency and rate set here. */
    private fun applyMoney() {
        Money.set(currency, fx)
        BuyWidget.refresh(appContext)
        liveTrack?.let { LiveTracking.show(appContext, it) }
    }

    /**
     * Reads the EUR/USD rate (/api/fx; every 10 minutes while the app is open, and at each background check). The last
     * valid rate is kept 7 days; older, or never read, the amounts stay in dollars with « Taux EUR/USD indisponible ».
     */
    suspend fun refreshFx(now: Long = System.currentTimeMillis()) {
        val c = client ?: return
        try {
            val body = c.fx()
            val r = Fx.parse(body, now)?.takeIf { now - it.fetchedAt < Fx.KEEP_MS }
            if (r != null) {
                prefs.edit().putString(Fx.KEY, Fx.encode(body)).apply()
                fx = if (offlineSince != null) r.copy(stale = true) else r
                fxError = null
            } else {
                fx = Fx.saved(prefs.getString(Fx.KEY, null), now)
                fxError = body.error ?: "taux indisponible"
            }
        } catch (e: AltimException.Unauthorized) {
            throw e
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            fx = Fx.saved(prefs.getString(Fx.KEY, null), now)
            fxError = "taux indisponible (${e.message})"
        }
        applyMoney()
    }

    /** Background checks: the rate read again when older than 10 minutes. */
    suspend fun refreshFxIfOld(now: Long = System.currentTimeMillis()) {
        val f = fx
        if (f != null && !f.stale && now - f.fetchedAt < Fx.REFRESH_MS) return
        try {
            refreshFx(now)
        } catch (_: AltimException.Unauthorized) {
            fxError = "taux indisponible (session expirée)"
        }
    }

    fun updateScoreWeights(w: ScoreWeights) {
        scoreWeights = w
        save(ScoreWeights.serializer(), "scoreWeights", w)
    }

    fun updateRisk(r: RiskSettings) {
        risk = r.sanitized()
        save(RiskSettings.serializer(), "risk", risk)
    }

    /**
     * Every decision received goes through the configuration diff (verdict or level changed since the last one seen
     * for this asset and mode). A cached answer served offline is not a new sighting.
     */
    fun recordDecision(d: Decision, personal: Boolean = d.isPersonal, now: Double = System.currentTimeMillis().toDouble()): ConfigTransition? {
        d.kind?.let { k -> recentDecisions["${k.raw}:${d.symbol}"] = now to d }
        val digests = DecisionDigests.record(decisionDigests, d, personal, now)
        if (digests != decisionDigests) {
            decisionDigests = digests
            prefs.edit().putString(DecisionDigests.KEY, DecisionDigests.encode(digests)).apply()
        }
        // An answer more than an hour old (served from the offline cache) is not compared: it would tell a false change.
        if (now - d.asOf >= CONFIG_MAX_ANSWER_AGE) return null
        val u = ConfigChanges.apply(configChanges, d, personal, now)
        configChanges = u.state
        prefs.edit().putString(ConfigChanges.KEY, ConfigChanges.encode(u.state)).apply()
        return u.transition
    }

    /** The full decision last seen for this asset (Radar badge, less than 12 h old); null: "Décision…". */
    fun radarDecision(asset: Asset, now: Double = System.currentTimeMillis().toDouble()): DecisionDigest? =
        DecisionDigests.fresh(decisionDigests, asset, now)

    /** When the informational decision of this asset was last compared (null: never). */
    fun lastDecisionCheck(asset: Asset): Double? = ConfigChanges.lastSeen(configChanges, asset)

    private fun storeChangeNotices(s: ChangeNoticeState) {
        changeNotices = s
        prefs.edit().putString(ChangeNotices.KEY, ChangeNotices.encode(s)).apply()
    }

    fun clearTransitions() {
        configChanges = configChanges.copy(transitions = emptyList())
        prefs.edit().putString(ConfigChanges.KEY, ConfigChanges.encode(configChanges)).apply()
    }

    /** Kept for the Radar with the time they were measured (Mes avoirs, once its market data is loaded). */
    fun saveDangers(items: List<Danger>, now: Double = System.currentTimeMillis().toDouble()) {
        val state = PortfolioRisk.DangerState(1, now, items)
        dangers = state
        prefs.edit().putString("dangers.v1", PortfolioRisk.encodeDangers(items, now)).apply()
    }

    fun updateTimeframe(t: com.maxlestage.altim.kit.Timeframe) {
        timeframe = t
        prefs.edit().putString(com.maxlestage.altim.kit.Timeframe.KEY, t.raw).apply()
    }

    fun updateSelection(market: Kind = selectionMarket, horizon: Horizon = selectionHorizon) {
        selectionMarket = market
        selectionHorizon = horizon
        prefs.edit().putString("selectionMarket", market.raw).putString("selectionHorizon", horizon.raw).apply()
    }

    fun updateAlerts(context: Context, enabled: Boolean = alertsEnabled, strongOnly: Boolean = alertsStrongOnly) {
        alertsEnabled = enabled
        alertsStrongOnly = strongOnly
        prefs.edit().putBoolean("alertsEnabled", enabled).putBoolean("alertsStrongOnly", strongOnly).apply()
        BuyWidget.refresh(context)
        BuyAlerts.schedule(context, needsChecks)
    }

    private fun initialSwitch(key: String): Boolean {
        if (!prefs.contains(key)) prefs.edit().putBoolean(key, prefs.getBoolean("alertsEnabled", false)).apply()
        return prefs.getBoolean(key, false)
    }

    fun updateConfigAlerts(context: Context, enabled: Boolean) {
        configAlertsEnabled = enabled
        prefs.edit().putBoolean("configAlertsEnabled", enabled).apply()
        BuyAlerts.schedule(context, needsChecks)
    }

    /** Turned on: measured again from the last state shown on Mes avoirs (the dangers of before are not kept). */
    fun updateDangerAlerts(context: Context, enabled: Boolean) {
        if (enabled && !dangerAlertsEnabled) storeChangeNotices(changeNotices.copy(dangerActive = null))
        dangerAlertsEnabled = enabled
        prefs.edit().putBoolean("dangerAlertsEnabled", enabled).apply()
        BuyAlerts.schedule(context, needsChecks)
    }

    /** Réglages → Live Activity: the button "Suivre" of the asset pages; turned off, the running following stays. */
    fun updateLiveActivityEnabled(on: Boolean) {
        liveActivityEnabled = on
        prefs.edit().putBoolean("liveActivityEnabled", on).apply()
    }

    // ---------- Live following (iOS Live Activity) ----------

    /** One following at a time: following an asset replaces the previous one. */
    fun startLiveTrack(asset: Asset, price: Double, change: Double?) {
        val alert = lastAlerts.firstOrNull { it.id == asset.id }
        val now = System.currentTimeMillis()
        lastTrackPush = now
        storeLiveTrack(LiveTrack.state(asset, price, change, alert, null, now))
    }

    fun stopLiveTrack() {
        liveTrack = null
        prefs.edit().remove(LiveTrack.KEY).apply()
        LiveTracking.cancel(appContext)
    }

    /** Live price tick of the followed asset (at most every 5 s). */
    fun liveTrackTick(tick: com.maxlestage.altim.kit.LiveTick) {
        val s = liveTrack ?: return
        val now = System.currentTimeMillis()
        val next = LiveTrack.tick(s, tick, lastTrackPush, now) ?: return
        lastTrackPush = now
        storeLiveTrack(next)
    }

    /** Result of an alert check: verdict and price of the followed asset. */
    fun refreshLiveTrack() {
        val s = liveTrack ?: return
        LiveTrack.refresh(s, lastAlerts, System.currentTimeMillis())?.let {
            lastTrackPush = it.updated
            storeLiveTrack(it)
        }
    }

    private fun storeLiveTrack(s: LiveTrackState) {
        liveTrack = s
        prefs.edit().putString(LiveTrack.KEY, LiveTrack.encode(s)).apply()
        LiveTracking.show(appContext, s)
    }

    fun updateNewsAlerts(context: Context, enabled: Boolean) {
        newsAlertsEnabled = enabled
        prefs.edit().putBoolean("newsAlertsEnabled", enabled).apply()
        BuyAlerts.schedule(context, needsChecks)
    }

    /**
     * The background check runs for the buy or news notifications, an armed price alert, or the configuration / danger
     * notifications when there is something to watch.
     */
    val needsChecks: Boolean get() = alertsEnabled || newsAlertsEnabled || priceTargets.any { it.triggered == null } ||
        (configAlertsEnabled && watchlist.isNotEmpty()) || (dangerAlertsEnabled && holdings.isNotEmpty())

    fun addTarget(context: Context, t: PriceTarget) = setTargets(context, priceTargets + t)
    fun removeTarget(context: Context, id: String) = setTargets(context, priceTargets.filterNot { it.id == id })
    /** Re-arms an alert (a move alert starts again from the current price). */
    fun rearmTarget(context: Context, id: String, current: Double? = null) = setTargets(context, priceTargets.map { if (it.id == id) it.rearmed(current) else it })

    private fun setTargets(context: Context?, list: List<PriceTarget>) {
        priceTargets = list
        save(ListSerializer(PriceTarget.serializer()), "priceTargets", list)
        context?.let { BuyAlerts.schedule(it, needsChecks) }
    }

    fun clearJournal() {
        journal = emptyList()
        save(ListSerializer(JournalEntry.serializer()), "journal", journal)
    }

    /** What one check found: new buy alerts, price alerts just reached (with the price), and news to tell. */
    data class CheckResult(
        val buy: List<BuyAlert>,
        val targets: List<Pair<PriceTarget, Double>>,
        val news: List<NewsItem> = emptyList(),
    )

    /**
     * One check of the alerts (background worker, app opening): price alerts first, then the server's buy rule applied
     * to the watch list, the holdings and the followed asset (kept for Réglages, the widget and the live following
     * whether the buy notifications are on or not), then the news. Returns what to notify, null without access; throws
     * when the server cannot be reached.
     */
    suspend fun checkAlerts(): CheckResult? {
        val c = client ?: return null
        val now = System.currentTimeMillis()
        // Price alerts: consensus quotes of the assets that still have an armed threshold.
        val armed = priceTargets.filter { it.triggered == null }
        var fired = emptyList<Pair<PriceTarget, Double>>()
        if (armed.isNotEmpty()) {
            val quotes = c.quotes(armed.map { it.asset }.distinctBy { it.id })
            val (updated, reached) = PriceTarget.evaluate(priceTargets, quotes.associate { "${it.kind.raw}:${it.symbol}" to it.price }, now)
            fired = reached
            if (reached.isNotEmpty()) setTargets(null, updated)
        }
        val fresh = checkBuyAlerts(c)
        val entries = (if (alertsEnabled) fresh else emptyList()).mapNotNull { a ->
            a.price?.let { JournalEntry(asset = a.asset, source = if (a.strong) JournalEntry.Source.STRONG_BUY else JournalEntry.Source.BUY, title = a.title, price = it, date = now) }
        } + fired.map { (t, p) -> JournalEntry(asset = t.asset, source = JournalEntry.Source.TARGET, title = "${t.asset.symbol} : ${t.label.lowercase()}", price = p, date = now) }
        if (entries.isNotEmpty()) {
            journal = AlertJournal.add(entries, journal)
            save(ListSerializer(JournalEntry.serializer()), "journal", journal)
        }
        val news = if (newsAlertsEnabled) checkNews(c, now) else emptyList()
        return CheckResult(fresh, fired, news)
    }

    private suspend fun checkBuyAlerts(c: AltimClient): List<BuyAlert> {
        // The asset followed live is checked too, even if it is neither on the radar nor held.
        val assets = (watchlist + holdings.map { it.asset } + listOfNotNull(activityAsset)).distinctBy { it.id }
        val items = c.alerts(assets)
        val tracker = prefs.getString("alertTracker", null)?.let { runCatching { AltimJson.decodeFromString(AlertTracker.serializer(), it) }.getOrNull() } ?: AlertTracker()
        val (next, fresh) = tracker.newAlerts(items, alertsStrongOnly)
        lastAlerts = items.sortedWith(compareBy<BuyAlert> { if (it.buy) 0 else 1 }.thenBy { it.symbol })
        val now = System.currentTimeMillis()
        lastAlertCheck = now
        prefs.edit().putString("alertTracker", AltimJson.encodeToString(AlertTracker.serializer(), next))
            .putString("lastAlerts", AltimJson.encodeToString(ListSerializer(BuyAlert.serializer()), lastAlerts))
            .putLong("lastAlertCheck", now)
            .apply()
        persistSession()
        BuyWidget.refresh(appContext)
        refreshLiveTrack()
        return fresh
    }

    /** A failed call is null (checked again next time), except an expired session, which stops the check. */
    private suspend fun <T> orNull(call: suspend () -> T): T? = try {
        call()
    } catch (e: AltimException.Unauthorized) {
        throw e
    } catch (e: kotlinx.coroutines.CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    /**
     * After a check that reached the server: the positions that newly became dangerous, then the configuration changes
     * of the radar (decisions re-read within about 20 seconds, the rest at the next check). One grouped notification
     * each at most; never the same change or danger twice (ChangeNotices). Nothing is checked offline.
     */
    suspend fun checkChanges(canNotify: Boolean): Pair<LocalNotice?, LocalNotice?> {
        val wantsDangers = dangerAlertsEnabled && holdings.isNotEmpty()
        val wantsConfig = configAlertsEnabled && watchlist.isNotEmpty()
        if (!(wantsDangers || wantsConfig) || offlineSince != null || !canNotify) return null to null
        val deadline = System.currentTimeMillis() + CONFIG_CHECK_BUDGET_MS
        var danger: LocalNotice? = null
        if (wantsDangers) {
            measureDangers()?.let { (found, unknown) ->
                val (n, s) = ChangeNotices.dangerNotice(
                    found, changeNotices, System.currentTimeMillis().toDouble(),
                    baseline = dangers?.let { ChangeNotices.dangerKeys(it.items) } ?: emptyList(),
                    nearStopUnknown = unknown,
                )
                storeChangeNotices(s)
                danger = n
            }
        }
        var config: LocalNotice? = null
        if (wantsConfig) {
            val found = checkConfigChanges(deadline)
            val (n, s) = ChangeNotices.configNotice(found, changeNotices)
            storeChangeNotices(s)
            config = n
        }
        return danger to config
    }

    /**
     * Background re-reading of the radar decisions, with the Radar's rule: the first 20 watched assets, 2 at a time,
     * those not compared in the last 15 minutes (here, on the Radar or on their page), the oldest first, until
     * [deadline]. Each one goes through the same configuration diff; returns the changes found. Stops at an expired
     * session; a failed asset is compared next time.
     */
    private suspend fun checkConfigChanges(deadline: Long): List<ConfigTransition> {
        val c = client ?: return emptyList()
        val now = System.currentTimeMillis().toDouble()
        val due = watchlist.take(MAX_BACKGROUND_DECISIONS)
            .map { it to lastDecisionCheck(it) }
            .filter { (_, last) -> last == null || now - last >= DECISION_EVERY }
            .sortedBy { (_, last) -> last ?: Double.NEGATIVE_INFINITY }
            .map { it.first }
        val found = mutableListOf<ConfigTransition>()
        for (pair in due.chunked(2)) {
            if (System.currentTimeMillis() >= deadline) break
            var unauthorized = false
            val got = coroutineScope {
                pair.map { a ->
                    async {
                        try {
                            c.decision(a)
                        } catch (e: AltimException.Unauthorized) {
                            unauthorized = true
                            null
                        } catch (e: kotlinx.coroutines.CancellationException) {
                            throw e
                        } catch (_: Exception) {
                            null
                        }
                    }
                }.awaitAll()
            }
            got.filterNotNull().forEach { d -> recordDecision(d, personal = false)?.let { found += it } }
            if (unauthorized) break
        }
        persistSession()
        return found
    }

    /**
     * Dangerous positions measured in the background like on Mes avoirs: quotes of the held assets and daily candles of
     * the lines that have a stop (the ATR of "near the stop"), 2 at a time, 20 assets at most. Null when the quotes
     * cannot be read or are the saved answers of the offline mode (an old price would tell a false danger). The second
     * value: ids of the lines with a stop whose candles could not be read.
     */
    private suspend fun measureDangers(): Pair<List<Danger>, Set<String>>? {
        val c = client ?: return null
        // In dollars like the quotes (a euro stop converted at the current rate; without a rate it is left out).
        val list = usdHoldings.holdings
        if (list.isEmpty()) return null
        val assets = list.map { it.asset }.distinctBy { it.id }
        val prices = orNull { c.quotes(assets) }?.associate { "${it.kind.raw}:${it.symbol}" to it.price }.orEmpty()
        if (prices.isEmpty() || offlineSince != null) return null
        val withStop = list.filter { it.stop != null }.map { it.asset }.distinctBy { it.id }.take(MAX_BACKGROUND_DECISIONS)
        var daily: Map<String, List<Candle>> = withStop.chunked(2).flatMap { pair ->
            coroutineScope { pair.map { a -> async { a.id to orNull { c.candles(a, "1d").candles }?.takeIf { it.isNotEmpty() } } }.awaitAll() }
        }.mapNotNull { (id, candles) -> candles?.let { id to it } }.toMap()
        // Candles served from the offline cache are old: the near-stop state is then unknown rather than wrong.
        if (offlineSince != null) daily = emptyMap()
        persistSession()
        val p = RiskPortfolio.of(list, 0.0, prices, daily)
        val unknown = list.filter { it.stop != null && daily[it.asset.id] == null }.map { it.id }.toSet()
        return PortfolioRisk.dangerousPositions(p, risk, daily, list.associate { it.id to it.stop }) to unknown
    }

    /** News to notify (NewsAlertTracker): a feed that fails does not stop the buy and price alerts. */
    private suspend fun checkNews(c: AltimClient, now: Long): List<NewsItem> {
        val mine = (watchlist + holdings.map { it.asset }).distinctBy { it.id }
        val report = try {
            c.news(mine)
        } catch (e: AltimException.Unauthorized) {
            throw e
        } catch (_: Exception) {
            return emptyList()
        }
        val tracker = prefs.getString("newsTracker", null)?.let { runCatching { AltimJson.decodeFromString(NewsAlertTracker.serializer(), it) }.getOrNull() } ?: NewsAlertTracker()
        val (next, fresh) = tracker.newAlerts(report.items, mine.map { it.id }.toSet(), now)
        prefs.edit().putString("newsTracker", AltimJson.encodeToString(NewsAlertTracker.serializer(), next)).apply()
        return fresh
    }

    /** From a tapped notification: "crypto:BTC" → the asset to open. */
    fun openFromNotification(id: String?) {
        if (id == null) return
        val (kind, symbol) = id.split(":").takeIf { it.size == 2 } ?: return
        val k = Kind.of(kind) ?: return
        pendingOpen = (watchlist + holdings.map { it.asset }).firstOrNull { it.id == id } ?: Asset(symbol, k, symbol)
    }

    // ---------- Access ----------

    /** Reopens the saved access (server + encrypted store) without asking anything, apart from the fingerprint / face. */
    private fun restore() {
        val url = prefs.getString("server", null)?.let { AltimClient.normalize(it, BuildConfig.DEBUG) } ?: return
        val creds = secure.get(SecureStore.Key.CREDENTIALS)?.let { runCatching { AltimJson.decodeFromString(Credentials.serializer(), it) }.getOrNull() }
        val open = prefs.getBoolean("openServer", false)
        if (creds == null && !open) return
        serverUrl = url
        user = creds?.user
        // Empty password = 2FA account: no silent re-login (a wrong attempt would count towards the lockout).
        val usable = creds?.takeIf { it.password.isNotEmpty() }
        client = makeClient(url, usable, secure.get(SecureStore.Key.SESSION))
        phase = if (biometricLock) Phase.LOCKED else Phase.READY
    }

    /** Checks the server and returns what it asks for (user/password, 2FA code, or nothing for a local server). */
    suspend fun probe(server: String): Pair<HttpUrl, AccessMode> {
        val url = AltimClient.normalize(server, BuildConfig.DEBUG) ?: throw AltimException.InvalidServer
        return url to AltimClient(url, null).accessMode()
    }

    suspend fun connect(url: HttpUrl, mode: AccessMode, user: String, password: String, code: String) {
        var creds: Credentials? = null
        if (mode is AccessMode.Login) {
            val c = Credentials(user.trim(), password)
            val first = AltimClient(url, null)
            first.login(c, code.trim())
            creds = c
            secure.set(SecureStore.Key.SESSION, first.sessionCookie)
            // With 2FA, a silent re-login is impossible (the code changes): the password is kept only without 2FA.
            val stored = if (mode.needsCode) c.copy(password = "") else c
            secure.set(SecureStore.Key.CREDENTIALS, AltimJson.encodeToString(Credentials.serializer(), stored))
            client = makeClient(url, if (mode.needsCode) null else c, first.sessionCookie)
        } else {
            client = makeClient(url, null, null)
        }
        prefs.edit().putString("server", url.toString()).putBoolean("openServer", mode is AccessMode.Open).apply()
        serverUrl = url
        this.user = creds?.user
        phase = Phase.READY
    }

    suspend fun logout() {
        live.stop()
        // Alert state and decisions of this server: another server would show them as its own.
        prefs.edit().remove("alertTracker").remove("lastAlerts").remove("newsTracker").remove(DecisionDigests.KEY).apply()
        lastAlerts = emptyList()
        decisionDigests = emptyMap()
        client?.logout()
        secure.clear()
        responseCache.clear()
        offlineSince = null
        prefs.edit().remove("openServer").apply()
        BuyWidget.refresh(appContext)
        client = null
        user = null
        phase = Phase.SETUP
    }

    /** Called after each API call: keeps the (possibly renewed) session cookie for the next launch. */
    fun persistSession() {
        val cookie = client?.sessionCookie ?: return
        if (cookie != secure.get(SecureStore.Key.SESSION)) secure.set(SecureStore.Key.SESSION, cookie)
    }

    /** The session expired and could not be renewed (2FA on, or password changed): back to the login screen. */
    fun sessionLost() {
        live.stop()
        // The dead cookie is dropped: the next launch asks to log in instead of failing silently again.
        secure.set(SecureStore.Key.SESSION, null)
        client = null
        phase = Phase.SETUP
    }

    // ---------- Lock (fingerprint / face / screen lock) ----------

    fun unlocked() {
        phase = Phase.READY
    }

    /** Locks again after 2 minutes in the background. */
    fun didEnterBackground() {
        lastBackground = System.currentTimeMillis()
        live.stop()
    }

    fun willEnterForeground() {
        resumeCount++
        val t = lastBackground
        lastBackground = null
        if (phase == Phase.READY && biometricLock && t != null && System.currentTimeMillis() - t > 120_000) phase = Phase.LOCKED
    }

    // ---------- Lists ----------

    fun watch(a: Asset) {
        if (watchlist.any { it.id == a.id }) return
        updateWatchlist(watchlist + a)
    }

    fun unwatch(a: Asset) = updateWatchlist(watchlist.filterNot { it.id == a.id })
    fun isWatched(a: Asset) = watchlist.any { it.id == a.id }

    fun updateWatchlist(list: List<Asset>) {
        watchlist = list
        save(ListSerializer(Asset.serializer()), "watchlist", list)
    }

    fun updateHoldings(list: List<Holding>) {
        holdings = list
        if (list.isEmpty()) saveDangers(emptyList())
        save(ListSerializer(Holding.serializer()), "holdings", list)
    }

    // ---------- Simulation (paper trading) ----------

    private fun storePaper(s: PaperState) {
        paper = s
        prefs.edit().putString("paper", encodePaper(s)).apply()
    }

    /** Simulated purchase (nothing is sent anywhere): the engine's French error when refused. */
    fun paperBuy(order: OpenOrder, decision: Decision? = null, note: String = ""): String? {
        val now = System.currentTimeMillis().toDouble()
        val r: PaperResult = openPosition(paper, order, now)
        if (r.error == null) {
            storePaper(r.state)
            // Written in the journal with the decision shown and the note ("Pourquoi j'entre").
            r.state.positions.lastOrNull()?.let { pos ->
                recordTrade(
                    NewTradeEntry(
                        UUID.randomUUID().toString(), now, "paper", "buy", pos.symbol, pos.kind, pos.name, pos.entry,
                        quantity = pos.quantity, amount = pos.invested, stop = pos.stop, targets = listOf(pos.target), note = note, refId = pos.id, decision = decision,
                    ),
                )
            }
        }
        return r.error
    }

    // ---------- Automatic journal ----------

    private fun storeJournal(s: TradeJournalState) {
        val edit = prefs.edit()
        if (tradeJournalError != null) prefs.getString(TradeJournal.KEY, null)?.let { edit.putString("${TradeJournal.KEY}.invalid", it) }
        edit.putString(TradeJournal.KEY, TradeJournal.encode(s)).apply()
        tradeJournal = s
        tradeJournalError = null
    }

    /** The decision seen on this phone for this asset, when recent enough to be "the one of the moment". */
    fun recentDecision(asset: Asset, now: Double = System.currentTimeMillis().toDouble()): Decision? {
        val (at, d) = recentDecisions[asset.id] ?: return null
        return d.takeIf { now - at <= TradeJournal.DECISION_MAX_AGE && now - d.asOf <= TradeJournal.DECISION_MAX_AGE }
    }

    /** Writes an entry and completes it in the background with the market data of that moment. Returns its id. */
    fun recordTrade(n: NewTradeEntry): String {
        val e = TradeJournal.createEntry(n)
        storeJournal(TradeJournal.addEntry(tradeJournal, e))
        scope.launch { runCatching { enrichTrade(e) } }
        return e.id
    }

    /** A real purchase or sale saved in « Mes avoirs » (the decision of the moment when one was seen recently). */
    fun recordRealTrade(asset: Asset, side: String, price: Double, quantity: Double, stop: Double? = null, note: String = "", refId: String? = null): String? {
        if (!(price > 0) || !(quantity > 0)) return null
        return recordTrade(
            NewTradeEntry(
                UUID.randomUUID().toString(), System.currentTimeMillis().toDouble(), "real", side, asset.symbol, asset.kind, asset.name, price,
                quantity = quantity, amount = price * quantity, stop = stop, note = note, refId = refId, decision = recentDecision(asset),
            ),
        )
    }

    fun setJournalNote(id: String, note: String) = storeJournal(TradeJournal.patchEntry(tradeJournal, id, note = note))
    fun deleteJournalEntry(id: String) = storeJournal(TradeJournal.removeEntry(tradeJournal, id))

    /** Macro stress, ATR and relative volume of that moment (and the decision when none was loaded); a failed fetch leaves the field unknown. */
    private suspend fun enrichTrade(e: TradeEntry) {
        val c = client ?: return
        val asset = e.asset
        val macro = runCatching { c.macro() }.getOrNull()
        val candles = runCatching { c.candles(asset, "1d").candles }.getOrNull()
        val decision = if (e.decision == null) runCatching { c.decision(asset) }.getOrNull() else null
        var market = JournalMarket(macroScore = macro?.score, macroLevel = macro?.level)
        if (!candles.isNullOrEmpty()) {
            val (atrPct, relVol) = TradeJournal.marketFromCandles(candles, e.createdAt)
            val current = tradeJournal.entries.firstOrNull { it.id == e.id }
            market = market.copy(atrPct = atrPct, relativeVolume = if (current?.market?.relativeVolume == null) relVol else null)
        }
        decision?.let { d ->
            val m = TradeJournal.marketFromDecision(d)
            market = market.copy(regime = m.regime, regimeLabel = m.regimeLabel, relativeVolume = m.relativeVolume ?: market.relativeVolume, events = m.events)
        }
        val current = tradeJournal.entries.firstOrNull { it.id == e.id } ?: return
        if (current.market.regime == null && market.regime == null) {
            macro?.regime?.let { r -> market = market.copy(regime = TradeJournal.regimeRaw(r.kind), regimeLabel = r.label) }
        }
        storeJournal(TradeJournal.patchEntry(tradeJournal, e.id, market = market, decision = decision?.let(TradeJournal::snapshotDecision)))
    }

    fun paperSell(id: String, price: Double): String? {
        val r = closePosition(paper, id, price, System.currentTimeMillis().toDouble())
        if (r.error == null) storePaper(r.state)
        return r.error
    }

    fun paperReset(capital: Double) {
        paperJustClosed = emptyList()
        storePaper(newPaper(capital, System.currentTimeMillis().toDouble()))
    }

    /**
     * Automatic exits: daily candles of the open positions that have a stop or a target, then the engine's rule
     * (day after the purchase, stop first, gap at the open). An asset whose candles fail is simply checked next time.
     */
    suspend fun checkPaperExits(): List<PaperTrade> {
        val c = client ?: return emptyList()
        val watched = paper.positions.filter { it.stop != null || it.target != null }.map { it.asset }.distinctBy { it.id }
        if (watched.isEmpty()) return emptyList()
        val candles = watched.mapNotNull { a ->
            try {
                a.id to c.candles(a, "1d").candles
            } catch (e: AltimException.Unauthorized) {
                throw e
            } catch (_: Exception) {
                null
            }
        }.toMap()
        persistSession()
        // The state may have changed during the calls (a manual sale): the rule runs on the current one.
        val r = checkExits(paper, candles)
        if (r.closed.isNotEmpty()) {
            storePaper(r.state)
            paperJustClosed = paperJustClosed + r.closed
        }
        return r.closed
    }

    private fun <T> save(serializer: kotlinx.serialization.KSerializer<T>, key: String, value: T) {
        prefs.edit().putString(key, AltimJson.encodeToString(serializer, value)).apply()
    }

    private fun <T> load(serializer: kotlinx.serialization.KSerializer<T>, key: String): T? =
        prefs.getString(key, null)?.let { runCatching { AltimJson.decodeFromString(serializer, it) }.getOrNull() }
}
