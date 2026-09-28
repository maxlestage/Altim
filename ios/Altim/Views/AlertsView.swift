import SwiftUI
import AltimKit

/// Alerts tab: what can be bought right now (same rule as the notifications), the price alerts chosen by the user,
/// and the journal of the notifications received with what each one gave since (measured, without fees).
struct AlertsView: View {
    @Environment(AppModel.self) private var model
    @State private var alerts: [BuyAlert]?
    @State private var quotes: [String: Double] = [:]
    @State private var error: String?

    var body: some View {
        List {
            if !model.alertsEnabled {
                Section {
                    Notice(text: "Notifications d'achat désactivées : activez-les dans Réglages pour être prévenu même app fermée.")
                }
                .listRowBackground(Color.clear)
            }

            Section("Achetables maintenant") {
                if let error {
                    Text(error).foregroundStyle(Theme.sell)
                } else if let alerts {
                    let buyable = alerts.filter(\.buy)
                    if buyable.isEmpty {
                        Text("Rien d'achetable pour l'instant selon la règle d'Altim (signal 4 h ou zone d'achat, sans blocage).")
                            .font(.footnote).foregroundStyle(Theme.textSecondary)
                    }
                    ForEach(buyable) { a in
                        NavigationLink(value: a.asset) { BuyRow(alert: a) }
                    }
                } else {
                    ProgressView("Vérification du radar et des avoirs…")
                }
            }
            .listRowBackground(Theme.surface.opacity(0.6))

            Section {
                if model.priceTargets.isEmpty {
                    Text("Aucune alerte de prix. Sur la fiche d'un actif, bouton cloche : « préviens-moi si BTC passe sous 80 000 $ ».")
                        .font(.footnote).foregroundStyle(Theme.textSecondary)
                }
                ForEach(model.priceTargets) { t in
                    NavigationLink(value: t.asset) { TargetRow(target: t, price: price(t.asset)) }
                        .swipeActions {
                            Button(role: .destructive) { model.removeTarget(t.id) } label: { Label("Supprimer", systemImage: "trash") }
                            if t.triggered != nil {
                                Button { model.rearmTarget(t.id, current: price(t.asset)) } label: { Label("Réarmer", systemImage: "arrow.counterclockwise") }.tint(Theme.cyan)
                            }
                        }
                }
            } header: {
                Text("Alertes de prix")
            } footer: {
                if !model.priceTargets.isEmpty { Text("Glissez vers la gauche pour supprimer ou réarmer une alerte.") }
            }
            .listRowBackground(Theme.surface.opacity(0.6))

            Section {
                if let s = AlertJournal.summary(model.journal, prices: prices) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Depuis leur envoi, \(s.up) alerte(s) d'achat sur \(s.count) sont en hausse (\(Int(s.upShare.rounded())) %), variation moyenne \(Format.percent(s.average, digits: 1)).")
                            .font(.subheadline)
                        Text("Mesure simple depuis chaque alerte, sans frais ni règle de sortie, sur vos propres alertes : une indication, pas une preuve. Les alertes de moins d'une heure et vos alertes de prix n'y sont pas comptées.")
                            .font(.caption).foregroundStyle(Theme.textSecondary)
                    }
                }
                if model.journal.isEmpty {
                    Text("Les notifications reçues s'afficheront ici, avec ce qu'elles ont donné depuis.")
                        .font(.footnote).foregroundStyle(Theme.textSecondary)
                }
                ForEach(model.journal.prefix(100)) { e in
                    NavigationLink(value: e.asset) { JournalRow(entry: e, price: price(e.asset)) }
                }
                if !model.journal.isEmpty {
                    Button("Vider le journal", role: .destructive) { model.clearJournal() }
                }
            } header: {
                Text("Journal des alertes")
            }
            .listRowBackground(Theme.surface.opacity(0.6))
        }
        .listStyle(.insetGrouped)
        .altimScreen()
        .navigationTitle("Alertes")
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .refreshable { await load() }
        .task { await load() }
    }

    /// Live price first, consensus quote otherwise.
    private func price(_ a: Asset) -> Double? { model.live.price(a)?.price ?? quotes[a.id] }

    private var prices: [String: Double] {
        var p = quotes
        for e in model.journal { if let t = model.live.price(e.asset) { p[e.asset.id] = t.price } }
        return p
    }

    private func load() async {
        guard let client = model.client else { return }
        let mine = model.watchlist + model.holdings.map(\.asset)
        let tracked = mine + model.priceTargets.map(\.asset) + model.journal.map(\.asset)
        let unique = { (list: [Asset]) in Array(Dictionary(list.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }).values) }
        do {
            alerts = try await client.alerts(unique(mine))
            error = nil
            let q = try await client.quotes(unique(tracked))
            quotes = Dictionary(q.map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a })
            model.persistSession()
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct BuyRow: View {
    var alert: BuyAlert

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(alert.symbol).font(Theme.mono(15, weight: .bold))
                Spacer()
                Text(Format.price(alert.price)).font(Theme.mono(14))
                Badge(text: alert.strong ? "ACHAT CONSEILLÉ" : "ACHAT POSSIBLE", tone: .good)
            }
            ForEach(alert.reasons + alert.cautions, id: \.self) { Text($0).font(.caption).foregroundStyle(.white.opacity(0.85)) }
        }
        .padding(.vertical, 4)
    }
}

private struct TargetRow: View {
    var target: PriceTarget
    var price: Double?

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text("\(target.asset.symbol) · \(target.label.lowercased())").font(.subheadline.weight(.semibold))
            if let done = target.triggered {
                Text("Atteinte le \(Format.date(done.timeIntervalSince1970 * 1000, time: true))").font(.caption).foregroundStyle(Theme.buy)
            } else if let price, let move = target.move {
                Text("Prix actuel \(Format.price(price)) · variation \(Format.percent((price / target.price - 1) * 100, digits: 1)) sur ±\(Format.plain(move, digits: 1)) %")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            } else if let price {
                Text("Prix actuel \(Format.price(price)) · encore \(Format.percent((target.price / price - 1) * 100, digits: 1))")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            } else {
                Text("En attente").font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
    }
}

private struct JournalRow: View {
    var entry: JournalEntry
    var price: Double?

    var body: some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.title).font(.footnote).lineLimit(1)
                Text("\(Format.date(entry.date.timeIntervalSince1970 * 1000, time: true)) · à \(Format.price(entry.price))")
                    .font(.caption2).foregroundStyle(Theme.textSecondary)
            }
            Spacer(minLength: 4)
            Badge(text: entry.source == .target ? "PRIX" : entry.source == .strongBuy ? "CONSEILLÉ" : "POSSIBLE", tone: entry.source == .target ? .neutral : .good)
            ChangeText(value: entry.change(at: price))
        }
    }
}

/// "Alerte de prix" of an asset page: above / below, threshold prefilled with the current price.
struct PriceTargetSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let asset: Asset
    let current: Double?
    /// 0: falls below, 1: rises above, 2: moves by ±X % from the current price.
    @State private var mode = 0
    @State private var text = ""
    @State private var moveText = "5"
    @State private var denied = false

    private var above: Bool { mode == 1 }

    private static func number(_ s: String) -> Double? {
        Double(s.replacingOccurrences(of: "\u{202F}", with: "").replacingOccurrences(of: " ", with: "").replacingOccurrences(of: ",", with: "."))
    }

    private var value: Double? { Self.number(text) }
    private var move: Double? { Self.number(moveText) }

    /// The threshold must be on the right side of the current price, otherwise it would fire at once.
    private var sideOK: Bool {
        if mode == 2 { return current != nil && (move ?? 0) > 0 && (move ?? 100) < 100 }
        guard let v = value, v > 0 else { return false }
        guard let current else { return true }
        return above ? v > current : v < current
    }

    var body: some View {
        NavigationStack {
            Form {
                if let current {
                    Section { KeyValue(key: "Prix actuel", value: Format.price(current)) }
                }
                Section {
                    Picker("Condition", selection: $mode) {
                        Text("Passe sous").tag(0)
                        Text("Passe au-dessus").tag(1)
                        Text("Bouge de ±").tag(2)
                    }
                    .pickerStyle(.segmented)
                    HStack {
                        if mode == 2 {
                            TextField("Variation", text: $moveText).keyboardType(.decimalPad).font(Theme.mono(18))
                            Text("%").foregroundStyle(Theme.textSecondary)
                        } else {
                            TextField("Prix", text: $text).keyboardType(.decimalPad).font(Theme.mono(18))
                            Text("$").foregroundStyle(Theme.textSecondary)
                        }
                    }
                    if mode == 2 {
                        Text("Une notification quand le prix s'écarte de ce pourcentage (à la hausse ou à la baisse) du prix actuel ; réarmée, elle repart du prix du moment.")
                            .font(.footnote).foregroundStyle(Theme.textSecondary)
                    } else if value != nil && !sideOK {
                        Text(above ? "Choisissez un prix au-dessus du prix actuel." : "Choisissez un prix en dessous du prix actuel.")
                            .font(.footnote).foregroundStyle(Theme.warning)
                    }
                    if denied {
                        Text("Notifications refusées : autorisez-les dans Réglages de l'iPhone → Notifications → Altim.")
                            .font(.footnote).foregroundStyle(Theme.warning)
                    }
                } footer: {
                    Text("Vérifiée avec les alertes d'achat (en arrière-plan quand iOS le permet, et à chaque ouverture) ; une seule notification, puis vous pouvez la réarmer dans l'onglet Alertes.")
                }
            }
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle("Alerte de prix · \(asset.symbol)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Créer") {
                        Task {
                            guard await BuyNotifications.authorize() else { return denied = true }
                            if mode == 2, let current, let move {
                                model.addTarget(PriceTarget(asset: asset, above: false, price: current, move: move))
                            } else if let v = value {
                                model.addTarget(PriceTarget(asset: asset, above: above, price: v))
                            } else {
                                return
                            }
                            dismiss()
                        }
                    }
                    .disabled(!sideOK)
                }
            }
            .onAppear {
                if text.isEmpty, let current { text = Format.plain(current, digits: current >= 1 ? 2 : 6) }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
