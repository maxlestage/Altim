import SwiftUI
import AltimKit

@main
struct AltimApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

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
            case .active: model.willEnterForeground()
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
        // Live prices of everything visible: watch list, holdings and the open asset.
        .task(id: followKey) {
            guard model.phase == .ready, scenePhase == .active else { return model.live.stop() }
            model.live.follow(followed, client: model.client) {
                model.sessionLost()
            }
        }
    }

    private var followed: [Asset] {
        model.watchlist + model.holdings.map(\.asset) + model.selectionAssets + (model.focus.map { [$0] } ?? [])
    }

    private var followKey: String {
        "\(model.phase)|\(scenePhase == .active)|" + Set(followed.map(\.id)).sorted().joined(separator: ",")
    }
}

struct MainTabs: View {
    var body: some View {
        TabView {
            NavigationStack { RadarView() }
                .tabItem { Label("Radar", systemImage: "dot.radiowaves.left.and.right") }
            NavigationStack { SelectionView() }
                .tabItem { Label("Sélection", systemImage: "list.number") }
            NavigationStack { HoldingsView() }
                .tabItem { Label("Mes avoirs", systemImage: "briefcase") }
            NavigationStack { SettingsView() }
                .tabItem { Label("Réglages", systemImage: "gearshape") }
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
