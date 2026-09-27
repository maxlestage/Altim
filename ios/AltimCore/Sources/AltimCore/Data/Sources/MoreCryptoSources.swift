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

// MARK: - Poloniex, HitBTC, WhiteBIT, CoinEx, XT, WOO X, BingX, LBank

/// Generic public exchange: URL builder + parser (same sources and formats as web/server/market.ts).
public struct SimpleExchangeSource: MarketSource {
    public let name: String
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    let intraday: Bool
    let makeURL: @Sendable (_ base: String, _ timeframe: Timeframe, _ limit: Int) -> URL?
    let parser: @Sendable (Data) throws -> [Candle]

    public func supports(_ timeframe: Timeframe) -> Bool { !(intraday && timeframe == .d1) }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard supports(timeframe), let url = makeURL(asset.base, timeframe, min(max(limit, 1), 500)) else {
            throw APIError.decoding("\(name) : unité de temps non prise en charge")
        }
        let now = Date()
        return try parser(try await SourceHTTP.get(transport, url))
            .map { Candle(time: $0.time, open: $0.open, high: $0.high, low: $0.low, close: $0.close, volume: $0.volume,
                          isClosed: $0.time.addingTimeInterval(timeframe.seconds) <= now) }
            .sorted { $0.time < $1.time }
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }
}

enum MoreExchanges {
    static func row(_ data: Data, key: String? = nil) throws -> [Any] {
        if let key { return (try JSON.object(data))[key] as? [Any] ?? [] }
        return try JSON.array(data)
    }

    static func c(_ t: Double?, _ o: Any?, _ h: Any?, _ l: Any?, _ cl: Any?, _ v: Any?, _ source: String) throws -> Candle {
        guard let t, let o = JSON.double(o), let h = JSON.double(h), let l = JSON.double(l), let cl = JSON.double(cl) else {
            throw APIError.decoding("bougie \(source)")
        }
        return Candle(time: Date(timeIntervalSince1970: t), open: o, high: h, low: l, close: cl, volume: JSON.double(v) ?? 0)
    }

    /// [low, high, open, close, amount, quantity, …, startTime (index 12), closeTime]
    static func poloniex(_ data: Data) throws -> [Candle] {
        try row(data).map { r in
            guard let r = r as? [Any], r.count >= 13 else { throw APIError.decoding("bougie Poloniex") }
            return try c(JSON.double(r[12]).map { $0 / 1000 }, r[2], r[1], r[0], r[3], r[5], "Poloniex")
        }
    }

    /// {timestamp ISO, open, close, min, max, volume}, most recent first.
    static func hitbtc(_ data: Data) throws -> [Candle] {
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return try row(data).map { r in
            guard let r = r as? [String: Any], let ts = r["timestamp"] as? String, let d = iso.date(from: ts) else {
                throw APIError.decoding("bougie HitBTC")
            }
            return try c(d.timeIntervalSince1970, r["open"], r["max"], r["min"], r["close"], r["volume"], "HitBTC")
        }
    }

    /// {"success": true, "result": [[time (s), open, close, high, low, base volume, quote volume]]}
    static func whitebit(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard (obj["success"] as? Bool) == true else { throw APIError.decoding("WhiteBIT") }
        return try (obj["result"] as? [[Any]] ?? []).map { r in
            guard r.count >= 6 else { throw APIError.decoding("bougie WhiteBIT") }
            return try c(JSON.double(r[0]), r[1], r[3], r[4], r[2], r[5], "WhiteBIT")
        }
    }

    /// {"code": 0, "data": [{created_at (ms), open, close, high, low, volume}]}
    static func coinex(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard JSON.double(obj["code"]) == 0 else { throw APIError.decoding("CoinEx") }
        return try (obj["data"] as? [[String: Any]] ?? []).map { r in
            try c(JSON.double(r["created_at"]).map { $0 / 1000 }, r["open"], r["high"], r["low"], r["close"], r["volume"], "CoinEx")
        }
    }

    /// {"rc": 0, "result": [{t (ms), o, c, h, l, q (base volume), v (quote volume)}]}, most recent first.
    static func xt(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard JSON.double(obj["rc"]) == 0 else { throw APIError.decoding("XT") }
        return try (obj["result"] as? [[String: Any]] ?? []).map { r in
            try c(JSON.double(r["t"]).map { $0 / 1000 }, r["o"], r["h"], r["l"], r["c"], r["q"], "XT")
        }
    }

    /// {"success": true, "rows": [{open, close, low, high, volume, start_timestamp (ms)}]}, most recent first.
    static func woox(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard (obj["success"] as? Bool) == true else { throw APIError.decoding("WOO X") }
        return try (obj["rows"] as? [[String: Any]] ?? []).map { r in
            try c(JSON.double(r["start_timestamp"]).map { $0 / 1000 }, r["open"], r["high"], r["low"], r["close"], r["volume"], "WOO X")
        }
    }

    /// {"code": 0, "data": [[time (ms), open, high, low, close, volume, …]]}, most recent first.
    static func bingx(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard JSON.double(obj["code"]) == 0 else { throw APIError.decoding("BingX") }
        return try (obj["data"] as? [[Any]] ?? []).map { r in
            guard r.count >= 6 else { throw APIError.decoding("bougie BingX") }
            return try c(JSON.double(r[0]).map { $0 / 1000 }, r[1], r[2], r[3], r[4], r[5], "BingX")
        }
    }

    /// {"result": "true", "data": [[time (s), open, high, low, close, volume]]}
    static func lbank(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard "\(obj["result"] ?? "")" == "true" || (obj["result"] as? Bool) == true else { throw APIError.decoding("LBank") }
        return try (obj["data"] as? [[Any]] ?? []).map { r in
            guard r.count >= 6 else { throw APIError.decoding("bougie LBank") }
            return try c(JSON.double(r[0]), r[1], r[2], r[3], r[4], r[5], "LBank")
        }
    }

    static func tf(_ t: Timeframe, _ values: [String]) -> String? {
        // values: 15m, 1h, 4h, 1d
        switch t {
        case .m15: return values[0]
        case .h1: return values[1]
        case .h4: return values[2]
        case .d1: return values[3]
        }
    }

    static func sources(transport: HTTPTransport) -> [SimpleExchangeSource] {
        [
            SimpleExchangeSource(name: "Poloniex", transport: transport, intraday: false, makeURL: { b, t, n in
                tf(t, ["MINUTE_15", "HOUR_1", "HOUR_4", "DAY_1"]).flatMap { URL(string: "https://api.poloniex.com/markets/\(b)_USDT/candles?interval=\($0)&limit=\(n)") }
            }, parser: poloniex),
            SimpleExchangeSource(name: "HitBTC", transport: transport, intraday: false, makeURL: { b, t, n in
                tf(t, ["M15", "H1", "H4", "D1"]).flatMap { URL(string: "https://api.hitbtc.com/api/3/public/candles/\(b)USDT?period=\($0)&limit=\(n)") }
            }, parser: hitbtc),
            SimpleExchangeSource(name: "WhiteBIT", transport: transport, intraday: false, makeURL: { b, t, n in
                tf(t, ["15m", "1h", "4h", "1d"]).flatMap { URL(string: "https://whitebit.com/api/v1/public/kline?market=\(b)_USDT&interval=\($0)&limit=\(n)") }
            }, parser: whitebit),
            SimpleExchangeSource(name: "CoinEx", transport: transport, intraday: false, makeURL: { b, t, n in
                tf(t, ["15min", "1hour", "4hour", "1day"]).flatMap { URL(string: "https://api.coinex.com/v2/spot/kline?market=\(b)USDT&period=\($0)&limit=\(n)") }
            }, parser: coinex),
            SimpleExchangeSource(name: "XT", transport: transport, intraday: false, makeURL: { b, t, n in
                tf(t, ["15m", "1h", "4h", "1d"]).flatMap { URL(string: "https://sapi.xt.com/v4/public/kline?symbol=\(b.lowercased())_usdt&interval=\($0)&limit=\(n)") }
            }, parser: xt),
            SimpleExchangeSource(name: "WOO X", transport: transport, intraday: false, makeURL: { b, t, n in
                tf(t, ["15m", "1h", "4h", "1d"]).flatMap { URL(string: "https://api.woox.io/v1/public/kline?symbol=SPOT_\(b)_USDT&type=\($0)&limit=\(n)") }
            }, parser: woox),
            // BingX and LBank daily candles start at 16:00 UTC (UTC+8): intraday timeframes only.
            SimpleExchangeSource(name: "BingX", transport: transport, intraday: true, makeURL: { b, t, n in
                tf(t, ["15m", "1h", "4h", "1d"]).flatMap { URL(string: "https://open-api.bingx.com/openApi/spot/v2/market/kline?symbol=\(b)-USDT&interval=\($0)&limit=\(n)") }
            }, parser: bingx),
            SimpleExchangeSource(name: "LBank", transport: transport, intraday: true, makeURL: { b, t, n in
                let since = Int(Date().timeIntervalSince1970 - Double(n) * t.seconds)
                return tf(t, ["minute15", "hour1", "hour4", "day1"]).flatMap {
                    URL(string: "https://api.lbkex.com/v2/kline.do?symbol=\(b.lowercased())_usdt&size=\(n)&type=\($0)&time=\(since)")
                }
            }, parser: lbank),
        ]
    }
}
