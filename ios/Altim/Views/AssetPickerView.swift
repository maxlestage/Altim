import SwiftUI
import AltimCore

/// Full catalogue, browsable by category and searchable. Several assets can be picked in one go.
struct AssetPickerView: View {
    let fixedKind: AssetClass?
    let title: String
    /// Ids of the selected assets ("crypto:BTC", "stock:AAPL").
    let selected: Set<String>
    let onToggle: (UniverseEntry) -> Void

    @Environment(AppServices.self) private var services
    @Environment(\.dismiss) private var dismiss
    /// nil = combined search (default without a fixed category); otherwise browsing one category.
    @State private var browsing: AssetClass?
    @State private var query = ""
    @State private var limit = 100

    init(fixedKind: AssetClass? = nil, title: String, selected: Set<String>, onToggle: @escaping (UniverseEntry) -> Void) {
        self.fixedKind = fixedKind
        self.title = title
        self.selected = selected
        self.onToggle = onToggle
        _browsing = State(initialValue: fixedKind)
    }

    private var kind: AssetClass { browsing ?? .crypto }
    private var list: [UniverseEntry] { services.universe.lists[kind] ?? [] }
    private var results: [UniverseEntry] {
        query.trimmingCharacters(in: .whitespaces).isEmpty ? list : AssetUniverse.search(list, query, limit: list.count)
    }

    var body: some View {
        let results = results
        NavigationStack {
            List {
                if fixedKind == nil {
                    Picker("Mode", selection: $browsing) {
                        Text("Recherche").tag(AssetClass?.none)
                        Text("Cryptos").tag(AssetClass?.some(.crypto))
                        Text("Actions").tag(AssetClass?.some(.stock))
                    }
                    .pickerStyle(.segmented)
                    .listRowBackground(Color.clear)
                }
                if browsing == nil {
                    AssetSearchSection(selected: selected, onToggle: onToggle)
                    Section {
                        Text("Tapez un symbole ou un nom : cryptos et actions sont cherchées ensemble. Les onglets permettent aussi de parcourir tout le catalogue.")
                            .font(.footnote).foregroundStyle(.secondary)
                    }
                } else {
                    Section {
                        TextField(kind == .crypto ? "Filtrer : BTC, Solana, PEPE…" : "Filtrer : Apple, NVDA, S&P 500…", text: $query)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        if list.isEmpty {
                            if let error = services.universe.errors[kind] {
                                Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(Theme.warning)
                            } else {
                                HStack { ProgressView(); Text("Chargement du catalogue…").foregroundStyle(.secondary) }
                            }
                        }
                        ForEach(results.prefix(limit)) { entry in row(entry) }
                        if results.count > limit {
                            Button("Afficher plus (\((results.count - limit).formatted()) restants)") { limit += 200 }
                        }
                    } header: {
                        Text(header(count: results.count))
                    } footer: {
                        if kind == .stock {
                            Text("Actions et ETF cotés aux États-Unis. Les actions cotées en euros ne sont pas encore prises en charge.")
                        }
                    }
                }
            }
            .onChange(of: query) { limit = 100 }
            .onChange(of: browsing) { limit = 100; query = "" }
            .task(id: browsing) {
                if let browsing { await services.universe.ensure(browsing, transport: services.transport) }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(selected.isEmpty ? "Terminé" : "Terminé (\(selected.count))") { dismiss() }
                }
            }
        }
    }

    private func header(count: Int) -> String {
        guard !list.isEmpty else { return kind == .crypto ? "Cryptos" : "Actions & ETF" }
        if !query.trimmingCharacters(in: .whitespaces).isEmpty { return "\(count.formatted()) résultat\(count > 1 ? "s" : "")" }
        return kind == .crypto ? "Toutes les cryptos : \(list.count.formatted())" : "Toutes les actions et ETF : \(list.count.formatted())"
    }

    private func row(_ e: UniverseEntry) -> some View {
        let on = selected.contains(e.id)
        return Button { onToggle(e) } label: {
            HStack(spacing: 12) {
                Image(systemName: on ? "checkmark.circle.fill" : "plus.circle")
                    .font(.title3)
                    .foregroundStyle(on ? Theme.cyan : Theme.textSecondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(e.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                    Text(e.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                }
                Spacer()
                Text(tag(e)).font(Theme.mono(10, weight: .regular)).foregroundStyle(Theme.cyan)
            }
        }
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    private func tag(_ e: UniverseEntry) -> String {
        if let r = e.rank { return "#\(r)" }
        if e.kind == .stock { return e.isETF ? "ETF" : "Action" }
        return "\(e.flag) plateforme\(e.flag > 1 ? "s" : "")"
    }
}
