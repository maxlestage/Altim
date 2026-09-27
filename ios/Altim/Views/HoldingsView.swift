import SwiftUI
import UniformTypeIdentifiers
import AltimCore

/// "Mes avoirs": what the user already owns (SQLite) and full analysis.
struct HoldingsView: View {
    @Environment(AppServices.self) private var services
    @State private var model = HoldingsViewModel()
    @State private var editing: EditTarget?
    @State private var cashText = ""
    @State private var exportURL: URL?
    @State private var importing = false
    @FocusState private var cashFocused: Bool

    struct EditTarget: Identifiable {
        let holding: Holding?
        var id: String { holding?.id ?? "new" }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                CyberGridBackground()
                ScrollView {
                    VStack(spacing: 14) {
                        if model.holdings.isEmpty { emptyCard }
                        cashCard
                        if let error = model.error {
                            Label(error, systemImage: "exclamationmark.triangle.fill")
                                .font(.caption).foregroundStyle(Theme.warning)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        if let a = model.analysis, !model.holdings.isEmpty {
                            summaryCard(a)
                            insightsCard(a)
                            SectionTitle(text: "Ligne par ligne")
                            ForEach(a.lines) { line in lineCard(line) }
                            riskCard(a)
                        }
                        backupCard
                    }
                    .padding()
                }
                .refreshable { await model.refreshMarket(services: services) }
            }
            .navigationTitle("Mes avoirs")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { editing = EditTarget(holding: nil) } label: { Image(systemName: "plus.circle.fill") }
                        .accessibilityLabel("Ajouter un avoir")
                }
            }
            .sheet(item: $editing) { target in
                HoldingEditSheet(initial: target.holding, existing: model.holdings) { holding, merge in
                    Task { await model.save(holding, merge: merge, services: services) }
                }
            }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.json]) { result in
                if case let .success(url) = result { Task { await model.importFile(url, services: services) } }
            }
        }
        .task {
            await model.load(services: services)
            cashText = model.cash > 0 ? String(model.cash) : ""
            exportURL = await model.exportFile(services: services)
        }
    }

    // MARK: Cards

    private var emptyCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Renseignez ce que vous possédez déjà").font(.headline)
            Text("Ajoutez chaque actif avec sa quantité et votre prix d'achat moyen. Altim calcule votre patrimoine, vos gains, vos risques et vous dit, ligne par ligne, quoi faire. Tout est enregistré sur cet iPhone (base SQLite).")
                .font(.subheadline).foregroundStyle(Theme.textSecondary)
            Button("AJOUTER MON PREMIER ACTIF") { editing = EditTarget(holding: nil) }
                .buttonStyle(NeonButtonStyle())
        }
        .glassCard()
    }

    private var cashCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Liquidités disponibles (USD)").font(.caption).foregroundStyle(Theme.textSecondary)
            TextField("0", text: $cashText)
                .keyboardType(.decimalPad)
                .font(Theme.mono(18))
                .focused($cashFocused)
                .onChange(of: cashFocused) { _, focused in
                    guard !focused else { return }
                    let v = Double(cashText.replacingOccurrences(of: ",", with: ".").replacingOccurrences(of: " ", with: "")) ?? 0
                    if v >= 0 { Task { await model.setCash(v, services: services) } }
                }
        }
        .glassCard()
    }

    private func summaryCard(_ a: HoldingsAnalyzer.Analysis) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Patrimoine total").font(.caption).foregroundStyle(Theme.textSecondary)
            Text(usd(a.total)).font(Theme.mono(30, weight: .bold)).neonGlow(Theme.cyan, radius: 6)
            HStack {
                Text("Plus-value latente").foregroundStyle(Theme.textSecondary)
                Spacer()
                Text("\(a.pnl >= 0 ? "+" : "−")\(usd(abs(a.pnl))) (\(Format.percent(a.pnlPercent)))")
                    .font(Theme.mono(13)).foregroundStyle(Theme.color(forChange: a.pnl))
            }
            .font(.subheadline)
            HStack {
                Text("Investi (prix d'achat)").foregroundStyle(Theme.textSecondary)
                Spacer()
                Text(usd(a.invested)).font(Theme.mono(13))
            }
            .font(.caption)
            AllocationBar(allocation: a.allocation)
            if model.isLoading {
                ProgressView("Actualisation des cours et des signaux…").font(.caption).tint(Theme.cyan)
            }
        }
        .glassCard()
    }

    private func insightsCard(_ a: HoldingsAnalyzer.Analysis) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle(text: "Ce qu'Altim vous dit")
            ForEach(a.insights.indices, id: \.self) { index in
                let insight = a.insights[index]
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: icon(insight.level)).foregroundStyle(color(insight.level))
                        .frame(width: 20)
                    Text(HoldingsAnalyzer.text(insight)).font(.subheadline)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .glassCard(glow: Theme.magenta)
    }

    private func lineCard(_ l: HoldingsAnalyzer.Line) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(l.name).font(.headline)
                    Text("\(l.quantity.formatted()) \(l.symbol) · PRU \(Format.price(l.averagePrice)) $")
                        .font(Theme.mono(11, weight: .regular)).foregroundStyle(Theme.textSecondary)
                }
                Spacer()
                Text(l.recommendation.label)
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .padding(.horizontal, 10).padding(.vertical, 5)
                    .foregroundStyle(recColor(l.recommendation))
                    .overlay(Capsule().strokeBorder(recColor(l.recommendation), lineWidth: 1))
            }
            HStack {
                figure("Valeur", usd(l.value), .white)
                figure("Gain / perte", "\(l.pnl >= 0 ? "+" : "−")\(usd(abs(l.pnl)))", Theme.color(forChange: l.pnl))
                figure("Cours", l.price.map { Format.price($0) } ?? "—", .white)
            }
            VStack(alignment: .leading, spacing: 4) {
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.white.opacity(0.07))
                        Capsule().fill(Theme.allocCrypto).frame(width: geo.size.width * min(1, l.weight / 100))
                    }
                }
                .frame(height: 6)
                Text(String(format: "%.1f %% du patrimoine", l.weight)).font(.caption2).foregroundStyle(Theme.textSecondary)
            }
            VStack(alignment: .leading, spacing: 6) {
                ForEach(l.reasons, id: \.self) { r in bullet(HoldingsAnalyzer.reasonText[r] ?? r) }
                if l.recommendation == .lighten, l.trimValue > 0, let p = l.price {
                    bullet("Suggestion : vendre environ \(usd(l.trimValue)) (≈ \((l.trimValue / p).formatted(.number.precision(.significantDigits(4)))) \(l.symbol)).")
                }
                if let stop = l.stop, let loss = l.lossAtStop {
                    bullet("Stop de protection conseillé : \(Format.price(stop)) $ (2 × la volatilité journalière) — perte limitée à ≈ \(usd(loss)).")
                }
            }
            HStack(spacing: 20) {
                Button("Modifier") { editing = EditTarget(holding: model.holdings.first { $0.id == l.id }) }
                Button("Supprimer", role: .destructive) { Task { await model.delete(l.id, services: services) } }
            }
            .font(.subheadline)
        }
        .glassCard(glow: recColor(l.recommendation))
    }

    private func riskCard(_ a: HoldingsAnalyzer.Analysis) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionTitle(text: "Risque du portefeuille")
            kv("Volatilité annuelle", a.risk.volatilityAnnual.map { String(format: "%.1f %%", $0) } ?? "—")
            kv("Perte possible sur 1 jour (1 fois sur 20)", a.risk.var95Day.map { usd($0) } ?? "—")
            kv("Perte si tous les stops sont touchés", usd(a.risk.lossAtStops))
            kv("Ligne la plus lourde", String(format: "%.1f %%", a.risk.maxWeight))
            kv("Diversification effective", String(format: "%.1f actif(s)", a.risk.effectiveAssets))
            kv("Corrélation moyenne", a.risk.averageCorrelation.map { String(format: "%.2f", $0) } ?? "—")
            Text("Calculé sur 90 jours de cours journaliers recoupés entre plusieurs sources. Les performances passées ne préjugent pas des performances futures.")
                .font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .glassCard(glow: Theme.violet)
    }

    private var backupCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle(text: "Sauvegarde")
            Text("Vos avoirs sont enregistrés dans une base SQLite sur cet iPhone. Le fichier d'export est compatible avec l'app web Altim.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
            HStack {
                if let exportURL {
                    ShareLink(item: exportURL) { Label("Exporter", systemImage: "square.and.arrow.up") }
                }
                Spacer()
                Button { importing = true } label: { Label("Importer", systemImage: "square.and.arrow.down") }
            }
            .font(.subheadline.bold())
        }
        .glassCard()
        .task(id: model.holdings.count) { exportURL = await model.exportFile(services: services) }
    }

    // MARK: Helpers

    private func usd(_ v: Double) -> String {
        v.formatted(.number.precision(.fractionLength(2)).locale(Locale(identifier: "fr_FR"))) + " $"
    }

    private func figure(_ title: String, _ value: String, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(Theme.textSecondary)
            Text(value).font(Theme.mono(12)).foregroundStyle(color).lineLimit(1).minimumScaleFactor(0.6)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func kv(_ k: String, _ v: String) -> some View {
        HStack(alignment: .top) {
            Text(k).foregroundStyle(Theme.textSecondary)
            Spacer()
            Text(v).font(Theme.mono(13))
        }
        .font(.subheadline)
    }

    private func bullet(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 6) {
            Text("•").foregroundStyle(Theme.cyan)
            Text(text).font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private func icon(_ level: HoldingsAnalyzer.InsightLevel) -> String {
        switch level {
        case .danger: return "exclamationmark.octagon.fill"
        case .warning: return "exclamationmark.triangle.fill"
        case .good: return "checkmark.seal.fill"
        case .info: return "info.circle.fill"
        }
    }

    private func color(_ level: HoldingsAnalyzer.InsightLevel) -> Color {
        switch level {
        case .danger: return Theme.sell
        case .warning: return Theme.warning
        case .good: return Theme.buy
        case .info: return Theme.cyan
        }
    }

    private func recColor(_ r: HoldingsAnalyzer.Recommendation) -> Color {
        switch r {
        case .sell, .protect: return Theme.sell
        case .strengthen: return Theme.buy
        case .unknown: return Theme.warning
        case .lighten, .hold: return Theme.cyan
        }
    }
}

/// Allocation by class: stacked bar + legend with values (color is never the only cue).
struct AllocationBar: View {
    let allocation: HoldingsAnalyzer.Allocation

    var body: some View {
        let parts: [(String, Double, Color)] = [
            ("Crypto", allocation.crypto, Theme.allocCrypto),
            ("Actions", allocation.stock, Theme.allocStock),
            ("Liquidités", allocation.cash, Theme.allocCash),
        ].filter { $0.1 > 0.05 }
        VStack(alignment: .leading, spacing: 8) {
            Text("Répartition").font(.caption).foregroundStyle(Theme.textSecondary)
            GeometryReader { geo in
                HStack(spacing: 2) {
                    ForEach(parts, id: \.0) { part in
                        RoundedRectangle(cornerRadius: 4).fill(part.2)
                            .frame(width: max(4, (geo.size.width - CGFloat(parts.count - 1) * 2) * part.1 / 100))
                    }
                }
            }
            .frame(height: 14)
            .accessibilityElement()
            .accessibilityLabel(parts.map { "\($0.0) \(String(format: "%.1f", $0.1)) %" }.joined(separator: ", "))
            HStack(spacing: 14) {
                ForEach(parts, id: \.0) { part in
                    HStack(spacing: 5) {
                        RoundedRectangle(cornerRadius: 3).fill(part.2).frame(width: 10, height: 10)
                        Text(part.0).foregroundStyle(Theme.textSecondary)
                        Text(String(format: "%.1f %%", part.1)).font(Theme.mono(12))
                    }
                    .font(.caption)
                }
            }
        }
    }
}

/// Add / edit a holding.
struct HoldingEditSheet: View {
    let initial: Holding?
    let existing: [Holding]
    let onSave: (Holding, Bool) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var asset: Asset?
    @State private var query = ""
    @State private var results: [Asset] = []
    @State private var quantity = ""
    @State private var averagePrice = ""
    @State private var merge = true

    var body: some View {
        NavigationStack {
            Form {
                if let asset {
                    Section("Actif") {
                        LabeledContent(asset.name, value: asset.base)
                        if initial == nil { Button("Changer d'actif") { self.asset = nil } }
                    }
                    Section {
                        TextField("Quantité détenue", text: $quantity).keyboardType(.decimalPad)
                        TextField("Prix d'achat moyen (USD)", text: $averagePrice).keyboardType(.decimalPad)
                        if isDuplicate {
                            Toggle("Ajouter à la ligne existante (PRU recalculé)", isOn: $merge)
                        }
                    } footer: {
                        if let q = number(quantity), let p = number(averagePrice), q > 0 {
                            Text("Montant investi : \((q * p).formatted(.number.precision(.fractionLength(2)))) $")
                        }
                    }
                } else {
                    Section("Crypto") {
                        TextField("Symbole (BTC, ETH, SOL…)", text: $query)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                        if !query.isEmpty {
                            let base = query.uppercased().filter { $0.isLetter || $0.isNumber }
                            Button("Crypto \(base)") {
                                asset = Asset(symbol: base + "USDT", name: base, assetClass: .crypto, quote: "USDT")
                            }
                            .disabled(base.count < 2)
                        }
                    }
                    Section("Actions / ETF") {
                        ForEach(results) { r in
                            Button { asset = r } label: {
                                VStack(alignment: .leading) {
                                    Text(r.name)
                                    Text(r.symbol).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            .task(id: query) {
                guard query.count >= 2, asset == nil else { results = []; return }
                try? await Task.sleep(for: .milliseconds(350))
                results = (try? await YahooMarketData().search(query)) ?? []
            }
            .navigationTitle(initial == nil ? "Ajouter un avoir" : "Modifier")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Enregistrer") { save() }.disabled(!isValid)
                }
            }
            .onAppear {
                if let h = initial {
                    asset = Asset(symbol: h.kind == .crypto ? "\(h.symbol)USDT" : h.symbol, name: h.name, assetClass: h.kind,
                                  quote: h.kind == .crypto ? "USDT" : "USD")
                    quantity = String(h.quantity)
                    averagePrice = String(h.averagePrice)
                }
            }
        }
    }

    private var isDuplicate: Bool {
        guard initial == nil, let asset else { return false }
        return existing.contains { $0.kind == asset.assetClass && $0.symbol == asset.base }
    }

    private var isValid: Bool {
        guard asset != nil, let q = number(quantity), let p = number(averagePrice) else { return false }
        return q > 0 && p >= 0
    }

    private func number(_ s: String) -> Double? {
        Double(s.replacingOccurrences(of: ",", with: ".").replacingOccurrences(of: " ", with: ""))
    }

    private func save() {
        guard let asset, let q = number(quantity), let p = number(averagePrice) else { return }
        let holding = Holding(id: initial?.id ?? UUID().uuidString, symbol: asset.base, kind: asset.assetClass,
                              name: asset.name, quantity: q, averagePrice: p)
        onSave(holding, isDuplicate && merge)
        dismiss()
    }
}
