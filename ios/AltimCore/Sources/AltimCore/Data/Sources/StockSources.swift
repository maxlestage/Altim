import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

private let utc = TimeZone(identifier: "UTC")!

private func isoDate(_ date: Date) -> String {
    let f = DateFormatter()
    f.locale = Locale(identifier: "en_US_POSIX")
    f.timeZone = utc
    f.dateFormat = "yyyy-MM-dd"
    return f.string(from: date)
}

/// Durée d'une bougie d'action (la séance journalière dure 6 h 30).
private func stockDuration(_ tf: Timeframe) -> TimeInterval { tf == .d1 ? 6.5 * 3600 : tf.seconds }

/// Historique demandé pour obtenir ~300 bougies clôturées.
private func lookbackDays(_ tf: Timeframe) -> Double {
    switch tf {
    case .m15: return 20
    case .h1: return 90
    case .h4: return 330
    case .d1: return 500
    }
}

// MARK: - Alpaca Market Data (flux IEX gratuit, clés du compte Alpaca)

public struct AlpacaMarketData: MarketSource {
    public let name = "Alpaca (IEX)"
    public let assetClass = AssetClass.stock
    let keyId: String
    let secret: String
    let transport: HTTPTransport

    public init(keyId: String, secret: String, transport: HTTPTransport = URLSessionTransport()) {
        self.keyId = keyId
        self.secret = secret
        self.transport = transport
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let tf = ["15m": "15Min", "1h": "1Hour", "4h": "4Hour", "1d": "1Day"][timeframe.rawValue]!
        let start = Date().addingTimeInterval(-lookbackDays(timeframe) * 86_400)
        let url = SourceHTTP.url("https://data.alpaca.markets/v2/stocks/\(asset.symbol)/bars",
                                 [("timeframe", tf), ("start", isoDate(start)), ("limit", "10000"),
                                  ("feed", "iex"), ("adjustment", "split")])
        let data = try await SourceHTTP.get(transport, url, headers: ["APCA-API-KEY-ID": keyId, "APCA-API-SECRET-KEY": secret])
        return Array(try Self.parse(data, timeframe: timeframe, now: Date()).suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    static func parse(_ data: Data, timeframe: Timeframe, now: Date) throws -> [Candle] {
        let bars = (try JSON.object(data))["bars"] as? [[String: Any]] ?? []
        let iso = ISO8601DateFormatter()
        return try bars.map { b in
            guard let ts = b["t"] as? String, let t = iso.date(from: ts), let o = JSON.double(b["o"]),
                  let h = JSON.double(b["h"]), let l = JSON.double(b["l"]), let c = JSON.double(b["c"]) else {
                throw APIError.decoding("barre Alpaca")
            }
            return Candle(time: t, open: o, high: h, low: l, close: c, volume: JSON.double(b["v"]) ?? 0,
                          isClosed: t.addingTimeInterval(stockDuration(timeframe)) <= now)
        }
    }
}

// MARK: - Twelve Data (clé gratuite : 800 requêtes / jour)

public struct TwelveDataMarketData: MarketSource {
    public let name = "Twelve Data"
    public let assetClass = AssetClass.stock
    let apiKey: String
    let transport: HTTPTransport

    public init(apiKey: String, transport: HTTPTransport = URLSessionTransport()) {
        self.apiKey = apiKey
        self.transport = transport
    }

    public func supports(_ asset: Asset) -> Bool { true }

    static func symbol(_ asset: Asset) -> String {
        asset.assetClass == .crypto ? "\(asset.base)/USD" : asset.symbol
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let interval = ["15m": "15min", "1h": "1h", "4h": "4h", "1d": "1day"][timeframe.rawValue]!
        let url = SourceHTTP.url("https://api.twelvedata.com/time_series",
                                 [("symbol", Self.symbol(asset)), ("interval", interval),
                                  ("outputsize", String(min(max(limit, 1), 5000))), ("timezone", "UTC"), ("apikey", apiKey)])
        return try Self.parse(try await SourceHTTP.get(transport, url), timeframe: timeframe,
                              isStock: asset.assetClass == .stock, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    static func parse(_ data: Data, timeframe: Timeframe, isStock: Bool, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard (obj["status"] as? String) == "ok", let values = obj["values"] as? [[String: Any]] else {
            throw APIError.decoding("Twelve Data : \(obj["message"] as? String ?? "réponse inattendue")")
        }
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = utc
        let duration = isStock ? stockDuration(timeframe) : timeframe.seconds
        return try values.map { v in
            guard let ds = v["datetime"] as? String else { throw APIError.decoding("date Twelve Data") }
            f.dateFormat = ds.count > 10 ? "yyyy-MM-dd HH:mm:ss" : "yyyy-MM-dd"
            guard var t = f.date(from: ds), let o = JSON.double(v["open"]), let h = JSON.double(v["high"]),
                  let l = JSON.double(v["low"]), let c = JSON.double(v["close"]) else {
                throw APIError.decoding("bougie Twelve Data")
            }
            // Daily stock candle: timestamped at the New York open (aligned with Yahoo).
            if isStock && timeframe == .d1 {
                let p = ds.split(separator: "-").compactMap { Int($0) }
                if p.count == 3, let open = newYorkOpen(year: p[0], month: p[1], day: p[2]) { t = open }
            }
            return Candle(time: t, open: o, high: h, low: l, close: c, volume: JSON.double(v["volume"]) ?? 0,
                          isClosed: t.addingTimeInterval(duration) <= now)
        }.reversed()
    }
}

// MARK: - Polygon.io (clé gratuite, données différées)

public struct PolygonMarketData: MarketSource {
    public let name = "Polygon"
    public let assetClass = AssetClass.stock
    let apiKey: String
    let transport: HTTPTransport

    public init(apiKey: String, transport: HTTPTransport = URLSessionTransport()) {
        self.apiKey = apiKey
        self.transport = transport
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        let (mult, span): (Int, String) = switch timeframe {
        case .m15: (15, "minute")
        case .h1: (1, "hour")
        case .h4: (4, "hour")
        case .d1: (1, "day")
        }
        let from = isoDate(Date().addingTimeInterval(-lookbackDays(timeframe) * 86_400))
        let to = isoDate(Date())
        let url = SourceHTTP.url("https://api.polygon.io/v2/aggs/ticker/\(asset.symbol)/range/\(mult)/\(span)/\(from)/\(to)",
                                 [("adjusted", "true"), ("sort", "asc"), ("limit", "50000"), ("apiKey", apiKey)])
        return Array(try Self.parse(try await SourceHTTP.get(transport, url), timeframe: timeframe, now: Date()).suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote { try await quoteFromCandles(self, asset) }

    static func parse(_ data: Data, timeframe: Timeframe, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        let status = obj["status"] as? String
        guard status == "OK" || status == "DELAYED" else {
            throw APIError.decoding("Polygon : \(obj["error"] as? String ?? obj["message"] as? String ?? "réponse inattendue")")
        }
        let results = obj["results"] as? [[String: Any]] ?? []
        return try results.map { r in
            guard let t = JSON.double(r["t"]), let o = JSON.double(r["o"]), let h = JSON.double(r["h"]),
                  let l = JSON.double(r["l"]), let c = JSON.double(r["c"]) else { throw APIError.decoding("barre Polygon") }
            let time = Date(timeIntervalSince1970: t / 1000)
            return Candle(time: time, open: o, high: h, low: l, close: c, volume: JSON.double(r["v"]) ?? 0,
                          isClosed: time.addingTimeInterval(stockDuration(timeframe)) <= now)
        }
    }
}

// MARK: - Finnhub (cours temps réel uniquement, clé gratuite)

public struct FinnhubQuotes: MarketSource {
    public let name = "Finnhub"
    public let assetClass = AssetClass.stock
    public var providesCandles: Bool { false }
    let apiKey: String
    let transport: HTTPTransport

    public init(apiKey: String, transport: HTTPTransport = URLSessionTransport()) {
        self.apiKey = apiKey
        self.transport = transport
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("Finnhub : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let url = SourceHTTP.url("https://finnhub.io/api/v1/quote", [("symbol", asset.symbol), ("token", apiKey)])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    static func parse(_ data: Data) throws -> Quote {
        let obj = try JSON.object(data)
        guard let price = JSON.double(obj["c"]), price > 0 else { throw APIError.decoding("Finnhub : symbole inconnu") }
        let time = JSON.double(obj["t"]).map { Date(timeIntervalSince1970: $0) } ?? Date()
        return Quote(price: price, changePercent24h: JSON.double(obj["dp"]) ?? 0, time: time)
    }
}

// MARK: - Nasdaq (daily history, no key)

public struct NasdaqMarketData: MarketSource {
    public let name = "Nasdaq"
    public let assetClass = AssetClass.stock
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func supports(_ timeframe: Timeframe) -> Bool { timeframe == .d1 }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard timeframe == .d1 else { throw APIError.decoding("Nasdaq : journalier uniquement") }
        let from = isoDate(Date().addingTimeInterval(-lookbackDays(.d1) * 86_400 * 1.5))
        // Symbol unknown as a stock: retry as an ETF.
        for assetClass in ["stocks", "etf"] {
            // Class shares: BRK-B (Yahoo) = BRK.B (Nasdaq).
            let symbol = asset.symbol.replacingOccurrences(of: "-", with: ".")
            let url = SourceHTTP.url("https://api.nasdaq.com/api/quote/\(symbol)/historical",
                                     [("assetclass", assetClass), ("fromdate", from), ("todate", isoDate(Date())), ("limit", "9999")])
            let data = try await SourceHTTP.get(transport, url, headers: ["Accept-Language": "en-US,en;q=0.9"])
            let candles = try Self.parse(data, now: Date())
            if !candles.isEmpty { return Array(candles.suffix(limit)) }
        }
        throw APIError.decoding("Nasdaq : symbole inconnu")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let c = try await candles(for: asset, timeframe: .d1, limit: 2)
        guard let last = c.last else { throw APIError.decoding("Nasdaq") }
        let prev = c.count > 1 ? c[c.count - 2].close : last.close
        return Quote(price: last.close, changePercent24h: (last.close / prev - 1) * 100, time: last.time)
    }

    /// Format : {"data":{"tradesTable":{"rows":[{"date":"09/25/2026","close":"$341.07","volume":"30,002,510",…}]}}}
    static func parse(_ data: Data, now: Date) throws -> [Candle] {
        let obj = try JSON.object(data)
        guard let rows = ((obj["data"] as? [String: Any])?["tradesTable"] as? [String: Any])?["rows"] as? [[String: Any]] else {
            return []
        }
        func num(_ v: Any?) -> Double? {
            (v as? String).flatMap { Double($0.replacingOccurrences(of: "$", with: "").replacingOccurrences(of: ",", with: "")) }
        }
        return try rows.map { r in
            guard let ds = r["date"] as? String else { throw APIError.decoding("date Nasdaq") }
            let p = ds.split(separator: "/").compactMap { Int($0) }
            guard p.count == 3, let t = newYorkOpen(year: p[2], month: p[0], day: p[1]),
                  let o = num(r["open"]), let h = num(r["high"]), let l = num(r["low"]), let c = num(r["close"]) else {
                throw APIError.decoding("bougie Nasdaq")
            }
            return Candle(time: t, open: o, high: h, low: l, close: c, volume: num(r["volume"]) ?? 0,
                          isClosed: t.addingTimeInterval(6.5 * 3600) <= now)
        }.reversed()
    }
}

// MARK: - Cboe (delayed quote, no key)

public struct CboeQuotes: MarketSource {
    /// Same provider as `CboeMarketData`: counted once in the independent sources.
    public let name = "Cboe (cours)"
    public let assetClass = AssetClass.stock
    public var providesCandles: Bool { false }
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("Cboe : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let url = SourceHTTP.url("https://www.cboe.com/education/tools/trade-optimizer/symbol-info/", [("symbol", asset.symbol.replacingOccurrences(of: "-", with: "."))])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    static func parse(_ data: Data) throws -> Quote {
        let obj = try JSON.object(data)
        guard (obj["success"] as? Bool) == true, let d = obj["details"] as? [String: Any],
              let price = JSON.double(d["current_price"]), price > 0 else { throw APIError.decoding("Cboe : symbole inconnu") }
        return Quote(price: price, changePercent24h: JSON.double(d["price_change_percent"]) ?? 0, time: Date())
    }
}

// MARK: - Sentiment : Fear & Greed (alternative.me)

public struct FearGreedIndex: Sendable, Hashable {
    public let value: Int
    public let classification: String
    public let time: Date

    public var label: String {
        switch value {
        case ..<25: return "Peur extrême"
        case ..<45: return "Peur"
        case ...55: return "Neutre"
        case ...75: return "Avidité"
        default: return "Avidité extrême"
        }
    }
}

public struct SocialSentiment: Sendable, Hashable {
    /// `nil` when no message states a sentiment.
    public let bullishPercent: Double?
    public let sampleSize: Int
    public let messages: Int
}

public struct SentimentProvider: Sendable {
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func cryptoFearGreed() async throws -> FearGreedIndex {
        try Self.parse(try await SourceHTTP.get(transport, URL(string: "https://api.alternative.me/fng/?limit=1")!))
    }

    /// Social sentiment (StockTwits): share of bullish messages among those that state a sentiment.
    public func social(_ asset: Asset) async throws -> SocialSentiment {
        let symbol = asset.assetClass == .crypto ? "\(asset.base).X" : asset.symbol
        let url = URL(string: "https://api.stocktwits.com/api/2/streams/symbol/\(symbol).json")!
        return try Self.parseSocial(try await SourceHTTP.get(transport, url))
    }

    static func parseSocial(_ data: Data) throws -> SocialSentiment {
        let messages = (try JSON.object(data))["messages"] as? [[String: Any]] ?? []
        let tags = messages.compactMap { (($0["entities"] as? [String: Any])?["sentiment"] as? [String: Any])?["basic"] as? String }
        let bullish = tags.filter { $0 == "Bullish" }.count
        return SocialSentiment(bullishPercent: tags.isEmpty ? nil : Double(bullish) / Double(tags.count) * 100,
                               sampleSize: tags.count, messages: messages.count)
    }

    static func parse(_ data: Data) throws -> FearGreedIndex {
        guard let row = ((try JSON.object(data))["data"] as? [[String: Any]])?.first,
              let value = JSON.double(row["value"]) else { throw APIError.decoding("Fear & Greed") }
        return FearGreedIndex(value: Int(value), classification: row["value_classification"] as? String ?? "",
                              time: JSON.double(row["timestamp"]).map { Date(timeIntervalSince1970: $0) } ?? Date())
    }
}

// MARK: - Class shares

/// BRK-B (Yahoo format, used by Altim) is written BRK.B at Nasdaq, Cboe, Robinhood and TradingView.
func dottedSymbol(_ symbol: String) -> String { symbol.replacingOccurrences(of: "-", with: ".") }

private func sessionCandle(_ date: String, _ o: Double?, _ h: Double?, _ l: Double?, _ c: Double?, _ v: Double?,
                           now: Date, source: String) throws -> Candle {
    let p = date.prefix(10).split(separator: "-").compactMap { Int($0) }
    guard p.count == 3, let t = newYorkOpen(year: p[0], month: p[1], day: p[2]), let o, let h, let l, let c else {
        throw APIError.decoding("bougie \(source)")
    }
    return Candle(time: t, open: o, high: h, low: l, close: c, volume: v ?? 0, isClosed: t.addingTimeInterval(6.5 * 3600) <= now)
}

// MARK: - Cboe daily history (no key)

public struct CboeMarketData: MarketSource {
    public let name = "Cboe"
    public let assetClass = AssetClass.stock
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func supports(_ timeframe: Timeframe) -> Bool { timeframe == .d1 }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard timeframe == .d1 else { throw APIError.decoding("Cboe : journalier uniquement") }
        let url = URL(string: "https://cdn.cboe.com/api/global/delayed_quotes/charts/historical/\(dottedSymbol(asset.symbol)).json")!
        return Array(try Self.parse(try await SourceHTTP.get(transport, url), now: Date()).suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let c = try await candles(for: asset, timeframe: .d1, limit: 2)
        guard let last = c.last else { throw APIError.decoding("Cboe") }
        let prev = c.count > 1 ? c[c.count - 2].close : last.close
        return Quote(price: last.close, changePercent24h: (last.close / prev - 1) * 100, time: last.time)
    }

    /// {"data": [{"date": "2026-09-24", "open": 764.0, "high": …, "low": …, "close": …, "volume": …}]}, since 2004.
    static func parse(_ data: Data, now: Date) throws -> [Candle] {
        let rows = (try JSON.object(data))["data"] as? [[String: Any]] ?? []
        return try rows.suffix(800).map { r in
            try sessionCandle(r["date"] as? String ?? "", JSON.double(r["open"]), JSON.double(r["high"]), JSON.double(r["low"]),
                              JSON.double(r["close"]), JSON.double(r["volume"]), now: now, source: "Cboe")
        }
    }
}

// MARK: - Robinhood (public market data, no key)

public struct RobinhoodMarketData: MarketSource {
    public let name = "Robinhood"
    public let assetClass = AssetClass.stock
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    /// Robinhood hourly bars start on the hour (Yahoo's at :30): daily only, to compare the same candles.
    public func supports(_ timeframe: Timeframe) -> Bool { timeframe == .d1 }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard timeframe == .d1 else { throw APIError.decoding("Robinhood : journalier uniquement") }
        let url = SourceHTTP.url("https://api.robinhood.com/marketdata/historicals/\(dottedSymbol(asset.symbol))/",
                                 [("interval", "day"), ("span", "5year"), ("bounds", "regular")])
        return Array(try Self.parse(try await SourceHTTP.get(transport, url), now: Date()).suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote { try await RobinhoodQuotes(transport: transport).quote(for: asset) }

    /// {"historicals": [{"begins_at": "2026-09-25T00:00:00Z", "open_price": "336.04", …, "session": "reg", "interpolated": false}]}
    static func parse(_ data: Data, now: Date) throws -> [Candle] {
        let rows = (try JSON.object(data))["historicals"] as? [[String: Any]] ?? []
        return try rows.filter { $0["session"] as? String == "reg" && ($0["interpolated"] as? Bool) != true }.map { r in
            try sessionCandle(r["begins_at"] as? String ?? "", JSON.double(r["open_price"]), JSON.double(r["high_price"]),
                              JSON.double(r["low_price"]), JSON.double(r["close_price"]), JSON.double(r["volume"]),
                              now: now, source: "Robinhood")
        }
    }
}

/// Robinhood live price: cross-checks the last intraday close (Yahoo is the only aligned hourly source).
public struct RobinhoodQuotes: MarketSource {
    public let name = "Robinhood (cours)"
    public let assetClass = AssetClass.stock
    public var providesCandles: Bool { false }
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("Robinhood : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let url = SourceHTTP.url("https://api.robinhood.com/quotes/", [("symbols", dottedSymbol(asset.symbol))])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    /// {"results": [{"last_trade_price": "341.04", "adjusted_previous_close": "335.92", …}]} (null for an unknown symbol).
    static func parse(_ data: Data) throws -> Quote {
        guard let r = ((try JSON.object(data))["results"] as? [Any])?.first as? [String: Any],
              let price = JSON.double(r["last_trade_price"]), price > 0 else { throw APIError.decoding("Robinhood : symbole inconnu") }
        let prev = JSON.double(r["adjusted_previous_close"]) ?? JSON.double(r["previous_close"])
        return Quote(price: price, changePercent24h: prev.map { (price / $0 - 1) * 100 } ?? 0, time: Date())
    }
}

// MARK: - TradingView (scanner, delayed quote, no key)

public struct TradingViewQuotes: MarketSource {
    public let name = "TradingView"
    public let assetClass = AssetClass.stock
    public var providesCandles: Bool { false }
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("TradingView : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        var request = URLRequest(url: URL(string: "https://scanner.tradingview.com/america/scan")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Altim/1.0", forHTTPHeaderField: "User-Agent")
        let body: [String: Any] = [
            "filter": [
                ["left": "name", "operation": "equal", "right": dottedSymbol(asset.symbol)],
                ["left": "exchange", "operation": "in_range", "right": ["NASDAQ", "NYSE", "AMEX", "CBOE"]],
            ],
            "columns": ["name", "exchange", "close", "change"],
            "range": [0, 5],
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, response) = try await transport.send(request)
        guard (200..<300).contains(response.statusCode) else { throw APIError.http(status: response.statusCode, message: "TradingView") }
        return try Self.parse(data)
    }

    /// {"data": [{"s": "NASDAQ:AAPL", "d": ["AAPL", "NASDAQ", 341.07, 1.53]}]}
    static func parse(_ data: Data) throws -> Quote {
        guard let row = ((try JSON.object(data))["data"] as? [[String: Any]])?.first, let d = row["d"] as? [Any], d.count >= 4,
              let price = JSON.double(d[2]), price > 0 else { throw APIError.decoding("TradingView : symbole inconnu") }
        return Quote(price: price, changePercent24h: JSON.double(d[3]) ?? 0, time: Date())
    }
}

// MARK: - StockAnalysis (daily history, no key)

public struct StockAnalysisMarketData: MarketSource {
    public let name = "StockAnalysis"
    public let assetClass = AssetClass.stock
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func supports(_ timeframe: Timeframe) -> Bool { timeframe == .d1 }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard timeframe == .d1 else { throw APIError.decoding("StockAnalysis : journalier uniquement") }
        let url = SourceHTTP.url("https://stockanalysis.com/api/symbol/s/\(dottedSymbol(asset.symbol).lowercased())/history",
                                 [("range", "5Y"), ("period", "daily")])
        return Array(try Self.parse(try await SourceHTTP.get(transport, url), now: Date()).suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let c = try await candles(for: asset, timeframe: .d1, limit: 2)
        guard let last = c.last else { throw APIError.decoding("StockAnalysis") }
        let prev = c.count > 1 ? c[c.count - 2].close : last.close
        return Quote(price: last.close, changePercent24h: (last.close / prev - 1) * 100, time: last.time)
    }

    /// {"data": [{"t": "2026-09-25", "o": 336.04, "h": …, "l": …, "c": …, "v": …}]}, most recent first.
    static func parse(_ data: Data, now: Date) throws -> [Candle] {
        let rows = (try JSON.object(data))["data"] as? [[String: Any]] ?? []
        return try rows.reversed().map { r in
            try sessionCandle(r["t"] as? String ?? "", JSON.double(r["o"]), JSON.double(r["h"]), JSON.double(r["l"]),
                              JSON.double(r["c"]), JSON.double(r["v"]), now: now, source: "StockAnalysis")
        }
    }
}

// MARK: - Webull (daily history and live price, no key)

/// Webull identifies securities by a numeric id, looked up by symbol (class shares written "BRK B").
enum WebullIds {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var cache: [String: Int] = [:]

    static func parse(_ data: Data, symbol: String) -> Int? {
        let rows = ((try? JSON.object(data))?["data"] as? [[String: Any]]) ?? []
        let hit = rows.first { r in
            (r["regionCode"] as? String) == "US"
                && ((r["disSymbol"] as? String) ?? "").replacingOccurrences(of: " ", with: "-").replacingOccurrences(of: ".", with: "-") == symbol
        }
        return hit.flatMap { JSON.double($0["tickerId"]).map { Int($0) } }
    }

    static func id(for symbol: String, transport: HTTPTransport) async throws -> Int {
        if let known = lock.withLock({ cache[symbol] }) { return known }
        let url = SourceHTTP.url("https://quotes-gw.webullfintech.com/api/search/pc/tickers",
                                 [("keyword", symbol.replacingOccurrences(of: "-", with: " ")), ("pageIndex", "1"), ("pageSize", "10")])
        guard let id = parse(try await SourceHTTP.get(transport, url), symbol: symbol) else {
            throw APIError.decoding("Webull : symbole inconnu")
        }
        lock.withLock { cache[symbol] = id }
        return id
    }
}

public struct WebullMarketData: MarketSource {
    public let name = "Webull"
    public let assetClass = AssetClass.stock
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func supports(_ timeframe: Timeframe) -> Bool { timeframe == .d1 }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard timeframe == .d1 else { throw APIError.decoding("Webull : journalier uniquement") }
        let id = try await WebullIds.id(for: asset.symbol, transport: transport)
        let url = SourceHTTP.url("https://quotes-gw.webullfintech.com/api/quote/charts/query",
                                 [("tickerIds", String(id)), ("type", "d1"), ("count", "800")])
        return Array(try Self.parse(try await SourceHTTP.get(transport, url), now: Date()).suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote { try await WebullQuotes(transport: transport).quote(for: asset) }

    /// [{"data": ["time (s, midnight New York),open,close,high,low,previousClose,volume,vwap", …]}], most recent first.
    static func parse(_ data: Data, now: Date) throws -> [Candle] {
        let rows = ((try JSON.array(data)).first as? [String: Any])?["data"] as? [String] ?? []
        let utc = ISO8601DateFormatter()
        return try rows.reversed().map { row in
            let f = row.split(separator: ",").map { Double($0) }
            guard f.count >= 7, let t = f[0] else { throw APIError.decoding("bougie Webull") }
            let day = String(utc.string(from: Date(timeIntervalSince1970: t)).prefix(10))
            return try sessionCandle(day, f[1], f[3], f[4], f[2], f[6], now: now, source: "Webull")
        }
    }
}

/// Webull live price (quote-only: cross-checks the intraday candles).
public struct WebullQuotes: MarketSource {
    public let name = "Webull (cours)"
    public let assetClass = AssetClass.stock
    public var providesCandles: Bool { false }
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("Webull : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let id = try await WebullIds.id(for: asset.symbol, transport: transport)
        let url = SourceHTTP.url("https://quotes-gw.webullfintech.com/api/stock/tickerRealTime/getQuote",
                                 [("tickerId", String(id)), ("includeSecu", "1")])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    /// {"close": "341.07", "changeRatio": "0.0153", …}
    static func parse(_ data: Data) throws -> Quote {
        let obj = try JSON.object(data)
        guard let price = JSON.double(obj["close"]), price > 0 else { throw APIError.decoding("Webull : cours absent") }
        return Quote(price: price, changePercent24h: (JSON.double(obj["changeRatio"]) ?? 0) * 100, time: Date())
    }
}

// MARK: - Zacks (quote feed, no key)

public struct ZacksQuotes: MarketSource {
    public let name = "Zacks"
    public let assetClass = AssetClass.stock
    public var providesCandles: Bool { false }
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        throw APIError.decoding("Zacks : cours uniquement")
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let url = SourceHTTP.url("https://quote-feed.zacks.com/index", [("t", dottedSymbol(asset.symbol))])
        return try Self.parse(try await SourceHTTP.get(transport, url))
    }

    /// {"AAPL": {"last": "341.07", "percent_net_change": "1.53…"}}
    static func parse(_ data: Data) throws -> Quote {
        guard let x = (try JSON.object(data)).values.first as? [String: Any], let price = JSON.double(x["last"]), price > 0 else {
            throw APIError.decoding("Zacks : symbole inconnu")
        }
        return Quote(price: price, changePercent24h: JSON.double(x["percent_net_change"]) ?? 0, time: Date())
    }
}
