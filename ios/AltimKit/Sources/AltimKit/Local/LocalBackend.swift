import Foundation

/// The Altim API answered on the iPhone itself: the same JSON as the server's routes, computed from market data the
/// app reads straight from the exchanges and stock providers. With it, Altim works without any server: the routes it
/// knows never leave the phone; the others go to the server when one is connected (Réglages), else say they need one.
public actor LocalBackend {
    let fetcher: MarketFetcher
    private var snapshots: [String: (at: Double, task: Task<LocalSnapshot, Error>)] = [:]
    private var quoteCache: [String: (at: Double, quote: Quote)] = [:]
    /// Quotes being read: simultaneous callers (live prices, Radar, alerts, Selection) wait for the same read.
    private var quoteReads: [String: Task<Quote?, Never>] = [:]
    private var fxCache: (at: Double, value: Fx.Response)?
    /// Long daily histories, shared while downloading: simultaneous callers wait for the same download.
    private var longCache: [String: (at: Double, task: Task<[Bar], Error>)] = [:]
    private var newsCache: [String: (at: Double, report: NewsReport)] = [:]
    /// The news feeds, shared by every news report for 5 minutes.
    private let feeds = FeedCache()
    var macroCache: (at: Double, info: MacroInfo)?
    /// The macro report being computed: the guards of several assets wait for the same one.
    var macroRead: Task<MacroInfo, Error>?
    /// Market-wide data read by every asset's guard or by several stocks at once (Fear & Greed, macro series,
    /// earnings calendar pages), shared for 10 minutes.
    let shared = FeedCache(ttl: 600_000)
    var sharedFetcher: MarketFetcher { SharedFeedFetcher(base: fetcher, cache: shared) }
    var earningsCache: [String: (at: Double, events: [EarningsEvent])] = [:]
    private var botModels: [String: (at: Double, task: Task<BotEngine.Model?, Never>)] = [:]
    private var botHistories: [Kind: (at: Double, task: Task<[[Bar]], Never>)] = [:]

    /// False for the widget, the Watch and Siri (little time and memory): the verdict is then the rule alone, without
    /// training the bot.
    let botEnabled: Bool

    /// Folder where the trained bots are kept between launches (nil: memory only).
    let botStore: URL?

    public init(fetcher: MarketFetcher = URLSessionFetcher(), bot: Bool = true, botStore: URL? = nil) {
        self.fetcher = fetcher
        botEnabled = bot
        self.botStore = botStore
    }

    /// The app's folder for the trained bots (Caches: iOS may empty it, the bots are then trained again).
    public static var defaultBotStore: URL? {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first?.appendingPathComponent("altim-bots", isDirectory: true)
    }

    /// Routes answered here.
    public static let routes: Set<String> = ["/api/candles", "/api/radar", "/api/tickers", "/api/search", "/api/fx", "/api/zones", "/api/backtest", "/api/history", "/api/news", "/api/alerts", "/api/guard", "/api/rotation", "/api/verdict", "/api/sectors", "/api/macro", "/api/earnings"]

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
            case "/api/earnings":
                let symbols = (query["symbols"] ?? "").split(separator: ",").map { String($0).trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                return (try Self.encode(await earnings(Array(symbols.prefix(30)))), 200)
            case "/api/macro":
                return (try Self.encode(try await macroReport()), 200)
            case "/api/sectors":
                return (await LocalSectors.report((query["symbols"] ?? "").split(separator: ",").map(String.init), fetcher: fetcher), 200)
            case "/api/verdict":
                let a = try Self.asset(query["symbol"], query["kind"])
                let d = try await snapshot(a, .d1)
                var v = await botVerdict(a, d.candles)
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
            var v = await botVerdict(a, d.candles)
            if d.reliability.level == "low" { v.action = .hold }
            let s = await chart ?? d
            return RadarRow(symbol: a.symbol, kind: a.kind, name: a.name, price: s.candles.last?.close, change: nil, priceSources: nil,
                            signal: v.state == .unknown ? nil : SignalSummary(action: v.action, score: v.score, confidence: v.confidence * d.reliability.score / 100),
                            reliability: d.reliability, agreeing: d.agreeing, sources: d.sources.count,
                            sparkline: s.candles.suffix(48).map(\.close), error: nil, botRisk: v.botRisk)
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
        // The same verdicts as each asset's page (trend bot's veto included).
        var verdicts: [String: TrendEngine.Verdict] = [:]
        for (a, bars) in data { verdicts[a.id] = await botVerdict(a, bars) }
        // The test replays the market filter too (Bitcoin is in the crypto list; SPY is read apart).
        let b = TrendEngine.benchmark(kind)
        let marketBars: [Bar]?
        if let own = data.first(where: { $0.0.id == b.id })?.1 { marketBars = own } else { marketBars = try? await longDaily(b) }
        let r = RotationReport(kind: kind, asOf: nowMs(), top: data.count, lookback: TrendEngine.lookback(kind),
                               picks: Rotation.picks(data, kind: kind, prices: p, verdicts: verdicts), scanned: data.count, test: Rotation.sleeves(data, kind: kind, market: marketBars),
                               notes: Rotation.notes)
        if !data.isEmpty { rotationCache[kind] = (nowMs(), r) }
        return r
    }

    // MARK: Bot

    /// The trained bot of a market, learned on the long histories of its universe (Rotation.universe), kept 12 hours;
    /// one training at a time, shared by every caller. Nil when too few histories could be read.
    func botModel(_ kind: Kind, _ target: BotEngine.Target = .gain(days: BotEngine.horizon)) async -> BotEngine.Model? {
        let key = "\(kind.rawValue):\(target)"
        if let c = botModels[key], nowMs() - c.at < 12 * 3_600_000 {
            if let m = await c.task.value { return m }
            if nowMs() - c.at < 600_000 { return nil }
            // Failed 10 minutes ago or more (network): the histories are read again.
            botHistories[kind] = nil
        }
        // A bot trained less than 12 hours ago at a previous launch: no download, no training.
        if botModels[key] == nil, let file = botStore?.appendingPathComponent("\(kind.rawValue)-\(target.days)-\(target.isFall ? "fall" : "gain").json"),
           let data = try? Data(contentsOf: file), let saved = try? JSONDecoder().decode(BotEngine.Model.self, from: data),
           saved.isCurrent, nowMs() - saved.trainedAt < 12 * 3_600_000, saved.trainedAt <= nowMs() {
            botModels[key] = (saved.trainedAt, Task { saved })
            return saved
        }
        let histories = universeHistories(kind)
        let store = botStore
        let task = Task<BotEngine.Model?, Never> {
            let h = await histories.value
            guard h.count >= 5 else { return nil }
            // Off the actor: the other routes keep answering while the bot learns.
            let model = await Task.detached(priority: .utility) { BotEngine.Model.train(h, target: target) }.value
            if let model, let store, let data = try? JSONEncoder().encode(model) {
                try? FileManager.default.createDirectory(at: store, withIntermediateDirectories: true)
                try? data.write(to: store.appendingPathComponent("\(kind.rawValue)-\(target.days)-\(target.isFall ? "fall" : "gain").json"), options: .atomic)
            }
            return model
        }
        botModels[key] = (nowMs(), task)
        return await task.value
    }

    /// Long daily histories of a market's universe, read once for all its bots (kept as long as the bots).
    private func universeHistories(_ kind: Kind) -> Task<[[Bar]], Never> {
        if let c = botHistories[kind], nowMs() - c.at < 12 * 3_600_000 { return c.task }
        let list = Rotation.universe[kind] ?? []
        let task = Task<[[Bar]], Never> {
            await withTaskGroup(of: [Bar]?.self) { g in
                for a in list { g.addTask { try? await self.longDaily(a) } }
                var out: [[Bar]] = []
                for await r in g { if let r, r.count > 400 { out.append(r) } }
                return out
            }
        }
        botHistories[kind] = (nowMs(), task)
        return task
    }

    /// The daily verdict with the bots: the trend bot's veto, the risk bot's expected fall (the rule alone when the
    /// bots are unavailable).
    func botVerdict(_ a: Asset, _ candles: [Bar]) async -> TrendEngine.Verdict {
        var v = TrendEngine.verdict(candles, kind: a.kind)
        guard v.state != .unknown else { return v }
        v = await marketVerdict(a, v, assetTime: candles.last?.time)
        guard botEnabled else { return v }
        async let trend = botModel(a.kind)
        async let risk = botModel(a.kind, .fall(days: BotEngine.horizon))
        if let m = await trend {
            v = BotEngine.apply(m.predict(candles), to: v)
            v.botTrainedAt = m.trainedAt
            v.botExamples = m.examples
        }
        v.botRisk = BotEngine.calibratedFall(await risk?.predict(candles), kind: a.kind)
        return v
    }

    /// The market filter applied to a verdict (the rule alone when the benchmark's candles cannot be read).
    /// Unreliable or stale market candles: the rule alone (`TrendEngine.marketUp(_:kind:reliability:assetTime:)`).
    func marketVerdict(_ a: Asset, _ v: TrendEngine.Verdict, assetTime: Double? = nil) async -> TrendEngine.Verdict {
        let b = TrendEngine.benchmark(a.kind)
        guard let snap = try? await snapshot(b, .d1) else { return v }
        let up = TrendEngine.marketUp(snap.candles, kind: a.kind, reliability: snap.reliability, assetTime: assetTime)
        return TrendEngine.applyMarket(up, kind: a.kind, to: v)
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
        let b = TrendEngine.benchmark(a.kind)
        // The market filter replayed too (the benchmark itself needs none); without its candles, the rule alone.
        let market = b.id == a.id ? nil : try? await longDaily(b)
        return TrendEngine.backtest(try await longDaily(a), asset: a, reliability: s.reliability, market: market)
    }

    /// Longest daily history: for cryptos the exchanges' ≈ 1 000 days when they agree, else the usual daily candles
    /// (stocks: Nasdaq's ≈ 5 years, checked against Robinhood's and Yahoo's).
    func longDaily(_ a: Asset) async throws -> [Bar] {
        let key = "long:\(a.id)"
        if let c = longCache[key], nowMs() - c.at < 3_600_000, let bars = try? await c.task.value { return bars }
        let fetcher = self.fetcher
        let task = Task<[Bar], Error> {
            let s = try await self.snapshot(a, .d1)
            var bars = s.candles
            let longSources = a.kind == .crypto ? Sources.cryptoLongDaily : Sources.stockLongDaily
            if let long = try? await Consensus.candles(a.symbol, kind: a.kind, .d1, sources: longSources, fetcher: fetcher, tolerance: a.kind == .crypto ? 0.5 : 1.0),
               long.candles.count > bars.count {
                bars = long.candles
            }
            return bars
        }
        longCache[key] = (nowMs(), task)
        do { return try await task.value } catch {
            // A failure is not kept: the next caller tries again.
            longCache[key] = nil
            throw error
        }
    }

    // MARK: News

    /// The news section, read again at most every 5 minutes for the same assets.
    func news(_ assets: [Asset]) async -> NewsReport {
        let key = assets.map(\.id).sorted().joined(separator: ",")
        if let c = newsCache[key], nowMs() - c.at < 300_000 { return c.report }
        let r = await NewsEngine.report(assets, fetcher: SharedFeedFetcher(base: fetcher, cache: feeds))
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

    /// Consensus quotes, kept 10 seconds (the live prices poll them); an asset already being read is not read twice.
    public func quotes(_ list: [Asset]) async -> [Quote] {
        let now = nowMs()
        var reads: [(Asset, Task<Quote?, Never>)] = []
        for a in list where quoteCache[a.id].map({ now - $0.at >= 10_000 }) ?? true {
            if let t = quoteReads[a.id] { reads.append((a, t)); continue }
            let fetcher = self.fetcher
            let t = Task { await QuoteSources.one(a, fetcher: fetcher) }
            quoteReads[a.id] = t
            reads.append((a, t))
        }
        for (a, t) in reads {
            let q = await t.value
            if quoteReads[a.id] == t { quoteReads[a.id] = nil }
            if let q { quoteCache[a.id] = (now, q) }
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
