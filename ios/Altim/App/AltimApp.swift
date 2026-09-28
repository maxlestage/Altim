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
            default: break
            }
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase

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
        model.watchlist + model.holdings.map(\.asset) + model.selectionAssets + [model.focus, model.activityAsset].compactMap { $0 }
    }

    private var followKey: String {
        "\(model.phase)|\(scenePhase == .active)|" + Set(followed.map(\.id)).sorted().joined(separator: ",")
    }
}

struct MainTabs: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        TabView {
            NavigationStack { RadarView() }
                .tabItem { Label("Radar", systemImage: "dot.radiowaves.left.and.right") }
            NavigationStack { SelectionView() }
                .tabItem { Label("Sélection", systemImage: "list.number") }
            NavigationStack { HoldingsView() }
                .tabItem { Label("Mes avoirs", systemImage: "briefcase") }
            NavigationStack { AlertsView() }
                .tabItem { Label("Alertes", systemImage: "bell.badge") }
            NavigationStack { SettingsView() }
                .tabItem { Label("Réglages", systemImage: "gearshape") }
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
