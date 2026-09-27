import Foundation
import Observation
import AltimKit

/// Live prices of the displayed assets (Server-Sent Events of /api/live), reconnected automatically.
@MainActor
@Observable
final class LivePrices {
    private(set) var ticks: [String: LiveTick] = [:]
    private(set) var connected = false
    /// Time of the last tick received (the badge says "en direct" only while ticks keep coming).
    private(set) var lastTick: Date? = nil

    @ObservationIgnored private var task: Task<Void, Never>? = nil
    @ObservationIgnored private var keys: [String] = []

    func price(_ a: Asset) -> LiveTick? { ticks[a.id] }

    /// Follows these assets (restarts the stream only when the list changes).
    func follow(_ assets: [Asset], client: AltimClient?, onExpired: @escaping @MainActor () -> Void) {
        let wanted = Array(Set(assets.map(\.id))).sorted()
        guard let client, !wanted.isEmpty else { return stop() }
        if wanted == keys, task != nil { return }
        stop()
        keys = wanted
        let unique = Dictionary(assets.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }).values.map { $0 }
        task = Task { [weak self] in
            var delay: UInt64 = 1
            while !Task.isCancelled {
                do {
                    for try await tick in client.liveTicks(unique) {
                        guard let self else { return }
                        self.buffer(tick)
                        delay = 1
                    }
                } catch AltimError.unauthorized {
                    if await client.renewSession() == false {
                        onExpired()
                        return
                    }
                    continue
                } catch {}
                self?.connected = false
                if Task.isCancelled { return }
                // 1 s, 2 s, 4 s … up to 30 s between attempts (network lost, server restart).
                try? await Task.sleep(nanoseconds: delay * 1_000_000_000)
                delay = min(30, delay * 2)
            }
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
            self.pending.removeAll()
            self.lastTick = Date()
            self.connected = true
        }
    }

    func stop() {
        flush?.cancel()
        flush = nil
        pending.removeAll()
        task?.cancel()
        task = nil
        keys = []
        connected = false
    }
}
