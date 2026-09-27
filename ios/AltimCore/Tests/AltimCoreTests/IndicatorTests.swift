import XCTest
@testable import AltimCore

final class IndicatorTests: XCTestCase {
    func testSMA() {
        let r = Indicators.sma([1, 2, 3, 4, 5], period: 3)
        XCTAssertNil(r[1])
        XCTAssertEqual(r[2], 2)
        XCTAssertEqual(r[4], 4)
    }

    func testEMASeededWithSMA() {
        let r = Indicators.ema([2, 4, 6, 8, 10], period: 3)
        XCTAssertEqual(r[2], 4)            // SMA(2,4,6)
        XCTAssertEqual(r[3]!, 6, accuracy: 1e-9) // 8*0.5 + 4*0.5
        XCTAssertEqual(r[4]!, 8, accuracy: 1e-9)
    }

    /// Exemple de référence StockCharts (RSI de Wilder, 14 périodes).
    func testRSIMatchesStockChartsReference() {
        let closes = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
                      45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64]
        let rsi = Indicators.rsi(closes, period: 14)
        XCTAssertNil(rsi[13])
        XCTAssertEqual(rsi[14]!, 70.53, accuracy: 0.1)
        XCTAssertEqual(rsi[15]!, 66.32, accuracy: 0.1)
        XCTAssertEqual(rsi[16]!, 66.55, accuracy: 0.1)
        XCTAssertEqual(rsi[17]!, 69.41, accuracy: 0.1)
        XCTAssertEqual(rsi[18]!, 66.36, accuracy: 0.1)
        XCTAssertEqual(rsi[19]!, 57.97, accuracy: 0.1)
    }

    func testRSIExtremes() {
        let up = Indicators.rsi((0..<30).map(Double.init), period: 14)
        XCTAssertEqual(up.last!, 100)
        let flat = Indicators.rsi(Array(repeating: 10, count: 30), period: 14)
        XCTAssertEqual(flat.last!, 50)
    }

    func testMACDLineIsFastMinusSlow() {
        let closes = Fixtures.candles(count: 120, drift: 0.002).closes
        let macd = Indicators.macd(closes)
        let fast = Indicators.ema(closes, period: 12)
        let slow = Indicators.ema(closes, period: 26)
        XCTAssertNil(macd.line[24])
        XCTAssertEqual(macd.line[100]!, fast[100]! - slow[100]!, accuracy: 1e-9)
        XCTAssertNotNil(macd.signal[33])
        XCTAssertNil(macd.signal[32])
        XCTAssertEqual(macd.histogram[100]!, macd.line[100]! - macd.signal[100]!, accuracy: 1e-9)
    }

    func testBollingerOnConstantSeries() {
        let b = Indicators.bollinger(Array(repeating: 5, count: 25))
        XCTAssertEqual(b.upper[24], 5)
        XCTAssertEqual(b.lower[24], 5)
    }

    func testATRAndTrueRange() {
        let t = Date()
        let candles = [
            Candle(time: t, open: 10, high: 12, low: 9, close: 11, volume: 1),
            Candle(time: t + 1, open: 11, high: 11.5, low: 10.5, close: 11, volume: 1),
            Candle(time: t + 2, open: 11, high: 15, low: 11, close: 14, volume: 1),
        ]
        XCTAssertEqual(Indicators.trueRange(candles), [3, 1, 4])
        let atr = Indicators.atr(candles, period: 2)
        XCTAssertEqual(atr[1], 2)
        XCTAssertEqual(atr[2], 3) // (2*1 + 4)/2
    }

    func testADXDetectsTrend() {
        let trending = Fixtures.candles(count: 200, drift: 0.01, amplitude: 0.2)
        let dmi = Indicators.adx(trending)
        XCTAssertGreaterThan(dmi.adx.last!!, 25)
        XCTAssertGreaterThan(dmi.plusDI.last!!, dmi.minusDI.last!!)
    }

    func testStochasticBounds() {
        let s = Indicators.stochastic(Fixtures.candles(count: 100, drift: 0))
        for v in s.k.compactMap({ $0 }) + s.d.compactMap({ $0 }) {
            XCTAssertGreaterThanOrEqual(v, 0)
            XCTAssertLessThanOrEqual(v, 100)
        }
    }

    func testOBV() {
        let t = Date()
        let c = [
            Candle(time: t, open: 1, high: 1, low: 1, close: 1, volume: 10),
            Candle(time: t + 1, open: 1, high: 2, low: 1, close: 2, volume: 5),
            Candle(time: t + 2, open: 2, high: 2, low: 1, close: 1, volume: 3),
        ]
        XCTAssertEqual(Indicators.obv(c), [0, 5, 2])
    }
}
