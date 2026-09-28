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
import com.maxlestage.altim.kit.ConfigChanges
import com.maxlestage.altim.kit.ConfigTransition
import com.maxlestage.altim.kit.Credentials
import com.maxlestage.altim.kit.Danger
import com.maxlestage.altim.kit.Decision
import com.maxlestage.altim.kit.PortfolioRisk
import com.maxlestage.altim.kit.RiskSettings
import com.maxlestage.altim.kit.ScoreWeights
import com.maxlestage.altim.kit.FileResponseCache
import com.maxlestage.altim.kit.Holding
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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.HttpUrl

/** App state: server access, lock, watch list and holdings (stored on the phone only). Same as the iPhone AppModel. */
class AppModel(context: Context, private val secure: SecretStore = SecureStore(context)) {
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
    var holdings by mutableStateOf(load(ListSerializer(Holding.serializer()), "holdings")?.map { it.cleaned() } ?: emptyList())
        private set
    /** Limits of Réglages → Prudence des conseils, checked in Mes avoirs (same defaults as the web app). */
    var risk by mutableStateOf(load(RiskSettings.serializer(), "risk")?.sanitized() ?: RiskSettings.DEFAULT)
        private set
    /** Weights of the decision's composite score (Réglages → Score composite), sent only when not the defaults. */
    var scoreWeights by mutableStateOf(ScoreWeights.parse(prefs.getString("scoreWeights", null)))
        private set
    /** Configuration changes of the decisions seen on this phone (validated when read back; damaged = empty). */
    var configChanges by mutableStateOf(ConfigChanges.parse(prefs.getString(ConfigChanges.KEY, null)))
        private set
    /** Last "positions devenues dangereuses" measured on Mes avoirs, shown on the Radar with their time. */
    var dangers by mutableStateOf(PortfolioRisk.parseDangers(prefs.getString("dangers.v1", null)))
        private set
    /** Budget in dollars for the Sélection tab (0 = not set). */
    var budget by mutableStateOf(prefs.getFloat("budget", 0f).toDouble())
        private set
    var selectionMarket by mutableStateOf(Kind.of(prefs.getString("selectionMarket", null)) ?: Kind.STOCK)
        private set
    var selectionHorizon by mutableStateOf(Horizon.of(prefs.getString("selectionHorizon", null)) ?: Horizon.MO1)
        private set

    /** Buy notifications (Réglages): on by default once allowed; "strong only" = signal and zone together. */
    var alertsEnabled by mutableStateOf(prefs.getBoolean("alertsEnabled", false))
        private set
    var alertsStrongOnly by mutableStateOf(prefs.getBoolean("alertsStrongOnly", false))
        private set
    /** News alerts (Réglages): serious escalation, or a story about one of my assets told by at least 3 sources. */
    var newsAlertsEnabled by mutableStateOf(prefs.getBoolean("newsAlertsEnabled", false))
        private set
    /** Time of the last background check and what it found (shown in Réglages). */
    var lastAlertCheck by mutableStateOf(prefs.getLong("lastAlertCheck", 0L).takeIf { it > 0 })
        private set
    var lastBuyable by mutableStateOf(prefs.getInt("lastBuyable", 0))
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
    var paper by mutableStateOf(decodePaper(prefs.getString("paper", null)) ?: newPaper(Paper.DEFAULT_CAPITAL, System.currentTimeMillis().toDouble()))
        private set
    /** Positions just closed by their stop or target (shown once in the Simulation part). */
    var paperJustClosed by mutableStateOf<List<PaperTrade>>(emptyList())
    /** Incremented each time the app comes back to the foreground (the Simulation part checks its exits again). */
    var resumeCount by mutableIntStateOf(0)
        private set

    /** Asset to open, from a tapped notification. */
    var pendingOpen by mutableStateOf<Asset?>(null)
    /** A news notification was tapped: the Actu tab opens. */
    var pendingNews by mutableStateOf(false)

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

    init {
        restore()
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

    fun updateBudget(v: Double) {
        budget = v
        prefs.edit().putFloat("budget", v.toFloat()).apply()
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
        if (offlineSince != null) return null
        val u = ConfigChanges.apply(configChanges, d, personal, now)
        configChanges = u.state
        prefs.edit().putString(ConfigChanges.KEY, ConfigChanges.encode(u.state)).apply()
        return u.transition
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

    fun updateNewsAlerts(context: Context, enabled: Boolean) {
        newsAlertsEnabled = enabled
        prefs.edit().putBoolean("newsAlertsEnabled", enabled).apply()
        BuyAlerts.schedule(context, needsChecks)
    }

    /** The background check runs for the buy alerts, the news alerts or at least one armed price alert. */
    val needsChecks: Boolean get() = alertsEnabled || newsAlertsEnabled || priceTargets.any { it.triggered == null }

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

    /** What one background check found: new buy alerts and price alerts just reached (with the price). */
    data class CheckResult(val buy: List<BuyAlert>, val targets: List<Pair<PriceTarget, Double>>, val news: List<NewsItem> = emptyList())

    /**
     * One check of the buy alerts (background worker): the server's rule applied to the watch list and the holdings;
     * returns the alerts to notify (new or changed situation only), null without access.
     */
    suspend fun checkAlerts(): CheckResult? {
        val c = client ?: return null
        val now = System.currentTimeMillis()
        var fresh = emptyList<BuyAlert>()
        if (alertsEnabled) {
            val assets = (watchlist + holdings.map { it.asset }).distinctBy { it.id }
            val items = c.alerts(assets)
            val tracker = prefs.getString("alertTracker", null)?.let { runCatching { AltimJson.decodeFromString(AlertTracker.serializer(), it) }.getOrNull() } ?: AlertTracker()
            val (next, newOnes) = tracker.newAlerts(items, alertsStrongOnly)
            fresh = newOnes
            lastBuyable = items.count { it.buy }
            prefs.edit().putString("alertTracker", AltimJson.encodeToString(AlertTracker.serializer(), next)).putInt("lastBuyable", lastBuyable)
                .putString(BuyWidget.KEY_ITEMS, AltimJson.encodeToString(ListSerializer(BuyAlert.serializer()), items.filter { it.buy }))
                .apply()
        }
        // Price alerts: consensus quotes of the assets that still have an armed threshold.
        val armed = priceTargets.filter { it.triggered == null }
        var fired = emptyList<Pair<PriceTarget, Double>>()
        if (armed.isNotEmpty()) {
            val quotes = c.quotes(armed.map { it.asset }.distinctBy { it.id })
            val (updated, reached) = PriceTarget.evaluate(priceTargets, quotes.associate { "${it.kind.raw}:${it.symbol}" to it.price }, now)
            fired = reached
            if (reached.isNotEmpty()) setTargets(null, updated)
        }
        val entries = fresh.mapNotNull { a ->
            a.price?.let { JournalEntry(asset = a.asset, source = if (a.strong) JournalEntry.Source.STRONG_BUY else JournalEntry.Source.BUY, title = a.title, price = it, date = now) }
        } + fired.map { (t, p) -> JournalEntry(asset = t.asset, source = JournalEntry.Source.TARGET, title = "${t.asset.symbol} : ${t.label.lowercase()}", price = p, date = now) }
        if (entries.isNotEmpty()) {
            journal = AlertJournal.add(entries, journal)
            save(ListSerializer(JournalEntry.serializer()), "journal", journal)
        }
        val news = if (newsAlertsEnabled) checkNews(c, now) else emptyList()
        lastAlertCheck = now
        prefs.edit().putLong("lastAlertCheck", now).apply()
        persistSession()
        BuyWidget.refresh(appContext)
        return CheckResult(fresh, fired, news)
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
        responseCache.clear()
        offlineSince = null
        client?.logout()
        secure.clear()
        prefs.edit().remove("openServer").remove(BuyWidget.KEY_ITEMS).remove("newsTracker").remove("lastAlertCheck").apply()
        lastAlertCheck = null
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
    fun paperBuy(order: OpenOrder): String? {
        val r: PaperResult = openPosition(paper, order, System.currentTimeMillis().toDouble())
        if (r.error == null) storePaper(r.state)
        return r.error
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
