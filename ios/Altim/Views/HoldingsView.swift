import SwiftUI
import AltimKit

/// Portfolio: value at live prices, gain / loss, concentration, and the daily signal of each line.
/// The lines stay on this iPhone; only the symbols are sent to the server to get prices and signals.
struct HoldingsView: View {
    @Environment(AppModel.self) private var model
    @State private var quotes: [String: Double] = [:]
    @State private var signals: [String: RadarRow] = [:]
    /// Daily candles of the held assets and of the benchmarks (Bitcoin, S&P 500 via SPY): betas, clusters, ATR, today.
    @State private var daily: [String: [Candle]] = [:]
    /// The market data (quotes and candles) was read at least once: the dangers can be measured and kept.
    @State private var marketLoaded = false
    @State private var editing: Holding?
    @State private var adding = false
    @State private var error: String?
    /// 0 = real holdings, 1 = simulation (paper trading), 2 = journal.
    @State private var mode = 0

    var body: some View {
        Group {
            if mode == 1 {
                PaperView(mode: $mode)
            } else if mode == 2 {
                JournalView(mode: $mode)
            } else {
                realList
            }
        }
        .altimScreen()
        .navigationTitle("Mes avoirs")
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .toolbar {
            if mode == 0 {
                Button { adding = true } label: { Image(systemName: "plus") }.accessibilityLabel("Ajouter un avoir")
            }
        }
        .sheet(isPresented: $adding) { HoldingForm(holding: nil) }
        .sheet(item: $editing) { HoldingForm(holding: $0) }
    }

    /// Risk beyond the summary: stress scenarios through the betas, limits of the settings, dangerous positions.
    private struct RiskView {
        var portfolio: RiskPortfolio
        var betas: [String: BetaEstimate]
        var stress: [StressResult]
        var limits: [LimitCheck]
        var dangers: [Danger]
        var dailyLossReached: Bool { limits.contains { $0.code == "daily_loss" && $0.level == .danger } }
    }

    private func riskView(_ prices: [String: Double]) -> RiskView? {
        guard marketLoaded, !model.holdings.isEmpty else { return nil }
        let p = RiskPortfolio(holdings: model.holdings, prices: prices, daily: daily)
        let betas = RiskEngine.betas(p, daily: daily)
        let today = RiskEngine.dailyChange(p, daily: daily, now: Date().timeIntervalSince1970 * 1000)
        return RiskView(portfolio: p, betas: betas, stress: RiskEngine.stressTest(p, betas: betas),
                        limits: RiskEngine.checkLimits(p, model.risk, clusters: RiskEngine.clusters(p, daily: daily), daily: today),
                        dangers: RiskEngine.dangerousPositions(p, model.risk, daily: daily))
    }

    private var realList: some View {
        let prices = self.prices
        let portfolio = Portfolio(holdings: model.holdings, prices: prices)
        let risk = riskView(prices)
        let dangerById = Dictionary((risk?.dangers ?? []).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return List {
            Section { PortfolioModePicker(mode: $mode) }.listRowBackground(Color.clear)
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
                if let risk, risk.dailyLossReached {
                    Section { Notice(text: RiskEngine.dailyLossReached, tone: .bad) }.listRowBackground(Color.clear)
                }
                if let risk, !risk.dangers.isEmpty {
                    Section { DangerNotice(dangers: risk.dangers) }.listRowBackground(Color.clear)
                }
                Section { HistoryCard() }.listRowBackground(Color.clear)
                Section { RebalanceCard(portfolio: portfolio) }.listRowBackground(Color.clear)
                Section { SaleCard(portfolio: portfolio) }.listRowBackground(Color.clear)
                Section { ProjectionCard(start: portfolio.total) }.listRowBackground(Color.clear)
                Section {
                    ForEach(portfolio.lines) { line in
                        NavigationLink(value: line.holding.asset) { lineView(line, danger: dangerById[line.id.uuidString]) }
                            .swipeActions(edge: .trailing) {
                                Button(role: .destructive) { model.holdings.removeAll { $0.id == line.id } } label: { Label("Supprimer", systemImage: "trash") }
                                Button { editing = line.holding } label: { Label("Modifier", systemImage: "pencil") }.tint(Theme.violet)
                            }
                            .listRowBackground(Theme.surface.opacity(0.6))
                    }
                } header: {
                    HStack { Text("Lignes · signal 1 jour"); Spacer(); LiveBadge() }
                } footer: {
                    Text("Glissez une ligne vers la gauche pour la modifier ou la supprimer (et saisir votre stop). Vos avoirs restent sur cet iPhone.")
                }
                if let risk {
                    Section { LimitsCard(checks: risk.limits) }.listRowBackground(Color.clear)
                    Section { StressCard(portfolio: risk.portfolio, results: risk.stress, betas: risk.betas) }.listRowBackground(Color.clear)
                    Section { WhatIfCard(portfolio: risk.portfolio, daily: daily) }.listRowBackground(Color.clear)
                    Section { SectorCard(lines: risk.portfolio.lines.map { Sectors.Line(symbol: $0.symbol, kind: $0.kind, value: $0.value) }) }
                        .listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.insetGrouped)
        .onChange(of: dangerKey(risk?.dangers), initial: true) { _, _ in saveDangers(risk?.dangers) }
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

    /// Kept for the Radar only once the market data is read (the ids and reasons, not the prices, decide a change).
    private func dangerKey(_ d: [Danger]?) -> String {
        guard let d else { return model.holdings.isEmpty ? "empty" : "" }
        return d.map { "\($0.id):\($0.reasons.map(\.code.rawValue).joined(separator: "+"))" }.joined(separator: ",")
    }

    private func saveDangers(_ d: [Danger]?) {
        if model.holdings.isEmpty {
            if model.dangers?.items.isEmpty == false { model.dangers = DangerState(at: Date().timeIntervalSince1970 * 1000, items: []) }
        } else if let d {
            model.dangers = DangerState(at: Date().timeIntervalSince1970 * 1000, items: d)
        }
    }

    private func lineView(_ line: PortfolioLine, danger: Danger?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            lineRow(line)
            if let danger {
                VStack(alignment: .leading, spacing: 3) {
                    Label("Position devenue dangereuse", systemImage: "exclamationmark.triangle.fill")
                        .font(.caption.weight(.semibold)).foregroundStyle(Theme.sell)
                    ForEach(danger.reasons, id: \.code) { r in
                        Text("• \(r.text)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            if let stop = line.holding.stop {
                Text(RiskText.userStop(stop, price: line.price)).font(.caption).foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func lineRow(_ line: PortfolioLine) -> some View {
        let h = line.holding
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(h.asset.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                Text("\(Format.quantity(h.quantity)) · \(Format.price(line.price))").font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                // The daily technical signal as a direction only: the verdict is the asset's Décision card.
                if let s = signals[h.asset.id]?.signal { TechnicalLine(action: s.action, interval: "1 j") }
            }
            Spacer(minLength: 4)
            VStack(alignment: .trailing, spacing: 3) {
                Text(line.value.map { Format.money($0) } ?? "—").font(Theme.mono(14)).foregroundStyle(.white)
                if let gp = line.gainPercent { ChangeText(value: gp) } else if let w = line.weight { Text("\(Int(w.rounded())) %").font(.caption).foregroundStyle(Theme.textSecondary) }
            }
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
        await loadDaily(client, assets: assets)
    }

    /// Daily candles of each held asset and of the benchmarks not held (a failed one is simply missing: beta 1 said).
    private func loadDaily(_ client: AltimClient, assets: [Asset]) async {
        var all = assets
        for kind in Set(assets.map(\.kind)) {
            let ref = RiskEngine.benchmark(kind).asset
            if !all.contains(where: { $0.id == ref.id }) { all.append(ref) }
        }
        let fetched = await withTaskGroup(of: (String, [Candle]?).self) { group in
            for a in all { group.addTask { (a.id, try? await client.candles(a, interval: "1d").candles) } }
            var out: [String: [Candle]] = [:]
            for await (id, c) in group { if let c, !c.isEmpty { out[id] = c } }
            return out
        }
        guard !Task.isCancelled else { return }
        daily = fetched
        marketLoaded = !quotes.isEmpty || !fetched.isEmpty
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
    @State private var stopText = ""
    @State private var current: Double?
    /// Inscribe the purchase or sale in the trading journal (a first entry of holdings is usually past purchases:
    /// not journaled unless asked; later additions and edits are).
    @State private var journal = true
    @State private var note = ""

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
                Section {
                    TextField("Mon stop en $ (facultatif)", text: $stopText).keyboardType(.decimalPad)
                } footer: {
                    Text("Prix auquel vous comptez vendre pour limiter la perte. Altim vous alerte quand le cours s'en approche (moins d'une volatilité journalière) ou le casse. Aucun ordre n'est passé.")
                }
                if let text = journalText {
                    Section {
                        Toggle(isOn: $journal) { Text(text).font(.footnote).fixedSize(horizontal: false, vertical: true) }.tint(Theme.cyan)
                        if journal { JournalNoteField(text: $note) }
                    }
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
                if holding == nil && asset == nil && quantity.isEmpty { journal = !model.holdings.isEmpty }
                guard let holding, asset == nil else { return }
                asset = holding.asset
                quantity = Format.quantity(holding.quantity)
                average = holding.averagePrice.map { Format.quantity($0) } ?? ""
                stopText = holding.stop.map { Format.quantity($0) } ?? ""
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
        guard stopText.trimmingCharacters(in: .whitespaces).isEmpty || (Self.number(stopText) ?? -1) > 0 else { return false }
        return average.isEmpty || (Self.number(average) ?? -1) > 0
    }

    /// Purchase or sale this edit records (nil: nothing to write in the journal).
    private var change: TradeJournal.HoldingChange? {
        guard valid, let asset, let q = Self.number(quantity) else { return nil }
        let avg = average.isEmpty ? nil : Self.number(average)
        let live = current ?? model.live.price(asset)?.price
        guard let holding else { return (avg ?? live).map { TradeJournal.HoldingChange(side: .buy, quantity: q, price: $0, implied: false) } }
        return TradeJournal.holdingChange(before: (holding.quantity, holding.averagePrice), after: (q, avg), livePrice: live)
    }

    private var journalText: String? {
        guard let c = change, let asset else { return nil }
        if holding == nil { return "Achats faits aujourd'hui : les inscrire au journal" }
        return "\(c.side == .buy ? "Achat" : "Vente") de \(Format.quantity(c.quantity)) \(asset.symbol) à \(Format.price(c.price))\(c.implied ? " (déduit du nouveau PRU)" : " (cours actuel)") : l'inscrire au journal"
    }

    private func save() {
        guard let asset, let q = Self.number(quantity) else { return }
        let avg = average.isEmpty ? nil : Self.number(average)
        let stop = stopText.trimmingCharacters(in: .whitespaces).isEmpty ? nil : Self.number(stopText)
        let change = self.change
        if let holding, let i = model.holdings.firstIndex(where: { $0.id == holding.id }) {
            model.holdings[i] = Holding(id: holding.id, asset: asset, quantity: q, averagePrice: avg, stop: stop)
            if journal, let c = change {
                model.recordTrade(source: .real, side: c.side, asset: asset, price: c.price, quantity: c.quantity, stop: c.side == .buy ? stop : nil,
                                  note: note, refId: holding.id.uuidString)
            }
        } else {
            let line = Holding(asset: asset, quantity: q, averagePrice: avg, stop: stop)
            model.holdings.append(line)
            if journal, let c = change {
                model.recordTrade(source: .real, side: .buy, asset: asset, price: c.price, quantity: c.quantity, stop: stop, note: note, refId: line.id.uuidString)
            }
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
