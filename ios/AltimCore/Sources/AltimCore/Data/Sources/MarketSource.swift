import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// A named market data source, used by the consensus engine.
public protocol MarketSource: MarketDataProvider {
    var name: String { get }
    var assetClass: AssetClass { get }
    /// `false` for quote-only sources (no candles).
    var providesCandles: Bool { get }
    func supports(_ asset: Asset) -> Bool
    func supports(_ timeframe: Timeframe) -> Bool
}

public extension MarketSource {
    var providesCandles: Bool { true }
    func supports(_ asset: Asset) -> Bool { asset.assetClass == assetClass }
    func supports(_ timeframe: Timeframe) -> Bool { true }
}

/// Opening of the New York session (9:30 local time, US daylight saving time handled) for a given date.
func newYorkOpen(year: Int, month: Int, day: Int) -> Date? {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = TimeZone(identifier: "America/New_York")!
    return cal.date(from: DateComponents(year: year, month: month, day: day, hour: 9, minute: 30))
}

enum SourceHTTP {
    static func get(_ transport: HTTPTransport, _ url: URL, headers: [String: String] = [:]) async throws -> Data {
        var request = URLRequest(url: url)
        request.setValue("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Altim/1.0", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        for (k, v) in headers { request.setValue(v, forHTTPHeaderField: k) }
        let (data, response) = try await transport.send(request)
        if response.statusCode == 451 || response.statusCode == 403 { throw APIError.unavailableInRegion }
        guard (200..<300).contains(response.statusCode) else {
            let body = String(data: data.prefix(200), encoding: .utf8) ?? ""
            throw APIError.http(status: response.statusCode, message: body)
        }
        return data
    }

    static func url(_ base: String, _ query: [(String, String)]) -> URL {
        var c = URLComponents(string: base)!
        c.queryItems = query.map { URLQueryItem(name: $0.0, value: $0.1) }
        return c.url!
    }
}

/// Price and 24h change derived from 1h candles (the last, possibly unfinished candle included).
func quoteFromCandles(_ source: MarketDataProvider, _ asset: Asset) async throws -> Quote {
    let candles = try await source.candles(for: asset, timeframe: .h1, limit: 30)
        .filter(\.isValid).sorted { $0.time < $1.time }
    guard let last = candles.last else { throw APIError.decoding("cours vide") }
    let dayAgo = candles.last { $0.time <= last.time.addingTimeInterval(-86_400 + 1) } ?? candles.first!
    let change = dayAgo.close > 0 ? (last.close / dayAgo.close - 1) * 100 : 0
    return Quote(price: last.close, changePercent24h: change, time: last.time)
}

/// Aggregates regular candles (24/7 markets) into larger buckets aligned on UTC.
public enum CandleAggregator {
    public static func aggregate(_ candles: [Candle], from step: TimeInterval, to bucket: TimeInterval) -> [Candle] {
        guard bucket > step, step > 0 else { return candles }
        let perBucket = Int((bucket / step).rounded())
        var groups: [(key: TimeInterval, items: [Candle])] = []
        for c in candles.sorted(by: { $0.time < $1.time }) {
            let key = floor(c.time.timeIntervalSince1970 / bucket) * bucket
            if groups.last?.key == key { groups[groups.count - 1].items.append(c) } else { groups.append((key, [c])) }
        }
        return groups.enumerated().compactMap { index, group in
            let items = group.items
            let isLast = index == groups.count - 1
            // Incomplete buckets are dropped, except the current one (marked unfinished).
            guard items.count == perBucket || isLast, let first = items.first, let last = items.last else { return nil }
            return Candle(time: Date(timeIntervalSince1970: group.key), open: first.open,
                          high: items.map(\.high).max()!, low: items.map(\.low).min()!, close: last.close,
                          volume: items.reduce(0) { $0 + $1.volume },
                          isClosed: items.count == perBucket && items.allSatisfy(\.isClosed))
        }
    }
}
