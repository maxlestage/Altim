import Foundation

/// The Altim API answered on the iPhone itself: the same JSON as the server's routes, computed from market data the
/// app reads straight from the exchanges and stock providers. With it, Altim works without any server: the routes it
/// knows never leave the phone; the others go to the server when one is connected (Réglages), else say they need one.
public actor LocalBackend {
    let fetcher: MarketFetcher
    private var snapshots: [String: (at: Double, task: Task<LocalSnapshot, Error>)] = [:]
    private var quoteCache: [String: (at: Double, quote: Quote)] = [:]
    private var fxCache: (at: Double, value: Fx.Response)?
    private var longCache: [String: (at: Double, bars: [Bar])] = [:]
    private var newsCache: [String: (at: Double, report: NewsReport)] = [:]
    var macroCache: (at: Double, info: MacroInfo)?

    public init(fetcher: MarketFetcher = URLSessionFetcher()) { self.fetcher = fetcher }

    /// Routes answered here.
    public static let routes: Set<String> = ["/api/candles", "/api/radar", "/api/tickers", "/api/search", "/api/fx", "/api/zones", "/api/backtest", "/api/history", "/api/news", "/api/alerts", "/api/guard", "/api/rotation", "/api/verdict", "/api/sectors", "/api/macro"]

    /// JSON body and status of a route, nil when it is not one of `routes`.
    public func answer(path: String, query: [String: String]) async -> (Data, Int)? {
        guard Self.routes.contains(path) else { return nil }
        do {
            switch path {
            case "/api/candles":
                let a = try Self.asset(query["symbol"], query["kind"])
                let tf = try Self.timeframe(query["interval"])
                return (try Self.encode(try await snapshot(a, tf).json), 200)
            case "/api/radar":
                let list = Self.assets(query["symbols"])
                let tf = try Self.timeframe(query["interval"] ?? "4h")
                return (try Self.encode(await radar(list, tf)), 200)
            case "/api/tickers":
                return (try Self.encode(await quotes(Self.assets(query["symbols"]))), 200)
            case "/api/search":
                let limit = Int(query["limit"] ?? "") ?? 20
                return (try Self.encode(await LocalSearch.search(query["q"] ?? "", limit: limit, fetcher: fetcher)), 200)
            case "/api/fx":
                return (try Self.encode(await fx()), 200)
            case "/api/zones":
                return (try Self.encode(try await zones(try Self.asset(query["symbol"], query["kind"]))), 200)
            case "/api/macro":
                return (try Self.encode(try await macroReport()), 200)
            case "/api/sectors":
                return (await LocalSectors.report((query["symbols"] ?? "").split(separator: ",").map(String.init), fetcher: fetcher), 200)
            case "/api/verdict":
                let a = try Self.asset(query["symbol"], query["kind"])
                let d = try await snapshot(a, .d1)
                var v = TrendEngine.verdict(d.candles, kind: a.kind)
                if d.reliability.level == "low" && v.action != .hold {
                    v.action = .hold
                    v.text = "Données insuffisamment fiables (sources en désaccord) : verdict suspendu. " + v.text
                }
                return (try Self.encode(v), 200)
            case "/api/rotation":
                guard let kind = Kind(rawValue: query["kind"] ?? "") else { throw MarketError("Paramètre kind invalide") }
                return (try Self.encode(await rotation(kind)), 200)
            case "/api/guard":
                return (try Self.encode(try await guardReport(try Self.asset(query["symbol"], query["kind"]))), 200)
            case "/api/alerts":
                return (await alerts(Self.assets(query["symbols"])), 200)
            case "/api/news":
                return (try Self.encode(await news(Self.assets(query["symbols"]))), 200)
            case "/api/history":
                let days = Int(query["days"] ?? "90") ?? 0
                guard [30, 90, 365, 730].contains(days) else { throw MarketError("Paramètre days invalide (30 | 90 | 365 | 730)") }
                return (try Self.encode(await history(Self.assets(query["symbols"]), days: days)), 200)
            case "/api/backtest":
                return (try Self.encode(try await backtest(try Self.asset(query["symbol"], query["kind"]))), 200)
            default:
                return nil
            }
        } catch let e as MarketError {
            return (Self.errorBody(e.message), e.message.hasPrefix("Paramètre") ? 400 : 502)
        } catch {
            return (Self.errorBody(error.localizedDescription), 500)
        }
    }

    // MARK: Snapshots

    /// Multi-source candles + quality + reliability of an asset, kept 1 to 10 minutes depending on the timeframe.
    func snapshot(_ a: Asset, _ tf: Timeframe) async throws -> LocalSnapshot {
        let key = "\(a.id):\(tf.rawValue)"
        let now = nowMs()
        if let c = snapshots[key], now - c.at < Self.ttl(tf) {
            if let v = try? await c.task.value { return v }
        }
        let fetcher = self.fetcher
        let task = Task { try await LocalSnapshot.build(a, tf, fetcher: fetcher) }
        snapshots[key] = (now, task)
        do { return try await task.value } catch {
            snapshots[key] = nil
            throw error
        }
    }

    static func ttl(_ tf: Timeframe) -> Double {
        switch tf {
        case .h1: return 60_000
        case .h4: return 120_000
        case .d1: return 300_000
        case .d4, .w1: return 600_000
        }
    }

    func radar(_ list: [Asset], _ tf: Timeframe) async -> [RadarRow] {
        async let prices = quotes(list)
        let rows: [RadarRow] = await withTaskGroup(of: (Int, RadarRow).self) { g in
            for (i, a) in list.enumerated() {
                g.addTask { (i, await self.radarRow(a, tf)) }
            }
            var out: [(Int, RadarRow)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map(\.1)
        }
        let qs = await prices
        return rows.map { r in
            var r = r
            if let q = qs.first(where: { $0.symbol == r.symbol && $0.kind == r.kind }) {
                r.price = q.price
                r.change = q.change
                r.priceSources = "\(q.agreeing)/\(q.total)"
            }
            return r
        }
    }

    /// A radar row: the verdict of the daily trend engine (whatever the chosen timeframe, which only draws the
    /// sparkline), suspended to ATTENDRE when the data is unreliable.
    func radarRow(_ a: Asset, _ tf: Timeframe) async -> RadarRow {
        async let chart = try? snapshot(a, tf)
        do {
            let d = try await snapshot(a, .d1)
            var v = TrendEngine.verdict(d.candles, kind: a.kind)
            if d.reliability.level == "low" { v.action = .hold }
            let s = await chart ?? d
            return RadarRow(symbol: a.symbol, kind: a.kind, name: a.name, price: s.candles.last?.close, change: nil, priceSources: nil,
                            signal: v.state == .unknown ? nil : SignalSummary(action: v.action, score: v.score, confidence: v.confidence * d.reliability.score / 100),
                            reliability: d.reliability, agreeing: d.agreeing, sources: d.sources.count,
                            sparkline: s.candles.suffix(48).map(\.close), error: nil)
        } catch {
            return RadarRow(symbol: a.symbol, kind: a.kind, name: a.name, price: nil, change: nil, priceSources: nil, signal: nil,
                            reliability: nil, agreeing: nil, sources: nil, sparkline: nil,
                            error: (error as? MarketError)?.message ?? error.localizedDescription)
        }
    }

    // MARK: Rotation

    private var rotationCache: [Kind: (at: Double, report: RotationReport)] = [:]

    /// « Sélection Altim » of a market: this week's picks and the rule replayed on the same assets (kept 6 hours).
    func rotation(_ kind: Kind) async -> RotationReport {
        if let c = rotationCache[kind], nowMs() - c.at < 6 * 3_600_000 { return c.report }
        let list = Rotation.universe[kind] ?? []
        async let prices = quotes(list)
        let data: [(Asset, [Bar])] = await withTaskGroup(of: (Int, Asset, [Bar]?).self) { g in
            for (i, a) in list.enumerated() { g.addTask { (i, a, try? await self.longDaily(a)) } }
            var out: [(Int, Asset, [Bar]?)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.compactMap { r in r.2.map { (r.1, $0) } }
        }
        let p = Dictionary((await prices).map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a })
        let r = RotationReport(kind: kind, asOf: nowMs(), top: data.count, lookback: TrendEngine.lookback(kind),
                               picks: Rotation.picks(data, kind: kind, prices: p), scanned: data.count, test: Rotation.sleeves(data, kind: kind),
                               notes: Rotation.notes)
        if !data.isEmpty { rotationCache[kind] = (nowMs(), r) }
        return r
    }

    // MARK: Zones

    /// Fibonacci buy zones of the three horizons; the status and distance follow the current consensus price.
    func zones(_ a: Asset) async throws -> ZonesReport {
        async let h4 = try? snapshot(a, .h4)
        async let d1 = try? snapshot(a, .d1)
        async let w1 = try? snapshot(a, .w1)
        async let q = quotes([a])
        let (s4, sd, sw) = (await h4, await d1, await w1)
        guard s4 != nil || sd != nil || sw != nil else { throw MarketError("Toutes les sources ont échoué") }
        let price = await q.first?.price ?? s4?.candles.last?.close ?? sd?.candles.last?.close
        let zones = FibEngine.zones(h4: s4?.candles ?? [], daily: sd?.candles ?? [], weekly: sw?.candles ?? [], price: price)
        return ZonesReport(symbol: a.symbol, kind: a.kind, price: price, asOf: nowMs(), zones: zones, macro: try? await macroReport())
    }

    // MARK: Backtest

    /// The signal replayed on two years of daily candles (`Backtest.run`).
    func backtest(_ a: Asset) async throws -> BacktestReport {
        let s = try await snapshot(a, .d1)
        return TrendEngine.backtest(try await longDaily(a), asset: a, reliability: s.reliability)
    }

    /// Longest daily history: for cryptos the exchanges' ≈ 1 000 days when they agree, else the usual daily candles
    /// (stocks: Nasdaq's ≈ 5 years, checked against Robinhood's and Yahoo's).
    func longDaily(_ a: Asset) async throws -> [Bar] {
        let key = "long:\(a.id)"
        if let c = longCache[key], nowMs() - c.at < 3_600_000 { return c.bars }
        let s = try await snapshot(a, .d1)
        var bars = s.candles
        let longSources = a.kind == .crypto ? Sources.cryptoLongDaily : Sources.stockLongDaily
        if let long = try? await Consensus.candles(a.symbol, kind: a.kind, .d1, sources: longSources, fetcher: fetcher, tolerance: a.kind == .crypto ? 0.5 : 1.0),
           long.candles.count > bars.count {
            bars = long.candles
        }
        longCache[key] = (nowMs(), bars)
        return bars
    }

    // MARK: News

    /// The news section, read again at most every 5 minutes for the same assets.
    func news(_ assets: [Asset]) async -> NewsReport {
        let key = assets.map(\.id).sorted().joined(separator: ",")
        if let c = newsCache[key], nowMs() - c.at < 300_000 { return c.report }
        let r = await NewsEngine.report(assets, fetcher: fetcher)
        if !r.items.isEmpty { newsCache[key] = (nowMs(), r) }
        return r
    }

    // MARK: History

    /// Daily closes of each asset over `days` (+ a week), plus Bitcoin and SPY (the benchmarks of the portfolio history).
    func history(_ held: [Asset], days: Int) async -> HistoryResponse {
        var unique: [Asset] = []
        for a in held + [Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), Asset(symbol: "SPY", kind: .stock, name: "S&P 500 (SPY)")]
        where !unique.contains(where: { $0.id == a.id }) { unique.append(a) }
        let from = nowMs() - Double(days + 7) * 86_400_000
        let series: [HistoryResponse.Series] = await withTaskGroup(of: (Int, HistoryResponse.Series).self) { g in
            for (i, a) in unique.enumerated() {
                g.addTask {
                    guard let bars = try? await self.longDaily(a) else {
                        return (i, .init(symbol: a.symbol, kind: a.kind, closes: [], error: "historique indisponible"))
                    }
                    return (i, .init(symbol: a.symbol, kind: a.kind, closes: bars.filter { $0.time >= from }.map { [$0.time, $0.close] }, error: nil))
                }
            }
            var out: [(Int, HistoryResponse.Series)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map(\.1)
        }
        return HistoryResponse(asOf: nowMs(), days: days, series: series)
    }

    // MARK: Quotes

    /// Consensus quotes, kept 10 seconds (the live prices poll them).
    public func quotes(_ list: [Asset]) async -> [Quote] {
        let now = nowMs()
        let missing = list.filter { a in quoteCache[a.id].map { now - $0.at >= 10_000 } ?? true }
        if !missing.isEmpty {
            for q in await QuoteSources.consensus(missing, fetcher: fetcher) { quoteCache["\(q.kind.rawValue):\(q.symbol)"] = (now, q) }
        }
        return list.compactMap { quoteCache[$0.id]?.quote }
    }

    func fx() async -> Fx.Response {
        let now = nowMs()
        if let c = fxCache, now - c.at < 300_000 { return c.value }
        let r = await LocalFx.read(fetcher: fetcher, now: now)
        if r.rate != nil { fxCache = (now, r) }
        return r
    }

    // MARK: Parameters

    static func asset(_ symbol: String?, _ kind: String?) throws -> Asset {
        guard let k = Kind(rawValue: kind ?? "") else { throw MarketError("Paramètre kind invalide") }
        let s = (symbol ?? "").trimmingCharacters(in: .whitespaces).uppercased()
        guard !s.isEmpty, s.count <= 15, s.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "." || $0 == "-" }) else {
            throw MarketError("Paramètre symbol invalide")
        }
        return Asset(symbol: s, kind: k, name: name(s, k))
    }

    static func timeframe(_ raw: String?) throws -> Timeframe {
        guard let t = Timeframe(rawValue: raw ?? "") else { throw MarketError("Paramètre interval invalide") }
        return t
    }

    /// "BTC:crypto,AAPL:stock" → assets (bad entries skipped, 20 at most like the server).
    static func assets(_ raw: String?) -> [Asset] {
        (raw ?? "").split(separator: ",").compactMap { part in
            let p = part.split(separator: ":")
            guard p.count == 2 else { return nil }
            return try? asset(String(p[0]), String(p[1]))
        }.prefix(20).map { $0 }
    }

    static func name(_ symbol: String, _ kind: Kind) -> String {
        if let a = Asset.defaults.first(where: { $0.symbol == symbol && $0.kind == kind }) { return a.name }
        if kind == .crypto, let c = LocalSearch.cryptos.first(where: { $0.0 == symbol }) { return c.1 }
        return symbol
    }

    static func encode<T: Encodable>(_ v: T) throws -> Data { try JSONEncoder().encode(v) }
    static func errorBody(_ message: String) -> Data { (try? JSONEncoder().encode(["error": message])) ?? Data() }
}

/// Validated candles of one asset and timeframe.
struct LocalSnapshot: Sendable {
    var asset: Asset
    var timeframe: Timeframe
    var candles: [Bar]
    var source: String
    var agreeing: Int
    var conflict: Bool
    var sources: [SourceCheck]
    var quality: QualityReport
    var reliability: Reliability

    static func build(_ a: Asset, _ tf: Timeframe, fetcher: MarketFetcher, now: Double = nowMs()) async throws -> LocalSnapshot {
        let intraStock = a.kind == .stock && (tf == .h1 || tf == .h4)
        async let live: Quote? = intraStock ? QuoteSources.one(a, fetcher: fetcher) : nil
        var c = try await Consensus.candles(a.symbol, kind: a.kind, tf, sources: Sources.candles(a.kind, tf), fetcher: fetcher,
                                            tolerance: a.kind == .crypto ? 0.5 : 1.0, now: now)
        // Stocks in hours have a single candle provider: its last close is checked against the live prices.
        if intraStock, let last = c.candles.last?.close, let q = await live {
            let known = Set(c.sources.map(\.name))
            for s in q.sources where !known.contains(s.name) {
                if let p = s.price, p > 0 {
                    let dev = abs(p / last - 1) * 100
                    c.sources.append(SourceCheck(name: "\(s.name) (cours)", ok: dev <= (tf == .h1 ? 2 : 3), deviation: dev, error: nil))
                } else {
                    c.sources.append(SourceCheck(name: "\(s.name) (cours)", ok: false, deviation: nil, error: s.error ?? "non coté"))
                }
            }
        }
        let quality = SignalEngine.assessQuality(c.candles, intervalMs: tf.stepMs, kind: a.kind, now: now)
        // Distinct providers that agree (a provider's candles and its live price count once).
        let independent = Set(c.sources.filter(\.ok).map { $0.name.replacingOccurrences(of: " (cours)", with: "") })
        let rel = SignalEngine.reliability(quality: quality.score, independent: independent.count, conflict: c.conflict)
        return LocalSnapshot(asset: a, timeframe: tf, candles: c.candles, source: c.source, agreeing: c.sources.filter(\.ok).count,
                             conflict: c.conflict, sources: c.sources, quality: quality, reliability: rel)
    }

    func signal(higher: [Bar]?) -> LocalSignal? {
        SignalEngine.analyze(candles, higher: higher, intervalMs: timeframe.stepMs).map { SignalEngine.gate($0, reliability, issues: quality.issues) }
    }

    /// `/api/candles` body.
    var json: Snapshot {
        Snapshot(symbol: asset.symbol, kind: asset.kind, interval: timeframe.rawValue, candles: candles.map(\.candle), source: source,
                 sources: sources.map { Snapshot.Source(name: $0.name, ok: $0.ok, deviation: $0.deviation.flatMap { $0.isFinite ? $0 : nil }, error: $0.error) },
                 agreeing: agreeing, reliability: reliability)
    }
}
