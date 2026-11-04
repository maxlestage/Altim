import SwiftUI
import UserNotifications
import AppIntents
import AltimKit

/// Launch-time registrations: background check of the alerts, notification taps, Watch link.
@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate {
    let model = AppModel()
    private let notifications = NotificationDelegate()

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let model = model
        BuyNotifications.register { model }
        BuyNotifications.schedule(enabled: model.needsChecks)
        notifications.open = { model.open(assetID: $0) }
        notifications.openNews = { model.pendingNews = true }
        UNUserNotificationCenter.current().delegate = notifications
        WatchBridge.shared.refresh = { await BuyNotifications.run(model) }
        WatchBridge.shared.activate()
        model.activityAsset = LiveActivities.shared.current
        return true
    }
}

@main
struct AltimApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @Environment(\.scenePhase) private var scenePhase
    private var model: AppModel { delegate.model }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .preferredColorScheme(.dark)
                .tint(Theme.cyan)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background: model.didEnterBackground()
            case .active:
                model.willEnterForeground()
                Task { await BuyNotifications.foregroundCheck(model) }
                Task { await model.paperCheckExits() }
            default: break
            }
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The logo traced at launch (none with Reduce Motion).
    @State private var trace = true

    var body: some View {
        Group {
            if !model.acceptedDisclaimer {
                DisclaimerView()
            } else {
                switch model.phase {
                case .locked: LockView()
                case .ready: MainTabs()
                }
            }
        }
        .overlay {
            if trace && !reduceMotion {
                LaunchTraceView { trace = false }
            }
        }
        // App switcher snapshot: the holdings are hidden as soon as the app is not in the foreground.
        .overlay {
            if scenePhase != .active && model.phase == .ready {
                ZStack {
                    AppBackground()
                    Image(systemName: "lock.shield").font(.system(size: 48)).foregroundStyle(Theme.cyan)
                }
                .ignoresSafeArea()
            }
        }
        // Live prices of everything visible: watch list, holdings and the open asset.
        .task(id: followKey) {
            guard model.phase == .ready, scenePhase == .active else { return model.live.stop() }
            model.live.follow(followed, client: model.client)
        }
    }

    private var followed: [Asset] {
        model.watchlist + model.holdings.map(\.asset) + model.paper.positions.map(\.asset) + model.selectionAssets
            + [model.focus, model.activityAsset].compactMap { $0 }
    }

    private var followKey: String {
        "\(model.phase)|\(scenePhase == .active)|\(model.moneyStamp.rawValue)|" + Set(followed.map(\.id)).sorted().joined(separator: ",")
    }
}

struct MainTabs: View {
    @Environment(AppModel.self) private var model
    @State private var tab = 0

    var body: some View {
        @Bindable var model = model
        TabView(selection: $tab) {
            NavigationStack { RadarView() }
                .tabItem { Label("Radar", systemImage: "dot.radiowaves.left.and.right") }.tag(0)
            NavigationStack { RotationView() }
                .tabItem { Label("Sélection", systemImage: "list.number") }.tag(1)
            NavigationStack { HoldingsView() }
                .tabItem { Label("Mes avoirs", systemImage: "briefcase") }.tag(2)
            NavigationStack { AlertsView() }
                .tabItem { Label("Alertes", systemImage: "bell.badge") }.tag(3)
            NavigationStack { NewsView() }
                .tabItem { Label("Actu", systemImage: "newspaper") }.tag(4)
        }
        // Amounts are formatted when the screens are built: rebuilt when the currency shown changes (euros ⇄ dollars).
        .id(model.moneyStamp)
        // EUR/USD rate read now and every 10 minutes.
        .task { model.startFx() }
        // Tapped news notification: the Actu tab.
        .onChange(of: model.pendingNews, initial: true) { _, open in
            guard open else { return }
            tab = 4
            model.pendingNews = false
        }
        // Server or network down: the saved answers are shown, with their date.
        .safeAreaInset(edge: .top, spacing: 0) {
            if let since = model.offlineSince { OfflineBanner(since: since) }
        }
        // Tapped notification: the asset opens above the tabs.
        .sheet(item: $model.pendingOpen) { asset in
            NavigationStack {
                AssetDetailView(asset: asset)
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Fermer") { model.pendingOpen = nil } } }
            }
            .environment(model)
        }
    }
}

struct OfflineBanner: View {
    var since: Date

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "icloud.slash").foregroundStyle(Theme.warning)
            Text("Hors ligne : données du \(Format.date(since.timeIntervalSince1970 * 1000, time: true)). Reconnexion automatique.")
                .font(.caption)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
        .background(Theme.warning.opacity(0.18))
        .background(.ultraThinMaterial)
        .accessibilityElement(children: .combine)
    }
}

struct LockView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(spacing: 24) {
            Spacer()
            Image(systemName: "lock.shield").font(.system(size: 56)).foregroundStyle(Theme.cyan).neonGlow(Theme.cyan)
            Text("Altim est verrouillé").font(Theme.display(24))
            Text("Face ID ou le code de l'iPhone protège vos avoirs et votre accès.")
                .font(.subheadline).foregroundStyle(Theme.textSecondary).multilineTextAlignment(.center)
            Spacer()
            Button("Déverrouiller") { Task { await model.unlock() } }
                .buttonStyle(NeonButtonStyle())
        }
        .padding(24)
        .background(AppBackground())
        .task { await model.unlock() }
    }
}

// MARK: - Siri and Shortcuts

/// « Dis Siri, que dit le radar Altim ? » : the signals of the watch list, computed on the iPhone (no server, no
/// account). The watch list is personal: the iPhone must be unlocked.
struct RadarSummaryIntent: AppIntent {
    static var title: LocalizedStringResource = "Résumé du radar"
    static var description = IntentDescription("Les signaux des actifs de votre radar, calculés par Altim sur l'appareil, sans serveur.")
    static var authenticationPolicy: IntentAuthenticationPolicy = .requiresAuthentication

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let watchlist = Array((LocalStore.load([Asset].self, "watchlist") ?? Asset.defaults).prefix(8))
        guard !watchlist.isEmpty else { return .result(dialog: "Votre radar est vide.") }
        let rows = try await AltimClient.standalone(local: LocalBackend(bot: false)).radar(watchlist, interval: Timeframe.standard.rawValue)
        return .result(dialog: "\(SiriText.radar(rows))")
    }
}

/// « Cours avec Altim » : the consensus price of one asset, its 24 h move and its 4 h signal.
struct AssetPriceIntent: AppIntent {
    static var title: LocalizedStringResource = "Cours d'un actif"
    static var description = IntentDescription("Le prix médian des sources qui s'accordent, la variation sur 24 h et le signal technique, calculés par Altim sur l'appareil, sans serveur.")

    @Parameter(title: "Actif", requestValueDialog: "Quel actif ? Par exemple Bitcoin ou Apple.")
    var name: String

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let client = AltimClient.standalone(local: LocalBackend(bot: false))
        let wanted = name.trimmingCharacters(in: .whitespaces)
        let found = try await client.search(wanted, limit: 5)
        guard let item = found.first(where: { $0.symbol == wanted.uppercased() }) ?? found.first else {
            return .result(dialog: "Je ne trouve pas « \(wanted) ».")
        }
        let rows = try await client.radar([item.asset], interval: Timeframe.standard.rawValue)
        return .result(dialog: "\(SiriText.asset(item.asset, rows.first))")
    }
}

/// « État des marchés » : the market filter (Bitcoin, S&P 500) read on the device.
struct MarketsIntent: AppIntent {
    static var title: LocalizedStringResource = "État des marchés"
    static var description = IntentDescription("Le filtre de marché d'Altim : le Bitcoin pour les cryptos, le S&P 500 pour les actions, calculé sur l'appareil à partir des cours publics, sans serveur.")

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let states = await MarketBoard.read(AltimClient.standalone(local: LocalBackend(bot: false)))
        return .result(dialog: "\(SiriText.markets(states))")
    }
}

struct AltimShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: RadarSummaryIntent(),
                    phrases: ["Que dit le radar \(.applicationName)", "Résumé du radar \(.applicationName)", "\(.applicationName) radar"],
                    shortTitle: "Résumé du radar", systemImageName: "dot.radiowaves.left.and.right")
        AppShortcut(intent: AssetPriceIntent(),
                    phrases: ["Cours avec \(.applicationName)", "Prix avec \(.applicationName)"],
                    shortTitle: "Cours d'un actif", systemImageName: "chart.line.uptrend.xyaxis")
        AppShortcut(intent: MarketsIntent(),
                    phrases: ["État des marchés \(.applicationName)", "Marchés \(.applicationName)"],
                    shortTitle: "État des marchés", systemImageName: "globe")
    }
}
