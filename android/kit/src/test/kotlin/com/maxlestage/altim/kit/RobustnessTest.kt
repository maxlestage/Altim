package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.builtins.ListSerializer
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import java.nio.file.Files
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Retries and offline mode against a real HTTP test server (and a closed port for "no network"). */
class RobustnessTest {
    private val macro = """{"score":0,"level":"calm","factors":[],"themes":[],"values":{}}"""
    private lateinit var server: MockWebServer
    private val dir = Files.createTempDirectory("altim-cache").toFile()

    @BeforeTest fun setUp() {
        AltimClient.retryDelaysMs = listOf(10L, 10L)
        server = MockWebServer().apply { start() }
    }

    @AfterTest fun tearDown() {
        server.close()
        dir.deleteRecursively()
    }

    private fun reply(code: Int, body: String) = server.enqueue(MockResponse.Builder().code(code).body(body).build())

    @Test fun temporaryServerErrorsAreRetried() = runBlocking {
        reply(503, "{}")
        reply(502, "{}")
        reply(200, macro)
        assertEquals("calm", AltimClient(server.url("/"), null).macro().level)
        assertEquals(3, server.requestCount)
    }

    @Test fun givesUpAfterThreeAttempts() = runBlocking {
        repeat(4) { reply(503, """{"error":"surcharge"}""") }
        val e = assertFailsWith<AltimException.Server> { AltimClient(server.url("/"), null).macro() }
        assertEquals("surcharge", e.message)
        assertEquals(3, server.requestCount)
    }

    @Test fun loginIsNeverRetried() = runBlocking {
        repeat(3) { reply(503, "") }
        // 503 on the login form = private access not configured on the server; above all: a single request.
        assertFailsWith<AltimException.NotConfigured> { AltimClient(server.url("/"), null).login(Credentials("max", "x")) }
        assertEquals(1, server.requestCount)
    }

    @Test fun offlineServesLastGoodAnswerWithItsDate() = runBlocking {
        val cache = FileResponseCache(dir)
        var status: Long? = -1L
        val url = server.url("/")
        val client = AltimClient(url, null, cache = cache, onStatus = { status = it })
        reply(200, macro)
        client.macro()
        assertNull(status, "online: no stale date")
        // Server down (500) after the retries: the cached answer.
        repeat(3) { reply(500, """{"error":"panne"}""") }
        assertEquals("calm", client.macro().level)
        assertNotNull(status)
        // No network at all (nothing listens any more): same.
        server.close()
        status = null
        assertEquals("calm", client.macro().level)
        assertTrue(System.currentTimeMillis() - status!! < 60_000)
        // A search is never served from the cache.
        assertFailsWith<AltimException.Network> { client.search("btc") }
    }

    @Test fun cachePruneKeepsTheMostRecent() {
        val cache = FileResponseCache(dir, limit = 5)
        for (i in 0 until 12) cache.save("/api/radar?i=$i", "$i")
        cache.prune()
        assertEquals(5, dir.listFiles { f -> f.name.endsWith(".json") }!!.size)
        assertNotEquals(FileResponseCache.fileName("/api/radar?symbols=BTC:crypto"), FileResponseCache.fileName("/api/radar?symbols=ETH:crypto"))
    }
}

class NewsTest {
    @Test fun decodeRealNews() {
        val r = AltimJson.decodeFromString(NewsReport.serializer(), requireNotNull(javaClass.getResource("/fixtures/news.json")).readText())
        assertTrue(r.items.size > 10)
        assertEquals(r.top.size, r.topItems.size)
        assertTrue(r.sources.size > 10)
        assertTrue(r.items.all { it.safeUrl != null })
        assertTrue(r.items.any { it.category == "actifs" && it.assets.isNotEmpty() })
        assertNull(r.items[0].copy(link = "javascript:alert(1)").safeUrl)
        assertEquals("il y a 3 h", r.items[0].copy(time = (System.currentTimeMillis() - 3 * 3_600_000).toDouble()).age())
        check(ListSerializer(NewsItem.serializer()).descriptor.serialName.isNotEmpty())
    }
}
