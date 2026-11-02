import Foundation

// Live prices read by the iPhone from several independent providers, compared with their median (the server's
// backend/src/quotes.rs): a provider more than 0.5 % (crypto) or 1.5 % (stocks) away from the median is set aside.

struct PriceQuote: Sendable, Equatable {
    var price: Double
    /// 24 h change (crypto) or change since the last close (stocks), in %.
    var change: Double?
}

struct QuoteSource: Sendable {
    let name: String
    let kind: Kind
    let fetch: @Sendable (MarketFetcher, String) async throws -> PriceQuote
}

enum QuoteSources {
    static func quote(_ price: Double, _ change: Double?) throws -> PriceQuote {
        guard let p = finitePositive(price) else { throw MarketError("non coté") }
        return PriceQuote(price: p, change: change.flatMap { $0.isFinite ? $0 : nil })
    }

    static let crypto: [QuoteSource] = [
        QuoteSource(name: "Binance", kind: .crypto) { f, b in
            let d = try await f.json("https://api.binance.com/api/v3/ticker/24hr?symbol=\(b)USDT")
            return try quote(num(at(d, "lastPrice")), num(at(d, "priceChangePercent")))
        },
        QuoteSource(name: "OKX", kind: .crypto) { f, b in
            let d = try await f.json("https://www.okx.com/api/v5/market/ticker?instId=\(b)-USDT")
            let t = obj(arr(at(d, "data"))?.first)
            let last = num(t?["last"]), open = num(t?["open24h"])
            return try quote(last, open > 0 ? (last / open - 1) * 100 : nil)
        },
        QuoteSource(name: "Coinbase", kind: .crypto) { f, b in
            let d = try await f.json("https://api.exchange.coinbase.com/products/\(b)-USD/stats")
            let last = num(at(d, "last")), open = num(at(d, "open"))
            return try quote(last, open > 0 ? (last / open - 1) * 100 : nil)
        },
        QuoteSource(name: "Kraken", kind: .crypto) { f, b in
            let d = try await f.json("https://api.kraken.com/0/public/Ticker?pair=\(krakenBase(b))USD")
            if let e = at(d, "error") as? [Any], !e.isEmpty { throw MarketError("non coté") }
            let t = obj(obj(at(d, "result"))?.values.first)
            // Kraken's "o" is today's opening price (midnight UTC), not 24 h ago: its price counts, not its change.
            return try quote(num(arr(t?["c"])?.first), nil)
        },
        QuoteSource(name: "KuCoin", kind: .crypto) { f, b in
            let d = try await f.json("https://api.kucoin.com/api/v1/market/stats?symbol=\(b)-USDT")
            return try quote(num(at(d, "data", "last")), num(at(d, "data", "changeRate")) * 100)
        },
        QuoteSource(name: "Gate.io", kind: .crypto) { f, b in
            let d = try await f.json("https://api.gateio.ws/api/v4/spot/tickers?currency_pair=\(b)_USDT")
            let t = obj(arr(d)?.first)
            return try quote(num(t?["last"]), num(t?["change_percentage"]))
        },
        QuoteSource(name: "Bitfinex", kind: .crypto) { f, b in
            let d = try await f.json("https://api-pub.bitfinex.com/v2/ticker/t\(b)USD")
            guard let r = arr(d), r.count >= 7 else { throw MarketError("non coté") }
            return try quote(num(r[6]), num(r[5]) * 100)
        },
    ]

    static let stock: [QuoteSource] = [
        QuoteSource(name: "Yahoo Finance", kind: .stock) { f, s in
            let d = try await f.json("https://query1.finance.yahoo.com/v8/finance/chart/\(Sources.enc(s))?interval=1d&range=5d")
            let r = obj(arr(at(d, "chart", "result"))?.first)
            let price = num(at(r, "meta", "regularMarketPrice"))
            // The previous session's close is the second-to-last daily close (as the server reads it): the meta's
            // `chartPreviousClose` is the close before the 5-day range, a 5-day change.
            let closes = (arr(at(obj(arr(at(r, "indicators", "quote"))?.first), "close")) ?? []).map { num($0) }.filter(\.isFinite)
            let prev = closes.count > 1 ? closes[closes.count - 2] : .nan
            return try quote(price, prev > 0 ? (price / prev - 1) * 100 : nil)
        },
        QuoteSource(name: "Nasdaq", kind: .stock) { f, s in
            // Nasdaq wants the asset class: the one that answered last time is tried first (an ETF no longer costs a
            // failed "stocks" request at every refresh).
            let classes = NasdaqClass.shared.order(s)
            for cls in classes {
                guard let d = try? await f.json("https://api.nasdaq.com/api/quote/\(Sources.enc(Sources.dotted(s)))/info?assetclass=\(cls)") else { continue }
                let p = obj(at(d, "data", "primaryData"))
                let price = num(p?["lastSalePrice"])
                if price.isFinite && price > 0 {
                    NasdaqClass.shared.remember(s, cls)
                    return try quote(price, num(p?["percentageChange"]))
                }
            }
            throw MarketError("non coté")
        },
        QuoteSource(name: "Robinhood", kind: .stock) { f, s in
            let d = try await f.json("https://api.robinhood.com/quotes/?symbols=\(Sources.enc(Sources.dotted(s)))")
            let r = obj(arr(at(d, "results"))?.first)
            let price = num(r?["last_trade_price"]), prev = num(r?["adjusted_previous_close"])
            return try quote(price, prev > 0 ? (price / prev - 1) * 100 : nil)
        },
    ]

    static func sources(_ kind: Kind) -> [QuoteSource] { kind == .crypto ? crypto : stock }

    /// Sources that quote several assets in one request (by source name): symbol → quote. A symbol they do not quote
    /// is absent; a failed request falls back to one request per asset.
    typealias Batch = @Sendable (MarketFetcher, [String]) async throws -> [String: PriceQuote]
    static let batches: [Kind: [String: Batch]] = [
        .crypto: [
            "Binance": { f, syms in
                // Binance refuses the whole list if one symbol is unknown: the caller then reads them one by one.
                let list = syms.map { "%22\($0)USDT%22" }.joined(separator: ",")
                guard let rows = arr(try await f.json("https://api.binance.com/api/v3/ticker/24hr?symbols=%5B\(list)%5D")) else { throw MarketError("réponse illisible") }
                var out: [String: PriceQuote] = [:]
                for r in rows.compactMap({ obj($0) }) {
                    guard let sym = r["symbol"] as? String, sym.hasSuffix("USDT"), let q = try? quote(num(r["lastPrice"]), num(r["priceChangePercent"])) else { continue }
                    out[String(sym.dropLast(4))] = q
                }
                return out
            },
            "Kraken": { f, syms in
                let pairs = syms.map { "\(krakenBase($0))USD" }
                let d = try await f.json("https://api.kraken.com/0/public/Ticker?pair=\(pairs.joined(separator: ","))")
                // An unknown pair makes Kraken refuse the whole list: the caller then reads them one by one.
                if let e = at(d, "error") as? [Any], !e.isEmpty { throw MarketError("liste refusée") }
                guard let result = obj(at(d, "result")) else { throw MarketError("réponse illisible") }
                var out: [String: PriceQuote] = [:]
                for (sym, pair) in zip(syms, pairs) {
                    // Older assets come back under their legacy name (XBTUSD → XXBTZUSD, ETHUSD → XETHZUSD).
                    let base = krakenBase(sym)
                    guard let t = obj(result[pair] ?? result["X\(base)ZUSD"]),
                          let q = try? quote(num(arr(t["c"])?.first), nil) else { continue }
                    out[sym] = q
                }
                return out
            },
            "Bitfinex": { f, syms in
                let list = syms.map { "t\($0)USD" }.joined(separator: ",")
                guard let rows = arr(try await f.json("https://api-pub.bitfinex.com/v2/tickers?symbols=\(list)")) else { throw MarketError("réponse illisible") }
                var out: [String: PriceQuote] = [:]
                for r in rows.compactMap({ arr($0) }) where r.count >= 8 {
                    // The list form starts with the symbol: change at 6, last price at 7 (5 and 6 for one ticker).
                    guard let sym = r[0] as? String, sym.hasPrefix("t"), sym.hasSuffix("USD"), let q = try? quote(num(r[7]), num(r[6]) * 100) else { continue }
                    out[String(sym.dropFirst().dropLast(3))] = q
                }
                return out
            },
        ],
        .stock: [
            "Robinhood": { f, syms in
                let dotted = Dictionary(syms.map { (Sources.dotted($0), $0) }, uniquingKeysWith: { a, _ in a })
                let list = dotted.keys.sorted().map { Sources.enc($0) }.joined(separator: ",")
                guard let rows = arr(at(try await f.json("https://api.robinhood.com/quotes/?symbols=\(list)"), "results")) else { throw MarketError("réponse illisible") }
                var out: [String: PriceQuote] = [:]
                for r in rows.compactMap({ obj($0) }) {
                    let price = num(r["last_trade_price"]), prev = num(r["adjusted_previous_close"])
                    guard let sym = r["symbol"] as? String, let original = dotted[sym],
                          let q = try? quote(price, prev > 0 ? (price / prev - 1) * 100 : nil) else { continue }
                    out[original] = q
                }
                return out
            },
        ],
    ]

    /// Consensus quotes by asset id, the batch sources asked once for all the assets of a market.
    static func consensusByID(_ assets: [Asset], fetcher: MarketFetcher, unlisted: Unlisted = Unlisted()) async -> [String: Quote] {
        var pre: [String: [String: PriceQuote]] = [:] // "kind:source" → symbol → quote
        for kind in [Kind.crypto, .stock] {
            let syms = Array(Set(assets.filter { $0.kind == kind }.map(\.symbol))).sorted()
            guard syms.count >= 2, let list = batches[kind] else { continue }
            let got: [(String, [String: PriceQuote]?)] = await withTaskGroup(of: (String, [String: PriceQuote]?).self) { g in
                for (name, batch) in list {
                    // Symbols this source does not list are left out: one of them made the whole list refused.
                    let listed = syms.filter { !unlisted.contains(name, $0) }
                    g.addTask { (name, listed.isEmpty ? [:] : try? await batch(fetcher, listed)) }
                }
                var out: [(String, [String: PriceQuote]?)] = []
                for await r in g { out.append(r) }
                return out
            }
            for (name, d) in got { if let d { pre["\(kind.rawValue):\(name)"] = d } }
        }
        let snapshot = pre
        return await withTaskGroup(of: (String, Quote?).self) { g in
            for a in assets { g.addTask { (a.id, await one(a, fetcher: fetcher, prefetched: snapshot, unlisted: unlisted)) } }
            var out: [String: Quote] = [:]
            for await (id, q) in g { if let q { out[id] = q } }
            return out
        }
    }

    /// Consensus quote of each asset (assets no provider quotes are left out).
    static func consensus(_ assets: [Asset], fetcher: MarketFetcher) async -> [Quote] {
        await withTaskGroup(of: (Int, Quote?).self) { g in
            for (i, a) in assets.enumerated() {
                g.addTask { (i, await one(a, fetcher: fetcher)) }
            }
            var out: [(Int, Quote?)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.compactMap(\.1)
        }
    }

    static func one(_ a: Asset, fetcher: MarketFetcher, prefetched: [String: [String: PriceQuote]] = [:], unlisted: Unlisted = Unlisted()) async -> Quote? {
        let list = sources(a.kind)
        let results: [(String, Result<PriceQuote, MarketError>)] = await withTaskGroup(of: (Int, String, Result<PriceQuote, MarketError>).self) { g in
            for (i, s) in list.enumerated() {
                g.addTask {
                    if unlisted.contains(s.name, a.symbol) { return (i, s.name, .failure(MarketError("non coté"))) }
                    if let batch = prefetched["\(a.kind.rawValue):\(s.name)"] {
                        return (i, s.name, batch[a.symbol].map { .success($0) } ?? .failure(MarketError("non coté")))
                    }
                    do { return (i, s.name, .success(try await s.fetch(fetcher, a.symbol))) } catch {
                        let m = (error as? MarketError)?.message ?? ""
                        if m == "non coté" || m == "HTTP 400" || m == "HTTP 404" { unlisted.mark(s.name, a.symbol) }
                        return (i, s.name, .failure((error as? MarketError) ?? MarketError(error.localizedDescription)))
                    }
                }
            }
            var out: [(Int, String, Result<PriceQuote, MarketError>)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map { ($0.1, $0.2) }
        }
        return combine(a, results)
    }

    static func combine(_ a: Asset, _ results: [(String, Result<PriceQuote, MarketError>)]) -> Quote? {
        let found = results.compactMap { r -> (String, PriceQuote)? in if case .success(let q) = r.1 { return (r.0, q) } else { return nil } }
        guard !found.isEmpty else { return nil }
        let reference = Indicators.median(found.map(\.1.price))
        let tol = a.kind == .crypto ? 0.5 : 1.5
        func near(_ x: Double, _ y: Double) -> Bool { abs(x / y - 1) * 100 <= tol }
        var agree = found.filter { near($0.1.price, reference) }
        if agree.isEmpty {
            // The median fell between two camps (two sources apart, or a 2–2 split): it is no price at all. The largest
            // group of sources close to one another is kept, the first listed source winning a tie.
            for f in found {
                let group = found.filter { near($0.1.price, f.1.price) }
                if group.count > agree.count { agree = group }
            }
        }
        let changes = agree.compactMap(\.1.change)
        return Quote(symbol: a.symbol, kind: a.kind, name: a.name, price: Indicators.median(agree.map(\.1.price)),
                     change: changes.isEmpty ? nil : Indicators.median(changes), agreeing: agree.count, total: results.count,
                     sources: results.map { name, r in
                         switch r {
                         case .success(let q): return Quote.Source(name: name, ok: agree.contains { $0.0 == name }, price: q.price, error: nil)
                         case .failure(let e): return Quote.Source(name: name, ok: false, price: nil, error: e.message)
                         }
                     })
    }
}

// MARK: - Search

enum LocalSearch {
    /// Common cryptos, ranked: found even when the search providers do not answer.
    static let cryptos: [(String, String)] = [
        ("BTC", "Bitcoin"), ("ETH", "Ethereum"), ("USDT", "Tether"), ("XRP", "XRP"), ("BNB", "BNB"), ("SOL", "Solana"),
        ("USDC", "USD Coin"), ("DOGE", "Dogecoin"), ("TRX", "TRON"), ("ADA", "Cardano"), ("LINK", "Chainlink"),
        ("AVAX", "Avalanche"), ("XLM", "Stellar"), ("SUI", "Sui"), ("BCH", "Bitcoin Cash"), ("HBAR", "Hedera"),
        ("LTC", "Litecoin"), ("TON", "Toncoin"), ("SHIB", "Shiba Inu"), ("DOT", "Polkadot"), ("XMR", "Monero"),
        ("UNI", "Uniswap"), ("PEPE", "Pepe"), ("AAVE", "Aave"), ("NEAR", "NEAR Protocol"), ("APT", "Aptos"),
        ("ICP", "Internet Computer"), ("ETC", "Ethereum Classic"), ("POL", "Polygon"), ("ARB", "Arbitrum"),
        ("OP", "Optimism"), ("ATOM", "Cosmos"), ("FIL", "Filecoin"), ("INJ", "Injective"), ("RENDER", "Render"),
        ("ALGO", "Algorand"), ("VET", "VeChain"), ("TIA", "Celestia"), ("SEI", "Sei"), ("BONK", "Bonk"),
    ]

    static func search(_ q: String, limit: Int, fetcher: MarketFetcher) async -> [SearchItem] {
        let text = String(q.trimmingCharacters(in: .whitespaces).prefix(30))
        guard !text.isEmpty else { return [] }
        let up = text.uppercased()
        async let stocks = yahoo(text, fetcher: fetcher)
        async let coins = gecko(text, fetcher: fetcher)
        var local: [SearchItem] = []
        for (i, c) in cryptos.enumerated() where c.0.hasPrefix(up) || c.1.uppercased().contains(up) {
            local.append(SearchItem(symbol: c.0, name: c.1, kind: .crypto, rank: i + 1, etf: nil))
        }
        var seen = Set<String>()
        var out: [SearchItem] = []
        // An exact symbol first, then the providers' order.
        let all = (local + (await coins) + (await stocks)).sorted { a, b in (a.symbol == up ? 0 : 1) < (b.symbol == up ? 0 : 1) }
        for item in all where seen.insert(item.id).inserted { out.append(item) }
        return Array(out.prefix(max(1, min(limit, 50))))
    }

    static func yahoo(_ q: String, fetcher: MarketFetcher) async -> [SearchItem] {
        guard let d = try? await fetcher.json("https://query1.finance.yahoo.com/v1/finance/search?q=\(Sources.enc(q))&quotesCount=12&newsCount=0") else { return [] }
        return (arr(at(d, "quotes")) ?? []).compactMap { obj($0) }.compactMap { r in
            guard let type = r["quoteType"] as? String, type == "EQUITY" || type == "ETF", let sym = r["symbol"] as? String,
                  !sym.contains("."), !sym.contains("=") else { return nil }
            let name = (r["longname"] as? String) ?? (r["shortname"] as? String) ?? sym
            return SearchItem(symbol: sym.replacingOccurrences(of: "-", with: "."), name: name, kind: .stock, rank: nil, etf: type == "ETF")
        }
    }

    static func gecko(_ q: String, fetcher: MarketFetcher) async -> [SearchItem] {
        guard let d = try? await fetcher.json("https://api.coingecko.com/api/v3/search?query=\(Sources.enc(q))") else { return [] }
        return (arr(at(d, "coins")) ?? []).compactMap { obj($0) }.prefix(10).compactMap { r in
            guard let sym = (r["symbol"] as? String)?.uppercased(), sym.allSatisfy({ $0.isLetter || $0.isNumber }) else { return nil }
            return SearchItem(symbol: sym, name: (r["name"] as? String) ?? sym, kind: .crypto, rank: (r["market_cap_rank"] as? NSNumber)?.intValue, etf: nil)
        }
    }
}

// MARK: - EUR/USD

enum LocalFx {
    /// Dollars per euro outside this band are a broken answer (EUR/USD has stayed within 0.82–1.60 since 1999): the
    /// next source is read instead, so that no amount in euros is ever converted with an absurd rate.
    static let plausible: ClosedRange<Double> = 0.5...2.5

    static func sane(_ x: Double) -> Double? { finitePositive(x).flatMap { plausible.contains($0) ? $0 : nil } }

    /// EUR/USD from Yahoo Finance, else the ECB, else Frankfurter: euros per dollar (`rate`) and dollars per euro.
    static func read(fetcher: MarketFetcher, now: Double = nowMs()) async -> Fx.Response {
        if let d = try? await fetcher.json("https://query1.finance.yahoo.com/v8/finance/chart/EURUSD=X?interval=1d&range=5d"),
           let meta = obj(at(obj(arr(at(d, "chart", "result"))?.first), "meta")), let usdPerEur = sane(num(meta["regularMarketPrice"])) {
            let t = num(meta["regularMarketTime"])
            return response(usdPerEur, time: t.isFinite ? t * 1000 : now, source: "Yahoo Finance", now: now)
        }
        if let xml = try? await fetcher.text("https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml"),
           let r = xml.range(of: "currency='USD' rate='"), let end = xml[r.upperBound...].firstIndex(of: "'"),
           let usdPerEur = sane(Double(xml[r.upperBound..<end]) ?? .nan) {
            return response(usdPerEur, time: ecbDay(xml) ?? now, source: "BCE", now: now)
        }
        if let d = try? await fetcher.json("https://api.frankfurter.dev/v1/latest?base=USD&symbols=EUR"), let eur = finitePositive(num(at(d, "rates", "EUR"))), let usdPerEur = sane(1 / eur) {
            return response(usdPerEur, time: now, source: "Frankfurter", now: now)
        }
        return Fx.Response(base: "USD", quote: "EUR", rate: nil, usdPerEur: nil, time: nil, source: nil, fetchedAt: now, stale: nil,
                           error: "Taux EUR/USD indisponible (aucune source n'a répondu)")
    }

    /// The day of the ECB reference rate (`<Cube time='2026-10-30'>`, published around 16:00 CET), at 15:00 UTC: the
    /// rate's real age, not the time it was read.
    static func ecbDay(_ xml: String) -> Double? {
        guard let r = xml.range(of: "time='"), let end = xml[r.upperBound...].firstIndex(of: "'") else { return nil }
        let parts = xml[r.upperBound..<end].split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        var c = DateComponents(timeZone: TimeZone(identifier: "UTC"), year: parts[0], month: parts[1], day: parts[2], hour: 15)
        c.calendar = Calendar(identifier: .gregorian)
        return c.date.map { $0.timeIntervalSince1970 * 1000 }
    }

    static func response(_ usdPerEur: Double, time: Double, source: String, now: Double) -> Fx.Response {
        Fx.Response(base: "USD", quote: "EUR", rate: 1 / usdPerEur, usdPerEur: usdPerEur, time: time, source: source, fetchedAt: now, stale: false, error: nil)
    }
}


/// The Nasdaq asset class ("stocks" or "etf") that quoted each symbol last.
final class NasdaqClass: @unchecked Sendable {
    static let shared = NasdaqClass()
    private let lock = NSLock()
    private var known: [String: String] = [:]
    func order(_ symbol: String) -> [String] {
        lock.lock(); defer { lock.unlock() }
        return known[symbol] == "etf" ? ["etf", "stocks"] : ["stocks", "etf"]
    }
    func remember(_ symbol: String, _ cls: String) {
        lock.lock(); known[symbol] = cls; lock.unlock()
    }
}


/// Symbols a quote source said it does not list (« non coté », HTTP 400 or 404 for that symbol alone): not asked
/// again, nor put in its lists, for 1 hour (a listing can come later, an odd answer can be a one-off).
final class Unlisted: @unchecked Sendable {
    private let lock = NSLock()
    private var since: [String: Double] = [:]
    static let keep = 3_600_000.0
    func contains(_ source: String, _ symbol: String) -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard let t = since["\(source):\(symbol)"] else { return false }
        return nowMs() - t < Self.keep
    }
    func mark(_ source: String, _ symbol: String) {
        lock.lock(); since["\(source):\(symbol)"] = nowMs(); lock.unlock()
    }
}
