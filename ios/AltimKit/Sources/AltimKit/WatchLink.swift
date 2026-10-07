import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// What the iPhone gives the Apple Watch so it can ask the server itself: the server's address and the session
/// cookie (never the password). watchOS does not allow a WebSocket for an ordinary app (TN3135), so the Watch reads
/// `/api/alerts` and the quotes over HTTPS every `WatchLink.refreshInterval` while its app is on screen.
public struct WatchLink: Codable, Sendable, Equatable {
    public var server: String
    public var session: String

    public init(server: String, session: String) {
        self.server = server
        self.session = session
    }

    public static let refreshInterval: TimeInterval = 30
    /// One reading (alerts + quotes) is given up after this long.
    public static let timeout: TimeInterval = 15
    public static let notLinked = "Ouvrez Altim sur l'iPhone une fois pour connecter la montre."

    /// A client of the server with this session (no credentials: an expired session is never renewed on the Watch).
    public func client(session urlSession: URLSession? = nil) -> AltimClient? {
        guard let url = AltimClient.normalize(server), !session.isEmpty else { return nil }
        return AltimClient(baseURL: url, credentials: nil, sessionCookie: session, session: urlSession ?? WatchLink.makeSession())
    }

    public static func makeSession() -> URLSession {
        let c = URLSessionConfiguration.ephemeral
        c.httpCookieStorage = nil
        c.httpShouldSetCookies = false
        c.httpCookieAcceptPolicy = .never
        c.urlCache = nil
        c.timeoutIntervalForRequest = timeout
        c.timeoutIntervalForResource = timeout
        return URLSession(configuration: c)
    }

    /// The French message of a failed reading.
    public static func message(for error: Error) -> String {
        switch error as? AltimError {
        case .unauthorized?, .notConfigured?, .refused?: return notLinked
        case .network?: return "Réseau indisponible : nouvel essai dans 30 s."
        case .server(let m)?: return m
        default: return "Mise à jour impossible : nouvel essai dans 30 s."
        }
    }

    /// "Vérifié il y a 12 s", "il y a 3 min", "il y a 2 h", "il y a 2 j".
    public static func checkedText(_ checked: Date?, now: Date) -> String {
        guard let checked else { return "Jamais vérifié" }
        let s = max(0, Int(now.timeIntervalSince(checked)))
        let ago: String
        switch s {
        case ..<60: ago = "\(s) s"
        case ..<3600: ago = "\(s / 60) min"
        case ..<86_400: ago = "\(s / 3600) h"
        default: ago = "\(s / 86_400) j"
        }
        return "Vérifié il y a \(ago)"
    }

    /// The alerts' prices replaced by the fresher quotes (same consensus as the iPhone), the buyable ones first.
    public static func merge(_ alerts: [BuyAlert], quotes: [String: Double]) -> [BuyAlert] {
        alerts.map { a in
            var a = a
            if let p = quotes[a.id], p.isFinite, p > 0 { a.price = p }
            return a
        }
        .sorted { ($0.buy ? 0 : 1, $0.strong ? 0 : 1, $0.symbol) < ($1.buy ? 0 : 1, $1.strong ? 0 : 1, $1.symbol) }
    }
}
