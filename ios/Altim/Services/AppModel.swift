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
    /// Lines as saved: average price and stop in the currency they were typed in (`costCurrency`, `stopCurrency`;
    /// absent = dollars). The engines and the server get dollars: `usdHoldings`.
    var holdings: [Holding] { didSet { LocalStore.save(holdings, "holdings") } }
    /// Display currency of every amount (Réglages): euros by default, dollars on request.
    var currency: Currency {
        didSet {
            defaults.set(currency.rawValue, forKey: "displayCurrency")
            applyMoney()
            if currency == .eur && fx == nil { Task { await refreshFx() } }
        }
    }
    /// Last valid EUR/USD rate (/api/fx, kept 7 days on the iPhone), nil when unknown: amounts then stay in dollars.
    private(set) var fx: FxRate?
    /// Why the last read of the rate failed (shown in Réglages), nil when it worked.
    private(set) var fxError: String?
    /// Currency actually shown when the screens were last built: they are rebuilt when it changes (MainTabs).
    private(set) var moneyStamp: Currency = .usd
    @ObservationIgnored private var fxTask: Task<Void, Never>?
    /// Buy notifications (Réglages) and "strong only" (signal and zone together).
    var alertsEnabled: Bool { didSet { defaults.set(alertsEnabled, forKey: "alertsEnabled") } }
    var alertsStrongOnly: Bool { didSet { defaults.set(alertsStrongOnly, forKey: "alertsStrongOnly") } }
    /// News alerts (Réglages): serious escalation told by 2 sources, or a story about one of my assets told by 3.
    var newsAlertsEnabled: Bool { didSet { defaults.set(newsAlertsEnabled, forKey: "newsAlertsEnabled") } }
    /// Notifications (Réglages) of the configuration changes of the radar and of the positions that became dangerous,
    /// found by the background check.
    var configAlertsEnabled: Bool { didSet { defaults.set(configAlertsEnabled, forKey: "configAlertsEnabled") } }
    var dangerAlertsEnabled: Bool { didSet { defaults.set(dangerAlertsEnabled, forKey: "dangerAlertsEnabled") } }
    /// What these notifications already told: no second notification of the same change or danger.
    var changeNotices: ChangeNoticeState { didSet { LocalStore.save(changeNotices, "changeNotices") } }
    /// A news notification was tapped: the Actu tab opens.
    var pendingNews = false
    /// Live Activity (lock screen + Dynamic Island) offered on the asset pages.
    var liveActivityEnabled: Bool { didSet { defaults.set(liveActivityEnabled, forKey: "liveActivityEnabled") } }
    /// Last alerts computed (Watch, Réglages) and when.
    var lastAlerts: [BuyAlert] = []
    var lastAlertCheck: Date? = nil
    /// Price alerts chosen by the user ("sous 80 000 €", in the currency they were typed in) and the journal of the
    /// notifications received (prices in dollars).
    var priceTargets: [PriceTarget] { didSet { LocalStore.save(priceTargets, "priceTargets") } }
    var journal: [JournalEntry] { didSet { LocalStore.save(journal, "journal") } }
    /// Simulation (paper trading): a virtual portfolio stored on the iPhone only. No real money, no order ever placed.
    var paper: PaperState { didSet { LocalStore.save(paper, "paper") } }
    /// Automatic trading journal: simulated purchases, real purchases and sales, reviewed later (iPhone only).
    var tradeJournal: TradeJournalState { didSet { saveTradeJournal() } }
    /// The saved trading journal could not be read: ignored, kept aside at the next write.
    var tradeJournalError: String?
    /// Decisions received during this session: the one "of the moment" for a journal entry.
    @ObservationIgnored private var recentDecisions: [String: (decision: Decision, at: Date)] = [:]
    /// Positions just closed by the automatic check (stop / target), shown once in the Simulation view.
    var paperJustClosed: [PaperTrade] = []
    @ObservationIgnored private var checkingPaper = false
    /// Asset to open, from a tapped notification.
    var pendingOpen: Asset? = nil
    /// Asset of the running Live Activity (its live price is followed to update it).
    var activityAsset: Asset? = nil
    /// Budget for the Sélection tab (0 = not set), in `budgetCurrency` (the currency it was typed in; older versions:
    /// dollars).
    var budget: Double { didSet { defaults.set(budget, forKey: "budget") } }
    var budgetCurrency: Currency { didSet { defaults.set(budgetCurrency.rawValue, forKey: "budgetCurrency") } }
    var selectionMarket: Kind { didSet { defaults.set(selectionMarket.rawValue, forKey: "selectionMarket") } }
    var selectionHorizon: Horizon { didSet { defaults.set(selectionHorizon.rawValue, forKey: "selectionHorizon") } }
    /// Risk limits (Réglages → Prudence des conseils), checked on Mes avoirs.
    var risk: RiskSettings { didSet { defaults.set(try? JSONEncoder().encode(risk), forKey: "riskSettings") } }
    /// Weights of the decision's composite score (Réglages), sent as `w=` only when not the defaults.
    var scoreWeights: ScoreWeights { didSet { defaults.set(try? JSONEncoder().encode(scoreWeights), forKey: "scoreWeights") } }
    /// Last decision seen per asset and the configuration changes (Radar), stored on the iPhone only.
    var configChanges: ConfigState { didSet { LocalStore.save(configChanges, "configChanges") } }
    /// The last decision seen for each asset (its page or the Radar's re-reading): the Radar's verdict.
    var decisionDigests: [String: DecisionDigest] { didSet { LocalStore.save(decisionDigests, "decisionDigests") } }
    /// Positions that became dangerous, last measured on Mes avoirs (shown again on the Radar with their time).
    var dangers: DangerState? { didSet { LocalStore.save(dangers, "dangers") } }

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
        // A stop that is not a positive number is dropped (the line itself stays), like on the web.
        holdings = (LocalStore.load([Holding].self, "holdings") ?? Self.migrate([Holding].self, "holdings") ?? []).map(\.cleaned)
        alertsEnabled = defaults.bool(forKey: "alertsEnabled")
        alertsStrongOnly = defaults.bool(forKey: "alertsStrongOnly")
        newsAlertsEnabled = defaults.bool(forKey: "newsAlertsEnabled")
        // On by default only when the buy notifications were already allowed; the first value is kept.
        let buyAlerts = defaults.bool(forKey: "alertsEnabled")
        for key in ["configAlertsEnabled", "dangerAlertsEnabled"] where defaults.object(forKey: key) == nil { defaults.set(buyAlerts, forKey: key) }
        configAlertsEnabled = defaults.bool(forKey: "configAlertsEnabled")
        dangerAlertsEnabled = defaults.bool(forKey: "dangerAlertsEnabled")
        changeNotices = LocalStore.load(ChangeNoticeState.self, "changeNotices") ?? ChangeNoticeState()
        liveActivityEnabled = defaults.object(forKey: "liveActivityEnabled") as? Bool ?? true
        lastAlerts = LocalStore.load([BuyAlert].self, "lastAlerts") ?? []
        priceTargets = LocalStore.load([PriceTarget].self, "priceTargets") ?? []
        journal = LocalStore.load([JournalEntry].self, "journal") ?? []
        let savedCurrency = Currency(rawValue: defaults.string(forKey: "displayCurrency") ?? "") ?? .eur
        let savedFx = Fx.saved(LocalStore.data("fx"))
        // A saved simulation is used only when well-formed (the file can be edited by hand), like on the web. A new
        // one starts with 10 000 in the display currency, kept in dollars like the prices.
        let startCapital = savedCurrency == .eur ? savedFx.map { Paper.defaultCapital / $0.rate } ?? Paper.defaultCapital : Paper.defaultCapital
        paper = LocalStore.load(PaperState.self, "paper").flatMap { Paper.isValid($0) ? $0 : nil }
            ?? Paper.new(capital: startCapital, now: Date().timeIntervalSince1970 * 1000)
        // An unreadable journal is ignored with a message, never a crash (kept aside at the next write).
        let savedJournal = TradeJournal.parseSaved(LocalStore.data("tradeJournal"))
        tradeJournal = savedJournal.state
        tradeJournalError = savedJournal.error
        lastAlertCheck = defaults.object(forKey: "lastAlertCheck") as? Date
        let savedBudget = StoredAmount.read(amount: defaults.double(forKey: "budget"), currency: defaults.string(forKey: "budgetCurrency"))
        budget = savedBudget?.amount ?? 0
        budgetCurrency = savedBudget?.currency ?? .usd
        currency = savedCurrency
        fx = savedFx
        selectionMarket = Kind(rawValue: defaults.string(forKey: "selectionMarket") ?? "") ?? .stock
        selectionHorizon = Horizon(rawValue: defaults.string(forKey: "selectionHorizon") ?? "") ?? .mo1
        risk = defaults.data(forKey: "riskSettings").flatMap { try? JSONDecoder().decode(RiskSettings.self, from: $0) } ?? .defaults
        scoreWeights = defaults.data(forKey: "scoreWeights").flatMap { try? JSONDecoder().decode(ScoreWeights.self, from: $0) } ?? .defaults
        // Damaged or older entries are dropped on reading (ConfigState validates each one).
        configChanges = LocalStore.load(ConfigState.self, "configChanges") ?? ConfigState()
        decisionDigests = LocalStore.load([String: DecisionDigest].self, "decisionDigests") ?? [:]
        dangers = LocalStore.load(DangerState.self, "dangers")
        applyMoney()
        moneyStamp = Money.displayCurrency
        restore()
    }

    // MARK: Display currency

    /// Every formatter (AltimKit's Money) reads the currency and the rate set here.
    private func applyMoney() {
        Money.setDisplay(currency, fx)
        WatchBridge.shared.sendDisplay(currency: currency, fx: fx)
    }

    /// Rebuilds the screens when the currency actually shown changed (called when the rate arrives or is lost, and
    /// when the Radar comes back from Réglages, so that changing the setting does not close Réglages).
    func syncMoneyStamp() {
        if moneyStamp != Money.displayCurrency { moneyStamp = Money.displayCurrency }
    }

    /// Holdings in dollars (engines, server, decision), at the current rate; `unconverted`: euro costs without a rate.
    var usdHoldings: UsdHoldings {
        _ = fx
        _ = currency
        return UsdHoldings(holdings)
    }

    /// Budget of the Sélection tab in dollars (0 when not set or not convertible).
    var budgetUsd: Double {
        _ = fx
        let v = StoredAmount.read(amount: budget, currency: budgetCurrency.rawValue)?.usd ?? 0
        return v.isFinite ? v : 0
    }

    /// Reads the EUR/USD rate now, then every 10 minutes while the app runs (a failed read keeps the last rate).
    func startFx() {
        guard fxTask == nil else { return }
        fxTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refreshFx()
                try? await Task.sleep(nanoseconds: UInt64(Fx.refreshInterval * 1e9))
            }
        }
    }

    /// Background checks: the rate read again when older than 10 minutes.
    func refreshFxIfOld() async {
        if let f = fx, !f.stale, Date().timeIntervalSince1970 * 1000 - f.fetchedAt < Fx.refreshInterval * 1000 { return }
        await refreshFx()
    }

    func refreshFx() async {
        guard let client else { return }
        do {
            let r = try await client.fx()
            if let rate = r.rate {
                fx = rate
                fxError = nil
                LocalStore.write(r.data, "fx")
            } else {
                // No source answered: the last rate stays while younger than 7 days.
                fxError = r.response.error ?? "taux indisponible"
                if let f = fx, Date().timeIntervalSince1970 * 1000 - f.fetchedAt >= 7 * 86_400_000 { fx = nil }
            }
        } catch AltimError.unauthorized {
            fxError = "taux indisponible (session expirée)"
        } catch {
            fxError = "taux indisponible (\(error.localizedDescription))"
        }
        applyMoney()
        syncMoneyStamp()
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
        LocalStore.remove("newsTracker")
        lastAlerts = []
        // Decisions of this server: another server would show them as its own.
        decisionDigests = [:]
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
    var needsChecks: Bool {
        alertsEnabled || newsAlertsEnabled || priceTargets.contains { $0.triggered == nil }
            || configAlertsEnabled && !watchlist.isEmpty || dangerAlertsEnabled && !holdings.isEmpty
    }

    func addTarget(_ t: PriceTarget) {
        priceTargets.append(t)
        BuyNotifications.schedule(enabled: needsChecks)
    }

    func removeTarget(_ id: UUID) {
        priceTargets.removeAll { $0.id == id }
        BuyNotifications.schedule(enabled: needsChecks)
    }

    /// Re-arms an alert (a move alert starts again from the current price).
    func rearmTarget(_ id: UUID, current: Double? = nil) {
        if let i = priceTargets.firstIndex(where: { $0.id == id }) { priceTargets[i] = priceTargets[i].rearmed(at: current) }
        BuyNotifications.schedule(enabled: needsChecks)
    }

    /// What one check found: new buy alerts and price alerts just reached (with the price).
    struct CheckResult {
        var buy: [BuyAlert]
        var targets: [(PriceTarget, Double)]
        var news: [NewsItem] = []
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
        let news = newsAlertsEnabled ? try await checkNews(client, now: now) : []
        return CheckResult(buy: fresh, targets: reached, news: news)
    }

    /// News to notify (NewsAlertTracker): a feed that fails does not stop the buy and price alerts.
    private func checkNews(_ client: AltimClient, now: Date) async throws -> [NewsItem] {
        var seen = Set<String>()
        let mine = (watchlist + holdings.map(\.asset)).filter { seen.insert($0.id).inserted }
        let report: NewsReport
        do {
            report = try await client.news(mine)
        } catch AltimError.unauthorized {
            throw AltimError.unauthorized
        } catch {
            return []
        }
        var tracker = LocalStore.load(NewsAlertTracker.self, "newsTracker") ?? NewsAlertTracker()
        let fresh = tracker.newAlerts(report.items, owned: Set(mine.map(\.id)), now: now)
        LocalStore.save(tracker, "newsTracker")
        return fresh
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

    // MARK: Configuration changes

    /// Every decision received goes through the configuration diff (verdict or level changed since the last one seen).
    /// An answer more than an hour old (served from the offline cache) is not compared: it would tell a false change.
    @discardableResult
    func recordDecision(_ d: Decision, personal: Bool, now: Date = Date()) -> ConfigTransition? {
        let ms = now.timeIntervalSince1970 * 1000
        recentDecisions["\(d.kind.rawValue):\(d.symbol)"] = (d, now)
        let digests = DecisionDigests.record(decisionDigests, d, personal: personal, now: ms)
        if digests != decisionDigests { decisionDigests = digests }
        guard ms - d.asOf < 3_600_000 else { return nil }
        let r = ConfigChanges.apply(configChanges, d, personal: personal, now: ms)
        configChanges = r.state
        return r.transition
    }

    func clearTransitions() { configChanges.transitions = [] }

    /// When the informational decision of this asset was last compared (nil: never).
    func lastDecisionCheck(_ a: Asset) -> Date? {
        configChanges.last[ConfigChanges.key(kind: a.kind, symbol: a.symbol, personal: false)].map { Date(timeIntervalSince1970: $0.at / 1000) }
    }

    // MARK: Background notifications of the configuration changes and dangerous positions

    /// Background re-reading of the radar decisions, with the Radar's rule: the first 20 watched assets, 2 at a time,
    /// those not compared in the last 15 minutes (here, on the Radar or on their page), the oldest first, until
    /// `deadline`. Each one goes through the same configuration diff; returns the changes found. Stops at an expired
    /// session; a failed asset is compared next time.
    func checkConfigChanges(until deadline: Date) async -> [ConfigTransition] {
        guard let client else { return [] }
        let now = Date()
        let due = watchlist.prefix(20)
            .map { (asset: $0, last: lastDecisionCheck($0)) }
            .filter { x in x.last.map { now.timeIntervalSince($0) >= RadarView.decisionEvery } ?? true }
            .sorted { ($0.last ?? .distantPast) < ($1.last ?? .distantPast) }
            .map { $0.asset }
        var found: [ConfigTransition] = []
        var i = 0
        while i < due.count && !Task.isCancelled && Date() < deadline {
            let pair = Array(due[i..<min(i + 2, due.count)])
            let got = await withTaskGroup(of: (decision: Decision?, unauthorized: Bool).self) { group in
                for a in pair {
                    group.addTask {
                        do {
                            return (try await client.decision(asset: a), false)
                        } catch AltimError.unauthorized {
                            return (nil, true)
                        } catch {
                            return (nil, false)
                        }
                    }
                }
                var out: [(decision: Decision?, unauthorized: Bool)] = []
                for await r in group { out.append(r) }
                return out
            }
            guard !Task.isCancelled else { break }
            for r in got {
                if let d = r.decision, let t = recordDecision(d, personal: false) { found.append(t) }
            }
            if got.contains(where: { $0.unauthorized }) { break }
            i += 2
        }
        persistSession()
        return found
    }

    /// Dangerous positions measured in the background like on Mes avoirs: quotes of the held assets and daily candles
    /// of the lines that have a stop (the ATR of "near the stop"), 2 at a time, 20 assets at most. nil when the quotes
    /// cannot be read or are the saved answers of the offline mode (an old price would tell a false danger).
    /// `nearStopUnknown`: ids of the lines with a stop whose candles could not be read.
    func measureDangers() async -> (dangers: [Danger], nearStopUnknown: Set<String>)? {
        guard let client, !holdings.isEmpty else { return nil }
        let holdings = usdHoldings.holdings
        let assets = Array(Dictionary(holdings.map { ($0.asset.id, $0.asset) }, uniquingKeysWith: { a, _ in a }).values)
        guard let q = try? await client.quotes(assets), !q.isEmpty else { return nil }
        // The offline status is told through the main actor just before: let it land first.
        await Task.yield()
        guard offlineSince == nil else { return nil }
        let prices = Dictionary(q.map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a })
        var seen = Set<String>()
        let withStop = Array(holdings.filter { $0.stop != nil }.map(\.asset).filter { seen.insert($0.id).inserted }.prefix(20))
        var daily: [String: [Candle]] = [:]
        var i = 0
        while i < withStop.count && !Task.isCancelled {
            let pair = Array(withStop[i..<min(i + 2, withStop.count)])
            let got = await withTaskGroup(of: (String, [Candle]?).self) { group in
                for a in pair { group.addTask { (a.id, try? await client.candles(a, interval: "1d").candles) } }
                var out: [(String, [Candle]?)] = []
                for await r in group { out.append(r) }
                return out
            }
            for (id, c) in got { if let c, !c.isEmpty { daily[id] = c } }
            i += 2
        }
        await Task.yield()
        // Candles served from the offline cache are old: the near-stop state is then unknown rather than wrong.
        if offlineSince != nil { daily = [:] }
        persistSession()
        let p = RiskPortfolio(holdings: holdings, prices: prices, daily: daily)
        let unknown = Set(holdings.filter { $0.stop != nil && daily[$0.asset.id] == nil }.map(\.id.uuidString))
        return (RiskEngine.dangerousPositions(p, risk, daily: daily), unknown)
    }

    // MARK: Trading journal

    /// The decision seen on this iPhone for this asset, when recent enough to be "the one of the moment".
    func recentDecision(_ a: Asset, now: Date = Date()) -> Decision? {
        guard let c = recentDecisions[a.id] else { return nil }
        let max = TradeJournal.decisionMaxAge
        return now.timeIntervalSince(c.at) * 1000 <= max && now.timeIntervalSince1970 * 1000 - c.decision.asOf <= max ? c.decision : nil
    }

    /// Writes an entry and completes it in the background with the market data of that moment (macro stress, ATR,
    /// relative volume, and the decision when none was loaded yet); a failed fetch only leaves the field unknown.
    @discardableResult
    func recordTrade(source: TradeJournalSource, side: TradeSide, asset: Asset, price: Double, quantity: Double?, amount: Double? = nil,
                     stop: Double? = nil, targets: [Double?] = [], note: String = "", refId: String? = nil, decision: Decision? = nil) -> String? {
        guard price.isFinite, price > 0 else { return nil }
        let now = nowMs
        let e = TradeJournal.create(.init(id: UUID().uuidString, now: now, source: source, side: side, symbol: asset.symbol, kind: asset.kind,
                                          name: asset.name, price: price, quantity: quantity, amount: amount ?? quantity.map { $0 * price },
                                          stop: stop, targets: targets, note: note, refId: refId, decision: decision ?? recentDecision(asset)))
        tradeJournal = TradeJournal.add(tradeJournal, e)
        Task { await enrichTrade(id: e.id, asset: asset, at: e.createdAt, hasDecision: e.decision != nil) }
        return e.id
    }

    private func enrichTrade(id: String, asset: Asset, at: Double, hasDecision: Bool) async {
        guard let client else { return }
        let macro = try? await client.macro()
        let candles = try? await client.candles(asset, interval: "1d")
        var decision: Decision?
        if !hasDecision { decision = try? await client.decision(asset: asset) }
        guard let entry = tradeJournal.entries.first(where: { $0.id == id }) else { return }
        var market = JournalMarket()
        if let macro {
            market.macroScore = macro.score
            market.macroLevel = macro.level
        }
        if let c = candles?.candles, !c.isEmpty {
            let m = TradeJournal.market(from: c, at: at)
            market.atrPct = m.atrPct
            if entry.market.relativeVolume == nil { market.relativeVolume = m.relativeVolume }
        }
        if let decision { market = market.merged(TradeJournal.market(from: decision)) }
        if entry.market.regime == nil && market.regime == nil, let r = macro?.regime {
            market.regime = r.kind.rawValue
            market.regimeLabel = r.label
        }
        tradeJournal = TradeJournal.patch(tradeJournal, id: id, market: market, decision: decision.map { TradeJournal.snapshot($0) })
    }

    func setJournalNote(_ id: String, _ note: String) { tradeJournal = TradeJournal.patch(tradeJournal, id: id, note: note) }
    func deleteJournalEntry(_ id: String) { tradeJournal = TradeJournal.remove(tradeJournal, id: id) }

    private func saveTradeJournal() {
        // The unreadable file is kept aside before being replaced.
        if tradeJournalError != nil, let bad = LocalStore.data("tradeJournal") { LocalStore.write(bad, "tradeJournal.invalid") }
        LocalStore.save(tradeJournal, "tradeJournal")
        if tradeJournalError != nil { tradeJournalError = nil }
    }

    // MARK: Simulation (paper trading)

    private var nowMs: Double { Date().timeIntervalSince1970 * 1000 }

    /// Simulated purchase: the engine's error text when refused, nil when done.
    func paperBuy(_ order: PaperOrder) -> String? {
        let r = Paper.open(paper, order, now: nowMs)
        if r.error == nil { paper = r.state }
        return r.error
    }

    /// Simulated sale of the whole position at `price` (nil: no price, refused by the engine).
    func paperSell(_ id: String, price: Double?) -> String? {
        let r = Paper.close(paper, id: id, price: price ?? 0, now: nowMs)
        if r.error == nil { paper = r.state }
        return r.error
    }

    /// Starts again from `capital` dollars (the simulation is kept in dollars like the prices): positions and journal
    /// erased.
    func paperReset(capital: Double) {
        paper = Paper.new(capital: capital, now: nowMs)
        paperJustClosed = []
    }

    /// Automatic exits: daily candles of the open positions that have a stop or a target, then the engine's rule
    /// (day after the opening, stop first, gap at the open). An asset whose candles fail is checked next time.
    func paperCheckExits() async {
        guard phase == .ready, let client, !checkingPaper else { return }
        var seen = Set<String>()
        let assets = paper.positions.filter { ($0.stop != nil || $0.target != nil) && seen.insert($0.key).inserted }.map(\.asset)
        guard !assets.isEmpty else { return }
        checkingPaper = true
        defer { checkingPaper = false }
        var candles: [String: [Candle]] = [:]
        for a in assets {
            do {
                candles[a.id] = try await client.candles(a, interval: "1d").candles
            } catch AltimError.unauthorized {
                return sessionLost()
            } catch {
                continue
            }
        }
        persistSession()
        // Applied to the state as it is now (a position may have been sold meanwhile).
        let r = Paper.checkExits(paper, candles: candles)
        guard !r.closed.isEmpty else { return }
        paper = r.state
        paperJustClosed += r.closed
    }

    /// From a tapped notification: "crypto:BTC" → the asset to open.
    func open(assetID: String) {
        let parts = assetID.split(separator: ":").map(String.init)
        guard parts.count == 2, let kind = Kind(rawValue: parts[0]) else { return }
        pendingOpen = (watchlist + holdings.map(\.asset)).first { $0.id == assetID } ?? Asset(symbol: parts[1], kind: kind, name: parts[1])
    }
}
