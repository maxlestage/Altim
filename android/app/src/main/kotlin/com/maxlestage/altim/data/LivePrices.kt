package com.maxlestage.altim.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.maxlestage.altim.kit.AltimClient
import com.maxlestage.altim.kit.AltimException
import com.maxlestage.altim.kit.Asset
import com.maxlestage.altim.kit.LiveTick
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** Live prices of the displayed assets (Server-Sent Events of /api/live), reconnected automatically. */
class LivePrices(private val scope: CoroutineScope) {
    val ticks = mutableStateMapOf<String, LiveTick>()
    /** Time of the last refresh (the badge says "en direct" only while ticks keep coming). */
    var lastTick by mutableStateOf<Long?>(null)
        private set

    private var job: Job? = null
    private var keys: List<String> = emptyList()
    private val pending = mutableMapOf<String, LiveTick>()
    private var flush: Job? = null

    fun price(a: Asset): LiveTick? = ticks[a.id]

    /** Follows these assets (restarts the stream only when the list changes). */
    fun follow(assets: List<Asset>, client: AltimClient?, onExpired: () -> Unit) {
        val unique = assets.associateBy { it.id }
        val wanted = unique.keys.sorted()
        if (client == null || wanted.isEmpty()) return stop()
        if (wanted == keys && job?.isActive == true) return
        stop()
        keys = wanted
        job = scope.launch {
            var wait = 1_000L
            while (isActive) {
                try {
                    client.liveTicks(unique.values.toList()).collect { tick ->
                        buffer(tick)
                        wait = 1_000L
                    }
                } catch (e: CancellationException) {
                    throw e
                } catch (e: AltimException.Unauthorized) {
                    if (!client.renewSession()) {
                        onExpired()
                        return@launch
                    }
                    continue
                } catch (_: Exception) {
                }
                // 1 s, 2 s, 4 s … up to 30 s between attempts (network lost, server restart).
                delay(wait)
                wait = minOf(30_000L, wait * 2)
            }
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
        job?.cancel()
        job = null
        flush?.cancel()
        flush = null
        pending.clear()
        keys = emptyList()
    }
}
