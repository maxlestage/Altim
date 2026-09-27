import Foundation

/// Result of checking one source.
public struct SourceCheck: Sendable, Hashable, Identifiable {
    public enum Status: Sendable, Hashable {
        /// Source whose candles feed the analysis.
        case primary
        /// Agrees with the consensus.
        case agrees
        /// Deviation beyond tolerance: discarded.
        case diverges
        /// Error or unavailable.
        case failed(String)
        /// Not used (enough sources already agree).
        case skipped
    }

    public let name: String
    public let status: Status
    /// Median deviation from the consensus, in %.
    public let deviationPercent: Double?
    public let lastPrice: Double?
    public let latency: TimeInterval?

    public var id: String { name }
    public var isHealthy: Bool { status == .primary || status == .agrees }
}

public enum ReliabilityLevel: String, Codable, Sendable, Hashable {
    case high, medium, low

    public var label: String {
        switch self {
        case .high: return "Fiabilité élevée"
        case .medium: return "Fiabilité moyenne"
        case .low: return "Fiabilité faible"
        }
    }
}

/// Validated market snapshot: candles from the best source, cross-checked with the others.
public struct MarketSnapshot: Sendable {
    public let asset: Asset
    public let timeframe: Timeframe
    public let candles: [Candle]
    public let primarySource: String
    public let checks: [SourceCheck]
    /// Median price across all sources that responded (quote sources included).
    public let consensusPrice: Double?
    public let quality: DataQualityReport
    /// Several sources responded but none agree: impossible to know which one is right.
    public let conflict: Bool

    public var agreeingSources: Int { checks.filter(\.isHealthy).count }

    /// Agreeing sources from distinct providers (Yahoo's two servers count as one).
    public var independentSources: Int {
        Set(checks.filter(\.isHealthy).map { $0.name.components(separatedBy: " (")[0] }).count
    }
    public var maxDeviationPercent: Double {
        checks.filter(\.isHealthy).compactMap(\.deviationPercent).max() ?? 0
    }

    /// Sources that returned usable data (candles or live price).
    public var respondingSources: Int { checks.filter { $0.isHealthy || $0.status == .diverges }.count }

    /// Overall score 0–100: data quality, capped by the number of agreeing sources.
    /// Score cap by number of independent agreeing sources: a decision needs several confirmations
    /// (1 source = no advice, 3 minimum for "high"). Same table as web/src/engine/reliability.ts.
    public static let reliabilityCap: [Double] = [40, 40, 60, 75, 90, 100]

    public var reliabilityScore: Double {
        let cap = Self.reliabilityCap[min(independentSources, 5)]
        return conflict ? min(quality.score, 30) : min(quality.score, cap)
    }

    public var reliability: ReliabilityLevel {
        switch reliabilityScore {
        case 75...: return .high
        case 50...: return .medium
        default: return .low
        }
    }

    public var summary: String {
        let divergent = checks.filter { $0.status == .diverges }.count
        if conflict { return "Sources en désaccord : données non vérifiables" }
        var text = "\(agreeingSources)/\(respondingSources) sources concordantes"
        if agreeingSources > 1 { text += String(format: ", écart max %.2f %%", maxDeviationPercent) }
        if divergent > 0 { text += " · \(divergent) écartée(s)" }
        return text
    }
}

public enum ConsensusError: Error, LocalizedError {
    case noSource
    case allFailed([SourceCheck])

    public var errorDescription: String? {
        switch self {
        case .noSource: return "Aucune source de données pour cet actif."
        case let .allFailed(checks):
            let details = checks.compactMap { c -> String? in
                if case let .failed(m) = c.status { return "\(c.name) : \(m)" }
                return nil
            }
            return "Toutes les sources ont échoué.\n" + details.joined(separator: "\n")
        }
    }
}

/// Multi-source market data with cross-validation.
///
/// 1. Candles are fetched from every eligible source in parallel (or, if `targetSources` is set,
///    in priority order until that many have responded).
/// 2. For each timestamp, the reference value is the median of the closes. Each source
///    is compared with that median; beyond `tolerance` it is discarded.
/// 3. The first agreeing source (by priority) with enough history feeds the analysis.
/// 4. Quote-only sources (CoinGecko, Finnhub) contribute to the consensus price.
public struct ConsensusMarketData: MarketDataProvider {
    public let sources: [MarketSource]
    public var targetSources: Int
    public var cryptoTolerancePercent: Double
    public var stockTolerancePercent: Double
    public var minimumCandles: Int

    public init(sources: [MarketSource], targetSources: Int = .max, cryptoTolerancePercent: Double = 0.5,
                stockTolerancePercent: Double = 1.0, minimumCandles: Int = 60) {
        self.sources = sources
        self.targetSources = targetSources
        self.cryptoTolerancePercent = cryptoTolerancePercent
        self.stockTolerancePercent = stockTolerancePercent
        self.minimumCandles = minimumCandles
    }

    /// Default configuration: every free source + keyed sources when available.
    public static func standard(transport: HTTPTransport, alpaca: (key: String, secret: String)? = nil,
                                twelveDataKey: String? = nil, polygonKey: String? = nil,
                                finnhubKey: String? = nil) -> ConsensusMarketData {
        var list: [MarketSource] = [
            BinanceSource(transport: transport),
            OKXMarketData(transport: transport),
            CoinbaseMarketData(transport: transport),
            KrakenMarketData(transport: transport),
            KuCoinMarketData(transport: transport),
            GateMarketData(transport: transport),
            BitfinexMarketData(transport: transport),
            BinanceSource.us(transport: transport),
            BitstampMarketData(transport: transport),
            GeminiMarketData(transport: transport),
            CryptoComMarketData(transport: transport),
            BitgetMarketData(transport: transport),
            MEXCMarketData(transport: transport),
            HTXMarketData(transport: transport),
            YahooMarketData(server: 1, transport: transport),
            NasdaqMarketData(transport: transport),
            RobinhoodMarketData(transport: transport),
            CboeMarketData(transport: transport),
        ]
        // Keyed sources (independent) go ahead of Yahoo's second server, which shares the same backend.
        if let a = alpaca, !a.key.isEmpty, !a.secret.isEmpty {
            list.append(AlpacaMarketData(keyId: a.key, secret: a.secret, transport: transport))
        }
        if let k = twelveDataKey, !k.isEmpty { list.append(TwelveDataMarketData(apiKey: k, transport: transport)) }
        if let k = polygonKey, !k.isEmpty { list.append(PolygonMarketData(apiKey: k, transport: transport)) }
        list.append(YahooMarketData(server: 2, transport: transport))
        list.append(CoinGeckoQuotes(transport: transport))
        list.append(CboeQuotes(transport: transport))
        list.append(RobinhoodQuotes(transport: transport))
        list.append(TradingViewQuotes(transport: transport))
        if let k = finnhubKey, !k.isEmpty { list.append(FinnhubQuotes(apiKey: k, transport: transport)) }
        return ConsensusMarketData(sources: list)
    }

    // MARK: MarketDataProvider

    public func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] {
        try await snapshot(for: asset, timeframe: timeframe, limit: limit).candles
    }

    public func quote(for asset: Asset) async throws -> Quote {
        let eligible = sources.filter { $0.supports(asset) }
        guard !eligible.isEmpty else { throw ConsensusError.noSource }
        let quotes = await withTaskGroup(of: Quote?.self) { group in
            for s in eligible.prefix(targetSources >= eligible.count ? eligible.count : targetSources + 2) { group.addTask { try? await s.quote(for: asset) } }
            var out: [Quote] = []
            for await q in group { if let q, q.price > 0 { out.append(q) } }
            return out
        }
        guard !quotes.isEmpty else { throw ConsensusError.allFailed([]) }
        let price = DataQuality.median(quotes.map(\.price))
        let change = DataQuality.median(quotes.map(\.changePercent24h))
        return Quote(price: price, changePercent24h: change, time: quotes.map(\.time).max() ?? Date())
    }

    // MARK: Snapshot

    private struct Fetched {
        let index: Int
        let name: String
        let candles: [Candle]?
        let quote: Double?
        let error: String?
        let latency: TimeInterval
    }

    public func snapshot(for asset: Asset, timeframe: Timeframe, limit: Int = 500, now: Date = Date()) async throws -> MarketSnapshot {
        let eligible = Array(sources.enumerated().filter { $0.element.supports(asset) })
        guard !eligible.isEmpty else { throw ConsensusError.noSource }
        let candleSources = eligible.filter { $0.element.providesCandles && $0.element.supports(timeframe) }
        let quoteSources = eligible.filter { !$0.element.providesCandles }

        // Waves in priority order until enough sources have responded.
        var fetched: [Fetched] = []
        var cursor = 0
        while cursor < candleSources.count {
            let succeeded = fetched.filter { $0.candles != nil }.count
            let needed = targetSources - succeeded
            if needed <= 0 { break }
            let wave = candleSources[cursor..<(cursor + min(needed, candleSources.count - cursor))]
            cursor += wave.count
            let results = await withTaskGroup(of: Fetched.self) { group in
                for (index, source) in wave {
                    group.addTask {
                        let start = Date()
                        do {
                            let c = try await source.candles(for: asset, timeframe: timeframe, limit: limit)
                            guard !c.isEmpty else { throw APIError.decoding("aucune bougie") }
                            return Fetched(index: index, name: source.name, candles: c, quote: nil, error: nil,
                                           latency: Date().timeIntervalSince(start))
                        } catch {
                            return Fetched(index: index, name: source.name, candles: nil, quote: nil,
                                           error: error.localizedDescription, latency: Date().timeIntervalSince(start))
                        }
                    }
                }
                var out: [Fetched] = []
                for await f in group { out.append(f) }
                return out
            }
            fetched += results
        }
        // Quote sources in parallel (independent cross-check of the latest price).
        let quotes = await withTaskGroup(of: Fetched.self) { group in
            for (index, source) in quoteSources {
                group.addTask {
                    let start = Date()
                    do {
                        let q = try await source.quote(for: asset)
                        return Fetched(index: index, name: source.name, candles: nil, quote: q.price, error: nil,
                                       latency: Date().timeIntervalSince(start))
                    } catch {
                        return Fetched(index: index, name: source.name, candles: nil, quote: nil,
                                       error: error.localizedDescription, latency: Date().timeIntervalSince(start))
                    }
                }
            }
            var out: [Fetched] = []
            for await f in group { out.append(f) }
            return out
        }
        let skipped = candleSources[cursor...].map { $0.element.name }

        return try Self.evaluate(asset: asset, timeframe: timeframe, fetched: fetched.sorted { $0.index < $1.index },
                                 quotes: quotes, skipped: skipped,
                                 tolerance: asset.assetClass == .crypto ? cryptoTolerancePercent : stockTolerancePercent,
                                 minimumCandles: minimumCandles, now: now)
    }

    private static func evaluate(asset: Asset, timeframe: Timeframe, fetched: [Fetched], quotes: [Fetched],
                                 skipped: [String], tolerance: Double, minimumCandles: Int, now: Date) throws -> MarketSnapshot {
        let series = fetched.compactMap { f -> (Fetched, [Candle])? in
            guard let c = f.candles else { return nil }
            let clean = c.sanitized()
            return clean.isEmpty ? nil : (f, clean)
        }
        let failed = fetched.filter { $0.candles == nil || $0.candles!.sanitized().isEmpty }.map {
            SourceCheck(name: $0.name, status: .failed($0.error ?? "données vides"), deviationPercent: nil,
                        lastPrice: nil, latency: $0.latency)
        }
        guard !series.isEmpty else { throw ConsensusError.allFailed(failed) }

        // Deviation of each source from the per-timestamp median (last 20 common candles).
        let deviations = Self.deviations(series.map { $0.1 })
        var checks: [SourceCheck] = []
        var primary: (name: String, candles: [Candle])?
        for (i, (f, candles)) in series.enumerated() {
            let dev = deviations[i]
            let agrees = series.count == 1 || (dev.map { $0 <= tolerance } ?? false)
            let rawLast = f.candles?.last(where: \.isValid)?.close
            var status: SourceCheck.Status = agrees ? .agrees : .diverges
            if agrees, primary == nil, candles.count >= minimumCandles {
                primary = (f.name, candles)
                status = .primary
            }
            checks.append(SourceCheck(name: f.name, status: status, deviationPercent: dev, lastPrice: rawLast, latency: f.latency))
        }
        // No agreeing source with enough history: take the longest agreeing one (the engine will reject it if too short).
        // Fewer than half of the responding sources agree: impossible to know which ones are right.
        let agreeingCount = checks.filter { $0.status == .agrees || $0.status == .primary }.count
        let conflict = series.count > 1 && agreeingCount * 2 < series.count
        if primary == nil {
            let fallbackIndex = checks.indices.filter { checks[$0].status == .agrees }
                .max { series[$0].1.count < series[$1].1.count } ?? 0
            primary = (series[fallbackIndex].0.name, series[fallbackIndex].1)
            let c = checks[fallbackIndex]
            checks[fallbackIndex] = SourceCheck(name: c.name, status: .primary, deviationPercent: c.deviationPercent,
                                                lastPrice: c.lastPrice, latency: c.latency)
        }

        // Consensus price: median of latest prices from agreeing sources and quote sources.
        var prices = checks.filter(\.isHealthy).compactMap(\.lastPrice)
        let reference = prices.isEmpty ? nil : DataQuality.median(prices)
        for q in quotes {
            if let p = q.quote, let ref = reference {
                let dev = abs(p / ref - 1) * 100
                // Quote sources are sampled a few seconds apart: double tolerance.
                let ok = dev <= tolerance * 2
                if ok { prices.append(p) }
                checks.append(SourceCheck(name: q.name, status: ok ? .agrees : .diverges, deviationPercent: dev,
                                          lastPrice: p, latency: q.latency))
            } else {
                checks.append(SourceCheck(name: q.name, status: .failed(q.error ?? "indisponible"), deviationPercent: nil,
                                          lastPrice: nil, latency: q.latency))
            }
        }
        checks += failed
        checks += skipped.map { SourceCheck(name: $0, status: .skipped, deviationPercent: nil, lastPrice: nil, latency: nil) }

        // Consensus candles: median of the agreeing sources (no single exchange's wick or bad tick drives the signal).
        let agreeingSeries = series.enumerated().filter { i, f in
            f.0.name != primary!.name && checks[i].status == .agrees
        }.map { $0.element.1 }
        let candles = Self.blend(primary!.candles, agreeingSeries)
        return MarketSnapshot(asset: asset, timeframe: timeframe, candles: candles, primarySource: primary!.name,
                              checks: checks, consensusPrice: prices.isEmpty ? nil : DataQuality.median(prices),
                              quality: DataQuality.assess(candles, timeframe: timeframe, assetClass: asset.assetClass, now: now),
                              conflict: conflict)
    }

    /// Consensus candles: for each candle of the primary source, the median open / high / low / close of every
    /// agreeing source that has that timestamp (the primary's own values when it is alone). The volume stays the
    /// primary's. Same logic as `blend` in web/server/market.ts.
    public static func blend(_ primary: [Candle], _ others: [[Candle]]) -> [Candle] {
        guard !others.isEmpty else { return primary }
        let maps = others.map { Dictionary($0.map { ($0.time, $0) }, uniquingKeysWith: { a, _ in a }) }
        return primary.map { c in
            let same = [c] + maps.compactMap { $0[c.time] }
            guard same.count >= 2 else { return c }
            let open = DataQuality.median(same.map(\.open))
            let close = DataQuality.median(same.map(\.close))
            let high = max(DataQuality.median(same.map(\.high)), open, close)
            let low = min(DataQuality.median(same.map(\.low)), open, close)
            return Candle(time: c.time, open: open, high: high, low: low, close: close, volume: c.volume, isClosed: c.isClosed)
        }
    }

    /// Median deviation (%) of each series from the per-timestamp median, over the (at most 20) most
    /// recent timestamps it shares with another source. With no shared timestamp (misaligned sessions),
    /// the last close is compared with the median of last closes.
    static func deviations(_ series: [[Candle]]) -> [Double?] {
        guard series.count > 1 else { return series.map { _ in 0 } }
        let maps = series.map { Dictionary($0.map { ($0.time, $0.close) }, uniquingKeysWith: { a, _ in a }) }
        let lastCloses = series.map { $0.last?.close ?? 0 }
        let refLast = DataQuality.median(lastCloses)
        return maps.indices.map { i in
            let shared = maps[i].keys.filter { t in maps.indices.contains { $0 != i && maps[$0][t] != nil } }
                .sorted().suffix(20)
            if shared.isEmpty {
                return refLast > 0 ? abs(lastCloses[i] / refLast - 1) * 100 : nil
            }
            let devs = shared.compactMap { t -> Double? in
                let ref = DataQuality.median(maps.compactMap { $0[t] })
                return ref > 0 ? abs(maps[i][t]! / ref - 1) * 100 : nil
            }
            return devs.isEmpty ? nil : DataQuality.median(devs)
        }
    }
}

/// Adjusts a signal to how reliable its data is: never a BUY/SELL on doubtful data.
public enum ReliabilityGate {
    public static func apply(_ signal: Signal, snapshot: MarketSnapshot) -> Signal {
        var action = signal.action
        var warnings = signal.warnings
        switch snapshot.reliability {
        case .low:
            if action != .hold {
                action = .hold
                warnings.insert("Données insuffisamment fiables (\(Int(snapshot.reliabilityScore))/100) : signal suspendu.", at: 0)
            }
        case .medium:
            if action == .strongBuy { action = .buy }
            if action == .strongSell { action = .sell }
            warnings.append("Fiabilité moyenne des données (\(snapshot.summary)).")
        case .high:
            break
        }
        warnings += snapshot.quality.issues
        return Signal(action: action, score: signal.score, confidence: signal.confidence * snapshot.reliabilityScore / 100,
                      price: signal.price, time: signal.time, factors: signal.factors, plan: signal.plan, warnings: warnings)
    }
}

/// Anti-error check before an order: the broker's price must match the market consensus.
public enum PriceGuard {
    public static func issue(brokerPrice: Double, consensusPrice: Double?, tolerancePercent: Double = 1) -> String? {
        guard let consensus = consensusPrice, consensus > 0, brokerPrice > 0 else {
            return "Impossible de vérifier le prix auprès de sources indépendantes."
        }
        let dev = abs(brokerPrice / consensus - 1) * 100
        guard dev > tolerancePercent else { return nil }
        return String(format: "Prix du courtier (%.6g) éloigné de %.2f %% du consensus du marché (%.6g) : ordre bloqué par sécurité.",
                      brokerPrice, dev, consensus)
    }
}
