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
}
