import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Data for the market guard (same sources and formats as web/server/guard.ts). Each input is optional:
/// a missing one only removes its factors, never blocks the guard.
public struct GuardDataProvider: Sendable {
    let transport: HTTPTransport
    public init(transport: HTTPTransport = URLSessionTransport()) { self.transport = transport }

    // MARK: Parsers

    /// OKX current funding rate: {"code":"0","data":[{"fundingRate":"-0.0000101"}]}.
    static func funding(_ data: Data) -> Double? {
        guard let obj = try? JSON.object(data), obj["code"] as? String == "0",
              let row = (obj["data"] as? [[String: Any]])?.first else { return nil }
        return JSON.double(row["fundingRate"])
    }

    /// OKX rubik series [[ts, value, …]], most recent first → values oldest first.
    static func rubik(_ data: Data, column: Int = 1) -> [Double] {
        guard let obj = try? JSON.object(data), obj["code"] as? String == "0", let rows = obj["data"] as? [[Any]] else { return [] }
        return rows.compactMap { $0.count > column ? JSON.double($0[column]) : nil }.reversed()
    }

    /// alternative.me Fear & Greed history, most recent first → oldest first.
    static func fearGreed(_ data: Data) -> [Double] {
        guard let obj = try? JSON.object(data), let rows = obj["data"] as? [[String: Any]] else { return [] }
        return rows.compactMap { JSON.double($0["value"]) }.reversed()
    }

    static func decode(_ s: String) -> String {
        var t = s
        if let r = t.range(of: "<![CDATA[") , let e = t.range(of: "]]>", range: r.upperBound..<t.endIndex) {
            t = String(t[r.upperBound..<e.lowerBound])
        }
        for (k, v) in [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&apos;", "'")] {
            t = t.replacingOccurrences(of: k, with: v)
        }
        return t.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static let rfc822: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
        return f
    }()

    static func between(_ s: Substring, _ open: String, _ close: String) -> String? {
        guard let a = s.range(of: open) else { return nil }
        let rest = s[a.upperBound...]
        let start: String.Index
        if open.hasSuffix(">") { start = a.upperBound } else {
            guard let gt = rest.firstIndex(of: ">") else { return nil }
            start = s.index(after: gt)
        }
        guard let b = s.range(of: close, range: start..<s.endIndex) else { return nil }
        return String(s[start..<b.lowerBound])
    }

    /// RSS 2.0 (Google News and others): title, date and source of each item.
    static func rss(_ xml: String) -> [MarketGuard.NewsItem] {
        let chunks: [String] = xml.components(separatedBy: "<item>")
        return chunks.dropFirst().compactMap { (chunk: String) -> MarketGuard.NewsItem? in
            let item = Substring(chunk.components(separatedBy: "</item>").first ?? chunk)
            guard let rawTitle = between(item, "<title>", "</title>"), let rawDate = between(item, "<pubDate>", "</pubDate>"),
                  let date = rfc822.date(from: decode(rawDate)) else { return nil }
            let title = decode(rawTitle)
            guard !title.isEmpty else { return nil }
            let source = between(item, "<source", "</source>").map(decode)
            return MarketGuard.NewsItem(title: title, time: date, source: source?.isEmpty == false ? source : nil)
        }
    }

    /// Cboe VIX history: {"data": [{"date", "close": "17.24"}]} → closes, oldest first.
    static func vix(_ data: Data) -> [Double] {
        guard let obj = try? JSON.object(data), let rows = obj["data"] as? [[String: Any]] else { return [] }
        return rows.suffix(60).compactMap { JSON.double($0["close"]) }
    }

    // MARK: Fetchers

    func get(_ url: String) async -> Data? { try? await SourceHTTP.get(transport, URL(string: url)!) }

    public func positioning(base: String) async -> MarketGuard.Positioning? {
        async let f = get("https://www.okx.com/api/v5/public/funding-rate?instId=\(base)-USDT-SWAP")
        async let ls = get("https://www.okx.com/api/v5/rubik/stat/contracts/long-short-account-ratio?ccy=\(base)&period=1H")
        async let oi = get("https://www.okx.com/api/v5/rubik/stat/contracts/open-interest-volume?ccy=\(base)&period=1H")
        let funding = (await f).flatMap(Self.funding)
        let ratio = (await ls).map { Self.rubik($0) } ?? []
        let interest = (await oi).map { Self.rubik($0) } ?? []
        if funding == nil && ratio.isEmpty && interest.isEmpty { return nil }
        return MarketGuard.Positioning(fundingRate: funding, longShortRatio: ratio, openInterest: interest)
    }

    public func sentiment(_ asset: Asset) async -> MarketGuard.Sentiment {
        async let fg: [Double] = asset.assetClass == .crypto ? ((await get("https://api.alternative.me/fng/?limit=30")).map(Self.fearGreed) ?? []) : []
        async let social = try? SentimentProvider(transport: transport).social(asset)
        let s = await social
        return MarketGuard.Sentiment(fearGreed: await fg, socialBullish: s?.bullishPercent, socialSample: s?.sampleSize ?? 0)
    }

    public func news(_ asset: Asset) async -> [MarketGuard.NewsItem] {
        let name = asset.name.replacingOccurrences(of: ",? (Inc|Corp|Corporation|Ltd|plc)\\.?$", with: "", options: [.regularExpression, .caseInsensitive])
        let q = asset.assetClass == .crypto
            ? "\"\(asset.name)\" crypto"
            : "\(asset.symbol.replacingOccurrences(of: "-", with: ".")) stock \"\(name)\""
        var c = URLComponents(string: "https://news.google.com/rss/search")!
        c.queryItems = [URLQueryItem(name: "q", value: "\(q) when:7d"), URLQueryItem(name: "hl", value: "en-US"),
                        URLQueryItem(name: "gl", value: "US"), URLQueryItem(name: "ceid", value: "US:en")]
        guard let data = try? await SourceHTTP.get(transport, c.url!, headers: ["Accept": "application/rss+xml, text/xml"]) else { return [] }
        return Self.rss(String(decoding: data, as: UTF8.self))
    }

    public func vix() async -> [Double] {
        (await get("https://cdn.cboe.com/api/global/delayed_quotes/charts/historical/_VIX.json")).map(Self.vix) ?? []
    }

    /// Guard inputs other than candles, fetched in parallel.
    public func inputs(for asset: Asset) async -> (positioning: MarketGuard.Positioning?, sentiment: MarketGuard.Sentiment,
                                                    news: [MarketGuard.NewsItem], vix: [Double]) {
        async let p = asset.assetClass == .crypto ? positioning(base: asset.base) : nil
        async let s = sentiment(asset)
        async let n = news(asset)
        async let v: [Double] = asset.assetClass == .stock ? vix() : []
        return (await p, await s, await n, await v)
    }
}
