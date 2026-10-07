import Foundation
import Observation
import AltimKit

/// Live data of the displayed assets on the WebSocket `/api/ws`: prices, the « can I buy now? » alerts, the Radar
/// verdicts and the EUR/USD rate, pushed by the server. Reconnected with a growing pause (1 s → 30 s), closed when
/// the app leaves the foreground (RootView stops it) and reopened when it comes back. After two failed openings in a
/// row (a network that blocks WebSockets), the prices come from the Server-Sent Events of `/api/live` as before.
@MainActor
@Observable
final class LivePrices {
    private(set) var ticks: [String: LiveTick] = [:]
    private(set) var connected = false
    /// Time of the last tick received.
    private(set) var lastTick: Date? = nil
    /// Time of the last message of any kind (the badge says « en direct » only while messages keep coming).
    private(set) var lastMessage: Date? = nil

    /// Pushed alerts of the followed assets (a part of them: one socket per 20 assets), with the server's check time.
    @ObservationIgnored var onAlerts: (@MainActor ([BuyAlert], Date) -> Void)?
    @ObservationIgnored var onVerdict: (@MainActor (VerdictPush) -> Void)?
    @ObservationIgnored var onFx: (@MainActor (Double) -> Void)?

    @ObservationIgnored private var tasks: [Task<Void, Never>] = []
    @ObservationIgnored private var keys: [String] = []

    func price(_ a: Asset) -> LiveTick? { ticks[a.id] }

    /// Follows these assets (restarts only when the list or the currency of the texts changes).
    func follow(_ assets: [Asset], client: AltimClient?, usd: Bool = Money.displayCurrency == .usd,
                onRenewed: @escaping @MainActor () -> Void = {}, onExpired: @escaping @MainActor () -> Void) {
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
                var failures = 0
                var sse = false
                while !Task.isCancelled {
                    let opened = OpenFlag()
                    do {
                        if sse {
                            for try await tick in client.liveTicks(chunk) {
                                guard let self else { return }
                                self.buffer(tick)
                                self.lastMessage = Date()
                                attempt = 0
                            }
                        } else {
                            for try await m in client.liveMessages(chunk, usd: usd, opened: { opened.set() }) {
                                guard let self else { return }
                                self.handle(m)
                                attempt = 0
                            }
                        }
                    } catch AltimError.unauthorized {
                        if await client.renewSession() == false {
                            onExpired()
                            return
                        }
                        onRenewed()
                        opened.set()
                    } catch {}
                    if !sse {
                        failures = opened.value ? 0 : failures + 1
                        if failures >= LiveMessage.fallbackAfter { sse = true }
                    }
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
        switch m {
        case .tick(let t): buffer(t)
        case .alerts(let items, let at): onAlerts?(items, at > 0 ? Date(timeIntervalSince1970: at / 1000) : Date())
        case .verdict(let v): onVerdict?(v)
        case .fx(let rate, _): onFx?(rate)
        case .hello, .checked, .pong, .error, .other: break
        }
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

/// The socket reached the server's `hello` (an opening that never does counts as failed).
final class OpenFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var done = false
    func set() { lock.lock(); done = true; lock.unlock() }
    var value: Bool { lock.lock(); defer { lock.unlock() }; return done }
}
