import SwiftUI
import AltimKit

/// Opportunités du moment (Sélection → Opportunités): the selection's universe scanned by category, with the measured
/// reason for each asset; market and filters kept on the iPhone. Same content as the web (Opportunities.tsx). Chips
/// wrap, filters are menus, results are stacked cards: nothing scrolls sideways.
struct OpportunitiesView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("opportunities.v1") private var savedRaw = ""
    @State private var report: OpportunityReport?
    @State private var error: String?
    @State private var pending = false

    private var saved: OppSaved { Opportunities.parseSaved(savedRaw.isEmpty ? nil : Data(savedRaw.utf8)) }

    private func update(_ change: (inout OppSaved) -> Void) {
        var s = saved
        change(&s)
        if let data = try? JSONEncoder().encode(s), let text = String(data: data, encoding: .utf8) { savedRaw = text }
    }

    var body: some View {
        let s = saved
        let market = s.market
        let filters = s.filters
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Ce qui bouge de façon notable aujourd'hui, catégorie par catégorie, avec la raison mesurée. Des pistes à examiner, pas des ordres d'achat.")
                    .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                FittingPicker(title: "Marché", selection: Binding(get: { market }, set: { m in update { $0.market = m } })) {
                    Text("Actions").tag(Kind.stock)
                    Text("Cryptos").tag(Kind.crypto)
                }
                filtersCard(market: market, filters: filters)
                if let error { Notice(text: error, tone: .warn) }
                if report == nil && error == nil {
                    Card {
                        Text(pending ? Opportunities.pendingText(market) : "Chargement…").font(.footnote).foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                        ProgressView().frame(maxWidth: .infinity)
                    }
                }
                if let report, report.kind == market { results(report, market: market, filters: filters) }
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle("Opportunités du moment")
        .navigationBarTitleDisplayMode(.inline)
        .task(id: market) { await load(market) }
        .refreshable { await load(market) }
    }

    // MARK: Filters

    private func filtersCard(market: Kind, filters: OppFilters) -> some View {
        let counts = report.map { Opportunities.countByCategory($0.items, filters) }
        let limited = report?.categories.filter { filters.categories.contains($0.id) && ($0.note != nil || $0.error != nil) } ?? []
        return Card(title: "Catégories") {
            WrapLayout(spacing: 6) {
                ForEach(OppCategory.all, id: \.self) { c in
                    let on = filters.categories.contains(c)
                    Button {
                        update { s in s.filters.categories = on ? s.filters.categories.filter { $0 != c } : s.filters.categories + [c] }
                    } label: {
                        TagChip(text: "\(c.short)\(counts.map { " · \($0[c] ?? 0)" } ?? "")", color: .white, selected: on)
                    }
                    .buttonStyle(.borderless)
                    .accessibilityAddTraits(on ? .isSelected : [])
                    .accessibilityHint(report?.categories.first { $0.id == c }?.rule ?? "")
                }
            }
            if market == .stock {
                menu("Capitalisation", Opportunities.caps.map(\.label), index: Opportunities.caps.firstIndex { $0.value == filters.minCap } ?? 0) { i in
                    update { $0.filters.minCap = Opportunities.caps[i].value }
                }
            } else {
                menu("Rang (capitalisation)", Opportunities.ranks.map(\.label), index: Opportunities.ranks.firstIndex { $0.value == filters.maxRank } ?? 0) { i in
                    update { $0.filters.maxRank = Opportunities.ranks[i].value }
                }
            }
            menu("Liquidité (volume échangé)", Opportunities.liquidity.map(\.label), index: Opportunities.liquidity.firstIndex { $0.value == filters.minLiquidity } ?? 0) { i in
                update { $0.filters.minLiquidity = Opportunities.liquidity[i].value }
            }
            menu("Volatilité max (ATR)", Opportunities.volatility.map(\.label), index: Opportunities.volatility.firstIndex { $0.value == filters.maxVolatility } ?? 0) { i in
                update { $0.filters.maxVolatility = Opportunities.volatility[i].value }
            }
            ForEach(limited) { c in
                if let e = c.error {
                    Notice(text: "\(c.id.short) : \(e)", tone: .warn)
                } else {
                    (Text(c.id.short).bold() + Text(" : \(c.note ?? "")")).font(.caption).foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func menu(_ label: String, _ options: [String], index: Int, _ set: @escaping (Int) -> Void) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack {
                Text(label).font(.subheadline).foregroundStyle(Theme.textSecondary)
                Spacer(minLength: 8)
                picker(label, options, index: index, set)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(label).font(.subheadline).foregroundStyle(Theme.textSecondary)
                picker(label, options, index: index, set)
            }
        }
    }

    private func picker(_ label: String, _ options: [String], index: Int, _ set: @escaping (Int) -> Void) -> some View {
        Picker(label, selection: Binding(get: { index }, set: set)) {
            ForEach(options.indices, id: \.self) { Text(options[$0]).tag($0) }
        }
        .pickerStyle(.menu)
        .tint(Theme.cyan)
    }

    // MARK: Results

    @ViewBuilder private func results(_ report: OpportunityReport, market: Kind, filters: OppFilters) -> some View {
        let shown = Opportunities.filter(report.items, filters)
        Text("Résultats · \(shown.count) sur \(report.scanned) analysé\(report.scanned > 1 ? "s" : "")").font(.headline)
        if shown.isEmpty {
            Text("Aucun actif ne remplit ces critères en ce moment.").font(.footnote).foregroundStyle(Theme.textSecondary)
        }
        ForEach(shown) { i in itemCard(i, market: market) }
        Card {
            DisclosureGroup("Règles, sources et limites") {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(report.categories) { c in
                        (Text(c.label).bold() + Text(" (\(c.analyzed) analysé\(c.analyzed > 1 ? "s" : "")) : \(c.rule)")
                         + Text(c.note.map { " \($0)" } ?? "").foregroundStyle(Theme.textSecondary)
                         + Text(c.error.map { " \($0)" } ?? "").foregroundStyle(Theme.sell))
                            .font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(report.notCovered, id: \.label) { n in
                        (Text("Non couvert — \(n.label)").bold() + Text(" : \(n.reason).")).font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    Text("\(report.universe). Sources : \(report.source). Séances closes uniquement.").font(.caption).foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.top, 6)
            }
            .font(.footnote)
            .tint(.white)
        }
        Text("Scan calculé à \(time(report.asOf)). Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.")
            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    private func time(_ ms: Double) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "HH:mm:ss"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }

    private func itemCard(_ i: OppItem, market: Kind) -> some View {
        let asset = Asset(symbol: i.symbol, kind: market, name: i.name)
        return Card {
            NavigationLink(value: asset) {
                HStack(alignment: .top, spacing: 8) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(i.name).font(.subheadline.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                        Text("\(i.symbol) · \(market == .crypto ? (i.rank.map { "rang \($0)" } ?? "crypto") : i.sector)")
                            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: 6)
                    VStack(alignment: .trailing, spacing: 2) {
                        Text(Format.price(i.price)).font(Theme.mono(14, weight: .bold)).foregroundStyle(.white)
                        if let c = i.change1d {
                            Text(Opportunities.signed(c)).font(Theme.mono(12)).foregroundStyle(c >= 0 ? Theme.buy : Theme.sell)
                        }
                    }
                }
            }
            .buttonStyle(.plain)
            ForEach(i.hits, id: \.category) { h in
                VStack(alignment: .leading, spacing: 3) {
                    TagChip(text: h.category.short, color: Theme.cyan)
                    Text(h.reason).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
            }
            let metrics = Opportunities.metrics(i)
            if !metrics.isEmpty {
                Text(metrics.joined(separator: " · ")).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            NavigationLink(value: asset) {
                Text("Voir la fiche").font(.footnote.weight(.semibold)).foregroundStyle(Theme.cyan)
            }
            .buttonStyle(.plain)
        }
    }

    // MARK: Loading

    /// The first scan takes ≈ 30 s: the server answers "pending" meanwhile, asked again every 5 seconds.
    private func load(_ market: Kind) async {
        guard let client = model.client else { return }
        if report?.kind != market { report = nil }
        error = nil
        while !Task.isCancelled {
            do {
                switch try await client.opportunities(market) {
                case .pending:
                    pending = true
                    try? await Task.sleep(for: .seconds(5))
                    continue
                case let .ready(r):
                    pending = false
                    report = r
                    model.persistSession()
                }
            } catch AltimError.unauthorized {
                model.sessionLost()
            } catch is CancellationError {
            } catch {
                self.error = error.localizedDescription
            }
            return
        }
    }
}

/// "Opportunités du moment →" at the top of Sélection.
struct OpportunitiesLink: View {
    var body: some View {
        NavigationLink {
            OpportunitiesView()
        } label: {
            (Text("Opportunités du moment →").bold().foregroundStyle(.white)
             + Text(" retournements, cassures, volumes anormaux, survendus, fondamentaux qui évoluent.").foregroundStyle(Theme.textSecondary))
                .font(.footnote)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.cyan.opacity(0.08)))
                .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Theme.cyan.opacity(0.4), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }
}
