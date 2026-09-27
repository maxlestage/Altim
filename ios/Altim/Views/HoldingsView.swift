import SwiftUI
import AltimKit

/// Portfolio: value at live prices, gain / loss, concentration, and the daily signal of each line.
/// The lines stay on this iPhone; only the symbols are sent to the server to get prices and signals.
struct HoldingsView: View {
    @Environment(AppModel.self) private var model
    @State private var quotes: [String: Double] = [:]
    @State private var signals: [String: RadarRow] = [:]
    @State private var editing: Holding?
    @State private var adding = false
    @State private var error: String?

    var body: some View {
        let portfolio = Portfolio(holdings: model.holdings, prices: prices)
        List {
            if model.holdings.isEmpty {
                Section {
                    VStack(spacing: 14) {
                        Image(systemName: "briefcase").font(.largeTitle).foregroundStyle(Theme.cyan)
                        Text("Ajoutez ce que vous possédez (cryptos, actions) pour suivre sa valeur en direct et le signal de chaque ligne.")
                            .font(.subheadline).multilineTextAlignment(.center).foregroundStyle(Theme.textSecondary)
                        Button("Ajouter un avoir") { adding = true }.buttonStyle(NeonButtonStyle())
                    }
                    .padding(.vertical, 12)
                }
                .listRowBackground(Color.clear)
            } else {
                Section { summary(portfolio) }.listRowBackground(Color.clear)
                if let error { Section { Notice(text: error, tone: .bad) }.listRowBackground(Color.clear) }
                Section {
                    ForEach(portfolio.lines) { line in
                        NavigationLink(value: line.holding.asset) { lineView(line) }
                            .swipeActions(edge: .trailing) {
                                Button(role: .destructive) { model.holdings.removeAll { $0.id == line.id } } label: { Label("Supprimer", systemImage: "trash") }
                                Button { editing = line.holding } label: { Label("Modifier", systemImage: "pencil") }.tint(Theme.violet)
                            }
                            .listRowBackground(Theme.surface.opacity(0.6))
                    }
                } header: {
                    HStack { Text("Lignes · signal 1 jour"); Spacer(); LiveBadge() }
                } footer: {
                    Text("Glissez une ligne vers la gauche pour la modifier ou la supprimer. Vos avoirs restent sur cet iPhone.")
                }
            }
        }
        .listStyle(.insetGrouped)
        .altimScreen()
        .navigationTitle("Mes avoirs")
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .toolbar {
            Button { adding = true } label: { Image(systemName: "plus") }.accessibilityLabel("Ajouter un avoir")
        }
        .sheet(isPresented: $adding) { HoldingForm(holding: nil) }
        .sheet(item: $editing) { HoldingForm(holding: $0) }
        .task(id: model.holdings.map(\.asset.id).joined(separator: ",")) { await load() }
        .refreshable { await load() }
    }

    /// Live price when available, otherwise the consensus quote.
    private var prices: [String: Double] {
        var p = quotes
        for h in model.holdings { if let t = model.live.price(h.asset) { p[h.asset.id] = t.price } }
        return p
    }

    private func summary(_ p: Portfolio) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Valeur totale").font(.caption).foregroundStyle(Theme.textSecondary)
            Text(Format.money(p.total)).font(Theme.mono(30, weight: .bold))
                .contentTransition(.numericText()).animation(.default, value: p.total)
            if let g = p.gain {
                HStack(spacing: 8) {
                    Text("\(g >= 0 ? "+" : "−")\(Format.money(abs(g)))").font(Theme.mono(14)).foregroundStyle(Theme.color(forChange: g))
                    ChangeText(value: p.gainPercent)
                    Text("depuis l'achat").font(.caption).foregroundStyle(Theme.textSecondary)
                }
            }
            if p.total > 0 {
                GeometryReader { geo in
                    HStack(spacing: 2) {
                        Rectangle().fill(Theme.allocCrypto).frame(width: geo.size.width * p.cryptoShare / 100)
                        Rectangle().fill(Theme.allocStock)
                    }
                    .clipShape(Capsule())
                }
                .frame(height: 8)
                .accessibilityLabel("Cryptos \(Int(p.cryptoShare.rounded())) %, actions \(Int((100 - p.cryptoShare).rounded())) %")
                HStack {
                    Label("Cryptos \(Int(p.cryptoShare.rounded())) %", systemImage: "circle.fill").foregroundStyle(Theme.allocCrypto)
                    Spacer()
                    Label("Actions \(Int((100 - p.cryptoShare).rounded())) %", systemImage: "circle.fill").foregroundStyle(Theme.allocStock)
                }
                .font(.caption)
                .labelStyle(DotLabel())
            }
            ForEach(p.warnings, id: \.self) { Notice(text: $0, tone: .warn) }
        }
        .glassCard()
    }

    private func lineView(_ line: PortfolioLine) -> some View {
        let h = line.holding
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(h.asset.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                Text("\(Format.quantity(h.quantity)) · \(Format.price(line.price))").font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
            }
            Spacer(minLength: 4)
            VStack(alignment: .trailing, spacing: 3) {
                Text(line.value.map { Format.money($0) } ?? "—").font(Theme.mono(14)).foregroundStyle(.white)
                if let gp = line.gainPercent { ChangeText(value: gp) } else if let w = line.weight { Text("\(Int(w.rounded())) %").font(.caption).foregroundStyle(Theme.textSecondary) }
            }
            if let s = signals[h.asset.id]?.signal { ActionBadge(action: s.action) }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    private func load() async {
        guard let client = model.client, !model.holdings.isEmpty else { return }
        let assets = Array(Dictionary(model.holdings.map { ($0.asset.id, $0.asset) }, uniquingKeysWith: { a, _ in a }).values)
        async let r = try? client.radar(assets, interval: "1d")
        do {
            let q = try await client.quotes(assets)
            quotes = Dictionary(q.map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a })
            error = nil
            model.persistSession()
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch {
            self.error = error.localizedDescription
        }
        if let rows = await r { signals = Dictionary(rows.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }) }
    }
}

private struct DotLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.icon.font(.system(size: 7))
            configuration.title.foregroundStyle(.white.opacity(0.85))
        }
    }
}

/// Add or edit a line: asset (search), quantity, average purchase price.
struct HoldingForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let holding: Holding?
    @State private var asset: Asset?
    @State private var query = ""
    @State private var results: [SearchItem] = []
    @State private var quantity = ""
    @State private var average = ""
    @State private var current: Double?

    var body: some View {
        NavigationStack {
            Form {
                Section("Actif") {
                    if let asset {
                        HStack {
                            VStack(alignment: .leading) {
                                Text(asset.symbol).font(Theme.mono(15, weight: .bold))
                                Text(asset.name).font(.caption).foregroundStyle(Theme.textSecondary)
                            }
                            Spacer()
                            if holding == nil { Button("Changer") { self.asset = nil } }
                        }
                    } else {
                        TextField("Rechercher : BTC, Apple, NVDA…", text: $query)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        ForEach(results) { item in
                            Button {
                                asset = item.asset
                                query = ""
                            } label: {
                                HStack {
                                    Text(item.symbol).font(Theme.mono(14, weight: .bold)).foregroundStyle(.white)
                                    Text(item.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                                    Spacer()
                                    Badge(text: item.kind.label, tone: .neutral)
                                }
                            }
                        }
                    }
                }
                Section {
                    TextField("Quantité (ex. 0,25)", text: $quantity).keyboardType(.decimalPad)
                    TextField(current.map { "Prix moyen d'achat (actuel \(Format.price($0)))" } ?? "Prix moyen d'achat en $ (facultatif)", text: $average)
                        .keyboardType(.decimalPad)
                } footer: {
                    Text("Le prix moyen d'achat sert seulement à calculer votre gain ou perte. Rien n'est envoyé au serveur.")
                }
            }
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle(holding == nil ? "Ajouter un avoir" : "Modifier")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Enregistrer", action: save).disabled(!valid) }
            }
            .task(id: query) { await search() }
            .task(id: asset?.id) { await quote() }
            .onAppear {
                guard let holding, asset == nil else { return }
                asset = holding.asset
                quantity = Format.quantity(holding.quantity)
                average = holding.averagePrice.map { Format.quantity($0) } ?? ""
            }
        }
        .presentationDetents([.large])
    }

    private static func number(_ s: String) -> Double? {
        let t = s.replacingOccurrences(of: "\u{202F}", with: "").replacingOccurrences(of: " ", with: "").replacingOccurrences(of: ",", with: ".")
        guard let v = Double(t), v.isFinite else { return nil }
        return v
    }

    private var valid: Bool {
        guard asset != nil, let q = Self.number(quantity), q > 0 else { return false }
        return average.isEmpty || (Self.number(average) ?? -1) > 0
    }

    private func save() {
        guard let asset, let q = Self.number(quantity) else { return }
        let avg = average.isEmpty ? nil : Self.number(average)
        if let holding, let i = model.holdings.firstIndex(where: { $0.id == holding.id }) {
            model.holdings[i] = Holding(id: holding.id, asset: asset, quantity: q, averagePrice: avg)
        } else {
            model.holdings.append(Holding(asset: asset, quantity: q, averagePrice: avg))
        }
        dismiss()
    }

    private func search() async {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty, let client = model.client else { return results = [] }
        try? await Task.sleep(nanoseconds: 250_000_000)
        guard !Task.isCancelled else { return }
        results = (try? await client.search(q, limit: 12)) ?? []
    }

    private func quote() async {
        guard let asset, let client = model.client else { return current = nil }
        current = (try? await client.quotes([asset]))?.first?.price
    }
}
