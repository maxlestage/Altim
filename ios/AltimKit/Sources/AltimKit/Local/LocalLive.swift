import Foundation

/// Live prices without a server: the consensus quotes read again every few seconds by the iPhone (5 s for cryptos,
/// 15 s for stocks, 60 s for a closed stock market), as ticks of the same stream as the server's WebSocket.
enum LocalLive {
    static func messages(_ assets: [Asset], local: LocalBackend, opened: @escaping @Sendable () -> Void = {}) -> AsyncThrowingStream<LiveMessage, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                continuation.yield(.hello(version: 1))
                opened()
                var last: [String: Double] = [:]
                var failures = 0
                while !Task.isCancelled {
                    let now = Date()
                    let due = assets.filter { a in
                        let every: Double = a.kind == .crypto ? 5 : marketOpen(now) ? 15 : 60
                        return (last[a.id].map { now.timeIntervalSince1970 - $0 >= every } ?? true)
                    }
                    if !due.isEmpty {
                        let quotes = await local.quotes(due)
                        failures = quotes.isEmpty ? failures + 1 : 0
                        // Several rounds without a single price: the network is gone, the caller reconnects later.
                        if failures >= 3 { return continuation.finish(throwing: MarketError("aucun cours")) }
                        for a in due { last[a.id] = now.timeIntervalSince1970 }
                        for q in quotes {
                            continuation.yield(.tick(LiveTick(symbol: q.symbol, kind: q.kind, price: q.price, change: q.change, agreeing: q.agreeing,
                                                              total: q.total, sources: q.sources.filter(\.ok).map(\.name),
                                                              time: now.timeIntervalSince1970 * 1000,
                                                              market: q.kind == .stock ? (marketOpen(now) ? "open" : "closed") : nil)))
                        }
                    }
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// New York regular session (Monday to Friday, 9:30 to 16:00; holidays not known).
    static func marketOpen(_ date: Date) -> Bool {
        var cal = Calendar(identifier: .gregorian)
        guard let tz = TimeZone(identifier: "America/New_York") else { return false }
        cal.timeZone = tz
        let c = cal.dateComponents([.weekday, .hour, .minute], from: date)
        guard let wd = c.weekday, (2...6).contains(wd), let h = c.hour, let m = c.minute else { return false }
        let minutes = h * 60 + m
        return minutes >= 570 && minutes < 960
    }
}
