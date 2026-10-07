package com.maxlestage.altim.kit

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Messages of the real-time socket `/api/ws` (protocol v1), as on iOS, and one exchange with a local server. */
class LiveSocketTest {
    @Test fun decodesServerMessages() {
        assertEquals(LiveMessage.Hello(1), LiveSocket.decode("""{"type":"hello","v":1,"maxAssets":20,"pingEvery":20}"""))
        val t = assertIs<LiveMessage.Tick>(LiveSocket.decode("""{"type":"tick","symbol":"BTC","kind":"crypto","price":60000.5,"sources":["OKX"],"time":1700000000000}"""))
        assertEquals("crypto:BTC" to 60000.5, t.tick.key to t.tick.price)
        val a = assertIs<LiveMessage.Alerts>(LiveSocket.decode(
            """{"type":"alerts","checkedAt":1700000000000,"items":[{"symbol":"AAPL","kind":"stock","name":"Apple","price":212.4,"buy":true,"key":"zone:medium","title":"AAPL"},{"symbol":"PEPE","kind":"crypto","name":"Pepe","error":"indisponible"}]}""",
        ))
        assertEquals(listOf("stock:AAPL", "crypto:PEPE"), a.items.map { it.id })
        assertEquals(listOf(true, false), a.items.map { it.buy })
        assertEquals(1_700_000_000_000.0, a.checkedAt)
        val v = assertIs<LiveMessage.Verdict>(LiveSocket.decode("""{"type":"verdict","symbol":"ETH","kind":"crypto","verdict":"wait","label":"ATTENDRE","chipNote":"zone plus bas"}"""))
        assertEquals("crypto:ETH" to "zone plus bas", v.verdict.key to v.verdict.chipNote)
        assertEquals(LiveMessage.Fx(0.91, 7.0), LiveSocket.decode("""{"type":"fx","rate":0.91,"asOf":7}"""))
        assertNull(LiveSocket.decode("""{"type":"fx","rate":0}"""))
        assertEquals(LiveMessage.Checked(9.0), LiveSocket.decode("""{"type":"checked","checkedAt":9}"""))
        assertEquals(LiveMessage.Pong, LiveSocket.decode("""{"type":"pong","time":1}"""))
        assertEquals(LiveMessage.Error("too_many_assets", "20 actifs au plus"), LiveSocket.decode("""{"type":"error","code":"too_many_assets","message":"20 actifs au plus"}"""))
        assertEquals(LiveMessage.Other, LiveSocket.decode("""{"type":"newer"}"""))
        assertNull(LiveSocket.decode("[1]"))
        assertNull(LiveSocket.decode("nope"))
    }

    @Test fun subscribeBackoffSilence() {
        val m = Json.parseToJsonElement(LiveSocket.subscribe(listOf(Asset("BTC", Kind.CRYPTO, "Bitcoin"), Asset("AAPL", Kind.STOCK, "Apple")), usd = false)).jsonObject
        assertEquals("subscribe", m["type"]!!.jsonPrimitive.content)
        assertEquals("EUR", m["currency"]!!.jsonPrimitive.content)
        assertEquals("BTC", m["assets"]!!.jsonArray[0].jsonObject["symbol"]!!.jsonPrimitive.content)
        val many = (0 until 25).map { Asset("C$it", Kind.CRYPTO, "C$it") }
        assertEquals(20, (Json.parseToJsonElement(LiveSocket.subscribe(many, usd = true)) as JsonObject)["assets"]!!.jsonArray.size)
        assertEquals(listOf(1_000L, 2_000L, 4_000L, 8_000L, 16_000L, 30_000L, 30_000L), (0 until 7).map(LiveSocket::backoffMs))
        assertTrue(LiveSocket.isLive(1_000, 30_000))
        assertFalse(LiveSocket.isLive(1_000, 50_000))
        assertFalse(LiveSocket.isLive(null, 1))
    }

    @Test fun socketRequestAndExchange() = runBlocking {
        val c = AltimClient("https://altim.example/".toHttpUrl(), null, "abc.def")
        val r = c.liveSocketRequest()
        assertEquals("https://altim.example/api/ws", r.url.toString()) // OkHttp shows wss as https
        assertEquals("altim_session=abc.def", r.header("Cookie"))
        assertNull(r.header("Origin"))

        val server = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse.Builder().webSocketUpgrade(object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    webSocket.send("""{"type":"hello","v":1}""")
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    if (text.contains("subscribe")) {
                        webSocket.send("""{"type":"tick","symbol":"BTC","kind":"crypto","price":60000,"time":1}""")
                        webSocket.send("""{"type":"alerts","checkedAt":2,"items":[{"symbol":"BTC","kind":"crypto","buy":true,"key":"zone"}]}""")
                    }
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    webSocket.close(1000, null)
                }
            }).build())
            var opened = false
            val client = AltimClient(server.url("/"), null, "abc.def")
            val got = withTimeout(10_000) { client.liveMessages(listOf(Asset("BTC", Kind.CRYPTO, "Bitcoin")), usd = false) { opened = true }.take(3).toList() }
            assertIs<LiveMessage.Hello>(got[0])
            assertEquals(60000.0, assertIs<LiveMessage.Tick>(got[1]).tick.price)
            assertTrue(assertIs<LiveMessage.Alerts>(got[2]).items.single().buy)
            assertTrue(opened)
            val upgrade = server.takeRequest()
            assertEquals("/api/ws", upgrade.url.encodedPath)
            assertEquals("altim_session=abc.def", upgrade.headers["Cookie"])
        } finally {
            server.close()
        }
    }
}
