import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/whatif.test.ts (same pseudo-random series, same portfolio, same expected values).
final class WhatIfTests: XCTestCase {
    let day = 86_400_000.0
    /// Date.UTC(2026, 0, 5): a Monday.
    let t0 = 1_767_571_200_000.0

    func series(_ closes: [Double], _ times: [Double]) -> [Candle] {
        closes.enumerated().map { Candle(time: times[$0.offset], open: $0.element, high: $0.element, low: $0.element, close: $0.element, volume: 1) }
    }

    func returns(_ n: Int, _ seed: Int) -> [Double] {
        var x = seed
        return (0..<n).map { _ in
            x = (x * 16807) % 2_147_483_647
            return (Double(x) / 2_147_483_647 - 0.5) * 0.04
        }
    }

    func path(_ r: [Double]) -> [Double] { r.reduce(into: [100.0]) { $0.append($0.last! * (1 + $1)) } }
    func everyDay(_ n: Int) -> [Double] { (0..<n).map { t0 + Double($0) * day } }
    func weekdays(_ n: Int) -> [Double] {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        return Array(everyDay(Int((Double(n) * 1.5).rounded(.up))).filter { ![1, 7].contains(cal.component(.weekday, from: Date(timeIntervalSince1970: $0 / 1000))) }.prefix(n))
    }

    func testBetaMeasuredOnSharedDaysOnly() {
        let days = weekdays(121)
        let q = returns(120, 11)
        let factor = series(path(q), days)
        // The crypto moves 1.5 × QQQ on weekdays and has extra weekend candles with big moves.
        let cryptoTimes = everyDay(Int((121 * 1.5).rounded(.up)))
        let scaled = path(q.map { 1.5 * $0 })
        let byDay = Dictionary(uniqueKeysWithValues: days.enumerated().map { ($0.element, scaled[$0.offset]) })
        var last = 100.0
        let crypto = series(cryptoTimes.map { t in
            if let v = byDay[t] {
                last = v
                return v
            }
            return last * 1.03
        }, cryptoTimes)
        let b = WhatIf.factorBeta(crypto, factor)
        XCTAssertTrue(b.estimated)
        XCTAssertEqual(b.days, 90)
        XCTAssertEqual(b.beta, 1.5, accuracy: 1e-6)
        XCTAssertEqual(b.correlation ?? 0, 1, accuracy: 1e-6)
        // "Et si… ?" uses one year of shared sessions.
        XCTAssertEqual(WhatIf.factorBeta(crypto, factor, days: WhatIf.betaDays).days, 120)
    }

    func testUnder30SharedDaysNotEstimated() {
        let t = everyDay(20)
        let b = WhatIf.factorBeta(series(path(returns(19, 3)), t), series(path(returns(19, 5)), t))
        XCTAssertFalse(b.estimated)
        XCTAssertEqual(b.days, 19)
        XCTAssertEqual(RiskEngine.minBetaDays, 30)
    }

    func holding(_ symbol: String, _ kind: Kind, _ quantity: Double) -> Holding {
        Holding(asset: Asset(symbol: symbol, kind: kind, name: symbol), quantity: quantity, averagePrice: 100)
    }

    var portfolio: RiskPortfolio {
        RiskPortfolio(holdings: [holding("NVDA", .stock, 30), holding("NVDA", .stock, 10), holding("QQQ", .stock, 20), holding("DOGE", .crypto, 20)],
                      cash: 2_000, prices: ["stock:NVDA": 100, "stock:QQQ": 100, "crypto:DOGE": 100], daily: [:])
    }

    func testEachLineMovesByItsBetaUncoveredNeverGuessed() throws {
        let r = WhatIf.run(portfolio, factor: .qqq, shock: -10, betas: [
            "stock:NVDA": FactorBeta(beta: 2, days: 90, estimated: true, correlation: 0.8),
            "crypto:DOGE": FactorBeta(beta: 1, days: 12, estimated: false, correlation: nil),
        ])
        XCTAssertEqual(r.base, 10_000)
        let nvda = try XCTUnwrap(r.lines.first { $0.symbol == "NVDA" })
        XCTAssertEqual(nvda.value, 4_000)
        XCTAssertEqual(nvda.movePercent, -20)
        XCTAssertEqual(nvda.loss, 800)
        XCTAssertEqual(nvda.beta, 2)
        let qqq = try XCTUnwrap(r.lines.first { $0.symbol == "QQQ" })
        XCTAssertTrue(qqq.reference)
        XCTAssertEqual(qqq.loss, 200)
        let doge = try XCTUnwrap(r.lines.first { $0.symbol == "DOGE" })
        XCTAssertNil(doge.loss)
        XCTAssertNil(doge.movePercent)
        XCTAssertEqual(r.loss, 1_000)
        XCTAssertEqual(r.lossPercent, 10)
        XCTAssertEqual(r.uncovered, ["DOGE"])
        XCTAssertEqual(r.uncoveredValue, 2_000)
        XCTAssertEqual(r.worst?.symbol, "NVDA")
        XCTAssertEqual(r.minDays, 90)
        XCTAssertEqual(r.maxDays, 90)
        // Texts of the card.
        XCTAssertEqual(WhatIf.lineDetail(nvda, factor: .qqq), "4\u{202F}000 $ · 40 % · bêta 2.00, corrélation 0.80 · −20 %")
        XCTAssertEqual(WhatIf.lineDetail(qqq, factor: .qqq), "2\u{202F}000 $ · 20 % · c'est ce marché lui-même (bêta 1)")
        XCTAssertEqual(WhatIf.lineDetail(doge, factor: .qqq), "2\u{202F}000 $ · 20 % · moins de 30 jours communs avec Nasdaq-100 (QQQ) : bêta non mesurable")
        XCTAssertEqual(WhatIf.signedUsd(r.loss), "−1\u{202F}000 $")
        XCTAssertTrue(WhatIf.footnote(r).hasPrefix("Bêta estimé sur 90 jours de rendements journaliers communs ; hypothèse"))
        XCTAssertEqual(WhatIf.intro(.qqq, shock: -10), "Comment le risque de ce portefeuille évolue si le Nasdaq-100 baisse de 10 % ? Chaque ligne bouge selon son bêta face à ce marché.")
    }

    func testAmountSpreadOnCurrentWeightsNeverBelowMinus100() throws {
        let r = WhatIf.run(portfolio, factor: .qqq, shock: -50, betas: ["stock:NVDA": FactorBeta(beta: 3, days: 60, estimated: true, correlation: 0.9)], amount: 1_000)
        XCTAssertTrue(r.scaled)
        XCTAssertEqual(r.cash, 200)
        let nvda = try XCTUnwrap(r.lines.first { $0.symbol == "NVDA" })
        XCTAssertEqual(nvda.value, 400)
        XCTAssertEqual(nvda.movePercent, -100)
        XCTAssertEqual(nvda.loss, 400)
        XCTAssertEqual(r.loss, 500)
        XCTAssertEqual(r.lossPercent, 50)
    }

    func testNegativeBetaGainsWorstIsBiggestLossOnly() throws {
        let r = WhatIf.run(portfolio, factor: .spy, shock: -10, betas: [
            "stock:NVDA": FactorBeta(beta: -0.5, days: 90, estimated: true, correlation: -0.3),
            "stock:QQQ": FactorBeta(beta: 1.2, days: 90, estimated: true, correlation: 0.95),
        ])
        XCTAssertEqual(r.lines.first { $0.symbol == "NVDA" }?.loss, -200)
        XCTAssertEqual(r.worst?.symbol, "QQQ")
        // Weak link to the chosen market: said next to the line.
        var weak = try XCTUnwrap(r.lines.first { $0.symbol == "NVDA" })
        XCTAssertFalse(WhatIf.lineDetail(weak, factor: .spy).contains("lien faible"))
        weak.correlation = -0.25
        XCTAssertTrue(WhatIf.lineDetail(weak, factor: .spy).hasSuffix(" · lien faible avec S&P 500 (SPY) : ce bêta explique mal les mouvements de la ligne, résultat peu fiable"))
    }
}
