import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Radar verdict of an asset pushed by the live socket when it changes (`{"type":"verdict",…}`).
public struct VerdictPush: Codable, Sendable, Equatable {
    public var symbol: String
    public var kind: Kind
    /// "buy" | "buyZone" | "wait" | "noPosition" | "trim" | "sell" (the decision's verdict).
    public var verdict: String
    public var label: String?
    public var rating: String?
    public var ratingLabel: String?
    public var chipNote: String?
    public var asOf: Double?
    public var key: String { "\(kind.rawValue):\(symbol)" }
}

/// One message of the real-time WebSocket `/api/ws` (protocol v1, see the server's README section « Temps réel »).
public enum LiveMessage: Sendable, Equatable {
    case hello(version: Int)
    case tick(LiveTick)
    /// The `/api/alerts` items of the subscription (sent after `subscribe`, then only when one changed).
    case alerts([BuyAlert], checkedAt: Double)
    /// The alerts were checked again and did not change.
    case checked(Double)
    case verdict(VerdictPush)
    case fx(rate: Double, asOf: Double?)
    case pong
    case error(code: String, message: String)
    /// A type this version does not know (a newer server): ignored.
    case other

    public static let protocolVersion = 1
    /// Application ping from the client: the server answers, so a silent socket is a dead one.
    public static let pingInterval: TimeInterval = 20
    /// Without any message for this long, the socket is closed and reopened (and the badge stops saying « en direct »).
    public static let silence: TimeInterval = 45
    /// Openings that fail in a row before falling back to the SSE stream (`/api/live`).
    public static let fallbackAfter = 2
    /// Same cap as the server.
    public static let maxAssets = 20

    /// Decodes one text frame; nil when it is not a JSON object with a `type`.
    public static func decode(_ data: Data) -> LiveMessage? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any], let type = obj["type"] as? String else { return nil }
        let number = { (k: String) -> Double? in (obj[k] as? NSNumber)?.doubleValue }
        let dec = JSONDecoder()
        switch type {
        case "hello": return .hello(version: (obj["v"] as? NSNumber)?.intValue ?? 0)
        case "tick": return (try? dec.decode(LiveTick.self, from: data)).map { .tick($0) }
        case "alerts":
            struct Body: Decodable { var items: [BuyAlert] }
            guard let body = try? dec.decode(Body.self, from: data) else { return nil }
            return .alerts(body.items, checkedAt: number("checkedAt") ?? 0)
        case "checked": return number("checkedAt").map { .checked($0) }
        case "verdict": return (try? dec.decode(VerdictPush.self, from: data)).map { .verdict($0) }
        case "fx":
            guard let rate = number("rate"), rate.isFinite, rate > 0 else { return nil }
            return .fx(rate: rate, asOf: number("asOf"))
        case "pong": return .pong
        case "error": return .error(code: obj["code"] as? String ?? "", message: obj["message"] as? String ?? "")
        default: return .other
        }
    }

    /// The `subscribe` message (replaces the previous subscription), the first `maxAssets` assets.
    public static func subscribe(_ assets: [Asset], interval: String = "4h", usd: Bool) -> String {
        let list = assets.prefix(maxAssets).map { ["kind": $0.kind.rawValue, "symbol": $0.symbol] }
        let body: [String: Any] = ["type": "subscribe", "assets": list, "interval": interval, "currency": usd ? "USD" : "EUR"]
        let data = (try? JSONSerialization.data(withJSONObject: body, options: [.sortedKeys])) ?? Data()
        return String(data: data, encoding: .utf8) ?? ""
    }

    /// Pause before reconnection `attempt` (0-based): 1 s, 2 s, 4 s, 8 s, 16 s, then 30 s.
    public static func backoff(_ attempt: Int) -> TimeInterval { min(30, pow(2, Double(min(attempt, 5)))) }

    /// « En direct » only while messages arrive.
    public static func isLive(lastMessage: Date?, now: Date) -> Bool {
        lastMessage.map { now.timeIntervalSince($0) < silence } ?? false
    }
}

extension LiveTick: Equatable {
    public static func == (a: LiveTick, b: LiveTick) -> Bool {
        a.symbol == b.symbol && a.kind == b.kind && a.price == b.price && a.change == b.change && a.time == b.time && a.market == b.market
    }
}

extension AltimClient {
    /// `wss://host/api/ws` (`ws://` for a plain-http development server), with the session cookie. No `Origin`:
    /// a native app has none, the server accepts it.
    public func liveSocketRequest() -> URLRequest {
        var r = request("/api/ws")
        var c = URLComponents(url: r.url!, resolvingAgainstBaseURL: false)!
        c.scheme = baseURL.scheme == "http" ? "ws" : "wss"
        r.url = c.url
        r.timeoutInterval = 30
        return withSession(r)
    }

    /// Everything that changes while the app is open, on one WebSocket: prices, alerts, verdicts, EUR/USD. Ends on a
    /// network error, a silent socket or an expired session (`.unauthorized`); the caller reconnects. `opened` is told
    /// when the server greeted the socket (an opening that never gets there counts as a failed one). Cancelling the
    /// consuming task closes the socket.
    public func liveMessages(_ assets: [Asset], interval: String = "4h", usd: Bool,
                             opened: @escaping @Sendable () -> Void = {}) -> AsyncThrowingStream<LiveMessage, Error> {
        AsyncThrowingStream { continuation in
            let config = URLSessionConfiguration.default
            config.httpShouldSetCookies = false
            config.httpCookieAcceptPolicy = .never
            config.urlCache = nil
            let session = URLSession(configuration: config)
            let task = session.webSocketTask(with: liveSocketRequest())
            task.maximumMessageSize = 4 * 1024 * 1024
            let subscribe = LiveMessage.subscribe(assets, interval: interval, usd: usd)
            let clock = LastMessage()
            let receiver = Task {
                do {
                    try await task.send(.string(subscribe))
                    while !Task.isCancelled {
                        let frame = try await task.receive()
                        let data: Data
                        switch frame {
                        case .string(let s): data = Data(s.utf8)
                        case .data(let d): data = d
                        @unknown default: continue
                        }
                        clock.touch()
                        guard let m = LiveMessage.decode(data) else { continue }
                        if case .hello = m { opened() }
                        continuation.yield(m)
                    }
                } catch {
                    let status = (task.response as? HTTPURLResponse)?.statusCode
                    continuation.finish(throwing: status == 401 ? AltimError.unauthorized : AltimError.network(error.localizedDescription))
                }
            }
            // Application ping every 20 s (answered by the server), and the silence check.
            let pinger = Task {
                while !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: UInt64(LiveMessage.pingInterval * 1e9))
                    if Task.isCancelled { return }
                    if !LiveMessage.isLive(lastMessage: clock.value, now: Date()) {
                        continuation.finish(throwing: AltimError.network("connexion en direct silencieuse"))
                        return
                    }
                    try? await task.send(.string(#"{"type":"ping"}"#))
                }
            }
            continuation.onTermination = { _ in
                receiver.cancel()
                pinger.cancel()
                task.cancel(with: .goingAway, reason: nil)
                session.invalidateAndCancel()
            }
            clock.touch()
            task.resume()
        }
    }
}

/// When the socket last said something (written by the receiving task, read by the pinger).
final class LastMessage: @unchecked Sendable {
    private let lock = NSLock()
    private var at: Date?
    func touch() { lock.lock(); at = Date(); lock.unlock() }
    var value: Date? { lock.lock(); defer { lock.unlock() }; return at }
}
