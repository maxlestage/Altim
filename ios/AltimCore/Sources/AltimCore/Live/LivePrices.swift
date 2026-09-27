import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Live prices: nothing frozen (same feeds and rules as web/server/live.ts).
///
/// - Crypto: real-time WebSocket feeds from 7 exchanges (OKX, Coinbase, Kraken, Bitfinex, Bitget, Gate, Crypto.com).
///   The live price is the median of the exchanges that agree (an exchange more than 1 % away is ignored).
/// - Stocks: no free real-time feed exists; the fast quote sources are polled every 5 s while the screen is open.
///   Cryptos listed on none of these exchanges are polled the same way through the multi-source consensus.
public struct LiveTick: Sendable, Hashable {
    public let assetID: String
    public let price: Double
    /// 24 h change in %, median of the sources that give it.
    public let change: Double?
    public let agreeing: Int
    public let total: Int
    public let sources: [String]
    public let time: Date
    /// Stocks: whether the US regular session is open (nil for a crypto).
    public let marketOpen: Bool?

    public init(assetID: String, price: Double, change: Double?, agreeing: Int, total: Int, sources: [String], time: Date, marketOpen: Bool?) {
        self.assetID = assetID
        self.price = price
        self.change = change
        self.agreeing = agreeing
        self.total = total
        self.sources = sources
        self.time = time
        self.marketOpen = marketOpen
    }
}

// MARK: - Exchange feeds (formats checked on real streams, see StreamingTests)

public enum LiveFeeds {
    public struct Update: Equatable, Sendable {
        public let base: String
        public let price: Double
        public let change: Double?
    }

    public struct Spec: Sendable {
        public let name: String
        public let url: URL
        let subscribe: @Sendable ([String]) -> [String]
        let unsubscribe: @Sendable ([String]) -> [String]
        let parse: @Sendable (Any, inout [Int: String]) -> [Update]
        /// Message sent periodically to keep the connection open.
        let ping: (every: TimeInterval, message: String)?
        /// Reply required by the exchange to some messages (heartbeats).
        let reply: @Sendable (Any) -> String?
    }

    static func json(_ o: Any) -> String {
        (try? JSONSerialization.data(withJSONObject: o)).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
    }

    static func change(_ price: Double, open: Double?) -> Double? {
        guard let open, open > 0 else { return nil }
        return (price / open - 1) * 100
    }

    static func rows(_ m: Any, channelKey: String, channel: String) -> [[String: Any]]? {
        guard let o = m as? [String: Any], (o["arg"] as? [String: Any])?[channelKey] as? String == channel else { return nil }
        return o["data"] as? [[String: Any]]
    }

    static func bitfinexSymbol(_ b: String) -> String { b.count > 3 ? "t\(b):USD" : "t\(b)USD" }

    public static let okx = Spec(
        name: "OKX", url: URL(string: "wss://ws.okx.com/ws/v5/public")!,
        subscribe: { [json(["op": "subscribe", "args": $0.map { ["channel": "tickers", "instId": "\($0)-USDT"] }])] },
        unsubscribe: { [json(["op": "unsubscribe", "args": $0.map { ["channel": "tickers", "instId": "\($0)-USDT"] }])] },
        parse: { m, _ in
            (rows(m, channelKey: "channel", channel: "tickers") ?? []).compactMap { d in
                guard let p = JSON.double(d["last"]), p > 0, let id = d["instId"] as? String else { return nil }
                return Update(base: id.replacingOccurrences(of: "-USDT", with: ""), price: p, change: change(p, open: JSON.double(d["open24h"])))
            }
        },
        ping: (25, "ping"), reply: { _ in nil })

    public static let coinbase = Spec(
        name: "Coinbase", url: URL(string: "wss://ws-feed.exchange.coinbase.com")!,
        subscribe: { [json(["type": "subscribe", "product_ids": $0.map { "\($0)-USD" }, "channels": ["ticker"]])] },
        unsubscribe: { [json(["type": "unsubscribe", "product_ids": $0.map { "\($0)-USD" }, "channels": ["ticker"]])] },
        parse: { m, _ in
            guard let o = m as? [String: Any], o["type"] as? String == "ticker", let p = JSON.double(o["price"]), p > 0,
                  let id = o["product_id"] as? String else { return [] }
            return [Update(base: id.replacingOccurrences(of: "-USD", with: ""), price: p, change: change(p, open: JSON.double(o["open_24h"])))]
        },
        ping: nil, reply: { _ in nil })

    public static let kraken = Spec(
        name: "Kraken", url: URL(string: "wss://ws.kraken.com/v2")!,
        subscribe: { [json(["method": "subscribe", "params": ["channel": "ticker", "symbol": $0.map { "\($0)/USD" }]])] },
        unsubscribe: { [json(["method": "unsubscribe", "params": ["channel": "ticker", "symbol": $0.map { "\($0)/USD" }]])] },
        parse: { m, _ in
            guard let o = m as? [String: Any], o["channel"] as? String == "ticker", let data = o["data"] as? [[String: Any]] else { return [] }
            return data.compactMap { d in
                guard let p = JSON.double(d["last"]), p > 0, let s = d["symbol"] as? String else { return nil }
                return Update(base: s.replacingOccurrences(of: "/USD", with: ""), price: p, change: JSON.double(d["change_pct"]))
            }
        },
        ping: (30, json(["method": "ping"])), reply: { _ in nil })

    public static let bitfinex = Spec(
        name: "Bitfinex", url: URL(string: "wss://api-pub.bitfinex.com/ws/2")!,
        subscribe: { $0.map { json(["event": "subscribe", "channel": "ticker", "symbol": bitfinexSymbol($0)]) } },
        unsubscribe: { _ in [] }, // by channel id, see LivePriceHub
        parse: { m, chanIds in
            if let o = m as? [String: Any], o["event"] as? String == "subscribed", o["channel"] as? String == "ticker",
               let id = (o["chanId"] as? NSNumber)?.intValue, let s = o["symbol"] as? String {
                var base = String(s.dropFirst())
                if base.hasSuffix(":USD") { base.removeLast(4) } else if base.hasSuffix("USD") { base.removeLast(3) }
                chanIds[id] = base
                return []
            }
            guard let a = m as? [Any], a.count >= 2, let id = (a[0] as? NSNumber)?.intValue, let base = chanIds[id],
                  let d = a[1] as? [Any], d.count > 6, let p = JSON.double(d[6]), p > 0 else { return [] }
            return [Update(base: base, price: p, change: JSON.double(d[5]).map { $0 * 100 })]
        },
        ping: nil, reply: { _ in nil })

    public static let bitget = Spec(
        name: "Bitget", url: URL(string: "wss://ws.bitget.com/v2/ws/public")!,
        subscribe: { [json(["op": "subscribe", "args": $0.map { ["instType": "SPOT", "channel": "ticker", "instId": "\($0)USDT"] }])] },
        unsubscribe: { [json(["op": "unsubscribe", "args": $0.map { ["instType": "SPOT", "channel": "ticker", "instId": "\($0)USDT"] }])] },
        parse: { m, _ in
            (rows(m, channelKey: "channel", channel: "ticker") ?? []).compactMap { d in
                guard let p = JSON.double(d["lastPr"]), p > 0, let id = d["instId"] as? String, id.hasSuffix("USDT") else { return nil }
                return Update(base: String(id.dropLast(4)), price: p, change: change(p, open: JSON.double(d["open24h"])))
            }
        },
        ping: (25, "ping"), reply: { _ in nil })

    public static let gate = Spec(
        name: "Gate.io", url: URL(string: "wss://api.gateio.ws/ws/v4/")!,
        subscribe: { [json(["time": Int(Date().timeIntervalSince1970), "channel": "spot.tickers", "event": "subscribe", "payload": $0.map { "\($0)_USDT" }])] },
        unsubscribe: { [json(["time": Int(Date().timeIntervalSince1970), "channel": "spot.tickers", "event": "unsubscribe", "payload": $0.map { "\($0)_USDT" }])] },
        parse: { m, _ in
            guard let o = m as? [String: Any], o["channel"] as? String == "spot.tickers", o["event"] as? String == "update",
                  let r = o["result"] as? [String: Any], let p = JSON.double(r["last"]), p > 0, let pair = r["currency_pair"] as? String else { return [] }
            return [Update(base: pair.replacingOccurrences(of: "_USDT", with: ""), price: p, change: JSON.double(r["change_percentage"]))]
        },
        ping: (20, json(["channel": "spot.ping"])), reply: { _ in nil })

    public static let cryptoCom = Spec(
        name: "Crypto.com", url: URL(string: "wss://stream.crypto.com/exchange/v1/market")!,
        subscribe: { [json(["id": 1, "method": "subscribe", "params": ["channels": $0.map { "ticker.\($0)_USDT" }]])] },
        unsubscribe: { [json(["id": 2, "method": "unsubscribe", "params": ["channels": $0.map { "ticker.\($0)_USDT" }]])] },
        parse: { m, _ in
            guard let o = m as? [String: Any], o["method"] as? String == "subscribe", let r = o["result"] as? [String: Any],
                  r["channel"] as? String == "ticker", let name = r["instrument_name"] as? String, let data = r["data"] as? [[String: Any]] else { return [] }
            return data.compactMap { d in
                guard let p = JSON.double(d["a"]), p > 0 else { return nil }
                return Update(base: name.replacingOccurrences(of: "_USDT", with: ""), price: p, change: JSON.double(d["c"]).map { $0 * 100 })
            }
        },
        ping: nil,
        reply: { m in
            guard let o = m as? [String: Any], o["method"] as? String == "public/heartbeat" else { return nil }
            return json(["id": o["id"] ?? 0, "method": "public/respond-heartbeat"])
        })

    public static let all: [Spec] = [okx, coinbase, kraken, bitfinex, bitget, gate, cryptoCom]
}

// MARK: - Consensus of live quotes

public enum LiveConsensus {
    public struct SourceQuote: Sendable, Hashable {
        public let price: Double
        public let change: Double?
        public let time: Date
        public init(price: Double, change: Double?, time: Date) {
            self.price = price
            self.change = change
            self.time = time
        }
    }

    static func median(_ v: [Double]) -> Double {
        let s = v.sorted()
        return s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2
    }

    /// Median of the fresh quotes; a source more than `tolerance` % from the median is ignored.
    public static func combine(_ quotes: [String: SourceQuote], assetID: String, now: Date, maxAge: TimeInterval = 120,
                               tolerance: Double = 1, marketOpen: Bool? = nil) -> LiveTick? {
        let fresh = quotes.filter { now.timeIntervalSince($0.value.time) <= maxAge && $0.value.price > 0 }
        guard !fresh.isEmpty else { return nil }
        let ref = median(fresh.map(\.value.price))
        let ok = fresh.filter { abs($0.value.price / ref - 1) * 100 <= tolerance }
        let changes = ok.compactMap(\.value.change).filter(\.isFinite)
        return LiveTick(assetID: assetID, price: median(ok.map(\.value.price)), change: changes.isEmpty ? nil : median(changes),
                        agreeing: ok.count, total: fresh.count, sources: ok.keys.sorted(),
                        time: ok.map(\.value.time).max() ?? now, marketOpen: marketOpen)
    }
}

/// US regular session (9:30 – 16:00 New York, weekdays). Holidays are not known: the price simply stops moving.
public enum USMarket {
    public static func isOpen(_ date: Date = Date()) -> Bool {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "America/New_York")!
        let c = cal.dateComponents([.year, .month, .day, .weekday], from: date)
        guard let wd = c.weekday, (2...6).contains(wd), let y = c.year, let m = c.month, let d = c.day,
              let open = newYorkOpen(year: y, month: m, day: d) else { return false }
        return date >= open && date < open.addingTimeInterval(6.5 * 3600)
    }
}

// MARK: - Hub

/// One WebSocket per exchange shared by every screen; subscriptions follow what is on screen.
public actor LivePriceHub {
    public typealias Fetch = @Sendable (Asset) async -> [(source: String, quote: Quote)]

    private let feeds: [LiveFeeds.Spec]
    private let session: URLSession
    /// REST quotes for stocks and for the cryptos no WebSocket exchange streams.
    private let rest: Fetch
    private let pollEvery: TimeInterval

    private var listeners: [UUID: (ids: Set<String>, cont: AsyncStream<LiveTick>.Continuation)] = [:]
    private var assets: [String: Asset] = [:]
    private var quotes: [String: [String: LiveConsensus.SourceQuote]] = [:]
    private var last: [String: LiveTick] = [:]
    private var lastEmit: [String: Date] = [:]
    private var pending: Set<String> = []
    /// Stocks: US session state at the last poll.
    private var marketOpen: [String: Bool] = [:]
    private var sockets: [String: URLSessionWebSocketTask] = [:]
    private var chanIds: [String: [Int: String]] = [:]
    private var retries: [String: Int] = [:]
    private var subscribed: [String: Set<String>] = [:]
    private var pollTask: Task<Void, Never>?

    public init(feeds: [LiveFeeds.Spec] = LiveFeeds.all, session: URLSession = .shared, pollEvery: TimeInterval = 5, rest: @escaping Fetch) {
        self.feeds = feeds
        self.session = session
        self.pollEvery = pollEvery
        self.rest = rest
    }

    /// Live ticks of these assets: the last known price right away, then every change (4 per second at most).
    public func stream(_ list: [Asset]) -> AsyncStream<LiveTick> {
        let id = UUID()
        let (stream, cont) = AsyncStream<LiveTick>.makeStream(bufferingPolicy: .bufferingNewest(64))
        let ids = Set(list.map(\.id))
        listeners[id] = (ids, cont)
        for a in list { assets[a.id] = a }
        for i in ids { if let t = last[i] { cont.yield(t) } }
        cont.onTermination = { [weak self] _ in
            Task { await self?.remove(id) }
        }
        syncSubscriptions()
        if pollTask == nil {
            pollTask = Task { [weak self] in
                while !Task.isCancelled {
                    await self?.poll()
                    try? await Task.sleep(nanoseconds: UInt64((self?.pollEvery ?? 5) * 1_000_000_000))
                }
            }
        }
        return stream
    }

    /// Number of watched assets (for tests and monitoring).
    public var watched: Int { watchedIDs.count }

    private var watchedIDs: Set<String> { listeners.values.reduce(into: Set<String>()) { $0.formUnion($1.ids) } }

    private func remove(_ id: UUID) {
        listeners[id] = nil
        syncSubscriptions()
        if listeners.isEmpty {
            pollTask?.cancel()
            pollTask = nil
        }
    }

    /// Opens / closes exchange subscriptions to match the watched cryptos.
    private func syncSubscriptions() {
        let bases = Set(watchedIDs.compactMap { assets[$0] }.filter { $0.assetClass == .crypto }.map(\.base))
        for spec in feeds {
            let current = subscribed[spec.name] ?? []
            let added = bases.subtracting(current), gone = current.subtracting(bases)
            subscribed[spec.name] = bases
            if bases.isEmpty {
                sockets[spec.name]?.cancel(with: .normalClosure, reason: nil)
                sockets[spec.name] = nil
                continue
            }
            if sockets[spec.name] == nil { connect(spec); continue }
            if !added.isEmpty { send(spec, spec.subscribe(added.sorted())) }
            if !gone.isEmpty {
                if spec.name == LiveFeeds.bitfinex.name {
                    for (cid, b) in chanIds[spec.name] ?? [:] where gone.contains(b) {
                        send(spec, [LiveFeeds.json(["event": "unsubscribe", "chanId": cid])])
                        chanIds[spec.name]?[cid] = nil
                    }
                } else {
                    send(spec, spec.unsubscribe(gone.sorted()))
                }
            }
        }
    }

    private func send(_ spec: LiveFeeds.Spec, _ messages: [String]) {
        guard let task = sockets[spec.name] else { return }
        for m in messages { task.send(.string(m)) { _ in } }
    }

    private func connect(_ spec: LiveFeeds.Spec) {
        let task = session.webSocketTask(with: spec.url)
        sockets[spec.name] = task
        chanIds[spec.name] = [:]
        task.resume()
        send(spec, spec.subscribe((subscribed[spec.name] ?? []).sorted()))
        Task { await receive(spec, task) }
        if let ping = spec.ping {
            Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: UInt64(ping.every * 1_000_000_000))
                    guard let self, await self.isCurrent(spec.name, task) else { return }
                    task.send(.string(ping.message)) { _ in }
                }
            }
        }
    }

    private func isCurrent(_ name: String, _ task: URLSessionWebSocketTask) -> Bool { sockets[name] === task }

    private func receive(_ spec: LiveFeeds.Spec, _ task: URLSessionWebSocketTask) async {
        while isCurrent(spec.name, task) {
            let message: URLSessionWebSocketTask.Message
            do {
                message = try await task.receive()
            } catch {
                return reconnect(spec, task)
            }
            retries[spec.name] = 0
            let text: String
            switch message {
            case .string(let s): text = s
            case .data(let d): text = String(decoding: d, as: UTF8.self)
            @unknown default: continue
            }
            handle(spec, text)
        }
    }

    /// Decodes one exchange message (internal for tests).
    func handle(_ spec: LiveFeeds.Spec, _ text: String, now: Date = Date()) {
        guard text != "pong", let obj = try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed]) else { return }
        if let r = spec.reply(obj) { send(spec, [r]) }
        var ids = chanIds[spec.name] ?? [:]
        let updates = spec.parse(obj, &ids)
        chanIds[spec.name] = ids
        for u in updates {
            for id in watchedIDs where assets[id]?.assetClass == .crypto && assets[id]?.base == u.base {
                ingest(id, source: spec.name, price: u.price, change: u.change, at: now)
            }
        }
    }

    private func reconnect(_ spec: LiveFeeds.Spec, _ task: URLSessionWebSocketTask) {
        guard isCurrent(spec.name, task) else { return }
        sockets[spec.name] = nil
        guard !(subscribed[spec.name] ?? []).isEmpty else { return }
        let n = retries[spec.name, default: 0]
        retries[spec.name] = n + 1
        let delay = min(30, pow(2, Double(n)))
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            await self?.reconnectIfNeeded(spec)
        }
    }

    private func reconnectIfNeeded(_ spec: LiveFeeds.Spec) {
        if sockets[spec.name] == nil, !(subscribed[spec.name] ?? []).isEmpty { connect(spec) }
    }

    /// Records one source's quote and schedules a tick (internal for tests).
    func ingest(_ id: String, source: String, price: Double, change: Double?, at now: Date = Date()) {
        quotes[id, default: [:]][source] = .init(price: price, change: change, time: now)
        schedule(id)
    }

    /// At most 4 ticks per second per asset: the latest state is always the one sent.
    private func schedule(_ id: String) {
        guard !pending.contains(id) else { return }
        let wait = max(0, 0.25 - Date().timeIntervalSince(lastEmit[id] ?? .distantPast))
        if wait == 0 { return emit(id) }
        pending.insert(id)
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(wait * 1_000_000_000))
            await self?.flush(id)
        }
    }

    private func flush(_ id: String) {
        pending.remove(id)
        emit(id)
    }

    private func emit(_ id: String) {
        let stock = assets[id]?.assetClass == .stock
        guard let q = quotes[id], let tick = LiveConsensus.combine(q, assetID: id, now: Date(), maxAge: stock ? 3600 : 120,
                                                                   tolerance: stock ? 1.5 : 1, marketOpen: stock ? marketOpen[id] : nil)
        else { return }
        let prev = last[id]
        last[id] = tick
        lastEmit[id] = Date()
        if let prev, prev.price == tick.price, prev.agreeing == tick.agreeing, prev.total == tick.total, prev.marketOpen == tick.marketOpen { return }
        for l in listeners.values where l.ids.contains(id) { l.cont.yield(tick) }
    }

    /// REST polling: every stock, plus the cryptos whose WebSocket feeds are silent (none lists them, or cut).
    func poll() async {
        let now = Date()
        let feedNames = Set(feeds.map(\.name))
        let targets = watchedIDs.compactMap { assets[$0] }.filter { a in
            guard a.assetClass == .crypto else { return true }
            return !(quotes[a.id] ?? [:]).contains { feedNames.contains($0.key) && now.timeIntervalSince($0.value.time) < 15 }
        }
        guard !targets.isEmpty else { return }
        let rest = self.rest
        let results = await withTaskGroup(of: (Asset, [(source: String, quote: Quote)]).self) { group in
            for a in targets { group.addTask { (a, await rest(a)) } }
            var out: [(Asset, [(source: String, quote: Quote)])] = []
            for await r in group { out.append(r) }
            return out
        }
        let t = Date()
        let open = USMarket.isOpen(t)
        for (a, list) in results where !list.isEmpty {
            for (source, q) in list where q.price > 0 {
                quotes[a.id, default: [:]][source] = .init(price: q.price, change: q.changePercent24h, time: t)
            }
            if a.assetClass == .stock { marketOpen[a.id] = open }
            schedule(a.id)
        }
    }

    public func close() {
        pollTask?.cancel()
        pollTask = nil
        for (_, s) in sockets { s.cancel(with: .normalClosure, reason: nil) }
        sockets = [:]
        subscribed = [:]
        for l in listeners.values { l.cont.finish() }
        listeners = [:]
    }
}

public extension LivePriceHub {
    /// Standard hub: stocks polled from Robinhood, TradingView, Zacks and Webull; cryptos without a live feed
    /// read from the multi-source consensus.
    static func standard(transport: HTTPTransport = URLSessionTransport(), market: ConsensusMarketData) -> LivePriceHub {
        let stockSources: [MarketSource] = [RobinhoodQuotes(transport: transport), TradingViewQuotes(transport: transport),
                                            ZacksQuotes(transport: transport), WebullQuotes(transport: transport)]
        return LivePriceHub { asset in
            if asset.assetClass == .crypto {
                guard let q = try? await market.quote(for: asset) else { return [] }
                return [(source: "Consensus", quote: q)]
            }
            return await withTaskGroup(of: (String, Quote?).self) { group in
                for s in stockSources { group.addTask { (s.name.replacingOccurrences(of: " (cours)", with: ""), try? await s.quote(for: asset)) } }
                var out: [(source: String, quote: Quote)] = []
                for await (n, q) in group { if let q { out.append((n, q)) } }
                return out
            }
        }
    }
}
