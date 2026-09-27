import SwiftUI
import AltimCore

/// Écran principal : tous les actifs suivis avec leur signal en temps réel.
struct RadarView: View {
    @Environment(AppSettings.self) private var settings
    @Environment(AppServices.self) private var services
    @State private var model = RadarViewModel()
    @State private var showAdd = false

    var body: some View {
        @Bindable var settings = settings
        NavigationStack {
            ZStack {
                CyberGridBackground()
                ScrollView {
                    VStack(spacing: 14) {
                        header
                        Picker("Unité de temps", selection: $settings.timeframe) {
                            ForEach(Timeframe.allCases) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)

                        opportunities

                        ForEach(settings.watchlist) { asset in
                            NavigationLink(value: asset) {
                                AssetRow(asset: asset, row: model.rows[asset.id])
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                Button(role: .destructive) {
                                    settings.watchlist.removeAll { $0.id == asset.id }
                                } label: { Label("Retirer du radar", systemImage: "trash") }
                            }
                        }
                    }
                    .padding(.horizontal)
                    .padding(.bottom, 24)
                }
                .refreshable { await refresh() }
            }
            .navigationTitle("Radar")
            .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0, timeframe: settings.timeframe) }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showAdd = true } label: { Image(systemName: "plus.circle.fill") }
                }
            }
            .sheet(isPresented: $showAdd) { AddAssetView() }
        }
        .task(id: "\(settings.timeframe.rawValue)-\(settings.watchlist.count)") {
            // Rafraîchissement automatique toutes les 60 s tant que l'écran est visible.
            while !Task.isCancelled {
                await refresh()
                try? await Task.sleep(for: .seconds(60))
            }
        }
    }

    private func refresh() async {
        await model.refresh(assets: settings.watchlist, timeframe: settings.timeframe, services: services,
                            notify: settings.notificationsEnabled)
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("MARCHÉS · \(settings.timeframe.label.uppercased())")
                    .font(Theme.mono(12)).foregroundStyle(Theme.textSecondary)
                if let date = model.lastUpdate {
                    Text("Mis à jour \(date.formatted(date: .omitted, time: .standard))")
                        .font(.caption2).foregroundStyle(Theme.textSecondary)
                }
            }
            Spacer()
            if let fg = model.fearGreed {
                VStack(spacing: 0) {
                    Text("\(fg.value)").font(Theme.mono(14, weight: .bold))
                        .foregroundStyle(fg.value < 45 ? Theme.sell : fg.value > 55 ? Theme.buy : Theme.cyan)
                    Text("F&G").font(.system(size: 8, weight: .bold, design: .monospaced)).foregroundStyle(Theme.textSecondary)
                }
            }
            if model.isRefreshing { ProgressView().tint(Theme.cyan) }
            Text("CONSEIL")
                .font(.system(size: 11, weight: .heavy, design: .monospaced))
                .padding(.horizontal, 8).padding(.vertical, 4)
                .foregroundStyle(Theme.cyan)
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.cyan, lineWidth: 1))
        }
    }

    /// Meilleures opportunités du moment (signaux forts avec confiance élevée).
    @ViewBuilder private var opportunities: some View {
        let top = settings.watchlist
            .compactMap { asset in model.rows[asset.id]?.signal.map { (asset, $0) } }
            .filter { $0.1.action != .hold && $0.1.confidence >= 40 }
            .sorted { $0.1.confidence > $1.1.confidence }
            .prefix(3)
        if !top.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                SectionTitle(text: "Opportunités détectées")
                ForEach(Array(top), id: \.0.id) { item in
                    NavigationLink(value: item.0) {
                        HStack {
                            Text(item.0.name).font(.headline)
                            Spacer()
                            Text("\(Int(item.1.confidence)) %").font(Theme.mono(14)).foregroundStyle(Theme.textSecondary)
                            ActionBadge(action: item.1.action, compact: true)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
            .glassCard(glow: Theme.magenta)
        }
    }
}

struct AssetRow: View {
    let asset: Asset
    let row: RadarViewModel.Row?

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(asset.name).font(.headline)
                Text("\(asset.symbol) · \(asset.assetClass.label)")
                    .font(Theme.mono(11, weight: .regular)).foregroundStyle(Theme.textSecondary)
            }
            Spacer(minLength: 8)
            if let spark = row?.sparkline, spark.count > 2 {
                Sparkline(values: spark).frame(width: 70, height: 30)
            }
            VStack(alignment: .trailing, spacing: 4) {
                if let price = row?.price, let change = row?.change24h {
                    Text(Format.price(price)).font(Theme.mono(15))
                    Text(Format.percent(change))
                        .font(Theme.mono(11)).foregroundStyle(Theme.color(forChange: change))
                } else if row?.error != nil {
                    Image(systemName: "wifi.exclamationmark").foregroundStyle(Theme.warning)
                } else {
                    ProgressView().tint(Theme.cyan)
                }
            }
            .frame(minWidth: 90, alignment: .trailing)
        }
        .overlay(alignment: .bottomLeading) {
            if let signal = row?.signal {
                HStack(spacing: 8) {
                    ActionBadge(action: signal.action, compact: true)
                    if let level = row?.reliability { ReliabilityBadge(level: level) }
                    if row?.error != nil {
                        Image(systemName: "clock.arrow.circlepath").font(.caption2).foregroundStyle(Theme.warning)
                    }
                }
                .offset(y: 30)
            }
        }
        .padding(.bottom, row?.signal != nil ? 26 : 0)
        .glassCard(glow: row?.signal.map { Theme.color(for: $0.action) } ?? Theme.cyan)
    }
}

/// Ajout d'un actif : crypto (paire USDT Binance) ou action (recherche Yahoo).
struct AddAssetView: View {
    @Environment(AppSettings.self) private var settings
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var results: [Asset] = []
    @State private var searching = false

    var body: some View {
        NavigationStack {
            List {
                if !query.isEmpty {
                    Section("Crypto (Binance)") {
                        let symbol = query.uppercased().replacingOccurrences(of: " ", with: "")
                        let pair = symbol.hasSuffix("USDT") ? symbol : symbol + "USDT"
                        Button {
                            add(Asset(symbol: pair, name: String(pair.dropLast(4)), assetClass: .crypto, quote: "USDT"))
                        } label: { Label(pair, systemImage: "bitcoinsign.circle") }
                    }
                }
                Section("Actions / ETF") {
                    if searching { ProgressView() }
                    ForEach(results) { asset in
                        Button { add(asset) } label: {
                            VStack(alignment: .leading) {
                                Text(asset.name)
                                Text(asset.symbol).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .searchable(text: $query, prompt: "BTC, SOL, Apple, NVDA…")
            .task(id: query) {
                guard query.count >= 2 else { results = []; return }
                try? await Task.sleep(for: .milliseconds(350))
                searching = true
                results = (try? await YahooMarketData().search(query)) ?? []
                searching = false
            }
            .navigationTitle("Ajouter un actif")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Fermer") { dismiss() } } }
        }
    }

    private func add(_ asset: Asset) {
        if !settings.watchlist.contains(asset) { settings.watchlist.append(asset) }
        dismiss()
    }
}
