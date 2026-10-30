import Foundation
import Observation
import AltimKit

/// Live prices of the displayed assets, read by the iPhone itself: the consensus quotes of the market sources, again
/// every few seconds (AltimKit LocalLive: 5 s for cryptos, 15 s for stocks, 60 s when the stock market is closed).
/// Restarted with a growing pause (1 s → 30 s) when the network drops; stopped when the app leaves the foreground
/// (RootView stops it) and started again when it comes back.
@MainActor
@Observable
final class LivePrices {
    private(set) var ticks: [String: LiveTick] = [:]
    private(set) var connected = false
    /// Time of the last tick received.
    private(set) var lastTick: Date? = nil
    /// Time of the last message of any kind (the badge says « en direct » only while messages keep coming).
    private(set) var lastMessage: Date? = nil

    @ObservationIgnored private var tasks: [Task<Void, Never>] = []
    @ObservationIgnored private var keys: [String] = []

    func price(_ a: Asset) -> LiveTick? { ticks[a.id] }

    /// Follows these assets (restarts only when the list or the currency of the texts changes).
    func follow(_ assets: [Asset], client: AltimClient?, usd: Bool = Money.displayCurrency == .usd) {
        let wanted = Array(Set(assets.map(\.id))).sorted() + [usd ? "USD" : "EUR"]
        guard let client, wanted.count > 1 else { return stop() }
        if wanted == keys, !tasks.isEmpty { return }
        stop()
        keys = wanted
        let unique = Dictionary(assets.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }).values.sorted { $0.id < $1.id }
        let chunks = stride(from: 0, to: unique.count, by: LiveMessage.maxAssets).map { Array(unique[$0..<min($0 + LiveMessage.maxAssets, unique.count)]) }
        tasks = chunks.map { chunk in
            Task { [weak self] in
                var attempt = 0
                while !Task.isCancelled {
                    do {
                        for try await m in client.liveMessages(chunk, usd: usd) {
                            guard let self else { return }
                            self.handle(m)
                            attempt = 0
                        }
                    } catch {}
                    self?.connected = false
                    if Task.isCancelled { return }
                    try? await Task.sleep(nanoseconds: UInt64(LiveMessage.backoff(attempt) * 1e9))
                    attempt += 1
                }
            }
        }
    }

    private func handle(_ m: LiveMessage) {
        lastMessage = Date()
        connected = true
        if case .tick(let t) = m { buffer(t) }
    }

    /// Cryptos tick several times per second: the screen is refreshed at most every 0.3 s.
    @ObservationIgnored private var pending: [String: LiveTick] = [:]
    @ObservationIgnored private var flush: Task<Void, Never>? = nil

    private func buffer(_ tick: LiveTick) {
        pending[tick.key] = tick
        guard flush == nil else { return }
        flush = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard let self else { return }
            self.flush = nil
            guard !Task.isCancelled, !self.pending.isEmpty else { return }
            self.ticks.merge(self.pending) { _, new in new }
            for tick in self.pending.values { LiveActivities.shared.update(tick: tick) }
            self.pending.removeAll()
            self.lastTick = Date()
            self.connected = true
        }
    }

    func stop() {
        flush?.cancel()
        flush = nil
        pending.removeAll()
        tasks.forEach { $0.cancel() }
        tasks = []
        keys = []
        connected = false
    }
}
