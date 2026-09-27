import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public struct Quote: Hashable, Sendable {
    public let price: Double
    public let changePercent24h: Double
    public let time: Date

    public init(price: Double, changePercent24h: Double, time: Date) {
        self.price = price
        self.changePercent24h = changePercent24h
        self.time = time
    }
}

/// Source de données de marché. Brancher ici un autre fournisseur (Bloomberg B-PIPE, Polygon, etc.).
public protocol MarketDataProvider: Sendable {
    func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle]
    func quote(for asset: Asset) async throws -> Quote
}

// MARK: - Binance (crypto)

public struct BinanceMarketData: MarketDataProvider {
    public let baseURL: URL
    let transport: HTTPTransport

    public init(baseURL: URL = URL(string: "https://api.binance.com")!, transport: HTTPTransport = URLSessionTransport()) {
        self.baseURL = baseURL
        self.transport = transport
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int = 500) async throws -> [Candle] {
        var components = URLComponents(url: baseURL.appendingPathComponent("/api/v3/klines"), resolvingAgainstBaseURL: false)!
        components.queryItems = [
            URLQueryItem(name: "symbol", value: asset.symbol),
            URLQueryItem(name: "interval", value: timeframe.rawValue),
            URLQueryItem(name: "limit", value: String(min(max(limit, 1), 1000))),
        ]
        let data = try await get(components.url!)
        return try Self.parseKlines(data, now: Date())
    }

    public func quote(for asset: Asset) async throws -> Quote {
        var components = URLComponents(url: baseURL.appendingPathComponent("/api/v3/ticker/24hr"), resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "symbol", value: asset.symbol)]
        let obj = try JSON.object(try await get(components.url!))
        guard let price = JSON.double(obj["lastPrice"]), let change = JSON.double(obj["priceChangePercent"]) else {
            throw APIError.decoding("ticker Binance")
        }
        let closeTime = JSON.double(obj["closeTime"]).map { Date(timeIntervalSince1970: $0 / 1000) } ?? Date()
        return Quote(price: price, changePercent24h: change, time: closeTime)
    }

    static func parseKlines(_ data: Data, now: Date) throws -> [Candle] {
        let rows = try JSON.array(data)
        return try rows.map { row in
            guard let r = row as? [Any], r.count >= 7,
                  let openTime = JSON.double(r[0]), let o = JSON.double(r[1]), let h = JSON.double(r[2]),
                  let l = JSON.double(r[3]), let c = JSON.double(r[4]), let v = JSON.double(r[5]),
                  let closeTime = JSON.double(r[6]) else { throw APIError.decoding("bougie Binance") }
            return Candle(time: Date(timeIntervalSince1970: openTime / 1000), open: o, high: h, low: l, close: c,
                          volume: v, isClosed: closeTime / 1000 < now.timeIntervalSince1970)
        }
    }

    private func get(_ url: URL) async throws -> Data {
        let (data, response) = try await transport.send(URLRequest(url: url))
        if response.statusCode == 451 || response.statusCode == 403 { throw APIError.unavailableInRegion }
        guard (200..<300).contains(response.statusCode) else {
            let message = (try? JSON.object(data))?["msg"] as? String ?? HTTPURLResponse.localizedString(forStatusCode: response.statusCode)
            throw APIError.http(status: response.statusCode, message: message)
        }
        return data
    }
}

// MARK: - Yahoo Finance (actions, ETF, indices, crypto en secours)

public struct YahooMarketData: MarketSource {
    public let name: String
    public let assetClass = AssetClass.stock
    let transport: HTTPTransport
    let baseURL: URL

    /// `server` : 1 ou 2 (query1 / query2) — deux serveurs indépendants pour la redondance.
    public init(server: Int = 1, transport: HTTPTransport = URLSessionTransport()) {
        self.transport = transport
        self.baseURL = URL(string: "https://query\(server).finance.yahoo.com")!
        self.name = server == 1 ? "Yahoo Finance" : "Yahoo Finance (2)"
    }

    /// Yahoo cote aussi les cryptos (BTC-USD) : source de secours.
    public func supports(_ asset: Asset) -> Bool { true }

    /// Symbole Yahoo : `BTC-USD` pour une crypto, inchangé pour une action.
    static func yahooSymbol(_ asset: Asset) -> String {
        asset.assetClass == .crypto ? "\(asset.base)-USD" : asset.symbol
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int = 500) async throws -> [Candle] {
        // Yahoo ne propose pas le 4 h : on agrège des bougies 1 h.
        let (interval, range): (String, String) = switch timeframe {
        case .m15: ("15m", "30d")
        case .h1: ("1h", "6mo")
        case .h4: ("1h", "2y")
        case .d1: ("1d", "5y")
        }
        let data = try await chart(symbol: Self.yahooSymbol(asset), interval: interval, range: range)
        var candles = try Self.parseChart(data, interval: timeframe == .h4 ? .h1 : timeframe,
                                          isStock: asset.assetClass == .stock, now: Date())
        if timeframe == .h4 { candles = Self.aggregate(candles, by: 4) }
        return Array(candles.suffix(limit))
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let data = try await chart(symbol: Self.yahooSymbol(asset), interval: "1d", range: "5d")
        let meta = try Self.meta(data)
        guard let price = JSON.double(meta["regularMarketPrice"]) else { throw APIError.decoding("cours Yahoo") }
        let previous = JSON.double(meta["chartPreviousClose"]) ?? JSON.double(meta["previousClose"]) ?? price
        let time = JSON.double(meta["regularMarketTime"]).map { Date(timeIntervalSince1970: $0) } ?? Date()
        return Quote(price: price, changePercent24h: previous > 0 ? (price / previous - 1) * 100 : 0, time: time)
    }

    private func chart(symbol: String, interval: String, range: String) async throws -> Data {
        let encoded = symbol.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? symbol
        var components = URLComponents(string: "\(baseURL.absoluteString)/v8/finance/chart/\(encoded)")!
        components.queryItems = [
            URLQueryItem(name: "interval", value: interval),
            URLQueryItem(name: "range", value: range),
            URLQueryItem(name: "includePrePost", value: "false"),
        ]
        var request = URLRequest(url: components.url!)
        request.setValue("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)", forHTTPHeaderField: "User-Agent")
        let (data, response) = try await transport.send(request)
        guard (200..<300).contains(response.statusCode) else {
            throw APIError.http(status: response.statusCode, message: "Symbole introuvable ou service indisponible")
        }
        return data
    }

    static func meta(_ data: Data) throws -> [String: Any] {
        let root = try JSON.object(data)
        guard let chart = root["chart"] as? [String: Any],
              let result = (chart["result"] as? [[String: Any]])?.first,
              let meta = result["meta"] as? [String: Any] else { throw APIError.decoding("graphique Yahoo") }
        return meta
    }

    static func parseChart(_ data: Data, interval: Timeframe, isStock: Bool, now: Date) throws -> [Candle] {
        let root = try JSON.object(data)
        guard let chart = root["chart"] as? [String: Any],
              let result = (chart["result"] as? [[String: Any]])?.first else {
            let message = ((root["chart"] as? [String: Any])?["error"] as? [String: Any])?["description"] as? String
            throw APIError.http(status: 404, message: message ?? "Aucune donnée")
        }
        guard let timestamps = result["timestamp"] as? [Any],
              let indicators = result["indicators"] as? [String: Any],
              let quote = (indicators["quote"] as? [[String: Any]])?.first else { return [] }
        let opens = quote["open"] as? [Any] ?? []
        let highs = quote["high"] as? [Any] ?? []
        let lows = quote["low"] as? [Any] ?? []
        let closes = quote["close"] as? [Any] ?? []
        let volumes = quote["volume"] as? [Any] ?? []

        // Durée réelle d'une bougie journalière d'action : la séance (6 h 30) et non 24 h.
        let duration = (isStock && interval == .d1) ? 6.5 * 3600 : interval.seconds
        var candles: [Candle] = []
        for i in timestamps.indices {
            guard let t = JSON.double(timestamps[i]),
                  i < closes.count, i < opens.count, i < highs.count, i < lows.count,
                  let o = JSON.double(opens[i]), let h = JSON.double(highs[i]),
                  let l = JSON.double(lows[i]), let c = JSON.double(closes[i]) else { continue }
            let v = i < volumes.count ? (JSON.double(volumes[i]) ?? 0) : 0
            let time = Date(timeIntervalSince1970: t)
            candles.append(Candle(time: time, open: o, high: h, low: l, close: c, volume: v,
                                  isClosed: time.timeIntervalSince1970 + duration <= now.timeIntervalSince1970))
        }
        return candles
    }

    /// Regroupe les bougies par paquets de `size` (utilisé pour fabriquer du 4 h à partir du 1 h).
    static func aggregate(_ candles: [Candle], by size: Int) -> [Candle] {
        guard size > 1 else { return candles }
        let calendar = Calendar(identifier: .gregorian)
        var result: [Candle] = []
        var bucket: [Candle] = []
        func flush() {
            guard let first = bucket.first, let last = bucket.last else { return }
            result.append(Candle(time: first.time, open: first.open, high: bucket.map(\.high).max()!,
                                 low: bucket.map(\.low).min()!, close: last.close,
                                 volume: bucket.reduce(0) { $0 + $1.volume }, isClosed: bucket.allSatisfy(\.isClosed)))
            bucket.removeAll()
        }
        for candle in candles {
            let hour = calendar.dateComponents(in: TimeZone(identifier: "UTC")!, from: candle.time).hour ?? 0
            if let first = bucket.first,
               !calendar.isDate(first.time, inSameDayAs: candle.time) || bucket.count == size || hour % size == 0 {
                flush()
            }
            bucket.append(candle)
        }
        flush()
        return result
    }

    /// Recherche d'actifs (actions, ETF, indices).
    public func search(_ query: String) async throws -> [Asset] {
        var components = URLComponents(string: "https://query2.finance.yahoo.com/v1/finance/search")!
        components.queryItems = [URLQueryItem(name: "q", value: query), URLQueryItem(name: "quotesCount", value: "10"),
                                 URLQueryItem(name: "newsCount", value: "0")]
        var request = URLRequest(url: components.url!)
        request.setValue("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)", forHTTPHeaderField: "User-Agent")
        let (data, _) = try await transport.send(request)
        let quotes = (try JSON.object(data))["quotes"] as? [[String: Any]] ?? []
        return quotes.compactMap { q in
            guard let symbol = q["symbol"] as? String,
                  let type = q["quoteType"] as? String, ["EQUITY", "ETF", "INDEX"].contains(type) else { return nil }
            let name = (q["shortname"] as? String) ?? (q["longname"] as? String) ?? symbol
            return Asset(symbol: symbol, name: name, assetClass: .stock, quote: "USD")
        }
    }
}

// MARK: - Routage avec repli

/// Choisit le bon fournisseur selon la classe d'actif, avec repli sur Yahoo pour la crypto
/// si Binance est indisponible (panne, restriction géographique).
public struct MarketRouter: MarketDataProvider {
    let crypto: MarketDataProvider
    let stocks: MarketDataProvider

    public init(crypto: MarketDataProvider = BinanceMarketData(), stocks: MarketDataProvider = YahooMarketData()) {
        self.crypto = crypto
        self.stocks = stocks
    }

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        guard asset.assetClass == .crypto else { return try await stocks.candles(for: asset, timeframe: timeframe, limit: limit) }
        do {
            return try await crypto.candles(for: asset, timeframe: timeframe, limit: limit)
        } catch {
            return try await stocks.candles(for: asset, timeframe: timeframe, limit: limit)
        }
    }

    public func quote(for asset: Asset) async throws -> Quote {
        guard asset.assetClass == .crypto else { return try await stocks.quote(for: asset) }
        do {
            return try await crypto.quote(for: asset)
        } catch {
            return try await stocks.quote(for: asset)
        }
    }
}
