import SwiftUI
import AltimKit

/// Watch list: live price, signal, reliability of the data, macro context.
struct RadarView: View {
    @Environment(AppModel.self) private var model
    @State private var rows: [String: RadarRow] = [:]
    @State private var macro: MacroInfo?
    @State private var error: String?
    @State private var loading = false
    @State private var query = ""
    @State private var results: [SearchItem] = []

    var body: some View {
        List {
            if !query.isEmpty {
                searchSection
            } else {
                Section { BriefCard() }.listRowBackground(Color.clear)
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
                Section {
                    ForEach(model.watchlist) { asset in
                        NavigationLink(value: asset) { RadarRowView(asset: asset, row: rows[asset.id]) }
                            .listRowBackground(Theme.surface.opacity(0.6))
                    }
                    .onDelete { model.watchlist.remove(atOffsets: $0) }
                    .onMove { model.watchlist.move(fromOffsets: $0, toOffset: $1) }
                } header: {
                    HStack {
                        Text("Signal 4 h · prix médian de 40 sources")
                        Spacer()
                        LiveBadge()
                    }
                } footer: {
                    Text("Le signal est une probabilité mesurée sur l'historique, jamais une certitude. Glissez vers la gauche pour retirer un actif.")
                }
                if model.watchlist.count >= 2 {
                    Section { CompareCard() }.listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.insetGrouped)
        .altimScreen()
        .navigationTitle("Radar")
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
        .task(id: model.watchlist.map(\.id).joined()) { await load() }
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
        guard let client = model.client else { return }
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
            let r = try await client.radar(model.watchlist)
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

    var body: some View {
        let tick = model.live.price(asset)
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(asset.symbol).font(Theme.mono(16, weight: .bold)).foregroundStyle(.white)
                Text(asset.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
            }
            .frame(minWidth: 70, alignment: .leading)
            if let spark = row?.sparkline, spark.count > 2 {
                Sparkline(values: spark).frame(width: 56, height: 26)
            }
            Spacer(minLength: 4)
            VStack(alignment: .trailing, spacing: 3) {
                Text(Format.price(tick?.price ?? row?.price)).font(Theme.mono(14)).foregroundStyle(.white)
                    .contentTransition(.numericText())
                    .animation(.default, value: tick?.price)
                ChangeText(value: tick?.change ?? row?.change)
            }
            VStack(alignment: .trailing, spacing: 4) {
                if let s = row?.signal { ActionBadge(action: s.action) } else if row?.error != nil { Badge(text: "INDISPO.", tone: .neutral) }
                if let r = row?.reliability, r.level != "high" { Badge(text: r.level == "medium" ? "FIAB. MOY." : "FIAB. FAIBLE", tone: r.tone) }
            }
            .frame(minWidth: 70, alignment: .trailing)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
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
