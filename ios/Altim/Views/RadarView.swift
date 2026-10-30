import SwiftUI
import AltimKit

/// Sort of the Radar (web: "Mon ordre", "Variation", "Décision").
enum RadarSort: String, CaseIterable {
    case mine, change, decision

    var label: String {
        switch self {
        case .mine: return "Mon ordre"
        case .change: return "Variation"
        case .decision: return "Décision"
        }
    }
}

/// Watch list: live price, full decision (the technical signal of the chosen timeframe only as a direction),
/// reliability of the data, macro context.
struct RadarView: View {
    @Environment(AppModel.self) private var model
    @State private var rows: [String: RadarRow] = [:]
    @State private var macro: MacroInfo?
    @State private var error: String?
    @State private var loading = false
    @State private var query = ""
    @State private var results: [SearchItem] = []
    @AppStorage("radar.sort") private var sortRaw = RadarSort.mine.rawValue
    /// Timeframe of the technical line, shared with the asset page (which also offers 1 h).
    @AppStorage(Timeframe.storageKey) private var intervalRaw = Timeframe.standard.rawValue

    private var timeframe: Timeframe { Timeframe.radar(intervalRaw) }
    private var timeframeBinding: Binding<Timeframe> {
        Binding(get: { timeframe }, set: { intervalRaw = $0.rawValue })
    }

    private var sort: RadarSort { RadarSort(rawValue: sortRaw) ?? .mine }
    private var nowMs: Double { Date().timeIntervalSince1970 * 1000 }

    /// "Décision": the full decision's rating (buy side first), then its confidence; "Variation": the largest move.
    private var sortedWatchlist: [Asset] {
        switch sort {
        case .mine:
            return model.watchlist
        case .change:
            return DecisionDigests.sortByChange(model.watchlist) { a in model.live.price(a)?.change ?? rows[a.id]?.change }
        case .decision:
            return DecisionDigests.sortByDecision(model.watchlist, model.decisionDigests, now: nowMs)
        }
    }

    private func moveRows(_ from: IndexSet, _ to: Int) { model.watchlist.move(fromOffsets: from, toOffset: to) }

    var body: some View {
        List {
            if !query.isEmpty {
                searchSection
            } else {
                if !model.isStandalone {
                    Section { BriefCard() }.listRowBackground(Color.clear)
                    Section { BotRadarCard() }.listRowBackground(Color.clear)
                }
                if model.watchlist.count >= 3 {
                    Section { MarketHeatmap(assets: model.watchlist, rows: rows) }.listRowBackground(Color.clear)
                }
                if let macro, macro.level != "calm" {
                    Section { MacroBanner(macro: macro) }.listRowBackground(Color.clear)
                }
                if let error { Section { ErrorView(message: error) { Task { await load() } } }.listRowBackground(Color.clear) }
                if let d = model.dangers, !d.items.isEmpty {
                    Section { DangerNotice(dangers: d.items, measuredAt: d.at) }.listRowBackground(Color.clear)
                }
                if !model.configChanges.transitions.isEmpty {
                    Section { ConfigChangesCard() }.listRowBackground(Color.clear)
                }
                let opportunities = DecisionDigests.opportunities(model.watchlist, model.decisionDigests, now: nowMs)
                if !opportunities.isEmpty {
                    Section {
                        ForEach(opportunities) { o in
                            NavigationLink(value: o.asset) {
                                HStack(spacing: 8) {
                                    Text(o.asset.name).font(.subheadline.bold()).foregroundStyle(.white).lineLimit(1)
                                    Spacer(minLength: 4)
                                    Text("confiance \(Int(o.decision.confidence.rounded()))").font(.caption).foregroundStyle(Theme.textSecondary)
                                    DecisionBadge(asset: o.asset)
                                }
                            }
                            .listRowBackground(Theme.surface.opacity(0.6))
                        }
                    } header: {
                        Text("Opportunités détectées")
                    }
                }
                Section {
                    FittingPicker(title: "Unité de temps de l'analyse technique", selection: timeframeBinding) {
                        ForEach(Timeframe.radarCases) { Text($0.label).tag($0) }
                    }
                }
                .listRowBackground(Color.clear)
                if model.watchlist.count > 1 {
                    Section {
                        FittingPicker(title: "Trier le radar", selection: $sortRaw) {
                            ForEach(RadarSort.allCases, id: \.rawValue) { Text($0.label).tag($0.rawValue) }
                        }
                    }
                    .listRowBackground(Color.clear)
                }
                Section {
                    let shown = sortedWatchlist
                    ForEach(shown) { asset in
                        NavigationLink(value: asset) { RadarRowView(asset: asset, row: rows[asset.id], timeframe: timeframe) }
                            .listRowBackground(Theme.surface.opacity(0.6))
                    }
                    .onDelete { offsets in
                        let ids = Set(offsets.map { shown[$0].id })
                        model.watchlist.removeAll { ids.contains($0.id) }
                    }
                    // Reordering only in "Mon ordre" (another sort would move the rows back).
                    .onMove(perform: sort == .mine ? moveRows : nil)
                } header: {
                    HStack {
                        Text(model.isStandalone ? "Verdict Altim · prix médian des sources" : "Décision · prix médian de 40 sources")
                        Spacer()
                        LiveBadge()
                    }
                } footer: {
                    Text(model.isStandalone
                         ? "Verdict Altim, calculé sur cet iPhone en données journalières : ACHAT quand la tendance de fond est haussière et le momentum positif, ACHAT FORT sur un repli dans cette tendance, VENTE quand les deux sont baissiers, ATTENDRE sinon ou si les sources ne s'accordent pas. L'unité choisie ci-dessus ne change que la courbe. Une règle testée sur le passé, jamais une certitude. Glissez vers la gauche pour retirer un actif."
                         : "Le verdict est la décision complète de l'actif (la même que sur sa page), relue toutes les 15 minutes ; la tendance technique \(timeframe.label) (unité choisie ci-dessus) n'en est qu'un indice. Une probabilité mesurée sur l'historique, jamais une certitude. Glissez vers la gauche pour retirer un actif.")
                }
                if model.watchlist.count >= 2 {
                    Section { CompareCard() }.listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.insetGrouped)
        .altimScreen()
        .navigationTitle("Radar")
        // Back from Réglages: the screens follow the currency chosen there.
        .onAppear { model.syncMoneyStamp() }
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .searchable(text: $query, prompt: "Ajouter : BTC, Apple, NVDA…")
        .task(id: query) { await search() }
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                NavigationLink { SettingsView() } label: { Image(systemName: "gearshape") }
                    .accessibilityLabel("Réglages")
            }
            ToolbarItem(placement: .topBarTrailing) { EditButton() }
        }
        .refreshable { await load() }
        .task(id: model.watchlist.map(\.id).joined() + "|" + timeframe.rawValue) { await load() }
        .task(id: model.watchlist.map(\.id).joined(separator: ",")) {
            // Decisions of the watched assets (market data only), re-read every 15 minutes while the radar is on screen:
            // each one goes through the configuration diff.
            while !Task.isCancelled {
                await checkDecisions()
                try? await Task.sleep(for: .seconds(Self.decisionEvery))
            }
        }
        .overlay { if loading && rows.isEmpty { ProgressView("Analyse des marchés…") } }
    }

    @ViewBuilder private var searchSection: some View {
        Section("Résultats") {
            if results.isEmpty { Text("Aucun résultat").foregroundStyle(Theme.textSecondary) }
            ForEach(results) { item in
                Button {
                    model.watch(item.asset)
                    query = ""
                } label: {
                    HStack {
                        VStack(alignment: .leading) {
                            Text(item.symbol).font(Theme.mono(15)).foregroundStyle(.white)
                            Text(item.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                        }
                        Spacer()
                        Badge(text: item.etf == true ? "ETF" : item.kind.label, tone: .neutral)
                        Image(systemName: model.isWatched(item.asset) ? "checkmark.circle.fill" : "plus.circle")
                            .foregroundStyle(Theme.cyan)
                    }
                }
            }
        }
    }

    private func search() async {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty, let client = model.client else { return results = [] }
        try? await Task.sleep(nanoseconds: 250_000_000)
        guard !Task.isCancelled else { return }
        results = (try? await client.search(q)) ?? []
    }

    /// The decisions of the radar are re-read at most this often (they are heavier than the signals).
    static let decisionEvery: Double = 15 * 60

    /// Two at a time (the server fetches fundamentals and order books for each one), 20 assets at most; an asset
    /// compared less than 15 minutes ago (here or on its page) waits. A failed one is compared at the next round.
    private func checkDecisions() async {
        guard let client = model.client, !client.isStandalone else { return }
        let due = model.watchlist.prefix(20).filter { a in
            model.lastDecisionCheck(a).map { Date().timeIntervalSince($0) >= Self.decisionEvery } ?? true
        }
        var i = 0
        while i < due.count && !Task.isCancelled {
            let pair = Array(due[i..<min(i + 2, due.count)])
            let got = await withTaskGroup(of: Decision?.self) { group in
                for a in pair { group.addTask { try? await client.decision(asset: a) } }
                var out: [Decision] = []
                for await d in group { if let d { out.append(d) } }
                return out
            }
            guard !Task.isCancelled else { return }
            for d in got { model.recordDecision(d, personal: false) }
            i += 2
        }
    }

    private func load() async {
        guard let client = model.client, !model.watchlist.isEmpty else { return }
        loading = true
        defer { loading = false }
        async let m = try? client.macro()
        do {
            let r = try await client.radar(model.watchlist, interval: timeframe.rawValue)
            rows = Dictionary(r.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            error = nil
            model.persistSession()
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch {
            self.error = error.localizedDescription
        }
        macro = await m
    }
}

struct RadarRowView: View {
    @Environment(AppModel.self) private var model
    var asset: Asset
    var row: RadarRow?
    var timeframe: Timeframe = .standard

    var body: some View {
        let tick = model.live.price(asset)
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(asset.symbol).font(Theme.mono(16, weight: .bold)).foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.7)
                    Text(asset.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                }
                .frame(minWidth: 70, alignment: .leading)
                if let spark = row?.sparkline, spark.count > 2 {
                    Sparkline(values: spark).frame(width: 56, height: 26)
                }
                Spacer(minLength: 4)
                VStack(alignment: .trailing, spacing: 3) {
                    // A long price shrinks a little rather than pushing the row past the screen (320 pt).
                    Text(Format.price(tick?.price ?? row?.price)).font(Theme.mono(14))
                        .lineLimit(1).minimumScaleFactor(0.6)
                        .liveTick(tick?.price ?? row?.price)
                    ChangeText(value: tick?.change ?? row?.change)
                }
                .layoutPriority(1)
            }
            // The chips side by side when they fit, else one under the other (iPhone SE, large Dynamic Type).
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 8) {
                    verdict
                    technical
                    Spacer(minLength: 0)
                    reliability
                }
                VStack(alignment: .leading, spacing: 4) {
                    verdict
                    technical
                    reliability
                }
            }
            // Why the chip says ATTENDRE: the decision's short reason, on its own line (web DecisionNote).
            DecisionNoteLine(asset: asset)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    /// The verdict: the full decision, never the technical signal alone. Without a server, the signal computed on the
    /// iPhone (already suspended to ATTENDRE on doubtful data) is the verdict, said as such.
    @ViewBuilder private var verdict: some View {
        if model.isStandalone {
            if let s = row?.signal {
                Badge(text: s.action.label, tone: s.action.tone)
                    .accessibilityLabel("Signal \(timeframe.label) : \(s.action.label)")
            } else if row == nil {
                Badge(text: "ANALYSE…", tone: .neutral).opacity(0.7)
            }
        } else {
            DecisionBadge(asset: asset)
        }
    }

    @ViewBuilder private var technical: some View {
        if model.isStandalone {
            if let s = row?.signal {
                Text("confiance \(Int(s.confidence.rounded()))").font(.caption2).foregroundStyle(Theme.textSecondary).lineLimit(1)
            } else if row?.error != nil {
                Text("données indisponibles").font(.caption2).foregroundStyle(Theme.textSecondary).lineLimit(1)
            }
        } else if let s = row?.signal {
            TechnicalLine(action: s.action, interval: timeframe.label)
        } else if row?.error != nil {
            Text("signal technique indisponible").font(.caption2).foregroundStyle(Theme.textSecondary).lineLimit(1)
        }
    }

    @ViewBuilder private var reliability: some View {
        if let r = row?.reliability, r.level != "high" { Badge(text: r.level == "medium" ? "FIAB. MOY." : "FIAB. FAIBLE", tone: r.tone) }
    }
}

struct MacroBanner: View {
    var macro: MacroInfo

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Contexte macro : \(macro.levelLabel)").font(.subheadline.bold())
                Spacer()
                Badge(text: "\(Int(macro.score))/100", tone: macro.tone)
            }
            if let r = macro.regime { RegimeLine(regime: r) }
            ForEach(macro.factors.prefix(3)) { f in
                Text("• \(f.text)").font(.caption).foregroundStyle(.white.opacity(0.85))
            }
            Text("Une zone d'achat peut céder si la situation mondiale se dégrade (guerre, crise, taux) : tailles réduites conseillées.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 14).fill(Theme.color(macro.tone).opacity(0.1)))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Theme.color(macro.tone).opacity(0.5)))
    }
}

/// « Carte de chaleur » : every watched asset as a tile coloured by its 24 h move (live prices), brighter the bigger the
/// move, its signal in the corner. A tap opens the asset. Tiles wrap (2 to 4 per line), nothing scrolls sideways.
struct MarketHeatmap: View {
    @Environment(AppModel.self) private var model
    var assets: [Asset]
    var rows: [String: RadarRow]

    var body: some View {
        let changes = assets.map { model.live.price($0)?.change ?? rows[$0.id]?.change }
        let scale = max(2, changes.compactMap { $0.map(abs) }.max() ?? 2)
        let up = changes.compactMap { $0 }.filter { $0 > 0 }.count
        let down = changes.compactMap { $0 }.filter { $0 < 0 }.count
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Carte de chaleur").font(.headline)
                Spacer()
                Text("\(up) ▲ · \(down) ▼").font(Theme.mono(12)).foregroundStyle(Theme.textSecondary)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 92), spacing: 8)], spacing: 8) {
                ForEach(Array(assets.enumerated()), id: \.element.id) { i, a in
                    NavigationLink(value: a) { tile(a, change: changes[i], scale: scale) }
                        .buttonStyle(.plain)
                }
            }
        }
        .padding(12)
        .glassCard()
    }

    private func tile(_ a: Asset, change: Double?, scale: Double) -> some View {
        let c = change ?? 0
        let strength = change == nil ? 0.05 : 0.15 + 0.55 * min(1, abs(c) / scale)
        let color = change == nil ? Color.white : c >= 0 ? Theme.buy : Theme.sell
        return VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 4) {
                Text(a.symbol).font(Theme.mono(14, weight: .bold)).foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.6)
                Spacer(minLength: 0)
                if let s = rows[a.id]?.signal, s.action != .hold {
                    Circle().fill(Theme.color(s.action.tone)).frame(width: 7, height: 7)
                        .accessibilityLabel(s.action.label)
                }
            }
            Text(Format.percent(change))
                .font(Theme.mono(13)).foregroundStyle(.white.opacity(0.9))
                .contentTransition(.numericText(value: c))
                .animation(.snappy, value: c)
        }
        .padding(10)
        .frame(maxWidth: .infinity, minHeight: 58, alignment: .topLeading)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(color.opacity(strength)))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(color.opacity(strength + 0.15), lineWidth: 1))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(a.name), \(change.map { Format.percent($0) } ?? "variation inconnue")")
    }
}
