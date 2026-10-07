package com.maxlestage.altim.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.BuyAlert
import com.maxlestage.altim.kit.LiveMessage
import com.maxlestage.altim.kit.LiveSocket
import com.maxlestage.altim.kit.LiveTick
import com.maxlestage.altim.kit.VerdictPush
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Live data of the displayed assets on the WebSocket `/api/ws`: prices, the « can I buy now? » alerts, the Radar
 * verdicts and the EUR/USD rate, pushed by the server. Reconnected with a growing pause (1 s → 30 s), closed when the
 * app goes to the background (ProcessLifecycleOwner, see Root) and reopened when it comes back. After two failed
 * openings in a row (a network that blocks WebSockets), the prices come from the Server-Sent Events of `/api/live`.
 */
class LivePrices(private val scope: CoroutineScope) {
    val ticks = mutableStateMapOf<String, LiveTick>()
    /** Time of the last refresh of the prices. */
    var lastTick by mutableStateOf<Long?>(null)
        private set
    /** Time of the last message of any kind (the badge says « en direct » only while messages keep coming). */
    var lastMessage by mutableStateOf<Long?>(null)
        private set

    /** Pushed alerts of the followed assets (one socket per 20 assets), with the server's check time. */
    var onAlerts: ((List<BuyAlert>, Long) -> Unit)? = null
    var onVerdict: ((VerdictPush) -> Unit)? = null
    var onFx: ((Double) -> Unit)? = null

    private var jobs: List<Job> = emptyList()
    private var keys: List<String> = emptyList()
    private val pending = mutableMapOf<String, LiveTick>()
    private var flush: Job? = null

    fun price(a: Asset): LiveTick? = ticks[a.id]

    /** Follows these assets (restarts only when the list or the currency of the texts changes). */
    fun follow(assets: List<Asset>, client: AltimClient?, usd: Boolean, onRenewed: () -> Unit = {}, onExpired: () -> Unit) {
        val unique = assets.associateBy { it.id }
        val wanted = unique.keys.sorted() + (if (usd) "USD" else "EUR")
        if (client == null || unique.isEmpty()) return stop()
        if (wanted == keys && jobs.any { it.isActive }) return
        stop()
        keys = wanted
        jobs = unique.values.sortedBy { it.id }.chunked(LiveSocket.MAX_ASSETS).map { chunk -> scope.launch { run(chunk, client, usd, onRenewed, onExpired) } }
    }

    private suspend fun run(chunk: List<Asset>, client: AltimClient, usd: Boolean, onRenewed: () -> Unit, onExpired: () -> Unit) {
        var attempt = 0
        var failures = 0
        var sse = false
        while (scope.isActive) {
            val opened = AtomicBoolean(false)
            try {
                if (sse) {
                    client.liveTicks(chunk).collect { tick ->
                        lastMessage = System.currentTimeMillis()
                        buffer(tick)
                        attempt = 0
                    }
                } else {
                    client.liveMessages(chunk, usd, onOpened = { opened.set(true) }).collect { m ->
                        handle(m)
                        attempt = 0
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: AltimException.Unauthorized) {
                if (!client.renewSession()) {
                    onExpired()
                    return
                }
                onRenewed()
                opened.set(true)
            } catch (_: Exception) {
            }
            if (!sse) {
                failures = if (opened.get()) 0 else failures + 1
                if (failures >= LiveSocket.FALLBACK_AFTER) sse = true
            }
            delay(LiveSocket.backoffMs(attempt))
            attempt++
        }
    }

    private fun handle(m: LiveMessage) {
        lastMessage = System.currentTimeMillis()
        when (m) {
            is LiveMessage.Tick -> buffer(m.tick)
            is LiveMessage.Alerts -> onAlerts?.invoke(m.items, if (m.checkedAt > 0) m.checkedAt.toLong() else System.currentTimeMillis())
            is LiveMessage.Verdict -> onVerdict?.invoke(m.verdict)
            is LiveMessage.Fx -> onFx?.invoke(m.rate)
            else -> {}
        }
    }

    /** Cryptos tick several times per second: the screen is refreshed at most every 0.3 s. */
    private fun buffer(tick: LiveTick) {
        pending[tick.key] = tick
        if (flush != null) return
        flush = scope.launch {
            delay(300)
            flush = null
            if (pending.isEmpty()) return@launch
            ticks.putAll(pending)
            pending.clear()
            lastTick = System.currentTimeMillis()
        }
    }

    fun stop() {
        jobs.forEach { it.cancel() }
        jobs = emptyList()
        flush?.cancel()
        flush = null
        pending.clear()
        keys = emptyList()
    }
}
