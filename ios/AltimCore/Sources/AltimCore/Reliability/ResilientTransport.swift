import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Transport HTTP fiabilisé :
/// - nouvelles tentatives avec attente exponentielle (lectures GET uniquement, jamais les ordres) ;
/// - respect de `Retry-After` sur les erreurs 429 / 418 (limitation de débit) ;
/// - disjoncteur par hôte : après plusieurs échecs, l'hôte est mis en pause pour laisser les autres sources prendre le relais.
public actor ResilientTransport: HTTPTransport {
    public struct Policy: Sendable {
        public var maxAttempts = 3
        public var baseDelay: TimeInterval = 0.4
        public var maxDelay: TimeInterval = 4
        public var failureThreshold = 3
        public var cooldown: TimeInterval = 60
        public init() {}
    }

    private struct HostState { var failures = 0; var openUntil: Date? }

    let inner: HTTPTransport
    let policy: Policy
    private let sleep: @Sendable (TimeInterval) async -> Void
    private let now: @Sendable () -> Date
    private var hosts: [String: HostState] = [:]

    public init(inner: HTTPTransport = URLSessionTransport(), policy: Policy = Policy(),
                sleep: @escaping @Sendable (TimeInterval) async -> Void = { try? await Task.sleep(nanoseconds: UInt64($0 * 1_000_000_000)) },
                now: @escaping @Sendable () -> Date = { Date() }) {
        self.inner = inner
        self.policy = policy
        self.sleep = sleep
        self.now = now
    }

    public enum TransportError: Error, LocalizedError, Equatable {
        case circuitOpen(host: String)

        public var errorDescription: String? {
            switch self {
            case let .circuitOpen(host): return "Source \(host) temporairement désactivée après plusieurs échecs."
            }
        }
    }

    public func isAvailable(host: String) -> Bool {
        guard let until = hosts[host]?.openUntil else { return true }
        return until <= now()
    }

    public nonisolated func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        try await perform(request)
    }

    private func perform(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let host = request.url?.host ?? "?"
        if let until = hosts[host]?.openUntil, until > now() {
            throw TransportError.circuitOpen(host: host)
        }
        let idempotent = (request.httpMethod ?? "GET") == "GET"
        let attempts = idempotent ? policy.maxAttempts : 1
        var lastError: Error = APIError.invalidResponse

        for attempt in 0..<attempts {
            do {
                let (data, response) = try await inner.send(request)
                let status = response.statusCode
                let retryable = status == 429 || status == 418 || (500...599).contains(status)
                if retryable && attempt < attempts - 1 {
                    let retryAfter = response.value(forHTTPHeaderField: "Retry-After").flatMap(TimeInterval.init)
                    await sleep(min(retryAfter ?? backoff(attempt), policy.maxDelay))
                    continue
                }
                if retryable { recordFailure(host) } else { recordSuccess(host) }
                return (data, response)
            } catch {
                lastError = error
                if error is CancellationError { throw error }
                if attempt < attempts - 1 { await sleep(backoff(attempt)) }
            }
        }
        recordFailure(host)
        throw lastError
    }

    private func backoff(_ attempt: Int) -> TimeInterval {
        min(policy.baseDelay * pow(2, Double(attempt)), policy.maxDelay)
    }

    private func recordFailure(_ host: String) {
        var state = hosts[host] ?? HostState()
        state.failures += 1
        if state.failures >= policy.failureThreshold {
            state.openUntil = now().addingTimeInterval(policy.cooldown)
            state.failures = 0
        }
        hosts[host] = state
    }

    private func recordSuccess(_ host: String) {
        hosts[host] = HostState()
    }
}
