import Foundation
import LocalAuthentication
import Observation
import AltimKit

/// App state: server access, lock, watch list and holdings (stored on the iPhone only).
@MainActor
@Observable
final class AppModel {
    enum Phase: Equatable { case setup, locked, ready }

    private(set) var phase: Phase = .setup
    private(set) var client: AltimClient? = nil
    private(set) var serverURL: URL? = nil
    private(set) var user: String? = nil

    var acceptedDisclaimer: Bool { didSet { defaults.set(acceptedDisclaimer, forKey: "acceptedDisclaimer") } }
    var faceIDLock: Bool { didSet { defaults.set(faceIDLock, forKey: "faceIDLock") } }
    var watchlist: [Asset] { didSet { LocalStore.save(watchlist, "watchlist") } }
    var holdings: [Holding] { didSet { LocalStore.save(holdings, "holdings") } }
    /// Buy notifications (Réglages) and "strong only" (signal and zone together).
    var alertsEnabled: Bool { didSet { defaults.set(alertsEnabled, forKey: "alertsEnabled") } }
    var alertsStrongOnly: Bool { didSet { defaults.set(alertsStrongOnly, forKey: "alertsStrongOnly") } }
    /// Live Activity (lock screen + Dynamic Island) offered on the asset pages.
    var liveActivityEnabled: Bool { didSet { defaults.set(liveActivityEnabled, forKey: "liveActivityEnabled") } }
    /// Last alerts computed (Watch, Réglages) and when.
    var lastAlerts: [BuyAlert] = []
    var lastAlertCheck: Date? = nil
    /// Price alerts chosen by the user ("sous 80 000 $") and the journal of the notifications received.
    var priceTargets: [PriceTarget] { didSet { LocalStore.save(priceTargets, "priceTargets") } }
    var journal: [JournalEntry] { didSet { LocalStore.save(journal, "journal") } }
    /// Asset to open, from a tapped notification.
    var pendingOpen: Asset? = nil
    /// Asset of the running Live Activity (its live price is followed to update it).
    var activityAsset: Asset? = nil
    /// Budget in dollars for the Sélection tab (0 = not set).
    var budget: Double { didSet { defaults.set(budget, forKey: "budget") } }
    var selectionMarket: Kind { didSet { defaults.set(selectionMarket.rawValue, forKey: "selectionMarket") } }
    var selectionHorizon: Horizon { didSet { defaults.set(selectionHorizon.rawValue, forKey: "selectionHorizon") } }

    /// Server unreachable: date of the saved answers shown instead (nil when online).
    var offlineSince: Date? = nil

    let live = LivePrices()
    @ObservationIgnored private let responseCache = FileResponseCache(
        directory: FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("api", isDirectory: true)
    )
    /// Asset open on screen (its live price is followed too).
    var focus: Asset? = nil
    /// Picks shown in the Sélection tab (live prices too).
    var selectionAssets: [Asset] = []
    @ObservationIgnored private let defaults = UserDefaults.standard
    @ObservationIgnored private var lastBackground: Date? = nil

    init() {
        acceptedDisclaimer = defaults.bool(forKey: "acceptedDisclaimer")
        faceIDLock = defaults.object(forKey: "faceIDLock") as? Bool ?? true
        // Former versions kept the lists in UserDefaults (backed up): moved once to the protected local store.
        watchlist = LocalStore.load([Asset].self, "watchlist") ?? Self.migrate([Asset].self, "watchlist") ?? Asset.defaults
        holdings = LocalStore.load([Holding].self, "holdings") ?? Self.migrate([Holding].self, "holdings") ?? []
        alertsEnabled = defaults.bool(forKey: "alertsEnabled")
        alertsStrongOnly = defaults.bool(forKey: "alertsStrongOnly")
        liveActivityEnabled = defaults.object(forKey: "liveActivityEnabled") as? Bool ?? true
        lastAlerts = LocalStore.load([BuyAlert].self, "lastAlerts") ?? []
        priceTargets = LocalStore.load([PriceTarget].self, "priceTargets") ?? []
        journal = LocalStore.load([JournalEntry].self, "journal") ?? []
        lastAlertCheck = defaults.object(forKey: "lastAlertCheck") as? Date
        budget = defaults.double(forKey: "budget")
        selectionMarket = Kind(rawValue: defaults.string(forKey: "selectionMarket") ?? "") ?? .stock
        selectionHorizon = Horizon(rawValue: defaults.string(forKey: "selectionHorizon") ?? "") ?? .mo1
        restore()
    }

    // MARK: Access

    /// Reopens the saved access (server + Keychain) without asking anything, apart from Face ID.
    private func restore() {
        guard let s = defaults.string(forKey: "server"), let url = AltimClient.normalize(s) else { return }
        let creds = KeychainStore.get(.credentials).flatMap { try? JSONDecoder().decode(Credentials.self, from: $0) }
        let cookie = KeychainStore.string(.session)
        let open = defaults.bool(forKey: "openServer")
        // Launched in the background while the iPhone is locked: the password is unreadable, the session is enough.
        guard creds != nil || cookie != nil || open else { return }
        serverURL = url
        user = creds?.user ?? defaults.string(forKey: "user")
        // Empty password = 2FA account: no silent re-login (a wrong attempt would count towards the lockout).
        let usable = creds.flatMap { $0.password.isEmpty ? nil : $0 }
        client = makeClient(url, credentials: usable, cookie: cookie)
        phase = faceIDLock ? .locked : .ready
    }

    /// Client with the offline cache: the last good answers are shown, dated, when the network or the server fails.
    private func makeClient(_ url: URL, credentials: Credentials?, cookie: String?) -> AltimClient {
        AltimClient(baseURL: url, credentials: credentials, sessionCookie: cookie, cache: responseCache) { [weak self] date in
            Task { @MainActor in self?.offlineSince = date }
        }
    }

    /// Checks the server and returns what it asks for (user/password, 2FA code, or nothing for a local server).
    func probe(_ server: String) async throws -> (URL, AccessMode) {
        guard let url = AltimClient.normalize(server) else { throw AltimError.invalidServer }
        let mode = try await AltimClient(baseURL: url, credentials: nil).accessMode()
        return (url, mode)
    }

    func connect(url: URL, mode: AccessMode, user: String, password: String, code: String) async throws {
        var creds: Credentials?
        if case .login = mode {
            let c = Credentials(user: user.trimmingCharacters(in: .whitespaces), password: password)
            let c1 = AltimClient(baseURL: url, credentials: nil)
            try await c1.login(c, code: code.trimmingCharacters(in: .whitespaces))
            creds = c
            KeychainStore.setString(c1.sessionCookie, for: .session)
            // With 2FA, a silent re-login is impossible (the code changes): the password is kept only without 2FA.
            if case .login(needsCode: false) = mode {
                KeychainStore.set(try JSONEncoder().encode(c), for: .credentials)
            } else {
                KeychainStore.set(try JSONEncoder().encode(Credentials(user: c.user, password: "")), for: .credentials)
            }
            client = makeClient(url, credentials: mode == .login(needsCode: false) ? c : nil, cookie: c1.sessionCookie)
        } else {
            client = makeClient(url, credentials: nil, cookie: nil)
        }
        defaults.set(url.absoluteString, forKey: "server")
        defaults.set(mode == .open, forKey: "openServer")
        defaults.set(creds?.user, forKey: "user")
        serverURL = url
        self.user = creds?.user
        phase = .ready
    }

    func logout() async {
        live.stop()
        LocalStore.remove("alertTracker")
        LocalStore.remove("lastAlerts")
        lastAlerts = []
        await client?.logout()
        KeychainStore.clear()
        responseCache.clear()
        offlineSince = nil
        defaults.removeObject(forKey: "openServer")
        client = nil
        user = nil
        phase = .setup
    }

    /// Called after each API call: keeps the (possibly renewed) session cookie for the next launch.
    func persistSession() {
        guard let cookie = client?.sessionCookie, cookie != KeychainStore.string(.session) else { return }
        KeychainStore.setString(cookie, for: .session)
    }

    /// The session expired and could not be renewed (2FA on, or password changed): back to the login screen.
    func sessionLost() {
        live.stop()
        // The dead cookie is dropped: the next launch asks to log in instead of failing silently again.
        KeychainStore.setString(nil, for: .session)
        client = nil
        phase = .setup
    }

    // MARK: Lock (Face ID / code of the iPhone)

    func unlock() async {
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) else {
            // Opens without asking only when the iPhone has no passcode at all (nothing to check against); any other
            // unavailability (Face ID locked out, busy…) keeps the app locked.
            if (error as? LAError)?.code == .passcodeNotSet { phase = .ready }
            return
        }
        if (try? await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: "Déverrouiller Altim")) == true {
            phase = .ready
        }
    }

    /// Locks again after 2 minutes in the background.
    func didEnterBackground() {
        lastBackground = Date()
        live.stop()
    }

    func willEnterForeground() {
        defer { lastBackground = nil }
        guard phase == .ready, faceIDLock, let t = lastBackground, Date().timeIntervalSince(t) > 120 else { return }
        phase = .locked
    }

    // MARK: Lists

    func watch(_ a: Asset) {
        guard !watchlist.contains(where: { $0.id == a.id }) else { return }
        watchlist.append(a)
    }

    func unwatch(_ a: Asset) { watchlist.removeAll { $0.id == a.id } }
    func isWatched(_ a: Asset) -> Bool { watchlist.contains { $0.id == a.id } }

    private static func migrate<T: Codable>(_ type: T.Type, _ key: String) -> T? {
        guard let value = UserDefaults.standard.data(forKey: key).flatMap({ try? JSONDecoder().decode(T.self, from: $0) }) else { return nil }
        LocalStore.save(value, key)
        UserDefaults.standard.removeObject(forKey: key)
        return value
    }

    // MARK: Buy alerts

    /// One check of the buy alerts (background refresh, app opening): the server's rule applied to the watch list and
    /// the holdings; returns the alerts to notify (new or changed situation only), nil without access.
    /// The background check runs for the buy alerts or for at least one armed price alert.
    var needsChecks: Bool { alertsEnabled || priceTargets.contains { $0.triggered == nil } }

    func addTarget(_ t: PriceTarget) {
        priceTargets.append(t)
        BuyNotifications.schedule(enabled: needsChecks)
    }

    func removeTarget(_ id: UUID) {
        priceTargets.removeAll { $0.id == id }
        BuyNotifications.schedule(enabled: needsChecks)
    }

    func rearmTarget(_ id: UUID) {
        if let i = priceTargets.firstIndex(where: { $0.id == id }) { priceTargets[i].triggered = nil }
        BuyNotifications.schedule(enabled: needsChecks)
    }

    /// What one check found: new buy alerts and price alerts just reached (with the price).
    struct CheckResult {
        var buy: [BuyAlert]
        var targets: [(PriceTarget, Double)]
    }

    func checkAlerts() async throws -> CheckResult? {
        guard let client else { return nil }
        let now = Date()
        // Price alerts: consensus quotes of the assets that still have an armed threshold.
        var reached: [(PriceTarget, Double)] = []
        let armed = priceTargets.filter { $0.triggered == nil }
        if !armed.isEmpty {
            let unique = Array(Dictionary(armed.map { ($0.asset.id, $0.asset) }, uniquingKeysWith: { a, _ in a }).values)
            let quotes = try await client.quotes(unique)
            let result = PriceTarget.evaluate(priceTargets, prices: Dictionary(quotes.map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a }), now: now)
            reached = result.fired
            if !reached.isEmpty { priceTargets = result.targets }
        }
        let fresh = try await checkBuyAlerts(client)
        let entries = (alertsEnabled ? fresh : []).compactMap { a in
            a.price.map { JournalEntry(asset: a.asset, source: a.strong ? .strongBuy : .buy, title: a.title, price: $0, date: now) }
        } + reached.map { t, p in JournalEntry(asset: t.asset, source: .target, title: "\(t.asset.symbol) : \(t.label.lowercased())", price: p, date: now) }
        if !entries.isEmpty { journal = AlertJournal.add(entries, to: journal) }
        return CheckResult(buy: fresh, targets: reached)
    }

    private func checkBuyAlerts(_ client: AltimClient) async throws -> [BuyAlert] {
        // The asset followed in the Live Activity is checked too, even if it is neither on the radar nor held.
        let all = watchlist + holdings.map(\.asset) + [activityAsset].compactMap { $0 }
        let assets = Array(Dictionary(all.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }).values)
        let items = try await client.alerts(assets)
        var tracker = LocalStore.load(AlertTracker.self, "alertTracker") ?? AlertTracker()
        let fresh = tracker.newAlerts(items, onlyStrong: alertsStrongOnly)
        LocalStore.save(tracker, "alertTracker")
        lastAlerts = items.sorted { ($0.buy ? 0 : 1, $0.symbol) < ($1.buy ? 0 : 1, $1.symbol) }
        LocalStore.save(lastAlerts, "lastAlerts")
        lastAlertCheck = Date()
        defaults.set(lastAlertCheck, forKey: "lastAlertCheck")
        persistSession()
        return fresh
    }

    func clearJournal() { journal = [] }

    /// From a tapped notification: "crypto:BTC" → the asset to open.
    func open(assetID: String) {
        let parts = assetID.split(separator: ":").map(String.init)
        guard parts.count == 2, let kind = Kind(rawValue: parts[0]) else { return }
        pendingOpen = (watchlist + holdings.map(\.asset)).first { $0.id == assetID } ?? Asset(symbol: parts[1], kind: kind, name: parts[1])
    }
}
