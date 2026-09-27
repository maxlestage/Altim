import Foundation

/// Full asset universe (port of `web/server/universe.ts`): every crypto listed against USD/USDT on the
/// exchanges Altim reads, and every stock / ETF listed in the United States (official Nasdaq Trader directory).
/// Lists come from the sources themselves, nothing is hard-coded except the order of the largest ETFs.
public struct UniverseEntry: Codable, Hashable, Sendable, Identifiable {
    public let symbol: String
    public let name: String
    public let kind: AssetClass
    /// Market capitalisation order (nil = unknown).
    public let rank: Int?
    /// Crypto: number of exchanges listing it. Stock: 1 = ETF.
    public let flag: Int

    public var id: String { "\(kind.rawValue):\(symbol)" }
    public var isETF: Bool { kind == .stock && flag == 1 }

    public init(symbol: String, name: String, kind: AssetClass, rank: Int?, flag: Int) {
        self.symbol = symbol
        self.name = name
        self.kind = kind
        self.rank = rank
        self.flag = flag
    }

    /// Asset used by the market data layer (crypto quoted in USDT, stocks in USD).
    public var asset: Asset {
        Asset(symbol: kind == .crypto ? "\(symbol)USDT" : symbol, name: name, assetClass: kind, quote: kind == .crypto ? "USDT" : "USD")
    }
}

public enum AssetUniverse {
    struct Listed: Hashable { let symbol: String; let name: String; let etf: Bool }
    struct Ranked: Hashable { let symbol: String; let name: String; let rank: Int }

    static let cryptoSymbol = try! NSRegularExpression(pattern: "^[A-Z0-9]{1,12}$")
    /// Leveraged tokens (BTC3L, ETH5S…) are derivatives, not coins you hold.
    static let leveraged = try! NSRegularExpression(pattern: "^[A-Z0-9]{2,}[2-9][LS]$")
    static let stockSymbol = try! NSRegularExpression(pattern: "^[A-Z][A-Z0-9\\-]{0,9}$")
    static let excludedName = try! NSRegularExpression(
        pattern: "\\b(warrants?|rights?|units?|notes? due|subordinated|debentures?|preferred)\\b", options: .caseInsensitive)
    static let stableOrFiat: Set<String> = ["USDT", "USD", "USDC", "DAI", "FDUSD", "TUSD", "BUSD", "USDE", "PYUSD", "USDP", "EUR", "GBP", "EURC", "EURT"]
    static let krakenAlias = ["XBT": "BTC", "XDG": "DOGE"]

    /// Largest US ETFs by assets under management (the Nasdaq screener gives no size for ETFs).
    public static let popularETFs = [
        "VOO", "IVV", "SPY", "VTI", "QQQ", "VUG", "VEA", "IEFA", "VTV", "BND", "AGG", "IWF", "GLD", "IEMG", "VXUS", "VGT", "IJH", "VWO",
        "VIG", "IJR", "SPLG", "XLK", "IWM", "SCHD", "VO", "RSP", "ITOT", "IBIT", "BNDX", "VB", "EFA", "IWD", "SCHX", "VYM", "TLT", "XLF",
        "SMH", "IAU", "SCHG", "QUAL", "IVW", "MUB", "VCIT", "SCHF", "VT", "XLV", "VNQ", "DIA", "IWR", "XLE", "ARKK", "SOXX", "SLV", "FBTC",
    ]

    static func matches(_ re: NSRegularExpression, _ s: String) -> Bool {
        re.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) != nil
    }

    // MARK: Parsers (same formats as the web tests)

    static func rows(_ data: Data, key: String? = nil) -> [[String: Any]] {
        guard let obj = try? JSONSerialization.jsonObject(with: data) else { return [] }
        if let key { return ((obj as? [String: Any])?[key] as? [[String: Any]]) ?? [] }
        return (obj as? [[String: Any]]) ?? []
    }

    static func okx(_ data: Data) -> [String] {
        rows(data, key: "data").filter { $0["quoteCcy"] as? String == "USDT" && $0["state"] as? String == "live" }.compactMap { $0["baseCcy"] as? String }
    }

    static func coinbase(_ data: Data) -> [String] {
        rows(data).filter {
            ["USD", "USDT"].contains($0["quote_currency"] as? String ?? "") && $0["status"] as? String == "online" && ($0["trading_disabled"] as? Bool) != true
        }.compactMap { $0["base_currency"] as? String }
    }

    static func kraken(_ data: Data) -> [String] {
        let result = ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["result"] as? [String: [String: Any]] ?? [:]
        return result.values.compactMap { $0["wsname"] as? String }
            .filter { $0.hasSuffix("/USD") || $0.hasSuffix("/USDT") }
            .map { String($0.split(separator: "/")[0]) }
            .map { krakenAlias[$0] ?? $0 }
    }

    static func kucoin(_ data: Data) -> [String] {
        rows(data, key: "data").filter { $0["quoteCurrency"] as? String == "USDT" && ($0["enableTrading"] as? Bool) == true }
            .compactMap { $0["baseCurrency"] as? String }
    }

    static func gate(_ data: Data) -> [String] {
        rows(data).filter { $0["quote"] as? String == "USDT" && $0["trade_status"] as? String == "tradable" }.compactMap { $0["base"] as? String }
    }

    static func gecko(_ data: Data) -> [Ranked] {
        rows(data).compactMap { x in
            guard let s = x["symbol"] as? String, let n = x["name"] as? String, let r = x["market_cap_rank"] as? Int else { return nil }
            return Ranked(symbol: s.uppercased(), name: n, rank: r)
        }
    }

    static func names(_ data: Data, key: String?, symbol: String, name: String) -> [(String, String)] {
        rows(data, key: key).compactMap { x in
            guard let s = x[symbol] as? String, let n = x[name] as? String else { return nil }
            return (s.uppercased(), n)
        }
    }

    /// nasdaqlisted.txt / otherlisted.txt (pipe separated, last line = file date).
    static func nasdaqDirectory(_ text: String) -> [Listed] {
        let lines = text.split(whereSeparator: \.isNewline).map(String.init)
        guard let head = lines.first?.components(separatedBy: "|") else { return [] }
        let symCol = head.firstIndex(of: "Symbol") ?? head.firstIndex(of: "ACT Symbol") ?? 0
        guard let nameCol = head.firstIndex(of: "Security Name"), let etfCol = head.firstIndex(of: "ETF"),
              let testCol = head.firstIndex(of: "Test Issue") else { return [] }
        var out: [Listed] = []
        for line in lines.dropFirst() {
            let f = line.components(separatedBy: "|")
            guard f.count >= head.count, f[testCol] != "Y" else { continue }
            let raw = f[symCol].trimmingCharacters(in: .whitespaces)
            let name = f[nameCol].trimmingCharacters(in: .whitespaces)
            if raw.isEmpty || raw.contains("$") || matches(excludedName, name) { continue }
            let symbol = raw.replacingOccurrences(of: ".", with: "-")
            guard matches(stockSymbol, symbol) else { continue }
            out.append(Listed(symbol: symbol, name: cleanStockName(name), etf: f[etfCol] == "Y"))
        }
        return out
    }

    /// Nasdaq screener: market capitalisation, used to rank the stocks.
    static func screener(_ data: Data) -> [String: Double] {
        let rows = (((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["data"] as? [String: Any])?["rows"] as? [[String: Any]] ?? []
        var out: [String: Double] = [:]
        for r in rows {
            guard let s = r["symbol"] as? String else { continue }
            let cap = Double((r["marketCap"] as? String ?? "").replacingOccurrences(of: ",", with: "")) ?? 0
            if cap > 0 {
                out[s.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ".", with: "-").replacingOccurrences(of: "/", with: "-")] = cap
            }
        }
        return out
    }

    /// "Apple Inc. - Common Stock" → "Apple Inc."; "Alphabet Inc. - Class A Common Stock" → "Alphabet Inc. Class A".
    public static func cleanStockName(_ raw: String) -> String {
        let name = raw.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        let cls = name.range(of: "\\bClass [A-Z]\\b", options: .regularExpression).map { String(name[$0]) }
        var n = name.replacingOccurrences(
            of: "\\s*-?\\s*\\b(Class [A-Z] )?(Common Stock|Ordinary Shares?|Common Shares?|American Depositary Shares?|Depositary Shares?|Shares of Beneficial Interest|Capital Stock)\\b.*$",
            with: "", options: [.regularExpression, .caseInsensitive])
        n = n.replacingOccurrences(of: "[\\s,-]+$", with: "", options: .regularExpression).trimmingCharacters(in: .whitespaces)
        if let cls, !n.contains(cls) { n += " \(cls)" }
        return n.isEmpty ? name.trimmingCharacters(in: .whitespaces) : n
    }

    // MARK: Building

    static func buildCrypto(_ exchanges: [[String]], gecko: [Ranked], names: [(String, String)] = []) -> [UniverseEntry] {
        var count: [String: Int] = [:]
        for list in exchanges {
            for b in Set(list.map { $0.uppercased() }) where matches(cryptoSymbol, b) && !stableOrFiat.contains(b) && !matches(leveraged, b) {
                count[b, default: 0] += 1
            }
        }
        var info: [String: (name: String, rank: Int?)] = [:]
        for g in gecko where info[g.symbol] == nil { info[g.symbol] = (g.name, g.rank) }
        for (s, n) in names {
            let clean = n.trimmingCharacters(in: .whitespaces)
            if !clean.isEmpty, clean.uppercased() != s, info[s] == nil { info[s] = (clean, nil) }
        }
        return count.map { s, n in UniverseEntry(symbol: s, name: info[s]?.name ?? s, kind: .crypto, rank: info[s]?.rank, flag: n) }
            .sorted(by: byRank)
    }

    static func buildStocks(_ directories: [[Listed]], caps: [String: Double]) -> [UniverseEntry] {
        var seen: [String: Listed] = [:]
        var order: [String] = []
        for dir in directories { for s in dir where seen[s.symbol] == nil { seen[s.symbol] = s; order.append(s.symbol) } }
        let ranked = caps.filter { seen[$0.key] != nil }.sorted { $0.value > $1.value || ($0.value == $1.value && $0.key < $1.key) }
        var rank: [String: Int] = [:]
        for (i, r) in ranked.enumerated() { rank[r.key] = i + 1 }
        let popular = { (s: String) in popularETFs.firstIndex(of: s) ?? Int.max }
        return order.map { s in UniverseEntry(symbol: s, name: seen[s]!.name, kind: .stock, rank: rank[s], flag: seen[s]!.etf ? 1 : 0) }
            .sorted { a, b in
                if a.rank != nil || b.rank != nil { return byRank(a, b) }
                let pa = popular(a.symbol), pb = popular(b.symbol)
                return pa != pb ? pa < pb : a.symbol < b.symbol
            }
    }

    /// Ranked first (by rank), then crypto listed on more exchanges, then alphabetical.
    static func byRank(_ a: UniverseEntry, _ b: UniverseEntry) -> Bool {
        switch (a.rank, b.rank) {
        case let (x?, y?): return x < y
        case (_?, nil): return true
        case (nil, _?): return false
        default: return a.flag != b.flag ? a.flag > b.flag : a.symbol < b.symbol
        }
    }

    // MARK: Search

    static func normalized(_ s: String) -> String {
        s.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "en_US_POSIX")).uppercased()
    }

    /// Exact symbol, then symbol prefix, then name (word start, then anywhere); ranked assets first.
    public static func search(_ list: [UniverseEntry], _ query: String, limit: Int = 50) -> [UniverseEntry] {
        let q = normalized(query.trimmingCharacters(in: .whitespaces))
        guard !q.isEmpty else { return Array(list.prefix(limit)) }
        let wordStart = try? NSRegularExpression(pattern: "\\b" + NSRegularExpression.escapedPattern(for: q))
        var scored: [(e: UniverseEntry, s: Int, i: Int)] = []
        for (i, e) in list.enumerated() {
            let name = normalized(e.name)
            let s: Int
            if e.symbol == q { s = 0 }
            else if e.symbol.hasPrefix(q) { s = 1 }
            else if let wordStart, matches(wordStart, name) { s = 2 }
            else if name.contains(q) { s = 3 }
            else { continue }
            scored.append((e, s, i))
        }
        return scored.sorted { $0.s != $1.s ? $0.s < $1.s : $0.i < $1.i }.prefix(limit).map(\.e)
    }

    // MARK: Loading

    /// Downloads both universes (a few seconds). Each source is optional: a missing one only shortens the list.
    public static func load(_ kind: AssetClass, transport: HTTPTransport = URLSessionTransport()) async throws -> [UniverseEntry] {
        @Sendable func get(_ url: String) async -> Data? {
            try? await SourceHTTP.get(transport, URL(string: url)!, headers: ["Accept": "application/json, text/plain"])
        }
        switch kind {
        case .crypto:
            async let okxD = get("https://www.okx.com/api/v5/public/instruments?instType=SPOT")
            async let cbD = get("https://api.exchange.coinbase.com/products")
            async let krD = get("https://api.kraken.com/0/public/AssetPairs")
            async let kcD = get("https://api.kucoin.com/api/v2/symbols")
            async let gtD = get("https://api.gateio.ws/api/v4/spot/currency_pairs")
            async let cbN = get("https://api.exchange.coinbase.com/currencies")
            async let kcN = get("https://api.kucoin.com/api/v3/currencies")
            // CoinGecko limits bursts: pages one after the other.
            var ranked: [Ranked] = []
            for page in 1...4 {
                guard let d = await get("https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=\(page)") else { break }
                ranked += gecko(d)
            }
            let exchanges = [okx(await okxD ?? Data()), coinbase(await cbD ?? Data()), kraken(await krD ?? Data()),
                             kucoin(await kcD ?? Data()), gate(await gtD ?? Data())]
            let fullNames = names(await cbN ?? Data(), key: nil, symbol: "id", name: "name")
                + names(await kcN ?? Data(), key: "data", symbol: "currency", name: "fullName")
            let list = buildCrypto(exchanges, gecko: ranked, names: fullNames)
            guard list.count >= 50 else { throw APIError.decoding("Liste des cryptos indisponible") }
            return list
        case .stock:
            async let nq = get("https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt")
            async let ot = get("https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt")
            async let sc = get("https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true")
            let dirs = [await nq, await ot].map { nasdaqDirectory(String(decoding: $0 ?? Data(), as: UTF8.self)) }
            let list = buildStocks(dirs, caps: screener(await sc ?? Data()))
            guard list.count >= 1000 else { throw APIError.decoding("Liste des actions indisponible") }
            return list
        }
    }
}
