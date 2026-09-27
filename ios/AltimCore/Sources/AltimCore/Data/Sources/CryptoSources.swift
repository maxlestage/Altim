import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

// Public crypto exchange sources (no key required). Each parser is covered by a test
// built from a real response sample.

private func closedFlag(start: Double, step: TimeInterval, now: Date) -> Bool {
    start + step <= now.timeIntervalSince1970
}

// MARK: - OKX

public struct OKXMarketData: MarketSource {
    public let name = "OKX"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let bar = ["15m": "15m", "1h": "1H", "4h": "4H", "1d": "1Dutc"][timeframe.rawValue]!
        let url = SourceHTTP.url("https://www.okx.com/api/v5/market/candles",
                                 [("instId", "\(asset.base)-USDT"), ("bar", bar), ("limit", "300")])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    static func parse(_ data: Data) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard (obj["code"] as? String) == "0", let rows = obj["data"] as? [[Any]] else {
            throw APIError.decoding("OKX : \(obj["msg"] as? String ?? "réponse inattendue")")
        }
        return try rows.map { r in
            guard r.count >= 9, let t = JSON.double(r[0]), let o = JSON.double(r[1]), let h = JSON.double(r[2]),
                  let l = JSON.double(r[3]), let c = JSON.double(r[4]), let v = JSON.double(r[5]) else {
                throw APIError.decoding("bougie OKX")
            }
            return Candle(time: Date(timeIntervalSince1970: t / 1000), open: o, high: h, low: l, close: c, volume: v,
                          isClosed: (r[8] as? String) == "1")
        }.reversed()
    }
}

// MARK: - Coinbase

public struct CoinbaseMarketData: MarketSource {
    public let name = "Coinbase"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        // Coinbase has no 4h: aggregated from 1h.
        let step: TimeInterval = timeframe == .h4 ? 3600 : timeframe.seconds
        let url = SourceHTTP.url("https://api.exchange.coinbase.com/products/\(asset.base)-USD/candles",
                                 [("granularity", String(Int(step)))])
        let candles = try Self.parse(try await SourceHTTP.get(transport, url), step: step, now: Date())
        return timeframe == .h4 ? CandleAggregator.aggregate(candles, from: 3600, to: 14_400) : candles
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// Format : [time, low, high, open, close, volume], du plus récent au plus ancien.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let rows = try JSON.array(data)
        return try rows.map { row in
            guard let r = row as? [Any], r.count >= 6, let t = JSON.double(r[0]), let l = JSON.double(r[1]),
                  let h = JSON.double(r[2]), let o = JSON.double(r[3]), let c = JSON.double(r[4]),
                  let v = JSON.double(r[5]) else { throw APIError.decoding("bougie Coinbase") }
            return Candle(time: Date(timeIntervalSince1970: t), open: o, high: h, low: l, close: c, volume: v,
                          isClosed: closedFlag(start: t, step: step, now: now))
        }.reversed()
    }
}

// MARK: - Kraken

public struct KrakenMarketData: MarketSource {
    public let name = "Kraken"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    static func pair(_ base: String) -> String {
        (["BTC": "XBT", "DOGE": "XDG"][base] ?? base) + "USD"
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let url = SourceHTTP.url("https://api.kraken.com/0/public/OHLC",
                                 [("pair", Self.pair(asset.base)), ("interval", String(Int(timeframe.seconds / 60)))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// Format : {"error":[],"result":{"XXBTZUSD":[[time,o,h,l,c,vwap,volume,count]],"last":…}}
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        if let errors = obj["error"] as? [String], !errors.isEmpty { throw APIError.decoding("Kraken : \(errors.joined())") }
        guard let result = obj["result"] as? [String: Any],
              let rows = result.first(where: { $0.key != "last" })?.value as? [[Any]] else {
            throw APIError.decoding("Kraken")
        }
        return try rows.map { r in
            guard r.count >= 7, let t = JSON.double(r[0]), let o = JSON.double(r[1]), let h = JSON.double(r[2]),
                  let l = JSON.double(r[3]), let c = JSON.double(r[4]), let v = JSON.double(r[6]) else {
                throw APIError.decoding("bougie Kraken")
            }
            return Candle(time: Date(timeIntervalSince1970: t), open: o, high: h, low: l, close: c, volume: v,
                          isClosed: closedFlag(start: t, step: step, now: now))
        }
    }
}

// MARK: - KuCoin

public struct KuCoinMarketData: MarketSource {
    public let name = "KuCoin"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let type = ["15m": "15min", "1h": "1hour", "4h": "4hour", "1d": "1day"][timeframe.rawValue]!
        // Without an explicit window, KuCoin returns only 100 candles.
        let end = Int(Date().timeIntervalSince1970)
        let start = end - Int(timeframe.seconds) * min(max(limit, 1), 1500)
        let url = SourceHTTP.url("https://api.kucoin.com/api/v1/market/candles",
                                 [("type", type), ("symbol", "\(asset.base)-USDT"),
                                  ("startAt", String(start)), ("endAt", String(end))])
        return try Self.parse(try await SourceHTTP.get(transport, url), step: timeframe.seconds, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// Format : [time, open, close, high, low, volume, turnover], du plus récent au plus ancien.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard (obj["code"] as? String) == "200000", let rows = obj["data"] as? [[Any]] else {
            throw APIError.decoding("KuCoin : \(obj["msg"] as? String ?? "réponse inattendue")")
        }
        return try rows.map { r in
            guard r.count >= 6, let t = JSON.double(r[0]), let o = JSON.double(r[1]), let c = JSON.double(r[2]),
                  let h = JSON.double(r[3]), let l = JSON.double(r[4]), let v = JSON.double(r[5]) else {
                throw APIError.decoding("bougie KuCoin")
            }
            return Candle(time: Date(timeIntervalSince1970: t), open: o, high: h, low: l, close: c, volume: v,
                          isClosed: closedFlag(start: t, step: step, now: now))
        }.reversed()
    }
}

// MARK: - Gate.io

public struct GateMarketData: MarketSource {
    public let name = "Gate.io"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let url = SourceHTTP.url("https://api.gateio.ws/api/v4/spot/candlesticks",
                                 [("currency_pair", "\(asset.base)_USDT"), ("interval", timeframe.rawValue),
                                  ("limit", String(min(max(limit, 1), 1000)))])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// Format : [time, quote volume, close, high, low, open, base volume, "true" si clôturée].
    static func parse(_ data: Data) throws -> [Candle] {
        let rows = try JSON.array(data)
        return try rows.map { row in
            guard let r = row as? [Any], r.count >= 7, let t = JSON.double(r[0]), let c = JSON.double(r[2]),
                  let h = JSON.double(r[3]), let l = JSON.double(r[4]), let o = JSON.double(r[5]),
                  let v = JSON.double(r[6]) else { throw APIError.decoding("bougie Gate.io") }
            let closed = r.count >= 8 ? (r[7] as? String) == "true" : true
            return Candle(time: Date(timeIntervalSince1970: t), open: o, high: h, low: l, close: c, volume: v, isClosed: closed)
        }
    }
}

// MARK: - Bitfinex

public struct BitfinexMarketData: MarketSource {
    public let name = "Bitfinex"
    public let assetClass = AssetClass.crypto
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    static func symbol(_ base: String) -> String { base.count > 3 ? "t\(base):USD" : "t\(base)USD" }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        // Bitfinex has no 4h: aggregated from 1h.
        let step: TimeInterval = timeframe == .h4 ? 3600 : timeframe.seconds
        let tf = ["900": "15m", "3600": "1h", "86400": "1D"][String(Int(step))]!
        let key = "trade:\(tf):\(Self.symbol(asset.base))".addingPercentEncoding(withAllowedCharacters: .urlPathAllowed)!
        let url = SourceHTTP.url("https://api-pub.bitfinex.com/v2/candles/\(key)/hist",
                                 [("limit", String(timeframe == .h4 ? 1000 : min(max(limit, 1), 1000)))])
        let candles = try Self.parse(try await SourceHTTP.get(transport, url), step: step, now: Date())
        return timeframe == .h4 ? CandleAggregator.aggregate(candles, from: 3600, to: 14_400) : candles
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    /// Format : [mts, open, close, high, low, volume], du plus récent au plus ancien.
    static func parse(_ data: Data, step: TimeInterval, now: Date) throws -> [Candle] {
        let rows = try JSON.array(data)
        return try rows.map { row in
            guard let r = row as? [Any], r.count >= 6, let t = JSON.double(r[0]), let o = JSON.double(r[1]),
                  let c = JSON.double(r[2]), let h = JSON.double(r[3]), let l = JSON.double(r[4]),
                  let v = JSON.double(r[5]) else { throw APIError.decoding("bougie Bitfinex") }
            return Candle(time: Date(timeIntervalSince1970: t / 1000), open: o, high: h, low: l, close: c, volume: v,
                          isClosed: closedFlag(start: t / 1000, step: step, now: now))
        }.reversed()
    }
}

// MARK: - Binance (and Binance.US) as named sources

public struct BinanceSource: MarketSource {
    public let name: String
    public let assetClass = AssetClass.crypto
    let inner: BinanceMarketData

    public init(name: String = "Binance", baseURL: URL = URL(string: "https://api.binance.com")!,
                transport: HTTPTransport = URLSessionTransport()) {
        self.name = name
        self.inner = BinanceMarketData(baseURL: baseURL, transport: transport)
    }

    public static func us(transport: HTTPTransport = URLSessionTransport()) -> BinanceSource {
        BinanceSource(name: "Binance.US", baseURL: URL(string: "https://api.binance.us")!, transport: transport)
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        try await inner.candles(for: asset, timeframe: timeframe, limit: limit)
    }

    public func quote(for asset: Asset) async throws -> Quote { try await inner.quote(for: asset) }
}

// MARK: - CoinGecko (quote only)

public struct CoinGeckoQuotes: MarketSource {
    public let name = "CoinGecko"
    public let assetClass = AssetClass.crypto
    public var providesCandles: Bool { false }
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    static let ids = ["BTC": "bitcoin", "ETH": "ethereum", "SOL": "solana", "BNB": "binancecoin", "XRP": "ripple",
                      "ADA": "cardano", "DOGE": "dogecoin", "AVAX": "avalanche-2", "DOT": "polkadot", "LINK": "chainlink",
                      "LTC": "litecoin", "TRX": "tron", "MATIC": "matic-network", "POL": "polygon-ecosystem-token",
                      "TON": "the-open-network", "SHIB": "shiba-inu", "ATOM": "cosmos", "UNI": "uniswap",
                      "NEAR": "near", "APT": "aptos", "ARB": "arbitrum", "OP": "optimism", "SUI": "sui", "PEPE": "pepe"]

    public func supports(_ asset: Asset) -> Bool { asset.assetClass == .crypto && Self.ids[asset.base] != nil }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("CoinGecko : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        guard let id = Self.ids[asset.base] else { throw APIError.decoding("CoinGecko : actif inconnu") }
        let url = SourceHTTP.url("https://api.coingecko.com/api/v3/simple/price",
                                 [("ids", id), ("vs_currencies", "usd"), ("include_24hr_change", "true"),
                                  ("include_last_updated_at", "true")])
        return try Self.parse(try await SourceHTTP.get(transport, url), id: id)
    }

    static func parse(_ data: Data, id: String) throws -> Quote {
        guard let row = try JSON.object(data)[id] as? [String: Any], let price = JSON.double(row["usd"]) else {
            throw APIError.decoding("CoinGecko")
        }
        let time = JSON.double(row["last_updated_at"]).map { Date(timeIntervalSince1970: $0) } ?? Date()
        return Quote(price: price, changePercent24h: JSON.double(row["usd_24h_change"]) ?? 0, time: time)
    }
}
