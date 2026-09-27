package com.maxlestage.altim.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.maxlestage.altim.kit.AccessMode
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.AltimJson
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.Credentials
import com.maxlestage.altim.kit.Holding
import com.maxlestage.altim.kit.Horizon
import com.maxlestage.altim.kit.Kind
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.HttpUrl

/** App state: server access, lock, watch list and holdings (stored on the phone only). Same as the iPhone AppModel. */
class AppModel(context: Context, private val secure: SecretStore = SecureStore(context)) {
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
    var holdings by mutableStateOf(load(ListSerializer(Holding.serializer()), "holdings") ?: emptyList())
        private set
    /** Budget in dollars for the Sélection tab (0 = not set). */
    var budget by mutableStateOf(prefs.getFloat("budget", 0f).toDouble())
        private set
    var selectionMarket by mutableStateOf(Kind.of(prefs.getString("selectionMarket", null)) ?: Kind.STOCK)
        private set
    var selectionHorizon by mutableStateOf(Horizon.of(prefs.getString("selectionHorizon", null)) ?: Horizon.MO1)
        private set

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

    fun updateSelection(market: Kind = selectionMarket, horizon: Horizon = selectionHorizon) {
        selectionMarket = market
        selectionHorizon = horizon
        prefs.edit().putString("selectionMarket", market.raw).putString("selectionHorizon", horizon.raw).apply()
    }

    // ---------- Access ----------

    /** Reopens the saved access (server + encrypted store) without asking anything, apart from the fingerprint / face. */
    private fun restore() {
        val url = prefs.getString("server", null)?.let { AltimClient.normalize(it) } ?: return
        val creds = secure.get(SecureStore.Key.CREDENTIALS)?.let { runCatching { AltimJson.decodeFromString(Credentials.serializer(), it) }.getOrNull() }
        val open = prefs.getBoolean("openServer", false)
        if (creds == null && !open) return
        serverUrl = url
        user = creds?.user
        // Empty password = 2FA account: no silent re-login (a wrong attempt would count towards the lockout).
        val usable = creds?.takeIf { it.password.isNotEmpty() }
        client = AltimClient(url, usable, secure.get(SecureStore.Key.SESSION))
        phase = if (biometricLock) Phase.LOCKED else Phase.READY
    }

    /** Checks the server and returns what it asks for (user/password, 2FA code, or nothing for a local server). */
    suspend fun probe(server: String): Pair<HttpUrl, AccessMode> {
        val url = AltimClient.normalize(server) ?: throw AltimException.InvalidServer
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
            client = AltimClient(url, if (mode.needsCode) null else c, first.sessionCookie)
        } else {
            client = AltimClient(url, null)
        }
        prefs.edit().putString("server", url.toString()).putBoolean("openServer", mode is AccessMode.Open).apply()
        serverUrl = url
        this.user = creds?.user
        phase = Phase.READY
    }

    suspend fun logout() {
        live.stop()
        client?.logout()
        secure.clear()
        prefs.edit().remove("openServer").apply()
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
        save(ListSerializer(Holding.serializer()), "holdings", list)
    }

    private fun <T> save(serializer: kotlinx.serialization.KSerializer<T>, key: String, value: T) {
        prefs.edit().putString(key, AltimJson.encodeToString(serializer, value)).apply()
    }

    private fun <T> load(serializer: kotlinx.serialization.KSerializer<T>, key: String): T? =
        prefs.getString(key, null)?.let { runCatching { AltimJson.decodeFromString(serializer, it) }.getOrNull() }
}
