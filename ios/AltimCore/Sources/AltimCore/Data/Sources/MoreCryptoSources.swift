import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

// Additional public crypto exchanges (no key), so a price is never judged on only a few sources.
// Same sources and formats as web/server/market.ts; parsers covered by tests built from real responses.

private func isClosed(_ start: TimeInterval, _ step: TimeInterval, _ now: Date) -> Bool {
    start + step <= now.timeIntervalSince1970
}

private func candle(_ t: TimeInterval, _ o: Double?, _ h: Double?, _ l: Double?, _ c: Double?, _ v: Double?,
                    step: TimeInterval, now: Date, source: String) throws -> Candle {
    guard let o, let h, let l, let c else { throw APIError.decoding("bougie \(source)") }
    return Candle(time: Date(timeIntervalSince1970: t), open: o, high: h, low: l, close: c, volume: v ?? 0,
                  isClosed: isClosed(t, step, now))
}

// MARK: - Bitstamp

public struct BitstampMarketData: MarketSource {
    public let name = "Bitstamp"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let url = SourceHTTP.url("https://www.bitstamp.net/api/v2/ohlc/\(asset.base.lowercased())usd/",
                                 [("step", String(Int(timeframe.seconds))), ("limit", String(min(max(limit, 1), 1000)))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// {"data": {"ohlc": [{"timestamp": "1790380800", "open": "84089.55", …}]}}, oldest first.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let rows = ((try JSON.object(data))["data"] as? [String: Any])?["ohlc"] as? [[String: Any]] ?? []
        return try rows.map { r in
            guard let t = JSON.double(r["timestamp"]) else { throw APIError.decoding("bougie Bitstamp") }
            return try candle(t, JSON.double(r["open"]), JSON.double(r["high"]), JSON.double(r["low"]), JSON.double(r["close"]),
                              JSON.double(r["volume"]), step: step, now: now, source: "Bitstamp")
        }
    }
}

// MARK: - Gemini

public struct GeminiMarketData: MarketSource {
    public let name = "Gemini"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        // No 4 h candles at Gemini: rebuilt from 1 h.
        let (tf, step): (String, TimeInterval) = switch timeframe {
        case .m15: ("15m", 900)
        case .h1, .h4: ("1hr", 3600)
        case .d1: ("1day", 86_400)
        }
        let url = URL(string: "https://api.gemini.com/v2/candles/\(asset.base.lowercased())usd/\(tf)")!
        let candles = try Self.parse(try await SourceHTTP.get(transport, url), step: step, now: Date())
        return timeframe == .h4 ? CandleAggregator.aggregate(candles, from: 3600, to: 14_400) : candles
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// [[ms, open, high, low, close, volume], …], most recent first.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        try JSON.array(data).map { row in
            guard let r = row as? [Any], r.count >= 6, let t = JSON.double(r[0]) else { throw APIError.decoding("bougie Gemini") }
            return try candle(t / 1000, JSON.double(r[1]), JSON.double(r[2]), JSON.double(r[3]), JSON.double(r[4]), JSON.double(r[5]),
                              step: step, now: now, source: "Gemini")
        }.reversed()
    }
}

// MARK: - Crypto.com

public struct CryptoComMarketData: MarketSource {
    public let name = "Crypto.com"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let tf = ["15m": "15m", "1h": "1h", "4h": "4h", "1d": "1D"][timeframe.rawValue]!
        let url = SourceHTTP.url("https://api.crypto.com/exchange/v1/public/get-candlestick",
                                 [("instrument_name", "\(asset.base)_USDT"), ("timeframe", tf), ("count", String(min(max(limit, 1), 300)))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// {"code": 0, "result": {"data": [{"o": "…", "h": "…", "l": "…", "c": "…", "v": "…", "t": ms}]}}, oldest first.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard JSON.double(obj["code"]) == 0 else { throw APIError.decoding("Crypto.com") }
        let rows = (obj["result"] as? [String: Any])?["data"] as? [[String: Any]] ?? []
        return try rows.map { r in
            guard let t = JSON.double(r["t"]) else { throw APIError.decoding("bougie Crypto.com") }
            return try candle(t / 1000, JSON.double(r["o"]), JSON.double(r["h"]), JSON.double(r["l"]), JSON.double(r["c"]),
                              JSON.double(r["v"]), step: step, now: now, source: "Crypto.com")
        }
    }
}

// MARK: - Bitget

public struct BitgetMarketData: MarketSource {
    public let name = "Bitget"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        // "1Dutc": daily candle starting at 00:00 UTC like every other source ("1day" starts at UTC+8).
        let tf = ["15m": "15min", "1h": "1h", "4h": "4h", "1d": "1Dutc"][timeframe.rawValue]!
        let url = SourceHTTP.url("https://api.bitget.com/api/v2/spot/market/candles",
                                 [("symbol", "\(asset.base)USDT"), ("granularity", tf), ("limit", String(min(max(limit, 1), 1000)))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// {"code": "00000", "data": [["ms", "open", "high", "low", "close", "volume", …]]}, oldest first.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard obj["code"] as? String == "00000" else { throw APIError.decoding("Bitget") }
        return try (obj["data"] as? [[Any]] ?? []).map { r in
            guard r.count >= 6, let t = JSON.double(r[0]) else { throw APIError.decoding("bougie Bitget") }
            return try candle(t / 1000, JSON.double(r[1]), JSON.double(r[2]), JSON.double(r[3]), JSON.double(r[4]), JSON.double(r[5]),
                              step: step, now: now, source: "Bitget")
        }
    }
}

// MARK: - MEXC (Binance-compatible format)

public struct MEXCMarketData: MarketSource {
    public let name = "MEXC"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let tf = ["15m": "15m", "1h": "60m", "4h": "4h", "1d": "1d"][timeframe.rawValue]!
        let url = SourceHTTP.url("https://api.mexc.com/api/v3/klines",
                                 [("symbol", "\(asset.base)USDT"), ("interval", tf), ("limit", String(min(max(limit, 1), 1000)))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// [[ms, "open", "high", "low", "close", "volume", closeMs, "quoteVolume"]], oldest first.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        try JSON.array(data).map { row in
            guard let r = row as? [Any], r.count >= 6, let t = JSON.double(r[0]) else { throw APIError.decoding("bougie MEXC") }
            return try candle(t / 1000, JSON.double(r[1]), JSON.double(r[2]), JSON.double(r[3]), JSON.double(r[4]), JSON.double(r[5]),
                              step: step, now: now, source: "MEXC")
        }
    }
}

// MARK: - HTX (ex-Huobi)

public struct HTXMarketData: MarketSource {
    public let name = "HTX"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    /// HTX daily candles start at 16:00 UTC (UTC+8): intraday timeframes only.
    public func supports(_ timeframe: Timeframe) -> Bool { timeframe != .d1 }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard supports(timeframe) else { throw APIError.decoding("HTX : pas de bougie journalière UTC") }
        let tf = ["15m": "15min", "1h": "60min", "4h": "4hour"][timeframe.rawValue]!
        let url = SourceHTTP.url("https://api.huobi.pro/market/history/kline",
                                 [("symbol", "\(asset.base.lowercased())usdt"), ("period", tf), ("size", String(min(max(limit, 1), 2000)))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// {"status": "ok", "data": [{"id": s, "open", "close", "low", "high", "amount"}]}, most recent first.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard obj["status"] as? String == "ok" else { throw APIError.decoding("HTX") }
        return try (obj["data"] as? [[String: Any]] ?? []).map { r in
            guard let t = JSON.double(r["id"]) else { throw APIError.decoding("bougie HTX") }
            return try candle(t, JSON.double(r["open"]), JSON.double(r["high"]), JSON.double(r["low"]), JSON.double(r["close"]),
                              JSON.double(r["amount"]), step: step, now: now, source: "HTX")
        }.reversed()
    }
}
