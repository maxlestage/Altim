import SwiftUI
import AltimCore

@main
struct AltimApp: App {
    @State private var settings = AppSettings()
    @State private var services = AppServices()

    init() {
        let appearance = UITabBarAppearance()
        appearance.configureWithTransparentBackground()
        appearance.backgroundEffect = UIBlurEffect(style: .systemUltraThinMaterialDark)
        UITabBar.appearance().standardAppearance = appearance
        UITabBar.appearance().scrollEdgeAppearance = appearance

        let nav = UINavigationBarAppearance()
        nav.configureWithTransparentBackground()
        nav.largeTitleTextAttributes = [.foregroundColor: UIColor.white]
        nav.titleTextAttributes = [.foregroundColor: UIColor.white]
        UINavigationBar.appearance().standardAppearance = nav
        UINavigationBar.appearance().scrollEdgeAppearance = nav
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(settings)
                .environment(services)
                .preferredColorScheme(.dark)
                .tint(Theme.cyan)
        }
    }
}

struct RootView: View {
    @Environment(AppSettings.self) private var settings

    var body: some View {
        if settings.acceptedDisclaimer {
            TabView {
                RadarView()
                    .tabItem { Label("Radar", systemImage: "dot.radiowaves.left.and.right") }
                HoldingsView()
                    .tabItem { Label("Mes avoirs", systemImage: "square.stack.3d.up.fill") }
                PortfolioView()
                    .tabItem { Label("Courtiers", systemImage: "chart.pie.fill") }
                SettingsView()
                    .tabItem { Label("Réglages", systemImage: "slider.horizontal.3") }
            }
        } else {
            DisclaimerView()
        }
    }
}
