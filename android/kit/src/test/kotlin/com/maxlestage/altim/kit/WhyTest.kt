package com.maxlestage.altim.kit

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** « Pourquoi ça bouge ? »: the /api/why contract (a sample shaped like backend/src/engine/why.rs) and the /api/ask call. */
class WhyTest {
    private val raw = requireNotNull(javaClass.getResource("/fixtures/why-sample.json")).readText()

    @Test fun decodesAndLabels() {
        val r = AltimJson.decodeFromString(WhyReport.serializer(), raw)
        assertEquals(4, r.factors.size)
        assertTrue(r.askEnabled)
        assertEquals("↘", Why.directionIcon("down"))
        assertEquals("baissier", Why.directionWord("down"))
        assertEquals("moyen", Why.magnitudeWord("medium"))
        assertEquals("Non couvert : financement des contrats perpétuels (Pas de contrats perpétuels pour une action).", Why.notCoveredText(r))
        assertTrue(!Why.canAsk("  a "))
        assertTrue(Why.canAsk("Pourquoi ?"))
        // An older or minimal answer still decodes (every added field optional).
        val min = AltimJson.decodeFromString(WhyReport.serializer(), """{"symbol":"BTC"}""")
        assertTrue(!min.askEnabled && min.factors.isEmpty())
    }

    @Test fun whyAndAskAddresses() = runBlocking {
        val server = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse.Builder().code(200).body(raw).build())
            server.enqueue(MockResponse.Builder().code(200).body("""{"answer":"Surtout le marché.","model":"m","question":"q","data":{"derniereDecision":{"label":"ATTENDRE"}},"disclaimer":"d"}""").build())
            server.enqueue(MockResponse.Builder().code(503).body("""{"error":"Questions indisponibles : aucune clé configurée sur le serveur."}""").build())
            val c = AltimClient(server.url("/"), null)
            val a = Asset("NVDA", Kind.STOCK, "Nvidia")
            assertEquals("NVDA", c.why(a).symbol)
            val get = server.takeRequest()
            assertEquals("/api/why", get.url.encodedPath)
            assertEquals("stock", get.url.queryParameter("kind"))
            val answer = c.ask(a, "La baisse vient-elle du marché ?")
            assertEquals("Surtout le marché.", answer.answer)
            assertEquals("Données utilisées : les observations ci-dessus et la dernière décision calculée.", Why.answerDataText(answer))
            val post = server.takeRequest()
            assertEquals("POST", post.method)
            assertEquals("/api/ask", post.url.encodedPath)
            assertTrue(post.headers["Content-Type"]!!.startsWith("application/json"))
            assertEquals("""{"symbol":"NVDA","kind":"stock","question":"La baisse vient-elle du marché ?"}""", post.body!!.utf8())
            // No key on the server: its French message is shown.
            val e = runCatching { c.ask(a, "Encore ?") }.exceptionOrNull()
            assertEquals("Questions indisponibles : aucune clé configurée sur le serveur.", e?.message)
        } finally {
            server.close()
        }
    }
}
