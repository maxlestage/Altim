import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public enum AltimError: LocalizedError, Equatable {
    case invalidServer
    case notAltim
    case notConfigured
    case wrongCredentials
    case locked
    case refused
    case unauthorized
    case server(String)
    case network(String)

    public var errorDescription: String? {
        switch self {
        case .invalidServer: return "Adresse du serveur invalide (exemple : https://mon-app.herokuapp.com)."
        case .notAltim: return "Ce serveur ne répond pas comme Altim. Vérifiez l'adresse."
        case .notConfigured: return "Accès privé non configuré sur le serveur : ajoutez les variables ALTIM_* dans Heroku → Settings → Config Vars."
        case .wrongCredentials: return "Identifiant, mot de passe ou code incorrect."
        case .locked: return "Trop d'essais. Réessayez dans 15 minutes."
        case .refused: return "Connexion refusée par le serveur."
        case .unauthorized: return "Session expirée : reconnectez-vous."
        case let .server(m): return m
        case let .network(m): return "Réseau indisponible (\(m))."
        }
    }
}

public struct Credentials: Codable, Sendable, Equatable {
    public var user: String
    public var password: String
    public init(user: String, password: String) {
        self.user = user
        self.password = password
    }
}

/// What the server asks for before giving access.
public enum AccessMode: Sendable, Equatable {
    /// Private access (Heroku with ALTIM_* set): user + password, and a 6-digit code if 2FA is on.
    case login(needsCode: Bool)
    /// Local development server without private access.
    case open
}

/// Session cookie of the server, shared between concurrent requests.
final class CookieJar: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: String?
    init(_ value: String?) { stored = value }
    var value: String? {
        get { lock.lock(); defer { lock.unlock() }; return stored }
        set { lock.lock(); stored = newValue; lock.unlock() }
    }
}

/// Never follows a redirect: at login it lets us read the session cookie; everywhere else it guarantees the cookie
/// (sent by hand on each request) never reaches another host, even if something in front of the server redirects.
final class NoRedirect: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

/// HTTP client of the Altim server. The session is the same cookie as the website (HttpOnly, 7 days),
/// read from the login answer and sent by hand (no shared cookie jar); when it expires, the client logs
/// in again with the saved credentials (Keychain) and replays the request once.
public final class AltimClient: Sendable {
    public let baseURL: URL
    public let session: URLSession
    private let credentials: Credentials?
    private let jar: CookieJar
    static let cookieName = "altim_session"

    /// Last good answers, served when the network or the server fails (offline mode); nil = no cache.
    private let cache: ResponseCache?
    /// Told after each call: nil when the server answered, the date of the data when a cached answer was served.
    private let onStatus: (@Sendable (Date?) -> Void)?

    public init(baseURL: URL, credentials: Credentials?, sessionCookie: String? = nil, session: URLSession = AltimClient.makeSession(),
                cache: ResponseCache? = nil, onStatus: (@Sendable (Date?) -> Void)? = nil) {
        self.baseURL = baseURL
        self.credentials = credentials
        self.session = session
        self.cache = cache
        self.onStatus = onStatus
        jar = CookieJar(sessionCookie)
    }

    /// Paths whose last answer is kept for the offline mode (not the search nor the login).
    static let cacheable: Set<String> = ["/api/radar", "/api/tickers", "/api/candles", "/api/guard", "/api/zones", "/api/macro", "/api/alerts", "/api/news", "/api/selection", "/api/history", "/api/brief", "/api/decision", "/api/calendar", "/api/strategies", "/api/why", "/api/opportunities", "/api/anomalies", "/api/sectors", "/api/validation"]
    /// Pauses before the 2nd and 3rd attempt of a read that failed on the network or a temporary server error.
    nonisolated(unsafe) static var retryDelays: [Double] = [0.5, 1.5]

    private let renewal = Renewal()

    /// Current session cookie value (the app keeps it in the Keychain to avoid a login at each launch).
    public var sessionCookie: String? { jar.value }

    public static func makeSession(configuration: URLSessionConfiguration = .default) -> URLSession {
        let c = configuration
        c.httpCookieStorage = nil
        c.httpShouldSetCookies = false
        c.httpCookieAcceptPolicy = .never
        c.urlCache = nil
        c.requestCachePolicy = .reloadIgnoringLocalCacheData
        c.timeoutIntervalForRequest = 45
        return URLSession(configuration: c, delegate: NoRedirect(), delegateQueue: nil)
    }

    /// "mon-app.herokuapp.com" → https://mon-app.herokuapp.com (http only for a local server).
    public static func normalize(_ text: String) -> URL? {
        var t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        while t.hasSuffix("/") { t.removeLast() }
        if t.isEmpty { return nil }
        if !t.contains("://") { t = "https://" + t }
        guard let url = URL(string: t), let scheme = url.scheme?.lowercased(), let host = url.host, !host.isEmpty else { return nil }
        // Plain http only for this device; ".local" names (anyone on the Wi-Fi can answer them) only in development.
        var local = host == "localhost" || host == "127.0.0.1"
        #if DEBUG
        local = local || host.hasSuffix(".local")
        #endif
        guard scheme == "https" || (scheme == "http" && local) else { return nil }
        if !url.path.isEmpty && url.path != "/" { return nil }
        return url
    }

    // MARK: Access

    /// Checks the server and tells whether it needs a login (and a 2FA code).
    public func accessMode() async throws -> AccessMode {
        let (data, response) = try await send(request("/login"))
        let html = String(decoding: data, as: UTF8.self)
        guard response.statusCode == 200 || response.statusCode == 302, html.contains("Altim") else { throw AltimError.notAltim }
        if html.contains("Accès privé non configuré") {
            // Production refuses everything; a local development server is open.
            let (_, probe) = try await send(request("/api/search", query: ["q": "btc", "limit": "1"]))
            if probe.statusCode == 200 { return .open }
            throw AltimError.notConfigured
        }
        return .login(needsCode: html.contains("name=\"code\""))
    }

    /// Same form as the website (the server checks the Origin against CSRF). Success = redirect to /health
    /// with the session cookie.
    public func login(_ c: Credentials, code: String = "") async throws {
        var r = request("/login")
        r.httpMethod = "POST"
        r.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        r.setValue(origin, forHTTPHeaderField: "Origin")
        r.httpBody = Self.form(["user": c.user, "password": c.password, "code": code, "next": "/health"]).data(using: .utf8)
        let config = URLSessionConfiguration.ephemeral
        config.httpShouldSetCookies = false
        config.httpCookieAcceptPolicy = .never
        config.timeoutIntervalForRequest = 45
        let once = URLSession(configuration: config, delegate: NoRedirect(), delegateQueue: nil)
        defer { once.finishTasksAndInvalidate() }
        let (_, response) = try await send(r, with: once)
        switch response.statusCode {
        case 302, 303:
            guard response.value(forHTTPHeaderField: "Location")?.hasSuffix("/health") == true,
                  let cookie = Self.sessionCookie(in: response.value(forHTTPHeaderField: "Set-Cookie") ?? "")
            else { throw AltimError.refused }
            jar.value = cookie
        case 401: throw AltimError.wrongCredentials
        case 429: throw AltimError.locked
        case 503: throw AltimError.notConfigured
        case 403: throw AltimError.refused
        default: throw AltimError.server("Connexion impossible (code \(response.statusCode)).")
        }
    }

    public func logout() async {
        var r = request("/logout")
        r.httpMethod = "POST"
        r.setValue(origin, forHTTPHeaderField: "Origin")
        _ = try? await send(r)
        jar.value = nil
    }

    static func sessionCookie(in header: String) -> String? {
        guard let start = header.range(of: "\(cookieName)=") else { return nil }
        let value = header[start.upperBound...].prefix { $0 != ";" && $0 != "," && $0 != " " }
        return value.isEmpty ? nil : String(value)
    }

    // MARK: API

    public func radar(_ assets: [Asset], interval: String = "4h") async throws -> [RadarRow] {
        try await batched(assets) { try await self.get("/api/radar", ["symbols": Self.list($0), "interval": interval]) }
    }

    public func quotes(_ assets: [Asset]) async throws -> [Quote] {
        try await batched(assets) { try await self.get("/api/tickers", ["symbols": Self.list($0)]) }
    }

    public func search(_ q: String, limit: Int = 20) async throws -> [SearchItem] {
        try await get("/api/search", ["q": String(q.prefix(30)), "limit": String(limit)])
    }

    public func candles(_ a: Asset, interval: String) async throws -> Snapshot {
        try await get("/api/candles", ["symbol": a.symbol, "kind": a.kind.rawValue, "interval": interval])
    }

    public func guardReport(_ a: Asset) async throws -> GuardReport {
        try await get("/api/guard", ["symbol": a.symbol, "kind": a.kind.rawValue])
    }

    public func zones(_ a: Asset) async throws -> ZonesReport {
        try await get("/api/zones", ["symbol": a.symbol, "kind": a.kind.rawValue])
    }

    public func macro() async throws -> MacroInfo { try await get("/api/macro", [:]) }

    func getNews(_ assets: [Asset]) async throws -> NewsReport {
        try await get("/api/news", assets.isEmpty ? [:] : ["symbols": Self.list(assets)])
    }

    func getBrief(_ assets: [Asset]) async throws -> Brief {
        try await get("/api/brief", ["symbols": Self.list(assets)])
    }

    func getHistory(_ assets: [Asset], days: Int) async throws -> HistoryResponse {
        try await get("/api/history", ["days": String(days)].merging(assets.isEmpty ? [:] : ["symbols": Self.list(assets)]) { a, _ in a })
    }

    func getAlerts(_ assets: [Asset]) async throws -> [BuyAlert] {
        try await get("/api/alerts", ["symbols": Self.list(assets)])
    }

    func getDecision(_ query: [String: String]) async throws -> Decision {
        try await get("/api/decision", query)
    }

    func getCalendar(_ query: [String: String]) async throws -> CalendarReport {
        try await get("/api/calendar", query)
    }

    func getStrategies(_ query: [String: String]) async throws -> StrategiesReport {
        try await get("/api/strategies", query)
    }

    func getWhy(_ query: [String: String]) async throws -> WhyReport {
        try await get("/api/why", query)
    }

    func getAnomalies(_ query: [String: String]) async throws -> AnomalyReport {
        try await get("/api/anomalies", query)
    }

    func getSectors(_ query: [String: String]) async throws -> SectorsReport {
        try await get("/api/sectors", query)
    }

    /// A read whose status matters (202 "pending" of a long scan).
    func getWithStatus(_ path: String, _ query: [String: String]) async throws -> (Data, Int) {
        try await authorized(request(path, query: query))
    }

    func decodeResponse<T: Decodable>(_ type: T.Type, _ data: Data, _ status: Int) throws -> T {
        try decode(type, data, status)
    }

    /// POST of a JSON body (same session, same Origin as the website); never retried nor served from the cache.
    func postJSON<T: Decodable>(_ path: String, _ body: Data) async throws -> T {
        var r = request(path)
        r.httpMethod = "POST"
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        r.setValue(origin, forHTTPHeaderField: "Origin")
        r.httpBody = body
        r.timeoutInterval = 45
        let (data, status) = try await authorized(r)
        return try decode(T.self, data, status)
    }

    public func selection(_ h: Horizon, kind: Kind) async throws -> SelectionResult {
        let (data, status) = try await authorized(request("/api/selection", query: ["horizon": h.rawValue, "kind": kind.rawValue]))
        if status == 202 { return .pending }
        return .ready(try decode(SelectionReport.self, data, status))
    }

    /// Request for the live price stream (Server-Sent Events), opened by the app with the same session.
    public func liveRequest(_ assets: [Asset]) -> URLRequest {
        var r = withSession(request("/api/live", query: ["symbols": Self.list(assets)]))
        r.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        r.timeoutInterval = 3600
        return r
    }

    /// Logs in again after a 401 (session expired) and tells whether it worked. Parallel callers share one login,
    /// and a failed attempt is not retried for a minute (a changed password must not burn the server's lock-out quota).
    public func renewSession() async -> Bool {
        guard let credentials else { return false }
        return await renewal.run { (try? await self.login(credentials)) != nil }
    }

    // MARK: Plumbing

    private var origin: String {
        var c = URLComponents()
        c.scheme = baseURL.scheme
        c.host = baseURL.host
        c.port = baseURL.port
        return c.string ?? baseURL.absoluteString
    }

    static func list(_ assets: [Asset]) -> String { assets.map { "\($0.symbol):\($0.kind.rawValue)" }.joined(separator: ",") }

    static func form(_ fields: [String: String]) -> String {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return fields.sorted { $0.key < $1.key }
            .map { "\($0.key)=\($0.value.addingPercentEncoding(withAllowedCharacters: allowed) ?? "")" }
            .joined(separator: "&")
    }

    func request(_ path: String, query: [String: String] = [:]) -> URLRequest {
        var c = URLComponents(url: baseURL, resolvingAgainstBaseURL: false)!
        c.path = path
        if !query.isEmpty { c.queryItems = query.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) } }
        // ":" and "," stay readable; "+" must be escaped (URLComponents leaves it, servers read it as a space).
        c.percentEncodedQuery = c.percentEncodedQuery?.replacingOccurrences(of: "+", with: "%2B")
        var r = URLRequest(url: c.url!)
        r.setValue("AltimiOS/1.0", forHTTPHeaderField: "User-Agent")
        return r
    }

    private func withSession(_ r: URLRequest) -> URLRequest {
        var r = r
        if let cookie = jar.value { r.setValue("\(Self.cookieName)=\(cookie)", forHTTPHeaderField: "Cookie") }
        return r
    }

    private func send(_ r: URLRequest, with s: URLSession? = nil) async throws -> (Data, HTTPURLResponse) {
        do {
            let (data, response) = try await (s ?? session).data(for: withSession(r))
            guard let http = response as? HTTPURLResponse else { throw AltimError.notAltim }
            return (data, http)
        } catch let e as AltimError {
            throw e
        } catch let e as URLError {
            throw AltimError.network(e.localizedDescription)
        }
    }

    /// A read (GET) is tried 3 times when the network drops or the server answers 502 / 503 / 504 (restart, overload):
    /// a short outage goes unnoticed. Never for the login (a failed attempt counts towards the lock-out).
    private func sendRetrying(_ r: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let idempotent = (r.httpMethod ?? "GET") == "GET"
        var attempt = 0
        while true {
            do {
                let (data, response) = try await send(r)
                if idempotent, [502, 503, 504].contains(response.statusCode), attempt < Self.retryDelays.count {
                    try await Task.sleep(nanoseconds: UInt64(Self.retryDelays[attempt] * 1e9))
                    attempt += 1
                    continue
                }
                return (data, response)
            } catch AltimError.network(let m) {
                guard idempotent, attempt < Self.retryDelays.count, !Task.isCancelled else { throw AltimError.network(m) }
                try await Task.sleep(nanoseconds: UInt64(Self.retryDelays[attempt] * 1e9))
                attempt += 1
            }
        }
    }

    private func authorized(_ r: URLRequest) async throws -> (Data, Int) {
        let key = r.url.map { "\($0.path)?\($0.query ?? "")" } ?? ""
        let cacheable = cache != nil && Self.cacheable.contains(r.url?.path ?? "")
        do {
            var (data, response) = try await sendRetrying(r)
            if response.statusCode == 401 {
                jar.value = nil
                guard await renewSession() else { throw AltimError.unauthorized }
                (data, response) = try await sendRetrying(r)
                if response.statusCode == 401 { throw AltimError.unauthorized }
            }
            if response.statusCode >= 500, cacheable, let saved = cache?.load(key) {
                onStatus?(saved.date)
                return (saved.data, 200)
            }
            if response.statusCode == 200 {
                if cacheable { cache?.save(key, data) }
                onStatus?(nil)
            }
            return (data, response.statusCode)
        } catch AltimError.network(let m) {
            // Offline: the last good answer, with its date for the banner.
            if cacheable, !Task.isCancelled, let saved = cache?.load(key) {
                onStatus?(saved.date)
                return (saved.data, 200)
            }
            throw AltimError.network(m)
        }
    }

    private func get<T: Decodable>(_ path: String, _ query: [String: String]) async throws -> T {
        let (data, status) = try await authorized(request(path, query: query))
        return try decode(T.self, data, status)
    }

    private func decode<T: Decodable>(_ type: T.Type, _ data: Data, _ status: Int) throws -> T {
        guard (200..<300).contains(status) else {
            let message = (try? JSONDecoder().decode([String: String].self, from: data))?["error"]
            throw AltimError.server(message ?? "Erreur \(status)")
        }
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw AltimError.server("Réponse illisible du serveur (\(error.localizedDescription)).")
        }
    }

    /// The server takes 20 assets per request: bigger lists are split into parallel batches.
    private func batched<T: Sendable>(_ assets: [Asset], _ call: @escaping @Sendable ([Asset]) async throws -> [T]) async throws -> [T] {
        let chunks = stride(from: 0, to: assets.count, by: 20).map { Array(assets[$0..<min($0 + 20, assets.count)]) }
        return try await withThrowingTaskGroup(of: (Int, [T]).self) { group in
            for (i, chunk) in chunks.enumerated() { group.addTask { (i, try await call(chunk)) } }
            var parts: [(Int, [T])] = []
            for try await p in group { parts.append(p) }
            return parts.sorted { $0.0 < $1.0 }.flatMap { $0.1 }
        }
    }
}

/// One re-login at a time, and not more than once a minute after a failure.
final class Renewal: @unchecked Sendable {
    private let lock = NSLock()
    private var running: Task<Bool, Never>?
    private var failedAt: Date?

    func run(_ attempt: @escaping @Sendable () async -> Bool) async -> Bool {
        lock.lock()
        if let failedAt, Date().timeIntervalSince(failedAt) < 60 {
            lock.unlock()
            return false
        }
        let task: Task<Bool, Never>
        if let running {
            task = running
        } else {
            task = Task { await attempt() }
            running = task
        }
        lock.unlock()
        let ok = await task.value
        lock.lock()
        if running == task { running = nil }
        failedAt = ok ? nil : Date()
        lock.unlock()
        return ok
    }
}
