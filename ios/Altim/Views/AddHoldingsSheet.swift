import SwiftUI
import AltimCore

/// Several holdings entered at once, in two sections: cryptos and stocks / ETF.
struct AddHoldingsSheet: View {
    let existing: [Holding]
    let onSave: ([Holding]) -> Void

    @Environment(AppServices.self) private var services
    @Environment(\.dismiss) private var dismiss
    @State private var lines: [Draft] = []
    @State private var prices: [String: Double] = [:]
    @State private var picking: PickTarget?

    struct Draft: Identifiable, Hashable {
        let entry: UniverseEntry
        var quantity = ""
        var averagePrice = ""
        var id: String { entry.id }
    }

    struct PickTarget: Identifiable {
        let kind: AssetClass
        var id: String { kind.rawValue }
    }

    private func number(_ s: String) -> Double? {
        Double(s.replacingOccurrences(of: ",", with: ".").replacingOccurrences(of: " ", with: ""))
    }

    /// Valid lines; an empty average price means "current price".
    private var ready: [Holding] {
        lines.compactMap { d in
            guard let q = number(d.quantity), q > 0 else { return nil }
            let p = d.averagePrice.trimmingCharacters(in: .whitespaces).isEmpty ? prices[d.id] : number(d.averagePrice)
            guard let p, p > 0 else { return nil }
            return Holding(symbol: d.entry.symbol, kind: d.entry.kind, name: d.entry.name, quantity: q, averagePrice: p)
        }
    }

    var body: some View {
        let ready = ready
        NavigationStack {
            Form {
                AssetSearchSection(selected: Set(lines.map(\.id)), onToggle: toggle, clearOnPick: true)
                Section {
                    Text("Cherchez chaque crypto ou action et touchez-la pour l'ajouter, puis indiquez la quantité et le prix moyen payé (vide = cours actuel). Vous pouvez aussi parcourir tout le catalogue.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                section(.crypto)
                section(.stock)
                if !ready.isEmpty {
                    Section {
                        LabeledContent("Montant investi", value: "\(ready.reduce(0) { $0 + $1.quantity * $1.averagePrice }.formatted(.number.precision(.fractionLength(2)))) $")
                        let skipped = lines.count - ready.count
                        if skipped > 0 {
                            Text("\(skipped) ligne\(skipped > 1 ? "s" : "") sans quantité : ignorée\(skipped > 1 ? "s" : "").")
                                .font(.footnote).foregroundStyle(Theme.warning)
                        }
                    }
                }
            }
            .navigationTitle("Ajouter des avoirs")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(ready.isEmpty ? "Enregistrer" : "Enregistrer (\(ready.count))") {
                        onSave(ready)
                        dismiss()
                    }
                    .disabled(ready.isEmpty)
                }
            }
            .sheet(item: $picking) { target in
                AssetPickerView(fixedKind: target.kind,
                                title: target.kind == .crypto ? "Toutes les cryptos" : "Toutes les actions et ETF",
                                selected: Set(lines.map(\.id)),
                                onToggle: toggle)
            }
            .task(id: lines.map(\.id)) { await loadPrices() }
        }
    }

    @ViewBuilder
    private func section(_ kind: AssetClass) -> some View {
        let rows = lines.filter { $0.entry.kind == kind }
        Section {
            ForEach(rows) { draft in
                VStack(alignment: .leading, spacing: 8) {
                    HStack(alignment: .firstTextBaseline) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(draft.entry.name).font(.headline).lineLimit(2)
                            Text(prices[draft.id].map { "\(draft.entry.symbol) · cours \(Format.price($0)) $" } ?? draft.entry.symbol)
                                .font(Theme.mono(11, weight: .regular)).foregroundStyle(Theme.textSecondary)
                        }
                        Spacer()
                        Button(role: .destructive) { toggle(draft.entry) } label: { Image(systemName: "xmark.circle.fill") }
                            .buttonStyle(.borderless)
                            .accessibilityLabel("Retirer \(draft.entry.name)")
                    }
                    HStack(spacing: 10) {
                        TextField("Quantité", text: binding(draft.id, \.quantity))
                            .keyboardType(.decimalPad)
                            .textFieldStyle(.roundedBorder)
                        TextField(prices[draft.id].map { "Prix moyen (\(Format.price($0)))" } ?? "Prix moyen ($)", text: binding(draft.id, \.averagePrice))
                            .keyboardType(.decimalPad)
                            .textFieldStyle(.roundedBorder)
                    }
                    if existing.contains(where: { $0.kind == kind && $0.symbol == draft.entry.symbol }) {
                        Text("Déjà dans vos avoirs : la quantité sera ajoutée et le prix moyen recalculé.")
                            .font(.caption).foregroundStyle(Theme.textSecondary)
                    }
                }
                .padding(.vertical, 4)
            }
            Button {
                picking = PickTarget(kind: kind)
            } label: {
                Label(kind == .crypto ? "Parcourir toutes les cryptos" : "Parcourir toutes les actions / ETF", systemImage: "list.bullet")
            }
        } header: {
            Text(kind == .crypto ? "Crypto\(rows.isEmpty ? "" : " · \(rows.count)")" : "Actions & ETF\(rows.isEmpty ? "" : " · \(rows.count)")")
        }
    }

    private func binding(_ id: String, _ key: WritableKeyPath<Draft, String>) -> Binding<String> {
        Binding(
            get: { lines.first { $0.id == id }?[keyPath: key] ?? "" },
            set: { value in
                if let i = lines.firstIndex(where: { $0.id == id }) { lines[i][keyPath: key] = value }
            })
    }

    private func toggle(_ entry: UniverseEntry) {
        if let i = lines.firstIndex(where: { $0.id == entry.id }) { lines.remove(at: i) } else { lines.append(Draft(entry: entry)) }
    }

    /// Current consensus price of each chosen asset (default average price).
    private func loadPrices() async {
        let missing = lines.map(\.entry).filter { prices[$0.id] == nil }
        guard !missing.isEmpty else { return }
        let market = services.market
        let found = await withTaskGroup(of: (String, Double?).self) { group in
            for e in missing {
                let asset = e.asset
                group.addTask { (e.id, try? await market.quote(for: asset).price) }
            }
            var out: [String: Double] = [:]
            for await (id, price) in group { if let price { out[id] = price } }
            return out
        }
        prices.merge(found) { _, new in new }
    }
}
