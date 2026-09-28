package com.maxlestage.altim.kit

import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.channels.trySendBlocking
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.Serializable
import okhttp3.Call
import okhttp3.Callback
import okhttp3.FormBody
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import java.io.IOException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** Errors shown to the user, same French messages as the iPhone app. */
sealed class AltimException(message: String) : Exception(message) {
    data object InvalidServer : AltimException("Adresse du serveur invalide (exemple : https://mon-app.herokuapp.com).")
    data object NotAltim : AltimException("Ce serveur ne répond pas comme Altim. Vérifiez l'adresse.")
    data object NotConfigured : AltimException("Accès privé non configuré sur le serveur : ajoutez les variables ALTIM_* dans Heroku → Settings → Config Vars.")
    data object WrongCredentials : AltimException("Identifiant, mot de passe ou code incorrect.")
    data object Locked : AltimException("Trop d'essais. Réessayez dans 15 minutes.")
    data object Refused : AltimException("Connexion refusée par le serveur.")
    data object Unauthorized : AltimException("Session expirée : reconnectez-vous.")
    class Server(message: String) : AltimException(message)
    class Network(detail: String) : AltimException("Réseau indisponible ($detail).")
}

@Serializable
data class Credentials(val user: String, val password: String)

/** What the server asks for before giving access. */
sealed interface AccessMode {
    /** Private access (Heroku with ALTIM_* set): user + password, and a 6-digit code if 2FA is on. */
    data class Login(val needsCode: Boolean) : AccessMode
    /** Local development server without private access. */
    data object Open : AccessMode
}

/**
 * HTTP client of the Altim server. The session is the same cookie as the website (HttpOnly, 7 days), read from the
 * login answer and sent by hand (no shared cookie jar); when it expires, the client logs in again with the saved
 * credentials and replays the request once.
 */
class AltimClient(
    val baseUrl: HttpUrl,
    private val credentials: Credentials?,
    sessionCookie: String? = null,
    private val http: OkHttpClient = defaultHttp,
    /** Last good answers, served when the network or the server fails (offline mode); null = no cache. */
    private val cache: ResponseCache? = null,
    /** Told after each call: null when the server answered, the date of the data when a cached answer was served. */
    private val onStatus: ((Long?) -> Unit)? = null,
) {
    private val cookie = AtomicReference(sessionCookie)
    private val renewLock = kotlinx.coroutines.sync.Mutex()
    @Volatile private var renewFailedAt = 0L

    /** Current session cookie value (the app keeps it encrypted to avoid a login at each launch). */
    val sessionCookie: String? get() = cookie.get()

    companion object {
        const val COOKIE_NAME = "altim_session"

        /** Paths whose last answer is kept for the offline mode (not the search nor the login). */
        val CACHEABLE = setOf("/api/radar", "/api/tickers", "/api/candles", "/api/guard", "/api/zones", "/api/macro", "/api/alerts", "/api/news", "/api/selection")

        /** Pauses before the 2nd and 3rd attempt of a read that failed on the network or a temporary server error. */
        @Volatile var retryDelaysMs = listOf(500L, 1_500L)
        private const val USER_AGENT = "AltimAndroid/1.0"

        val defaultHttp: OkHttpClient = OkHttpClient.Builder()
            .followRedirects(false)
            .followSslRedirects(false)
            .connectTimeout(20, TimeUnit.SECONDS)
            .readTimeout(45, TimeUnit.SECONDS)
            .build()

        /**
         * "mon-app.herokuapp.com" → https://mon-app.herokuapp.com/. Plain http only for this device, and for the
         * computer seen from the emulator (10.0.2.2) in development builds only ([dev]).
         */
        fun normalize(text: String, dev: Boolean = false): HttpUrl? {
            var t = text.trim().trimEnd('/')
            if (t.isEmpty()) return null
            if (!t.contains("://")) t = "https://$t"
            val url = t.toHttpUrlOrNull() ?: return null
            val host = url.host
            val local = host == "localhost" || host == "127.0.0.1" || (dev && host == "10.0.2.2")
            if (!(url.scheme == "https" || (url.scheme == "http" && local))) return null
            if (url.encodedPath != "/" || url.query != null) return null
            return url
        }

        fun list(assets: List<Asset>): String = assets.joinToString(",") { "${it.symbol}:${it.kind.raw}" }

        fun sessionCookie(headers: List<String>): String? {
            for (h in headers) {
                val start = h.indexOf("$COOKIE_NAME=")
                if (start < 0) continue
                val value = h.substring(start + COOKIE_NAME.length + 1).takeWhile { it != ';' && it != ',' && it != ' ' }
                if (value.isNotEmpty()) return value
            }
            return null
        }
    }

    // ---------- Access ----------

    /** Checks the server and tells whether it needs a login (and a 2FA code). */
    suspend fun accessMode(): AccessMode {
        val (status, body) = send(request("/login")).use { it.code to it.body.string() }
        if ((status != 200 && status != 302) || !body.contains("Altim")) throw AltimException.NotAltim
        if (body.contains("Accès privé non configuré")) {
            // Production refuses everything; a local development server is open.
            val probe = send(request("/api/search", mapOf("q" to "btc", "limit" to "1"))).use { it.code }
            if (probe == 200) return AccessMode.Open
            throw AltimException.NotConfigured
        }
        return AccessMode.Login(needsCode = body.contains("name=\"code\""))
    }

    /** Same form as the website (the server checks the Origin against CSRF). Success = redirect to /health with the cookie. */
    suspend fun login(c: Credentials, code: String = "") {
        val form = FormBody.Builder()
            .add("user", c.user)
            .add("password", c.password)
            .add("code", code)
            .add("next", "/health")
            .build()
        val r = requestBuilder("/login").post(form).header("Origin", origin).build()
        send(r, withSession = false).use { res ->
            when (res.code) {
                302, 303 -> {
                    val cookieValue = sessionCookie(res.headers("Set-Cookie"))
                    if (res.header("Location")?.endsWith("/health") != true || cookieValue == null) throw AltimException.Refused
                    cookie.set(cookieValue)
                }
                401 -> throw AltimException.WrongCredentials
                429 -> throw AltimException.Locked
                503 -> throw AltimException.NotConfigured
                403 -> throw AltimException.Refused
                else -> throw AltimException.Server("Connexion impossible (code ${res.code}).")
            }
        }
    }

    suspend fun logout() {
        runCatching { send(requestBuilder("/logout").post(FormBody.Builder().build()).header("Origin", origin).build()).close() }
        cookie.set(null)
    }

    /**
     * Logs in again after a 401 (session expired) and tells whether it worked. One login at a time (parallel callers
     * reuse its result), and a failed attempt is not retried for a minute: a changed password must not burn the
     * server's lock-out quota.
     */
    suspend fun renewSession(): Boolean {
        val c = credentials ?: return false
        val before = cookie.get()
        return renewLock.withLock {
            // Another caller renewed the session while we were waiting.
            if (cookie.get() != null && cookie.get() != before) return@withLock true
            if (System.currentTimeMillis() - renewFailedAt < 60_000) return@withLock false
            val ok = runCatching { login(c) }.isSuccess
            renewFailedAt = if (ok) 0L else System.currentTimeMillis()
            ok
        }
    }

    // ---------- API ----------

    suspend fun radar(assets: List<Asset>, interval: String = "4h"): List<RadarRow> =
        batched(assets) { get("/api/radar", mapOf("symbols" to list(it), "interval" to interval), ListSerializer(RadarRow.serializer())) }

    suspend fun quotes(assets: List<Asset>): List<Quote> =
        batched(assets) { get("/api/tickers", mapOf("symbols" to list(it)), ListSerializer(Quote.serializer())) }

    suspend fun search(q: String, limit: Int = 20): List<SearchItem> =
        get("/api/search", mapOf("q" to q.take(30), "limit" to "$limit"), ListSerializer(SearchItem.serializer()))

    suspend fun candles(a: Asset, interval: String): Snapshot =
        get("/api/candles", mapOf("symbol" to a.symbol, "kind" to a.kind.raw, "interval" to interval), Snapshot.serializer())

    suspend fun guardReport(a: Asset): GuardReport =
        get("/api/guard", mapOf("symbol" to a.symbol, "kind" to a.kind.raw), GuardReport.serializer())

    suspend fun zones(a: Asset): ZonesReport =
        get("/api/zones", mapOf("symbol" to a.symbol, "kind" to a.kind.raw), ZonesReport.serializer())

    suspend fun macro(): MacroInfo = get("/api/macro", emptyMap(), MacroInfo.serializer())

    /** News of the world, the markets, crypto and these assets (20 at most). */
    suspend fun news(assets: List<Asset>): NewsReport =
        get("/api/news", if (assets.isEmpty()) emptyMap() else mapOf("symbols" to list(assets.take(20))), NewsReport.serializer())

    suspend fun alerts(assets: List<Asset>): List<BuyAlert> =
        if (assets.isEmpty()) emptyList()
        else batched(assets) { get("/api/alerts", mapOf("symbols" to list(it)), ListSerializer(BuyAlert.serializer())) }

    suspend fun selection(h: Horizon, kind: Kind): SelectionResult {
        val (status, body) = authorized(request("/api/selection", mapOf("horizon" to h.raw, "kind" to kind.raw)))
        if (status == 202) return SelectionResult.Pending
        return SelectionResult.Ready(decode(SelectionReport.serializer(), body, status))
    }

    /**
     * Live prices of these assets: last known price right away, then every change (several per second for cryptos,
     * every 5 s for stocks). Ends on a network error or an expired session ([AltimException.Unauthorized]); the caller
     * reconnects. Cancelling the collector closes the connection.
     */
    fun liveTicks(assets: List<Asset>): Flow<LiveTick> = callbackFlow {
        val stream = http.newBuilder().readTimeout(60, TimeUnit.SECONDS).build() // heartbeat every 15 s from the server
        val call = stream.newCall(
            withSession(request("/api/live", mapOf("symbols" to list(assets))).newBuilder().header("Accept", "text/event-stream").build()),
        )
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                close(AltimException.Network(e.message ?: "connexion perdue"))
            }

            override fun onResponse(call: Call, response: Response) {
                response.use { res ->
                    if (res.code != 200) {
                        close(if (res.code == 401) AltimException.Unauthorized else AltimException.Server("Flux en direct refusé (code ${res.code})."))
                        return
                    }
                    val parser = SseParser()
                    val buf = ByteArray(8192)
                    try {
                        val input = res.body.byteStream()
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            for (t in parser.ticks(buf, n)) trySendBlocking(t)
                        }
                        close()
                    } catch (e: IOException) {
                        close(AltimException.Network(e.message ?: "connexion perdue"))
                    }
                }
            }
        })
        awaitClose { call.cancel() }
    }

    // ---------- Plumbing ----------

    private val origin: String
        get() = baseUrl.newBuilder().encodedPath("/").build().toString().trimEnd('/')

    internal fun request(path: String, query: Map<String, String> = emptyMap()): Request = requestBuilder(path, query).build()

    private fun requestBuilder(path: String, query: Map<String, String> = emptyMap()): Request.Builder {
        val url = baseUrl.newBuilder().encodedPath(path).apply {
            query.toSortedMap().forEach { (k, v) -> addQueryParameter(k, v) }
        }.build()
        return Request.Builder().url(url).header("User-Agent", USER_AGENT)
    }

    private fun withSession(r: Request): Request =
        cookie.get()?.let { r.newBuilder().header("Cookie", "$COOKIE_NAME=$it").build() } ?: r

    private suspend fun send(r: Request, withSession: Boolean = true): Response {
        val call = http.newCall(if (withSession) withSession(r) else r)
        return try {
            call.await()
        } catch (e: IOException) {
            throw AltimException.Network(e.message ?: e.javaClass.simpleName)
        }
    }

    /**
     * A read (GET) is tried 3 times when the network drops or the server answers 502 / 503 / 504 (restart, overload):
     * a short outage goes unnoticed. Never for the login (a failed attempt counts towards the lock-out).
     */
    private suspend fun sendRetrying(r: Request): Pair<Int, String> {
        val idempotent = r.method == "GET"
        var attempt = 0
        while (true) {
            try {
                val res = send(r).use { it.code to it.body.string() }
                if (idempotent && res.first in setOf(502, 503, 504) && attempt < retryDelaysMs.size) {
                    kotlinx.coroutines.delay(retryDelaysMs[attempt++])
                    continue
                }
                return res
            } catch (e: AltimException.Network) {
                if (!idempotent || attempt >= retryDelaysMs.size) throw e
                kotlinx.coroutines.delay(retryDelaysMs[attempt++])
            }
        }
    }

    private suspend fun authorized(r: Request): Pair<Int, String> {
        val key = "${r.url.encodedPath}?${r.url.encodedQuery ?: ""}"
        val cacheable = cache != null && r.url.encodedPath in CACHEABLE
        try {
            var res = sendRetrying(r)
            if (res.first == 401) {
                cookie.set(null)
                if (!renewSession()) throw AltimException.Unauthorized
                res = sendRetrying(r)
                if (res.first == 401) throw AltimException.Unauthorized
            }
            if (res.first >= 500 && cacheable) {
                cache?.load(key)?.let { (body, at) ->
                    onStatus?.invoke(at)
                    return 200 to body
                }
            }
            if (res.first == 200) {
                if (cacheable) cache?.save(key, res.second)
                onStatus?.invoke(null)
            }
            return res
        } catch (e: AltimException.Network) {
            // Offline: the last good answer, with its date for the banner.
            if (cacheable) cache?.load(key)?.let { (body, at) ->
                onStatus?.invoke(at)
                return 200 to body
            }
            throw e
        }
    }

    private suspend fun <T> get(path: String, query: Map<String, String>, serializer: KSerializer<T>): T {
        val (status, body) = authorized(request(path, query))
        return decode(serializer, body, status)
    }

    private fun <T> decode(serializer: KSerializer<T>, body: String, status: Int): T {
        if (status !in 200..299) {
            val message = runCatching { AltimJson.decodeFromString(MapSerializer(String.serializer(), String.serializer()), body)["error"] }.getOrNull()
            throw AltimException.Server(message ?: "Erreur $status")
        }
        return try {
            AltimJson.decodeFromString(serializer, body)
        } catch (e: Exception) {
            throw AltimException.Server("Réponse illisible du serveur (${e.message?.take(120)}).")
        }
    }

    /** The server takes 20 assets per request: bigger lists are split into parallel batches. */
    private suspend fun <T> batched(assets: List<Asset>, call: suspend (List<Asset>) -> List<T>): List<T> = coroutineScope {
        assets.chunked(20).map { async { call(it) } }.awaitAll().flatten()
    }
}

/** OkHttp call as a cancellable suspension (cancelling the coroutine cancels the request). */
private suspend fun Call.await(): Response = suspendCancellableCoroutine { cont ->
    enqueue(object : Callback {
        override fun onFailure(call: Call, e: IOException) {
            if (cont.isActive) cont.resumeWithException(e)
        }

        override fun onResponse(call: Call, response: Response) {
            if (cont.isActive) cont.resume(response) else response.close()
        }
    })
    cont.invokeOnCancellation { runCatching { cancel() } }
}
