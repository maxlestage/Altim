import XCTest
@testable import AltimKit

/// The backtest card shows what a winning and a losing trade gave on average, not only the share of winners.
final class BacktestWinLossTests: XCTestCase {
    func report(_ results: [Double]) -> BacktestReport {
        let trades = results.enumerated().map { i, r in
            BacktestReport.Trade(entryTime: Double(i), entryPrice: 100, exitTime: Double(i) + 1, exitPrice: 100 * (1 + r / 100), exit: "signal", result: r)
        }
        return BacktestReport(symbol: "X", kind: .stock, from: 0, to: 1, days: 300, strategyReturn: 0, holdReturn: 0, strategyDrawdown: 0,
                              holdDrawdown: 0, trades: trades, winRate: nil, exposure: 50, curve: [], reliability: nil, error: nil)
    }

    func testAverages() throws {
        let r = report([20, 10, -4, -2, -6])
        XCTAssertEqual(try XCTUnwrap(r.averageWin), 15, accuracy: 1e-9)
        XCTAssertEqual(try XCTUnwrap(r.averageLoss), -4, accuracy: 1e-9)
        XCTAssertEqual(r.winLossText, "\(Format.percent(15, digits: 0)) / \(Format.percent(-4, digits: 0))")
        XCTAssertNil(report([5, 3]).winLossText, "pas de perte : pas de comparaison inventée")
        XCTAssertNil(report([]).averageWin)
        XCTAssertFalse(BacktestReport.winRateNote.contains("garanti"), "une explication, pas une promesse")
    }

    /// Market history not read: the replay says it ran without the market filter, instead of claiming it.
    func testMarketMissingIsSaid() async {
        var r = report([5, -2])
        XCTAssertNil(r.marketMissingText)
        r.marketMissing = true
        XCTAssertEqual(r.marketMissingText, "Historique du S&P 500 (SPY) non lu pour l'instant : rejoué sans le filtre de marché, la règle de l'actif seule.")
    }

    func testFeeNoteQuotesTheMeasure() {
        XCTAssertTrue(BacktestReport.feeNote.contains("+41 % à +29 %"))
        XCTAssertFalse(BacktestReport.feeNote.contains("garanti"))
    }

    /// The user's fee per order: higher fees, lower replayed return, and the fee is kept in the report.
    func testUserFee() async throws {
        XCTAssertEqual(BacktestReport.clampFee(0.05), 0.02, "plafonné à 2 %")
        XCTAssertEqual(BacktestReport.clampFee(-1), 0)
        let bars = BotEngineTests.series(drift: 0.002)
        let a = Asset(symbol: "X", kind: .crypto, name: "X")
        let cheap = TrendEngine.backtest(bars, asset: a), dear = TrendEngine.backtest(bars, asset: a, fee: 0.01)
        XCTAssertFalse(cheap.trades.isEmpty)
        XCTAssertLessThan(dear.strategyReturn, cheap.strategyReturn, "des frais plus élevés coûtent du rendement")
        let data = [0.0015, 0.002, 0.0025, 0.001, 0.003, 0.0018].enumerated().map { i, d in
            (Asset(symbol: "A\(i)", kind: .crypto, name: "A\(i)"), BotEngineTests.series(drift: d))
        }
        let s1 = Rotation.sleeves(data, kind: .crypto), s2 = Rotation.sleeves(data, kind: .crypto, fee: 0.01)
        let r1 = try XCTUnwrap(s1, "replay de la Sélection"), r2 = try XCTUnwrap(s2)
        XCTAssertLessThan(r2.ret, r1.ret, "Sélection : les frais comptés aussi")
    }
}
