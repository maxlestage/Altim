import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

// Market data read by the iPhone itself, straight from the exchanges and stock data providers (the same public
// sources as the server's backend/src/market.rs), then compared source by source: a price is trusted only when
// several independent providers agree.

extension Timeframe {
    public var stepMs: Double {
        switch self {
        case .h1: return 3_600_000
        case .h4: return 14_400_000
        case .d1: return 86_400_000
        case .d4: return 4 * 86_400_000
        case .w1: return 7 * 86_400_000
        }
    }
    /// Next timeframe up, confirming the trend (1 d keeps none, 4 d is confirmed by the week).
    public var higher: Timeframe? {
        switch self {
        case .h1: return .h4
        case .h4: return .d1
        case .d4: return .w1
        case .d1, .w1: return nil
        }
    }
    var multiDay: Bool { self == .d4 || self == .w1 }
}

/// HTTP reads of the public market APIs (replaceable in the tests).
public protocol MarketFetcher: Sendable {
    func data(_ url: String, method: String, body: Data?) async throws -> Data
}

extension MarketFetcher {
    func json(_ url: String) async throws -> Any {
        let d = try await data(url, method: "GET", body: nil)
        do { return try JSONSerialization.jsonObject(with: d, options: [.fragmentsAllowed]) } catch { throw MarketError("réponse illisible") }
    }
    func text(_ url: String) async throws -> String { String(decoding: try await data(url, method: "GET", body: nil), as: UTF8.self) }
}

public struct MarketError: Error, LocalizedError, Equatable {
    public var message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

/// URLSession reads with Altim's User-Agent and a short timeout (a slow source must not hold the others).
public struct URLSessionFetcher: MarketFetcher {
    /// Same identification as the Altim server's reads (backend/src/quotes.rs `UA`).
    static let userAgent = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0"
    let session: URLSession
    public init(timeout: TimeInterval = 10) {
        let c = URLSessionConfiguration.ephemeral
        c.timeoutIntervalForRequest = timeout
        c.httpCookieStorage = nil
        c.httpShouldSetCookies = false
        c.urlCache = nil
        c.requestCachePolicy = .reloadIgnoringLocalCacheData
        session = URLSession(configuration: c)
    }
    public func data(_ url: String, method: String, body: Data?) async throws -> Data {
        guard let u = URL(string: url) else { throw MarketError("adresse invalide") }
        var r = URLRequest(url: u)
        r.httpMethod = method
        r.httpBody = body
        r.setValue(Self.userAgent, forHTTPHeaderField: "User-Agent")
        r.setValue("application/json, text/plain, */*", forHTTPHeaderField: "Accept")
        if body != nil { r.setValue("application/json", forHTTPHeaderField: "Content-Type") }
        do {
            let (data, response) = try await session.data(for: r)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            guard (200..<300).contains(status) else { throw MarketError("HTTP \(status)") }
            return data
        } catch let e as MarketError {
            throw e
        } catch let e as URLError {
            throw MarketError(e.code == .timedOut ? "délai dépassé" : "réseau (\(e.code.rawValue))")
        }
    }
}

// MARK: - JSON helpers

/// Number from a JSON value: numbers, numeric strings ("$1,234.5" included); NaN otherwise.
func num(_ v: Any?) -> Double {
    switch v {
    case let n as NSNumber: return n.doubleValue
    case let s as String:
        let t = s.replacingOccurrences(of: "$", with: "").replacingOccurrences(of: ",", with: "").replacingOccurrences(of: "%", with: "")
            .trimmingCharacters(in: .whitespaces)
        return Double(t) ?? .nan
    default: return .nan
    }
}

func obj(_ v: Any?) -> [String: Any]? { v as? [String: Any] }
func arr(_ v: Any?) -> [Any]? { v as? [Any] }
func at(_ v: Any?, _ path: String...) -> Any? {
    var cur = v
    for k in path { cur = obj(cur)?[k] }
    return cur
}

func finitePositive(_ x: Double) -> Double? { x.isFinite && x > 0 ? x : nil }

/// 9:30 New York time of a session day, in ms (the time the stock providers give their daily candles).
func nyOpen(_ y: Int, _ m: Int, _ d: Int) -> Double? {
    var cal = Calendar(identifier: .gregorian)
    guard let tz = TimeZone(identifier: "America/New_York") else { return nil }
    cal.timeZone = tz
    guard let date = cal.date(from: DateComponents(year: y, month: m, day: d, hour: 9, minute: 30)) else { return nil }
    return (date.timeIntervalSince1970 * 1000).rounded()
}

func nowMs() -> Double { (Date().timeIntervalSince1970 * 1000).rounded() }

/// "BTC" → Kraken's "XBT".
func krakenBase(_ b: String) -> String { b == "BTC" ? "XBT" : b == "DOGE" ? "XDG" : b }

// MARK: - Parsers (formats checked against real answers)

enum Parse {
    static func rows(_ v: Any?, _ row: ([Any]) -> Bar?) throws -> [Bar] {
        guard let list = arr(v) else { throw MarketError("format inattendu") }
        return list.compactMap { arr($0).flatMap(row) }
    }
    static func bar(_ t: Double, _ o: Any?, _ h: Any?, _ l: Any?, _ c: Any?, _ v: Any?) -> Bar {
        let vol = num(v)
        return Bar(time: t, open: num(o), high: num(h), low: num(l), close: num(c), volume: vol.isFinite ? vol : 0)
    }
    static func el(_ r: [Any], _ i: Int) -> Any? { i < r.count ? r[i] : nil }

    /// Binance, MEXC, Binance.US: [openTime ms, "o", "h", "l", "c", "v", …].
    static func binance(_ d: Any) throws -> [Bar] {
        if let o = obj(d), let msg = o["msg"] as? String { throw MarketError(String(msg.prefix(60))) }
        return try rows(d) { r in bar(num(el(r, 0)), el(r, 1), el(r, 2), el(r, 3), el(r, 4), el(r, 5)) }
    }
    /// OKX: {"code":"0","data":[[ts, o, h, l, c, vol, …]]}, most recent first.
    static func okx(_ d: Any) throws -> [Bar] {
        guard (at(d, "code") as? String) == "0" else { throw MarketError("OKX") }
        return try rows(at(d, "data")) { r in bar(num(el(r, 0)), el(r, 1), el(r, 2), el(r, 3), el(r, 4), el(r, 5)) }.reversed()
    }
    /// Coinbase: [[time s, low, high, open, close, volume]], most recent first.
    static func coinbase(_ d: Any) throws -> [Bar] {
        try rows(d) { r in bar(num(el(r, 0)) * 1000, el(r, 3), el(r, 2), el(r, 1), el(r, 4), el(r, 5)) }.reversed()
    }
    /// Kraken: {"error":[],"result":{"XXBTZUSD":[[time s, o, h, l, c, vwap, volume, count]], "last": …}}.
    static func kraken(_ d: Any) throws -> [Bar] {
        if let e = at(d, "error") as? [Any], !e.isEmpty { throw MarketError(e.map { "\($0)" }.joined(separator: ",")) }
        guard let result = obj(at(d, "result")), let list = result.first(where: { $0.key != "last" })?.value else { throw MarketError("Kraken") }
        return try rows(list) { r in bar(num(el(r, 0)) * 1000, el(r, 1), el(r, 2), el(r, 3), el(r, 4), el(r, 6)) }
    }
    /// KuCoin: {"code":"200000","data":[[time s, open, close, high, low, volume, turnover]]}, most recent first.
    static func kucoin(_ d: Any) throws -> [Bar] {
        guard (at(d, "code") as? String) == "200000" else { throw MarketError("KuCoin") }
        return try rows(at(d, "data")) { r in bar(num(el(r, 0)) * 1000, el(r, 1), el(r, 3), el(r, 4), el(r, 2), el(r, 5)) }.reversed()
    }
    /// Gate.io: [[time s, quote volume, close, high, low, open, base volume, closed]].
    static func gate(_ d: Any) throws -> [Bar] {
        if let o = obj(d), let m = o["message"] as? String { throw MarketError(String(m.prefix(60))) }
        return try rows(d) { r in bar(num(el(r, 0)) * 1000, el(r, 5), el(r, 3), el(r, 4), el(r, 2), el(r, 6)) }
    }
    /// Bitstamp: {"data":{"ohlc":[{"timestamp","open","high","low","close","volume"}]}}.
    static func bitstamp(_ d: Any) throws -> [Bar] {
        guard let list = arr(at(d, "data", "ohlc")) else { throw MarketError("Bitstamp") }
        return list.compactMap { obj($0) }.map { r in bar(num(r["timestamp"]) * 1000, r["open"], r["high"], r["low"], r["close"], r["volume"]) }
    }
    /// Bitfinex: [[ms, open, CLOSE, high, low, volume]], most recent first.
    static func bitfinex(_ d: Any) throws -> [Bar] {
        try rows(d) { r in bar(num(el(r, 0)), el(r, 1), el(r, 3), el(r, 4), el(r, 2), el(r, 5)) }.reversed()
    }
    /// Yahoo Finance chart: timestamps in seconds, quote arrays with nulls skipped.
    static func yahoo(_ d: Any) throws -> [Bar] {
        guard let r = obj(arr(at(d, "chart", "result"))?.first) else {
            throw MarketError((at(d, "chart", "error", "description") as? String) ?? "Yahoo : aucune donnée")
        }
        let q = obj(arr(at(r, "indicators", "quote"))?.first) ?? [:]
        let ts = arr(r["timestamp"]) ?? []
        func series(_ k: String) -> [Any] { arr(q[k]) ?? [] }
        let (o, h, l, c, v) = (series("open"), series("high"), series("low"), series("close"), series("volume"))
        var out: [Bar] = []
        for (i, t) in ts.enumerated() {
            let vals = [o, h, l, c].map { i < $0.count ? num($0[i]) : .nan }
            guard vals.allSatisfy(\.isFinite) else { continue }
            let vol = i < v.count ? num(v[i]) : 0
            out.append(Bar(time: num(t) * 1000, open: vals[0], high: vals[1], low: vals[2], close: vals[3], volume: vol.isFinite ? vol : 0))
        }
        return out
    }
    /// Nasdaq history: rows {"date":"10/29/2026","open":"$348.00",…}, most recent first; dated at the 9:30 NY open.
    static func nasdaq(_ d: Any) throws -> [Bar] {
        let rows = arr(at(d, "data", "tradesTable", "rows")) ?? []
        return rows.compactMap { obj($0) }.compactMap { r -> Bar? in
            let parts = ((r["date"] as? String) ?? "").split(separator: "/").compactMap { Int($0) }
            guard parts.count == 3, let t = nyOpen(parts[2], parts[0], parts[1]) else { return nil }
            return bar(t, r["open"], r["high"], r["low"], r["close"], r["volume"])
        }.reversed()
    }
    /// Robinhood daily history, regular session only.
    static func robinhood(_ d: Any) throws -> [Bar] {
        let list = arr(at(d, "historicals")) ?? []
        return list.compactMap { obj($0) }.compactMap { r -> Bar? in
            guard (r["session"] as? String) == "reg", (r["interpolated"] as? Bool) != true else { return nil }
            let parts = String(((r["begins_at"] as? String) ?? "").prefix(10)).split(separator: "-").compactMap { Int($0) }
            guard parts.count == 3, let t = nyOpen(parts[0], parts[1], parts[2]) else { return nil }
            return bar(t, r["open_price"], r["high_price"], r["low_price"], r["close_price"], r["volume"])
        }
    }
}

// MARK: - Candle helpers

enum Candles {
    /// Only closed candles, sorted. Stock daily sessions last 6 h 30, not 24 h.
    static func closed(_ c: [Bar], _ tf: Timeframe, kind: Kind, now: Double = nowMs()) -> [Bar] {
        let duration = kind == .stock && tf == .d1 ? 23_400_000 : tf.stepMs
        return c.filter { $0.time + duration <= now && $0.close.isFinite && $0.close > 0 }.sorted { $0.time < $1.time }
    }

    /// Candles of `from` ms grouped into complete candles of `to` ms (UTC-aligned).
    static func aggregate(_ candles: [Bar], from: Double, to: Double) -> [Bar] {
        let per = Int((to / from).rounded())
        var groups: [(Double, [Bar])] = []
        for c in candles.sorted(by: { $0.time < $1.time }) {
            let key = (c.time / to).rounded(.down) * to
            if groups.last?.0 == key { groups[groups.count - 1].1.append(c) } else { groups.append((key, [c])) }
        }
        return groups.filter { $0.1.count == per }.map { merge($0.0, $0.1) }
    }

    /// 1 h → 4 h for stocks: 4-bar blocks inside each New York session.
    static func aggregateSession(_ candles: [Bar], per: Int) -> [Bar] {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "America/New_York") ?? .current
        func day(_ t: Double) -> DateComponents { cal.dateComponents([.year, .month, .day], from: Date(timeIntervalSince1970: t / 1000)) }
        var out: [Bar] = []
        var bucket: [Bar] = []
        for c in candles {
            if !bucket.isEmpty && (bucket.count == per || day(bucket[0].time) != day(c.time)) {
                out.append(merge(bucket[0].time, bucket))
                bucket = []
            }
            bucket.append(c)
        }
        if !bucket.isEmpty { out.append(merge(bucket[0].time, bucket)) }
        return out
    }

    static func merge(_ time: Double, _ items: [Bar]) -> Bar {
        Bar(time: time, open: items[0].open, high: items.map(\.high).max() ?? .nan, low: items.map(\.low).min() ?? .nan,
            close: items[items.count - 1].close, volume: items.reduce(0) { $0 + $1.volume })
    }

    /// Monday 5 January 1970 00:00 UTC (the epoch was a Thursday): weeks start on Monday, 4 d buckets on the epoch.
    static func bucketStart(_ time: Double, _ tf: Timeframe) -> Double {
        let anchor = tf == .w1 ? 4 * 86_400_000.0 : 0
        return anchor + ((time - anchor) / tf.stepMs).rounded(.down) * tf.stepMs
    }

    /// Daily candles grouped into 4 d / 1 w buckets; the first (maybe cut) bucket is dropped.
    static func bucket(_ daily: [Bar], _ tf: Timeframe) -> [Bar] {
        var out: [Bar] = []
        for c in daily.sorted(by: { $0.time < $1.time }) {
            let key = bucketStart(c.time, tf)
            if var b = out.last, b.time == key {
                b.high = max(b.high, c.high)
                b.low = min(b.low, c.low)
                b.close = c.close
                b.volume += c.volume
                out[out.count - 1] = b
            } else {
                var n = c
                n.time = key
                out.append(n)
            }
        }
        if !out.isEmpty { out.removeFirst() }
        return out
    }

    /// Native weekly candles dated at their bucket start, one per bucket.
    static func align(_ candles: [Bar], _ tf: Timeframe) -> [Bar] {
        var out: [Bar] = []
        for c in candles.sorted(by: { $0.time < $1.time }) {
            let key = bucketStart(c.time, tf)
            if out.last?.time != key {
                var n = c
                n.time = key
                out.append(n)
            }
        }
        return out
    }

    /// Median deviation (%) of each series from the per-timestamp median, over the last 20 shared candles.
    static func deviations(_ series: [[Bar]]) -> [Double] {
        let maps = series.map { s in Dictionary(s.map { ($0.time, $0.close) }, uniquingKeysWith: { a, _ in a }) }
        return maps.indices.map { i in
            let m = maps[i]
            let shared = m.keys.filter { t in maps.indices.contains { $0 != i && maps[$0][t] != nil } }.sorted().suffix(20)
            guard !shared.isEmpty else { return .infinity }
            let devs = shared.map { t -> Double in
                let ref = Indicators.median(maps.compactMap { $0[t] })
                return abs(m[t]! / ref - 1) * 100
            }
            return Indicators.median(devs)
        }
    }

    /// Consensus candles: for each candle of the primary source, the median of every agreeing source with that time.
    static func blend(_ primary: [Bar], _ others: [[Bar]]) -> [Bar] {
        guard !others.isEmpty else { return primary }
        let maps = others.map { o in Dictionary(o.map { ($0.time, $0) }, uniquingKeysWith: { a, _ in a }) }
        return primary.map { c in
            let same = [c] + maps.compactMap { $0[c.time] }
            guard same.count >= 2 else { return c }
            func pick(_ f: (Bar) -> Double) -> Double { Indicators.median(same.map(f)) }
            let open = pick(\.open), close = pick(\.close)
            return Bar(time: c.time, open: open, high: max(pick(\.high), open, close), low: min(pick(\.low), open, close), close: close, volume: c.volume)
        }
    }
}

// MARK: - Candle sources

struct CandleSource: Sendable {
    let name: String
    let supports: @Sendable (Timeframe) -> Bool
    let fetch: @Sendable (MarketFetcher, String, Timeframe) async throws -> [Bar]
    init(_ name: String, supports: @escaping @Sendable (Timeframe) -> Bool = { !$0.multiDay },
         _ fetch: @escaping @Sendable (MarketFetcher, String, Timeframe) async throws -> [Bar]) {
        self.name = name
        self.supports = supports
        self.fetch = fetch
    }

    /// A daily source turned into a 4 d / 1 w one (its days grouped into buckets).
    func fromDaily(_ fetchDaily: (@Sendable (MarketFetcher, String) async throws -> [Bar])? = nil) -> CandleSource {
        let base = fetch
        let daily = fetchDaily ?? { f, s in try await base(f, s, .d1) }
        return CandleSource(name, supports: { $0.multiDay }) { f, s, tf in Candles.bucket(try await daily(f, s), tf) }
    }
}

enum Sources {
    static func pick(_ tf: Timeframe, _ h1: String, _ h4: String, _ d1: String) -> String { tf == .h1 ? h1 : tf == .h4 ? h4 : d1 }
    static func enc(_ s: String) -> String { s.addingPercentEncoding(withAllowedCharacters: .alphanumerics.union(.init(charactersIn: "-._~"))) ?? s }

    static let binance = CandleSource("Binance") { f, b, tf in
        try Parse.binance(try await f.json("https://api.binance.com/api/v3/klines?symbol=\(b)USDT&interval=\(tf.rawValue)&limit=500"))
    }
    static let okx = CandleSource("OKX") { f, b, tf in
        try Parse.okx(try await f.json("https://www.okx.com/api/v5/market/candles?instId=\(b)-USDT&bar=\(pick(tf, "1H", "4H", "1Dutc"))&limit=300"))
    }
    static let coinbase = CandleSource("Coinbase") { f, b, tf in
        let c = try Parse.coinbase(try await f.json("https://api.exchange.coinbase.com/products/\(b)-USD/candles?granularity=\(tf == .d1 ? 86400 : 3600)"))
        return tf == .h4 ? Candles.aggregate(c, from: Timeframe.h1.stepMs, to: Timeframe.h4.stepMs) : c
    }
    static let kraken = CandleSource("Kraken") { f, b, tf in
        try Parse.kraken(try await f.json("https://api.kraken.com/0/public/OHLC?pair=\(krakenBase(b))USD&interval=\(Int(tf.stepMs / 60_000))"))
    }
    static let kucoin = CandleSource("KuCoin") { f, b, tf in
        let end = Int(nowMs() / 1000)
        let start = end - Int(tf.stepMs / 1000) * 500
        return try Parse.kucoin(try await f.json("https://api.kucoin.com/api/v1/market/candles?type=\(pick(tf, "1hour", "4hour", "1day"))&symbol=\(b)-USDT&startAt=\(start)&endAt=\(end)"))
    }
    static let gate = CandleSource("Gate.io") { f, b, tf in
        try Parse.gate(try await f.json("https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=\(b)_USDT&interval=\(tf.rawValue)&limit=500"))
    }
    static let bitstamp = CandleSource("Bitstamp") { f, b, tf in
        try Parse.bitstamp(try await f.json("https://www.bitstamp.net/api/v2/ohlc/\(b.lowercased())usd/?step=\(Int(tf.stepMs / 1000))&limit=500"))
    }
    static let bitfinex = CandleSource("Bitfinex") { f, b, tf in
        let c = try Parse.bitfinex(try await f.json("https://api-pub.bitfinex.com/v2/candles/trade:\(tf == .d1 ? "1D" : "1h"):t\(b)USD/hist?limit=\(tf == .h4 ? 2000 : 500)"))
        return tf == .h4 ? Candles.aggregate(c, from: Timeframe.h1.stepMs, to: Timeframe.h4.stepMs) : c
    }

    /// Long daily histories (≈ 1 000 days) for 4 d / 1 w.
    static let longDaily: [(String, @Sendable (MarketFetcher, String) async throws -> [Bar])] = [
        ("Bitstamp", { f, b in try Parse.bitstamp(try await f.json("https://www.bitstamp.net/api/v2/ohlc/\(b.lowercased())usd/?step=86400&limit=1000")) }),
        ("Binance", { f, b in try Parse.binance(try await f.json("https://api.binance.com/api/v3/klines?symbol=\(b)USDT&interval=1d&limit=1000")) }),
        ("Gate.io", { f, b in try Parse.gate(try await f.json("https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=\(b)_USDT&interval=1d&limit=1000")) }),
        ("Kraken", { f, b in try Parse.kraken(try await f.json("https://api.kraken.com/0/public/OHLC?pair=\(krakenBase(b))USD&interval=1440")) }),
        ("OKX", { f, b in try Parse.okx(try await f.json("https://www.okx.com/api/v5/market/history-candles?instId=\(b)-USDT&bar=1Dutc&limit=300")) }),
    ]

    static let crypto: [CandleSource] = [binance, okx, coinbase, kraken, kucoin, gate, bitstamp, bitfinex]
    /// The long daily histories as daily sources (the backtest).
    static let cryptoLongDaily: [CandleSource] = longDaily.map { name, fetch in
        CandleSource(name, supports: { $0 == .d1 }) { f, b, _ in try await fetch(f, b) }
    }
    static let cryptoMultiDay: [CandleSource] = longDaily.map { name, fetch in
        CandleSource(name, supports: { $0.multiDay }) { f, b, tf in Candles.bucket(try await fetch(f, b), tf) }
    }

    static func dotted(_ s: String) -> String { s.replacingOccurrences(of: "-", with: ".") }

    static let yahoo = CandleSource("Yahoo Finance", supports: { _ in true }) { f, s, tf in
        let (interval, range): (String, String) = switch tf {
        case .d1: ("1d", "2y")
        case .h4: ("1h", "2y")
        case .h1: ("1h", "6mo")
        case .d4: ("1d", "5y")
        case .w1: ("1wk", "10y")
        }
        let c = try Parse.yahoo(try await f.json("https://query1.finance.yahoo.com/v8/finance/chart/\(enc(s))?interval=\(interval)&range=\(range)&includePrePost=false"))
        switch tf {
        case .h4: return Candles.aggregateSession(c, per: 4)
        case .d4: return Candles.bucket(c, .d4)
        case .w1: return Candles.align(c, .w1)
        default: return c
        }
    }
    static let nasdaq = CandleSource("Nasdaq", supports: { $0 == .d1 }) { f, s, _ in
        let fmt = DateFormatter()
        fmt.dateFormat = "yyyy-MM-dd"
        fmt.timeZone = TimeZone(identifier: "UTC")
        let to = fmt.string(from: Date()), from = fmt.string(from: Date().addingTimeInterval(-1300 * 86400))
        for cls in ["stocks", "etf"] {
            let c = try Parse.nasdaq(try await f.json("https://api.nasdaq.com/api/quote/\(enc(dotted(s)))/historical?assetclass=\(cls)&fromdate=\(from)&todate=\(to)&limit=9999"))
            if !c.isEmpty { return c }
        }
        throw MarketError("symbole inconnu")
    }
    static let robinhood = CandleSource("Robinhood", supports: { $0 == .d1 }) { f, s, _ in
        try Parse.robinhood(try await f.json("https://api.robinhood.com/marketdata/historicals/\(enc(dotted(s)))/?interval=day&span=5year&bounds=regular"))
    }

    static let stock: [CandleSource] = [yahoo, nasdaq, robinhood, nasdaq.fromDaily(), robinhood.fromDaily()]

    static func candles(_ kind: Kind, _ tf: Timeframe) -> [CandleSource] {
        switch kind {
        case .crypto: return tf.multiDay ? cryptoMultiDay : crypto
        case .stock: return stock.filter { $0.supports(tf) }
        }
    }
}

// MARK: - Consensus

public struct SourceCheck: Sendable, Equatable {
    public var name: String
    public var ok: Bool
    public var deviation: Double?
    public var error: String?
}

struct CandleConsensus: Sendable {
    var candles: [Bar]
    var source: String
    var agreeing: Int
    var conflict: Bool
    var sources: [SourceCheck]
}

enum Consensus {
    /// Asks every source in parallel, drops the lagging ones, compares the others with the median and blends the
    /// agreeing ones (tolerance in % of median deviation).
    static func candles(_ symbol: String, kind: Kind, _ tf: Timeframe, sources: [CandleSource], fetcher: MarketFetcher,
                        tolerance: Double, now: Double = nowMs()) async throws -> CandleConsensus {
        let list = sources.filter { $0.supports(tf) }
        var results: [(name: String, candles: [Bar]?, error: String?)] = await withTaskGroup(of: (Int, String, [Bar]?, String?).self) { g in
            for (i, s) in list.enumerated() {
                g.addTask {
                    do {
                        let c = Candles.closed(try await s.fetch(fetcher, symbol, tf), tf, kind: kind, now: now)
                        return c.isEmpty ? (i, s.name, nil, "vide") : (i, s.name, c, nil)
                    } catch {
                        return (i, s.name, nil, (error as? MarketError)?.message ?? error.localizedDescription)
                    }
                }
            }
            var out: [(Int, String, [Bar]?, String?)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map { ($0.1, $0.2, $0.3) }
        }
        // A source whose last candle lags behind the others (inactive market, delisted pair) is discarded.
        if let latest = results.compactMap({ $0.candles?.last?.time }).max() {
            for i in results.indices {
                if let last = results[i].candles?.last?.time, last < latest - 2 * tf.stepMs {
                    results[i].error = "en retard"
                    results[i].candles = nil
                }
            }
        }
        let ok = results.indices.filter { results[$0].candles != nil }
        guard !ok.isEmpty else { throw MarketError("Toutes les sources ont échoué") }
        let devs = ok.count > 1 ? Candles.deviations(ok.map { results[$0].candles! }) : [0]
        let agreeing = ok.indices.filter { devs[$0] <= tolerance }
        let primary = agreeing.first { results[ok[$0]].candles!.count >= 60 } ?? agreeing.first ?? 0
        let others = agreeing.filter { $0 != primary }.map { results[ok[$0]].candles! }
        let blended = Candles.blend(results[ok[primary]].candles!, others)
        let checks = results.indices.map { i -> SourceCheck in
            if let k = ok.firstIndex(of: i) { return SourceCheck(name: results[i].name, ok: devs[k] <= tolerance, deviation: devs[k], error: nil) }
            return SourceCheck(name: results[i].name, ok: false, deviation: nil, error: results[i].error)
        }
        return CandleConsensus(candles: blended, source: results[ok[primary]].name, agreeing: agreeing.count,
                               conflict: ok.count > 1 && agreeing.count * 2 < ok.count, sources: checks)
    }
}
