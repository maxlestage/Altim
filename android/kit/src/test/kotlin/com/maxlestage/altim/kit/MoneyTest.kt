package com.maxlestage.altim.kit

import java.time.ZoneId
import java.time.ZonedDateTime
import kotlin.math.abs
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Same cases as web/test/money.test.ts: display currency, /api/fx, saved amounts and their currency. */
class MoneyTest {
    private val nbsp = " "
    private val nnbsp = " "
    private val t0 = ZonedDateTime.of(2026, 9, 29, 12, 5, 0, 0, ZoneId.of("UTC")).toInstant().toEpochMilli()
    private val fx = FxRate(0.88, 1 / 0.88, t0, "Yahoo Finance", System.currentTimeMillis(), false)

    private fun close(expected: Double, actual: Double?, eps: Double = 1e-6) =
        assertTrue(actual != null && abs(expected - actual) < eps, "attendu $expected, obtenu $actual")

    @AfterTest
    fun reset() = Money.set(Currency.USD, null)

    // ---------- display currency ----------

    @Test
    fun dollarsByDefaultAndWithoutRate() {
        assertEquals(Currency.USD, Money.displayCurrency())
        assertEquals("212,40$nbsp$", Money.money(212.4))
        Money.set(Currency.EUR, null)
        assertEquals(Currency.USD, Money.displayCurrency())
        assertEquals("212,40$nbsp$", Money.price(212.4))
        assertTrue(Money.convert(100.0, Currency.EUR, Currency.USD).isNaN())
        assertNull(Money.fxLine(null))
        assertEquals("Taux EUR/USD indisponible : montants affichés en $.", Money.note())
    }

    @Test
    fun eurosWithRateEveryFormatterConverts() {
        Money.set(Currency.EUR, fx)
        assertEquals(Currency.EUR, Money.displayCurrency())
        assertEquals("212,40$nbsp€", Money.money(241.3636))
        assertEquals("0,4400$nbsp€", Money.price(0.5))
        assertEquals("8,8${nbsp}Md€", Money.compact(1e10))
        assertEquals("88$nnbsp€", Guidance.usd(100.0))
        assertEquals("1,76 Md€", Format.compactUsd(2e9))
        close(88.0, Money.toDisplay(100.0), 1e-10)
        close(100.0, Money.fromDisplay(88.0), 1e-10)
        val line = assertNotNull(Money.fxLine(fx, fx.time))
        assertTrue(Regex("^1 \\$ = 0,880 € · Yahoo Finance, \\d{2}:\\d{2}$").matches(line), line)
        // The same formatters the screens use.
        assertEquals("212,40 €", Format.price(241.3636))
        assertEquals("880 €", Format.money(1000.0))
        assertEquals("420 €", Opportunities.compactUsd(477.2727))
        assertEquals("≥ 8,8 Md€", Opportunities.CAPS[1].first)
    }

    @Test
    fun dollarsChosenNoConversionEvenWithRate() {
        Money.set(Currency.USD, fx)
        assertEquals("100,00$nbsp$", Money.money(100.0))
        assertEquals(100.0, Money.fromDisplay(100.0))
        assertNull(Money.note())
    }

    @Test
    fun rateLineOfSettings() {
        val r = FxRate(0.8819, 1 / 0.8819, ZonedDateTime.of(2026, 9, 29, 17, 15, 0, 0, ZoneId.of("Europe/Paris")).toInstant().toEpochMilli(), "Yahoo Finance", 0, false)
        assertEquals("1 $ = 0,8819 € · Yahoo Finance, 17:15", Money.fxLine(r, r.time + 60_000))
        assertEquals("1 $ = 0,8819 € · Yahoo Finance, 29 sept. 17:15", Money.fxLine(r, r.time + 86_400_000))
        Money.set(Currency.EUR, r.copy(stale = true))
        assertEquals("1 $ = 0,8819 € · Yahoo Finance, 17:15 (dernier taux connu)", Money.note(r.time))
    }

    // ---------- /api/fx ----------

    @Test
    fun validRateKeptMissingOneNeverReplacedByDefault() {
        val body = FxResponse("USD", "EUR", rate = 0.88, usdPerEur = 1.136, time = 1.0, source = "BCE", fetchedAt = 2.0, stale = false)
        assertEquals(0.88, Fx.parse(body)?.rate)
        assertNull(Fx.parse(body.copy(rate = null, error = "taux indisponible")))
        assertNull(Fx.parse(body.copy(rate = 88.0)))
        assertEquals(true, Fx.saved(Fx.encode(body), 2 + 3_600_000)?.stale)
        assertNull(Fx.saved(Fx.encode(body), 2 + 8 * 86_400_000L))
        assertNull(Fx.saved("{", 0))
        // A real answer of the server.
        val real = """{"base":"USD","quote":"EUR","rate":0.8819,"usdPerEur":1.1339,"time":1790694900000,"source":"Yahoo Finance","fetchedAt":1790695000000,"stale":false}"""
        assertEquals("Yahoo Finance", Fx.parse(AltimJson.decodeFromString(FxResponse.serializer(), real))?.source)
        val none = """{"base":"USD","quote":"EUR","rate":null,"usdPerEur":null,"time":null,"source":null,"fetchedAt":null,"stale":false,"error":"taux indisponible"}"""
        assertNull(Fx.parse(AltimJson.decodeFromString(FxResponse.serializer(), none)))
    }

    // ---------- saved amounts and their currency ----------

    private val apple = Asset("AAPL", Kind.STOCK, "Apple")
    private val btc = Asset("BTC", Kind.CRYPTO, "Bitcoin")
    private val holdings = listOf(
        // Old line: no tag = dollars.
        Holding("a", apple, 2.0, averagePrice = 150.0, stop = 140.0),
        Holding("b", btc, 0.1, averagePrice = 44_000.0, stop = 40_000.0, costCurrency = Currency.EUR, stopCurrency = Currency.EUR),
    )

    @Test
    fun euroCostsGoToEnginesInDollarsAndPnlBackInEurosIsPriceMinusCost() {
        Money.set(Currency.EUR, fx)
        val u = Holdings.toUsd(holdings)
        assertEquals(emptyList(), u.unconverted)
        assertEquals(150.0, u.holdings[0].averagePrice)
        close(50_000.0, u.holdings[1].averagePrice)
        close(40_000 / 0.88, u.holdings[1].stop)
        // BTC at 60 000 $ = 52 800 €: gain 0,1 × (52 800 − 44 000) = 880 €.
        val pnlUsd = 0.1 * (60_000 - u.holdings[1].averagePrice!!)
        close(880.0, Money.toDisplay(pnlUsd))
        // The average cost sent to the server (cost=) is in dollars.
        close(50_000.0, Decision.cost(u.holdings, btc))
        assertEquals("Prix de revient saisi en $, converti au taux du jour.", Holdings.costNote(holdings[0]))
        assertNull(Holdings.costNote(holdings[1]))
    }

    @Test
    fun withoutRateEuroAmountsAreFlaggedNeverReadAsDollars() {
        Money.set(Currency.EUR, null)
        val u = Holdings.toUsd(holdings)
        assertEquals(listOf("BTC"), u.unconverted)
        assertNull(u.holdings[1].averagePrice)
        assertNull(u.holdings[1].stop)
        assertEquals(150.0, u.holdings[0].averagePrice)
        assertNull(Decision.cost(u.holdings, btc))
    }

    @Test
    fun newPurchaseInEurosMergesDollarCostAtCurrentRate() {
        Money.set(Currency.EUR, fx)
        val m = assertNotNull(Holdings.mergeLine(holdings[0], 2.0, 200.0, Currency.EUR))
        assertEquals(Currency.EUR, m.costCurrency)
        assertEquals(4.0, m.quantity)
        close((2 * 150 * 0.88 + 2 * 200) / 4, m.averagePrice, 1e-9)
        Money.set(Currency.EUR, null)
        assertNull(Holdings.mergeLine(holdings[0], 1.0, 200.0, Currency.EUR))
    }

    @Test
    fun oldSavedHoldingsReadAsDollars() {
        val old = """[{"id":"a","asset":{"symbol":"AAPL","kind":"stock","name":"Apple"},"quantity":2,"averagePrice":150,"stop":140}]"""
        val h = AltimJson.decodeFromString(kotlinx.serialization.builtins.ListSerializer(Holding.serializer()), old).single()
        assertNull(h.costCurrency)
        assertEquals(Currency.USD, Currency.stored(h.costCurrency))
        val back = AltimJson.encodeToString(Holding.serializer(), h.copy(costCurrency = Currency.EUR))
        assertTrue(back.contains("\"costCurrency\":\"EUR\""), back)
    }

    @Test
    fun selectionBudgetBareNumberIsDollarsNewFormatCarriesCurrency() {
        assertEquals(Money.Typed(5000.0, Currency.USD), Money.parseBudget("5000"))
        assertEquals(Money.Typed(4000.0, Currency.EUR), Money.parseBudget("""{"amount":4000,"currency":"EUR"}"""))
        assertNull(Money.parseBudget("-3"))
        assertNull(Money.parseBudget(null))
        assertEquals(Money.Typed(4000.0, Currency.EUR), Money.parseBudget(Money.encodeBudget(Money.Typed(4000.0, Currency.EUR))))
    }

    // ---------- price alerts ----------

    @Test
    fun priceAlertInEurosComparedWithConvertedDollarPrice() {
        Money.set(Currency.EUR, fx)
        val t = PriceTarget(asset = btc, above = false, price = 70_000.0, currency = Currency.EUR)
        assertEquals("En dessous de 70${nnbsp}000,00 €", t.label)
        // 80 000 $ = 70 400 € (not reached), 79 000 $ = 69 520 € (reached).
        assertTrue(!t.isReached(80_000.0))
        assertTrue(t.isReached(79_000.0))
        // Without a rate a euro alert waits; an old dollar alert keeps working.
        Money.set(Currency.EUR, null)
        assertTrue(!t.isReached(1.0))
        assertTrue(PriceTarget(asset = btc, above = false, price = 80_000.0).isReached(79_000.0))
        Money.set(Currency.EUR, fx)
        val move = PriceTarget(asset = btc, above = true, price = 100.0, move = 5.0, currency = Currency.EUR)
        close(88.0, move.rearmed(100.0).price)
    }
}
