import SwiftUI
import WatchConnectivity
import AltimKit

/// Apple Watch app: which assets can be bought now, as computed by the iPhone (the Watch holds neither the password
/// nor the session). The notifications "achat possible" of the iPhone appear on the Watch when the iPhone is locked.
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

@MainActor
@Observable
final class WatchStore: NSObject, WCSessionDelegate {
    private(set) var alerts: [BuyAlert] = []
    private(set) var checked: Date? = nil
    private(set) var refreshing = false
    private(set) var message: String? = nil

    override init() {
        super.init()
        if let data = UserDefaults.standard.data(forKey: "alerts"), let saved = try? JSONDecoder().decode([BuyAlert].self, from: data) {
            alerts = saved
            checked = UserDefaults.standard.object(forKey: "checked") as? Date
        }
        if WCSession.isSupported() {
            WCSession.default.delegate = self
            WCSession.default.activate()
        }
    }

    /// Asks the iPhone for a fresh check (it must be reachable: nearby, Altim installed).
    func refresh() {
        let session = WCSession.default
        guard session.activationState == .activated, session.isReachable else {
            message = "iPhone injoignable : ouvrez Altim sur l'iPhone."
            return
        }
        refreshing = true
        message = nil
        session.sendMessage(["refresh": true], replyHandler: { _ in
            Task { @MainActor in self.refreshing = false }
        }, errorHandler: { _ in
            Task { @MainActor in
                self.refreshing = false
                self.message = "Échec de la mise à jour."
            }
        })
    }

    private func apply(_ context: [String: Any]) {
        guard let data = context["alerts"] as? Data, let items = try? JSONDecoder().decode([BuyAlert].self, from: data) else { return }
        alerts = items
        checked = context["checked"] as? Date
        UserDefaults.standard.set(data, forKey: "alerts")
        UserDefaults.standard.set(checked, forKey: "checked")
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

    var body: some View {
        List {
            let buyable = store.alerts.filter(\.buy)
            if store.alerts.isEmpty {
                Text("Ouvrez Altim sur l'iPhone et activez les notifications d'achat.").font(.footnote).foregroundStyle(.secondary)
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
                    store.refresh()
                } label: {
                    Label(store.refreshing ? "Mise à jour…" : "Mettre à jour", systemImage: "arrow.clockwise")
                }
                .disabled(store.refreshing)
                if let m = store.message { Text(m).font(.footnote).foregroundStyle(.orange) }
                if let checked = store.checked {
                    Text("Vérifié \(checked, style: .relative)").font(.footnote).foregroundStyle(.secondary)
                }
            } footer: {
                Text("Conseil indicatif : Altim ne passe aucun ordre.")
            }
        }
        .navigationTitle("Altim")
        .navigationDestination(for: BuyAlert.self) { AlertDetail(alert: $0) }
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
