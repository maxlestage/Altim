import SwiftUI
import UserNotifications
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
        WatchBridge.shared.link = { model.watchLink }
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
    /// The logo traced at launch, and again right after the login (none with Reduce Motion).
    @State private var trace = true

    var body: some View {
        Group {
            if !model.acceptedDisclaimer {
                DisclaimerView()
            } else {
                switch model.phase {
                case .setup: LoginView()
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
        .onChange(of: model.phase) { old, new in
            if old == .setup && new == .ready { trace = true }
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
            model.live.follow(followed, client: model.client, onRenewed: { model.persistSession() }) {
                model.sessionLost()
            }
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
            NavigationStack { SelectionView() }
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
