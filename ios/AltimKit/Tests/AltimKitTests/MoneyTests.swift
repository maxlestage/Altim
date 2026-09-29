import XCTest
@testable import AltimKit

/// Same cases as the web (web/test/money.test.ts): display currency, /api/fx, saved amounts and their currency.
final class MoneyTests: XCTestCase {
    let nnbsp = "\u{202F}"
    let nbsp = "\u{00A0}"
    // 29 September 2026, 12:05 UTC.
    let fx = FxRate(rate: 0.88, usdPerEur: 1 / 0.88, time: 1_790_683_500_000, source: "Yahoo Finance",
                    fetchedAt: Date().timeIntervalSince1970 * 1000, stale: false)

    override func tearDown() {
        Money.setDisplay(.usd, nil)
        super.tearDown()
    }

    // MARK: Display currency

    func testDollarsByDefaultAndWithoutARate() {
        XCTAssertEqual(Money.displayCurrency, .usd)
        XCTAssertEqual(Money.money(212.4), "212,40\(nbsp)$")
        Money.setDisplay(.eur, nil)
        XCTAssertEqual(Money.displayCurrency, .usd)
        XCTAssertEqual(Money.price(212.4), "212,40\(nbsp)$")
        XCTAssertTrue(Money.convert(100, from: .eur, to: .usd).isNaN)
        XCTAssertNil(Money.fxLine(nil))
        XCTAssertEqual(Money.note(), "Taux EUR/USD indisponible : montants affichés en $.")
    }

    func testEurosWithARateEveryFormatterConverts() throws {
        Money.setDisplay(.eur, fx)
        XCTAssertEqual(Money.displayCurrency, .eur)
        XCTAssertEqual(Money.money(241.3636), "212,40\(nbsp)€")
        XCTAssertEqual(Money.price(0.5), "0,4400\(nbsp)€")
        XCTAssertEqual(Money.compact(1e10), "8,8\(nbsp)Md€")
        XCTAssertEqual(DecisionText.usd(100), "88\(nnbsp)€")
        XCTAssertEqual(DecisionText.usdCompact(2e9), "1,76\(nnbsp)Md€")
        XCTAssertEqual(Money.toDisplay(100), 88, accuracy: 1e-10)
        XCTAssertEqual(Money.fromDisplay(88), 100, accuracy: 1e-10)
        let line = try XCTUnwrap(Money.fxLine(fx, now: Date(timeIntervalSince1970: fx.time / 1000)))
        XCTAssertNotNil(line.range(of: #"^1 \$ = 0,880 € · Yahoo Finance, \d{2}:\d{2}$"#, options: .regularExpression), line)
        XCTAssertEqual(Money.fxLine(fx, now: Date(timeIntervalSince1970: fx.time / 1000), timeZone: TimeZone(identifier: "Europe/Paris")!),
                       "1 $ = 0,880 € · Yahoo Finance, 14:05")
        // The other formatters of the app follow the same currency.
        XCTAssertEqual(Format.price(100), "88,00 €")
        XCTAssertEqual(Format.large(1e10), "8,8 Md€")
        XCTAssertEqual(Opportunities.compactUsd(1e6), "880 k€")
        XCTAssertEqual(WhatIf.usd(1000), "880 €")
        XCTAssertEqual(RiskText.usd(-1000), "880 €")
        XCTAssertEqual(Opportunities.caps[1].label, "≥ 8,8\(nbsp)Md€")
    }

    func testDollarsChosenNoConversionEvenWithARate() {
        Money.setDisplay(.usd, fx)
        XCTAssertEqual(Money.money(100), "100,00\(nbsp)$")
        XCTAssertEqual(Money.fromDisplay(100), 100)
        XCTAssertNil(Money.note())
        XCTAssertEqual(AltimClient.currencyQuery(), ["cur": "USD"])
        Money.setDisplay(.eur, fx)
        XCTAssertEqual(AltimClient.currencyQuery(), [:])
    }

    // MARK: /api/fx

    func testAValidRateIsKeptAMissingOneNeverReplacedByADefault() throws {
        let body = #"{"base":"USD","quote":"EUR","rate":0.88,"usdPerEur":1.136,"time":1,"source":"BCE","fetchedAt":2,"stale":false}"#
        XCTAssertEqual(Fx.parse(Data(body.utf8))?.rate, 0.88)
        let missing = #"{"base":"USD","quote":"EUR","rate":null,"usdPerEur":null,"time":null,"source":null,"fetchedAt":null,"stale":false,"error":"taux indisponible"}"#
        XCTAssertNil(Fx.parse(Data(missing.utf8)))
        XCTAssertNil(Fx.parse(Data(body.replacingOccurrences(of: "0.88", with: "88").utf8)))
        XCTAssertEqual(Fx.saved(Data(body.utf8), now: Date(timeIntervalSince1970: (2 + 3_600_000) / 1000))?.stale, true)
        XCTAssertNil(Fx.saved(Data(body.utf8), now: Date(timeIntervalSince1970: (2 + 8 * 86_400_000) / 1000)))
        XCTAssertNil(Fx.saved(Data("{".utf8)))
    }

    // MARK: Saved amounts and their currency

    let aapl = Asset(symbol: "AAPL", kind: .stock, name: "Apple")
    let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
    var stored: [Holding] {
        [
            // Old line: no tag = dollars.
            Holding(asset: aapl, quantity: 2, averagePrice: 150, stop: 140),
            Holding(asset: btc, quantity: 0.1, averagePrice: 44_000, stop: 40_000, costCurrency: .eur, stopCurrency: .eur),
        ]
    }

    func testEuroCostsGoToTheEnginesInDollarsAndThePnLBackInEurosIsPriceMinusCost() throws {
        Money.setDisplay(.eur, fx)
        let u = UsdHoldings(stored)
        XCTAssertEqual(u.unconverted, [])
        XCTAssertEqual(u.holdings[0].averagePrice, 150)
        XCTAssertEqual(try XCTUnwrap(u.holdings[1].averagePrice), 50_000, accuracy: 1e-6)
        XCTAssertEqual(try XCTUnwrap(u.holdings[1].stop), 40_000 / 0.88, accuracy: 1e-6)
        XCTAssertNil(u.holdings[1].costCurrency)
        // BTC at 60 000 $ = 52 800 €: gain 0,1 × (52 800 − 44 000) = 880 €.
        let pnlUsd = 0.1 * (60_000 - u.holdings[1].averagePrice!)
        XCTAssertEqual(Money.toDisplay(pnlUsd), 880, accuracy: 1e-6)
        let p = Portfolio(holdings: u.holdings, prices: [btc.id: 60_000])
        XCTAssertEqual(Money.toDisplay(try XCTUnwrap(p.lines.first { $0.holding.asset.id == btc.id }?.gain)), 880, accuracy: 1e-6)
        XCTAssertEqual(stored[0].costNote, "Prix de revient saisi en $, converti au taux du jour.")
        XCTAssertNil(stored[1].costNote)
    }

    func testWithoutARateEuroAmountsAreFlaggedNeverReadAsDollars() {
        Money.setDisplay(.eur, nil)
        let u = UsdHoldings(stored)
        XCTAssertEqual(u.unconverted, ["BTC"])
        XCTAssertNil(u.holdings[1].averagePrice)
        XCTAssertNil(u.holdings[1].stop)
        XCTAssertEqual(u.holdings[0].averagePrice, 150)
    }

    func testOldSavedLinesDecodeInDollarsAndTagsRoundTrip() throws {
        let old = #"[{"id":"6F1C1C4E-8E3B-4C39-9C3B-2B7B0B0F0A01","asset":{"symbol":"AAPL","kind":"stock","name":"Apple"},"quantity":2,"averagePrice":150}]"#
        let h = try JSONDecoder().decode([Holding].self, from: Data(old.utf8))
        XCTAssertNil(h[0].costCurrency)
        XCTAssertEqual(Money.stored(h[0].costCurrency), .usd)
        let again = try JSONDecoder().decode([Holding].self, from: JSONEncoder().encode(stored))
        XCTAssertEqual(again[1].costCurrency, .eur)
        XCTAssertEqual(again[1].stopCurrency, .eur)
        let unknown = old.replacingOccurrences(of: #""averagePrice":150"#, with: #""averagePrice":150,"costCurrency":"GBP""#)
        XCTAssertNil(try JSONDecoder().decode([Holding].self, from: Data(unknown.utf8))[0].costCurrency)
    }

    func testSelectionBudgetABareNumberIsDollarsTheNewFormatCarriesItsCurrency() {
        XCTAssertEqual(StoredAmount.read(amount: 5000, currency: nil), StoredAmount(amount: 5000, currency: .usd))
        XCTAssertEqual(StoredAmount.read(amount: 4000, currency: "EUR"), StoredAmount(amount: 4000, currency: .eur))
        XCTAssertNil(StoredAmount.read(amount: -3, currency: nil))
        XCTAssertNil(StoredAmount.read(amount: nil, currency: "EUR"))
    }

    func testPriceAlertsCompareInTheirOwnCurrency() {
        Money.setDisplay(.eur, fx)
        let below = PriceTarget(asset: btc, above: false, price: 50_000, currency: .eur)
        // 56 000 $ = 49 280 € ≤ 50 000 €; 60 000 $ = 52 800 € is not.
        XCTAssertTrue(below.isReached(by: 56_000))
        XCTAssertFalse(below.isReached(by: 60_000))
        XCTAssertEqual(below.label, "En dessous de 50\(nnbsp)000,00 €")
        // An old alert (no tag) stays in dollars.
        XCTAssertTrue(PriceTarget(asset: btc, above: false, price: 57_000).isReached(by: 56_000))
        // Without a rate a euro threshold waits, never compared with dollars.
        Money.setDisplay(.eur, nil)
        XCTAssertFalse(below.isReached(by: 1))
        Money.setDisplay(.eur, fx)
        let move = PriceTarget(asset: btc, above: false, price: 1, move: 5, currency: .eur).rearmed(at: 100)
        XCTAssertEqual(move.price, 88, accuracy: 1e-9)
    }

    func testTypedNumbers() {
        XCTAssertEqual(Money.parse("1 234,5"), 1234.5)
        XCTAssertEqual(Money.parse("80\u{202F}000 €"), 80_000)
        XCTAssertNil(Money.parse("abc"))
        XCTAssertEqual(Money.inputText(212.4), "212,4")
        XCTAssertEqual(Money.inputText(.nan), "")
    }
}
