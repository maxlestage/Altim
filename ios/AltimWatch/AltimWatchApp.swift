import Security
import SwiftUI
import WatchConnectivity
import AltimKit

/// Apple Watch app: which assets can be bought now and their current prices. The Watch reads them itself from the
/// server over HTTPS (`/api/alerts` and `/api/tickers`, through the iPhone's connection or Wi-Fi / cellular) when it
/// opens and every 30 s while it stays on screen, with the session the iPhone gave it (`WatchLink`, kept in the Watch's
/// Keychain; never the password). watchOS does not allow a WebSocket for an ordinary app (Apple TN3135), hence these
/// HTTPS readings. While the iPhone app is open, every change it receives on its live socket also lands here.
@main
struct AltimWatchApp: App {
    @State private var store = WatchStore()

    var body: some Scene {
        WindowGroup {
            NavigationStack { AlertsList() }
                .environment(store)
                .tint(Color(red: 0.0, green: 0.94, blue: 1.0))
        }
    }
}

/// The session given by the iPhone, in the Watch's Keychain (readable after the first unlock, this device only).
enum WatchKeychain {
    private static let base: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.maxlestage.altim.watch",
        kSecAttrAccount as String: "link",
    ]

    static func save(_ link: WatchLink?) {
        SecItemDelete(base as CFDictionary)
        guard let link, let data = try? JSONEncoder().encode(link) else { return }
        var attributes = base
        attributes[kSecValueData as String] = data
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        SecItemAdd(attributes as CFDictionary, nil)
    }

    static func load() -> WatchLink? {
        var query = base
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess, let data = item as? Data else { return nil }
        return try? JSONDecoder().decode(WatchLink.self, from: data)
    }
}

@MainActor
@Observable
final class WatchStore: NSObject, WCSessionDelegate {
    private(set) var alerts: [BuyAlert] = []
    private(set) var checked: Date? = nil
    private(set) var refreshing = false
    private(set) var message: String? = nil
    private(set) var link: WatchLink? = nil

    override init() {
        super.init()
        if let data = UserDefaults.standard.data(forKey: "alerts"), let saved = try? JSONDecoder().decode([BuyAlert].self, from: data) {
            alerts = saved
            checked = UserDefaults.standard.object(forKey: "checked") as? Date
        }
        link = WatchKeychain.load()
        Self.applyDisplay(currency: UserDefaults.standard.string(forKey: "currency"), fx: UserDefaults.standard.data(forKey: "fx"))
        if WCSession.isSupported() {
            WCSession.default.delegate = self
            WCSession.default.activate()
        }
    }

    /// Reads the alerts and the prices from the server now (15 s at most), whether or not the iPhone is nearby.
    func refresh() async {
        guard !refreshing else { return }
        guard let client = link?.client() else {
            message = WatchLink.notLinked
            return
        }
        let assets = alerts.map(\.asset)
        guard !assets.isEmpty else {
            message = "Ajoutez des actifs au Radar sur l'iPhone, puis ouvrez Altim une fois."
            return
        }
        refreshing = true
        defer { refreshing = false }
        do {
            async let fresh = client.alerts(assets)
            // The prices are a plus: the alerts alone are enough when the quotes fail.
            async let quotes = try? client.quotes(assets)
            let items = try await fresh
            let prices = Dictionary(((await quotes) ?? []).map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a })
            store(WatchLink.merge(items, quotes: prices), checked: Date())
            message = nil
        } catch {
            message = WatchLink.message(for: error)
        }
    }

    private func store(_ items: [BuyAlert], checked: Date?) {
        alerts = items
        self.checked = checked
        UserDefaults.standard.set(try? JSONEncoder().encode(items), forKey: "alerts")
        UserDefaults.standard.set(checked, forKey: "checked")
    }

    /// Display currency and rate chosen on the iPhone (euros by default; dollars until a rate is known).
    private static func applyDisplay(currency: String?, fx: Data?) {
        Money.setDisplay(Currency(rawValue: currency ?? "") ?? .eur, fx.flatMap { try? JSONDecoder().decode(FxRate.self, from: $0) })
    }

    private func apply(_ context: [String: Any]) {
        if let currency = context["currency"] as? String {
            let fx = context["fx"] as? Data
            Self.applyDisplay(currency: currency, fx: fx)
            UserDefaults.standard.set(currency, forKey: "currency")
            UserDefaults.standard.set(fx, forKey: "fx")
        }
        if let data = context["link"] as? Data {
            // Empty: the iPhone logged out, the Watch forgets the session.
            let next = data.isEmpty ? nil : try? JSONDecoder().decode(WatchLink.self, from: data)
            if next != link {
                link = next
                WatchKeychain.save(next)
                if next != nil, message == WatchLink.notLinked { message = nil }
            }
        }
        guard let data = context["alerts"] as? Data, let items = try? JSONDecoder().decode([BuyAlert].self, from: data) else { return }
        let at = context["checked"] as? Date
        // An older check of the iPhone never replaces a fresher reading of the Watch.
        if let mine = checked, let at, at < mine { return }
        store(items, checked: at)
    }

    nonisolated func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        let context = session.receivedApplicationContext
        Task { @MainActor in self.apply(context) }
    }

    nonisolated func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        Task { @MainActor in self.apply(applicationContext) }
    }
}

private let buyColor = Color(red: 0.22, green: 1.0, blue: 0.53)

struct AlertsList: View {
    @Environment(WatchStore.self) private var store
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        List {
            if store.link == nil {
                Text(WatchLink.notLinked).font(.footnote).foregroundStyle(.orange)
            }
            let buyable = store.alerts.filter(\.buy)
            if store.alerts.isEmpty && store.link != nil {
                Text("Ajoutez des actifs au Radar sur l'iPhone, puis ouvrez Altim une fois.").font(.footnote).foregroundStyle(.secondary)
            }
            if !buyable.isEmpty {
                Section("Achetables") {
                    ForEach(buyable) { a in NavigationLink(value: a) { AlertRow(alert: a) } }
                }
            }
            let others = store.alerts.filter { !$0.buy }
            if !others.isEmpty {
                Section("Pas pour l'instant") {
                    ForEach(others) { a in NavigationLink(value: a) { AlertRow(alert: a) } }
                }
            }
            Section {
                Button {
                    Task { await store.refresh() }
                } label: {
                    Label(store.refreshing ? "Mise à jour…" : "Mettre à jour", systemImage: "arrow.clockwise")
                }
                .disabled(store.refreshing)
                if let m = store.message { Text(m).font(.footnote).foregroundStyle(.orange) }
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    Text(WatchLink.checkedText(store.checked, now: ctx.date)).font(.footnote).foregroundStyle(.secondary)
                }
            } footer: {
                Text("Lu sur le serveur toutes les 30 s tant que l'app est à l'écran. Conseil indicatif : Altim ne passe aucun ordre.")
            }
        }
        .navigationTitle("Altim")
        .navigationDestination(for: BuyAlert.self) { AlertDetail(alert: $0) }
        // On screen: a reading now, then every 30 s; stopped when the screen goes off or the app leaves.
        .task(id: scenePhase == .active) {
            guard scenePhase == .active else { return }
            while !Task.isCancelled {
                await store.refresh()
                try? await Task.sleep(nanoseconds: UInt64(WatchLink.refreshInterval * 1e9))
            }
        }
    }
}

struct AlertRow: View {
    var alert: BuyAlert

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(alert.symbol).font(.headline.monospaced())
                Text(Format.price(alert.price)).font(.caption.monospaced()).foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: alert.buy ? (alert.strong ? "arrow.up.circle.fill" : "arrow.up.circle") : "pause.circle")
                .foregroundStyle(alert.buy ? buyColor : .secondary)
                .accessibilityLabel(alert.buy ? (alert.strong ? "Achat conseillé" : "Achat possible") : "Attendre")
        }
    }
}

struct AlertDetail: View {
    var alert: BuyAlert

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                Text(alert.name).font(.headline)
                Text(Format.price(alert.price)).font(.title3.monospaced())
                Text(alert.buy ? (alert.strong ? "ACHETER" : "ZONE D'ACHAT") : "ATTENDRE")
                    .font(.caption.bold())
                    .foregroundStyle(alert.buy ? buyColor : .secondary)
                ForEach(alert.reasons + alert.blockers + alert.cautions, id: \.self) { line in
                    Text(line).font(.footnote)
                }
                if alert.reasons.isEmpty && alert.blockers.isEmpty {
                    Text(alert.body).font(.footnote)
                }
            }
        }
        .navigationTitle(alert.symbol)
    }
}
