import SwiftUI
import AltimCore

/// One search field for everything: cryptos and stocks / ETF together, by symbol or name
/// (BTC, Solana, Apple, NVDA, S&P 500…). No list to scroll through.
struct AssetSearchSection: View {
    let selected: Set<String>
    let onToggle: (UniverseEntry) -> Void
    var clearOnPick = false
    var title = "Rechercher une crypto ou une action"

    @Environment(AppServices.self) private var services
    @State private var query = ""
    @FocusState private var focused: Bool

    private var ready: Bool { services.universe.lists[.crypto] != nil && services.universe.lists[.stock] != nil }
    private var results: [UniverseEntry] {
        AssetUniverse.searchAll(crypto: services.universe.lists[.crypto] ?? [], stocks: services.universe.lists[.stock] ?? [], query, limit: 20)
    }

    var body: some View {
        let results = results
        Section {
            TextField("BTC, Solana, Apple, NVDA, S&P 500…", text: $query)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
                .focused($focused)
                .task {
                    // Both catalogues, so that one search covers cryptos and stocks.
                    async let c: Void = services.universe.ensure(.crypto, transport: services.transport)
                    async let s: Void = services.universe.ensure(.stock, transport: services.transport)
                    _ = await (c, s)
                }
            if !query.trimmingCharacters(in: .whitespaces).isEmpty {
                if !ready {
                    HStack { ProgressView(); Text("Chargement du catalogue…").foregroundStyle(.secondary) }
                } else if results.isEmpty {
                    Text("Aucun résultat. Les actions cotées en euros ne sont pas encore prises en charge.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                ForEach(results) { e in
                    let on = selected.contains(e.id)
                    Button {
                        onToggle(e)
                        if clearOnPick { query = ""; focused = true }
                    } label: {
                        HStack(spacing: 12) {
                            Image(systemName: on ? "checkmark.circle.fill" : "plus.circle")
                                .font(.title3)
                                .foregroundStyle(on ? Theme.cyan : Theme.textSecondary)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(e.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                                Text(e.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                            }
                            Spacer()
                            Text(e.kind == .crypto ? "Crypto" : e.isETF ? "ETF" : "Action")
                                .font(Theme.mono(10, weight: .regular)).foregroundStyle(Theme.cyan)
                        }
                    }
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
        } header: {
            Text(title)
        }
    }
}
