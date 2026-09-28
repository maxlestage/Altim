package com.maxlestage.altim.kit

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.builtins.ListSerializer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.fail

/** Fixtures = real answers of the Altim server (September 2026), shared with the iPhone app's tests. */
private fun fixture(name: String): String =
    requireNotNull(KitTest::class.java.getResource("/fixtures/$name")) { name }.readText()

class KitTest {
    @Test fun radar() {
        val rows = AltimJson.decodeFromString(ListSerializer(RadarRow.serializer()), fixture("radar.json"))
        assertEquals(listOf("BTC", "AAPL"), rows.map { it.symbol })
        assertEquals(Action.BUY, rows[0].signal?.action)
        assertEquals("high", rows[0].reliability?.level)
        assertTrue((rows[0].sparkline?.size ?: 0) > 10)
    }

    @Test fun quotesAndSearch() {
        val q = AltimJson.decodeFromString(ListSerializer(Quote.serializer()), fixture("tickers.json"))
        assertTrue(q.isNotEmpty() && q[0].price > 0)
        val s = AltimJson.decodeFromString(ListSerializer(SearchItem.serializer()), fixture("search.json"))
        assertEquals(Asset("NVDA", Kind.STOCK, "NVIDIA Corporation"), s.first().asset)
    }

    @Test fun guardZonesMacro() {
        val g = AltimJson.decodeFromString(GuardReport.serializer(), fixture("guard.json"))
        assertEquals("Haussière", g.trendLabel)
        assertEquals("peu d'historique : compté à moitié", g.shock.factors.first().statusText)
        assertNotNull(g.macro)
        val z = AltimJson.decodeFromString(ZonesReport.serializer(), fixture("zones.json"))
        assertEquals(listOf("short", "medium", "long"), z.zones.map { it.horizon })
        assertEquals("Attendre le repli", z.zones[0].statusLabel)
        assertNotNull(z.zones[0].zone)
        assertTrue(z.zones[0].evidenceText.startsWith("Historique : sur 1 repli dans la zone"))
        val m = AltimJson.decodeFromString(MacroInfo.serializer(), fixture("macro.json"))
        assertEquals("Calme", m.levelLabel)
        assertNotNull(m.values["vix"])
    }

    @Test fun selectionAndAllocation() {
        val r = AltimJson.decodeFromString(SelectionReport.serializer(), fixture("sel.json"))
        assertEquals(Horizon.MO1, r.horizon)
        assertEquals(Kind.CRYPTO, r.market)
        assertEquals(Criterion.SIGNAL, r.orderedCriteria.first())
        assertEquals(5, r.orderedCriteria.toSet().size)
        assertNotNull(r.buy.first().plan)
        assertEquals("clear", r.validation?.edge)
        val amounts = r.allocate(10_000.0)
        assertEquals(r.buy.count { it.plan != null }, amounts.size)
        assertTrue(amounts.values.sum() <= 10_000.01)
        assertTrue(amounts.values.all { it <= 2000.0001 })
    }

    @Test fun candles() {
        val s = AltimJson.decodeFromString(Snapshot.serializer(), fixture("candles.json"))
        assertEquals(30, s.candles.size)
        assertTrue(s.agreeing > 10)
    }

    @Test fun horizonsMatchServer() {
        assertEquals(listOf("30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m"), Horizon.entries.map { it.raw })
    }
}

class AlertTest {
    @Test fun decodeRealAlerts() {
        val items = AltimJson.decodeFromString(ListSerializer(BuyAlert.serializer()), fixture("alerts.json"))
        assertEquals(listOf("BTC", "ETH", "AAPL", "ZZZZ"), items.map { it.symbol })
        assertTrue(items[0].buy)
        assertFalse(items[3].buy, "an unavailable asset is never buyable")
    }

    @Test fun trackerNotifiesOnlyOnChange() {
        fun a(s: String, buy: Boolean, strong: Boolean = false, key: String) = BuyAlert(s, Kind.CRYPTO, buy = buy, strong = strong, key = key)
        var t = AlertTracker()
        var clock = 0L
        fun step(items: List<BuyAlert>, onlyStrong: Boolean = false): List<String> {
            val (next, out) = t.newAlerts(items, onlyStrong, clock)
            t = next
            return out.map { it.symbol }
        }
        assertEquals(listOf("BTC"), step(listOf(a("BTC", true, key = "signal"), a("ETH", false, key = ""))))
        assertEquals(emptyList(), step(listOf(a("BTC", true, key = "signal"))), "same situation: no repeat")
        assertEquals(listOf("BTC"), step(listOf(a("BTC", true, true, "signal+zone:medium"))), "the zone is reached: new reason")
        assertEquals(emptyList(), step(listOf(a("BTC", true, key = "zone:medium"))), "a reason already notified")
        // The price flickers at the edge of the zone: no notification storm.
        clock += 60_000
        assertEquals(emptyList(), step(listOf(a("BTC", false, key = ""))))
        clock += 60_000
        assertEquals(emptyList(), step(listOf(a("BTC", true, key = "signal"))), "back within minutes: already notified")
        // Really gone for more than 6 hours, then buyable again: a new opportunity.
        clock += 60_000
        step(listOf(a("BTC", false, key = "")))
        clock += AlertTracker.COOLDOWN_MS + 1
        step(listOf(a("BTC", false, key = "")))
        assertEquals(listOf("BTC"), step(listOf(a("BTC", true, key = "signal"))), "buyable again after a real pause")
        t = AlertTracker()
        assertEquals(emptyList(), step(listOf(a("SOL", true, key = "signal")), onlyStrong = true))
        assertEquals(listOf("SOL"), step(listOf(a("SOL", true, true, "signal+zone:long")), onlyStrong = true))
    }
}

class PriceAlertTest {
    private val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
    private val aapl = Asset("AAPL", Kind.STOCK, "Apple")

    @Test fun targetsFireOnceWhenReached() {
        val below = PriceTarget(asset = btc, above = false, price = 80_000.0)
        val above = PriceTarget(asset = aapl, above = true, price = 350.0)
        var (list, fired) = PriceTarget.evaluate(listOf(below, above), mapOf("crypto:BTC" to 84_000.0, "stock:AAPL" to 341.0))
        assertTrue(fired.isEmpty())
        PriceTarget.evaluate(list, mapOf("crypto:BTC" to 79_900.0, "stock:AAPL" to 350.0)).let { (l, f) -> list = l; fired = f }
        assertEquals(listOf("AAPL", "BTC"), fired.map { it.first.asset.symbol }.sorted())
        assertEquals(79_900.0, fired.first { it.first.asset.symbol == "BTC" }.second)
        assertTrue(list.all { it.triggered != null })
        assertTrue(PriceTarget.evaluate(list, mapOf("crypto:BTC" to 70_000.0, "stock:AAPL" to 400.0)).second.isEmpty(), "never twice")
        assertTrue(PriceTarget.evaluate(listOf(below), emptyMap()).second.isEmpty())
        assertEquals("En dessous de 80\u202F000,00 $", below.label)
    }

    @Test fun journalSummaryIsHonest() {
        val now = System.currentTimeMillis()
        val old = now - 86_400_000
        val journal = AlertJournal.add(
            listOf(
                JournalEntry(asset = btc, source = JournalEntry.Source.BUY, title = "BTC", price = 80_000.0, date = old),
                JournalEntry(asset = aapl, source = JournalEntry.Source.STRONG_BUY, title = "AAPL", price = 400.0, date = old),
                JournalEntry(asset = btc, source = JournalEntry.Source.TARGET, title = "cible", price = 80_000.0, date = old),
                JournalEntry(asset = aapl, source = JournalEntry.Source.BUY, title = "trop récent", price = 300.0, date = now),
            ),
            emptyList(),
        )
        assertEquals("trop récent", journal.first().title)
        val s = AlertJournal.summary(journal, mapOf("crypto:BTC" to 88_000.0, "stock:AAPL" to 360.0), now)!!
        assertEquals(2, s.count)
        assertEquals(1, s.up)
        assertEquals(0.0, s.average, 1e-9)
        assertEquals(50.0, s.upShare)
        assertNull(AlertJournal.summary(emptyList(), emptyMap()))
        assertEquals(AlertJournal.LIMIT, AlertJournal.add(List(300) { journal[0].copy(id = "$it") }, emptyList()).size)
    }
}

class SseTest {
    @Test fun realStreamCutAnywhere() {
        val raw = fixture("live.txt").toByteArray()
        val whole = SseParser()
        val all = whole.ticks(raw)
        assertTrue(all.size >= 3)
        assertEquals(3000, whole.retryMs)
        assertEquals("crypto:BTC", all.first().key)
        assertTrue(all.any { it.market == "closed" })
        // Same stream delivered byte by byte: same ticks.
        val split = SseParser()
        val got = raw.flatMap { split.ticks(byteArrayOf(it)) }
        assertEquals(all.map { it.price }, got.map { it.price })
    }

    @Test fun commentsCrlfAndMultiline() {
        val p = SseParser()
        assertEquals(listOf("a\nb", "c"), p.feed(": ok\r\n\r\ndata: a\r\ndata: b\r\n\r\ndata:c\n\n".toByteArray()))
    }

    @Test fun hostileStreamStaysBounded() {
        val p = SseParser()
        p.feed(ByteArray(1_000_000) { 'A'.code.toByte() })
        assertEquals(listOf("ok"), p.feed("\ndata: ok\n\n".toByteArray()))
        val q = SseParser()
        val out = q.feed(("data: x\n".repeat(10_000) + "\n").toByteArray())
        assertTrue(out.all { it.split("\n").size <= 64 })
    }

    @Test fun utf8SplitInsideCharacter() {
        val p = SseParser()
        val out = "data: é€\n\n".toByteArray().flatMap { p.feed(byteArrayOf(it)) }
        assertEquals(listOf("é€"), out)
    }
}

class FormatAndClientTest {
    @Test fun frenchFormats() {
        assertEquals("84 518,16 $", Format.price(84518.16))
        assertEquals("0,3432 $", Format.price(0.3432))
        assertEquals("0,00001234 $", Format.price(0.00001234))
        assertEquals("—", Format.price(null))
        assertEquals("−0,40 %", Format.percent(-0.4))
        assertEquals("+1,25 %", Format.percent(1.25))
        assertEquals("1 500 $", Format.money(1500.0))
        assertEquals(10_000.0, Format.parse("10 000"))
        assertEquals(0.25, Format.parse("0,25"))
        assertNull(Format.parse("abc"))
    }

    @Test fun normalizeServer() {
        assertEquals("https://mon-app.herokuapp.com/", AltimClient.normalize("mon-app.herokuapp.com/").toString())
        assertEquals("https://x.herokuapp.com/", AltimClient.normalize(" https://x.herokuapp.com ").toString())
        assertEquals("http://10.0.2.2:4410/", AltimClient.normalize("http://10.0.2.2:4410", dev = true).toString())
        assertNull(AltimClient.normalize("http://10.0.2.2:4410"), "10.0.2.2 is a Wi-Fi address on a real phone")
        assertNull(AltimClient.normalize("http://altim.local"))
        assertNull(AltimClient.normalize("http://x.herokuapp.com"), "no plain http on the Internet: the password would travel in clear")
        assertNull(AltimClient.normalize("https://x.herokuapp.com/app"))
        assertNull(AltimClient.normalize(""))
    }

    @Test fun requests() {
        val c = AltimClient(AltimClient.normalize("x.herokuapp.com")!!, null)
        val r = c.request("/api/radar", mapOf("symbols" to AltimClient.list(Asset.defaults.take(2)), "interval" to "4h"))
        assertEquals("https://x.herokuapp.com/api/radar?interval=4h&symbols=BTC%3Acrypto%2CETH%3Acrypto", r.url.toString())
        assertEquals("q=a%2Bb", c.request("/api/search", mapOf("q" to "a+b")).url.encodedQuery)
    }

    @Test fun sessionCookieHeader() {
        assertEquals("abc.def-1", AltimClient.sessionCookie(listOf("altim_session=abc.def-1; Path=/; HttpOnly; SameSite=Strict; Max-Age=604800; Secure")))
        assertEquals("xyz", AltimClient.sessionCookie(listOf("other=1; Path=/", "altim_session=xyz; Path=/")))
        assertNull(AltimClient.sessionCookie(listOf("altim_session=; Path=/; Max-Age=0")))
        assertNull(AltimClient.sessionCookie(emptyList()))
    }

    @Test fun portfolio() {
        val btc = Holding(asset = Asset("BTC", Kind.CRYPTO, "Bitcoin"), quantity = 0.5, averagePrice = 60000.0)
        val aapl = Holding(asset = Asset("AAPL", Kind.STOCK, "Apple"), quantity = 10.0, averagePrice = 200.0)
        val p = Portfolio(listOf(aapl, btc), mapOf("crypto:BTC" to 80000.0, "stock:AAPL" to 300.0))
        assertEquals(43000.0, p.total)
        assertEquals(32000.0, p.cost)
        assertEquals(11000.0, p.gain)
        assertEquals("BTC", p.lines.first().holding.asset.symbol)
        assertEquals(33.333, p.lines.first().gainPercent!!, 0.01)
        assertEquals(40000.0 / 43000 * 100, p.cryptoShare, 0.001)
        assertEquals(2, p.warnings.size)
        val partial = Portfolio(listOf(aapl, btc), mapOf("stock:AAPL" to 300.0))
        assertEquals(3000.0, partial.total)
        assertTrue(partial.warnings.any { "BTC" in it })
    }
}

/** Against a running server: ALTIM_SERVER=http://localhost:4410 ALTIM_USER=… ALTIM_PASSWORD=… gradle :kit:test */
class LiveServerTest {
    @Test fun loginAndApi() = runBlocking {
        val server = System.getenv("ALTIM_SERVER") ?: return@runBlocking println("ALTIM_SERVER absent : test de bout en bout ignoré")
        val url = requireNotNull(AltimClient.normalize(server))
        val creds = System.getenv("ALTIM_USER")?.let { Credentials(it, System.getenv("ALTIM_PASSWORD") ?: "") }
        val anonymous = AltimClient(url, null)
        val mode = anonymous.accessMode()
        if (mode is AccessMode.Login) {
            val c = requireNotNull(creds)
            assertFailsWith<AltimException.Unauthorized> { anonymous.macro() }
            val e = runCatching { anonymous.login(Credentials(c.user, c.password + "x")) }.exceptionOrNull()
            assertEquals(AltimException.WrongCredentials, e)
        }
        // No cookie yet: logged in automatically on the first 401.
        val client = AltimClient(url, creds)
        val rows = client.radar(Asset.defaults)
        assertEquals(Asset.defaults.size, rows.size)
        if (mode is AccessMode.Login) assertNotNull(client.sessionCookie)
        // The saved cookie is enough for a new client (next launch), without credentials.
        AltimClient(url, null, client.sessionCookie).macro()
        assertEquals(3, client.zones(Asset.defaults[0]).zones.size)
        client.guardReport(Asset.defaults[5])
        assertFalse(client.search("sol").isEmpty())
        // Live prices: several ticks within a few seconds, for the assets asked.
        val ticks = withTimeout(30_000) { client.liveTicks(listOf(Asset.defaults[0], Asset.defaults[5])).take(5).toList() }
        assertTrue(ticks.all { it.key in setOf("crypto:BTC", "stock:AAPL") && it.price > 0 })
        // Without session, the stream says so instead of hanging.
        if (mode is AccessMode.Login) {
            try {
                withTimeout(15_000) { AltimClient(url, null).liveTicks(listOf(Asset.defaults[0])).toList() }
                fail("live stream open without login")
            } catch (e: AltimException.Unauthorized) {
                // expected
            }
        }
        assertTrue(client.candles(Asset.defaults[0], "1d").candles.size > 100)
    }
}
