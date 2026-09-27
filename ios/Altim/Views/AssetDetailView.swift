import SwiftUI
import Charts
import AltimKit

/// Everything about one asset: live price, chart, signal, Fibonacci buy zones by horizon, market guard, macro.
struct AssetDetailView: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @State private var interval = "4h"
    @State private var snapshot: Loadable<Snapshot> = .idle
    @State private var signal: RadarRow?
    @State private var zones: Loadable<ZonesReport> = .idle
    @State private var guardReport: Loadable<GuardReport> = .idle

    private static let intervals = [("1h", "1 h"), ("4h", "4 h"), ("1d", "1 j")]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                chartCard
                signalCard
                zonesCard
                guardCard
                if let macro = guardReport.value?.macro ?? zones.value?.macro { MacroCard(macro: macro) }
                if let news = guardReport.value?.inputs?.headlines, !news.isEmpty { newsCard(news) }
                Text("Altim ne passe aucun ordre : ces analyses sont des probabilités, à confronter à votre propre jugement.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle(asset.symbol)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            Button {
                model.isWatched(asset) ? model.unwatch(asset) : model.watch(asset)
            } label: {
                Image(systemName: model.isWatched(asset) ? "star.fill" : "star")
            }
            .accessibilityLabel(model.isWatched(asset) ? "Retirer du radar" : "Ajouter au radar")
        }
        .onAppear { model.focus = asset }
        .onDisappear { if model.focus == asset { model.focus = nil } }
        .task { await loadStatic() }
        .task(id: interval) { await loadChart() }
        .refreshable {
            await loadStatic()
            await loadChart()
        }
    }

    // MARK: Header

    private var header: some View {
        let tick = model.live.price(asset)
        return VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(asset.name).font(.title3.bold()).lineLimit(2)
                Spacer()
                Badge(text: asset.kind.label, tone: .neutral)
            }
            Text(Format.price(tick?.price ?? signal?.price ?? zones.value?.price))
                .font(Theme.mono(32, weight: .bold))
                .contentTransition(.numericText())
                .animation(.default, value: tick?.price)
            HStack(spacing: 10) {
                ChangeText(value: tick?.change ?? signal?.change)
                Text("24 h").font(.caption).foregroundStyle(Theme.textSecondary)
                Spacer()
                LiveBadge()
            }
            if let t = tick, let a = t.agreeing, let n = t.total {
                Text("Prix médian de \(a)/\(n) sources en accord" + (t.market == "closed" ? " · Bourse de New York fermée" : ""))
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
    }

    // MARK: Chart

    private var chartCard: some View {
        Card {
            Picker("Unité de temps", selection: $interval) {
                ForEach(Self.intervals, id: \.0) { Text($0.1).tag($0.0) }
            }
            .pickerStyle(.segmented)
            switch snapshot {
            case .idle, .loading:
                ProgressView().frame(maxWidth: .infinity, minHeight: 220)
            case let .failed(m):
                ErrorView(message: m) { Task { await loadChart() } }
            case let .loaded(s):
                PriceChart(candles: Array(s.candles.suffix(interval == "1d" ? 180 : 120)), zone: chartZone)
                    .frame(height: 220)
                HStack {
                    Text("\(s.agreeing) sources en accord").font(.caption).foregroundStyle(Theme.textSecondary)
                    Spacer()
                    Badge(text: s.reliability.label.uppercased(), tone: s.reliability.tone)
                }
                if let z = chartZone {
                    Text("Bande colorée : zone d'achat \(z.label.lowercased()) (\(Format.price(z.zone?.from)) – \(Format.price(z.zone?.to))).")
                        .font(.caption).foregroundStyle(Theme.textSecondary)
                }
            }
        }
    }

    /// Zone drawn on the chart: the horizon whose candles match the interval shown.
    private var chartZone: FibZone? {
        zones.value?.zones.first { $0.horizon == (interval == "1d" ? "medium" : "short") && $0.zone != nil }
    }

    // MARK: Signal

    @ViewBuilder private var signalCard: some View {
        if let s = signal?.signal {
            Card(title: "Signal \(Self.intervals.first { $0.0 == interval }?.1 ?? interval)") {
                HStack {
                    ActionBadge(action: s.action)
                    Spacer()
                    Text("score \(Int(s.score.rounded())) · confiance \(Int(s.confidence.rounded())) %").font(Theme.mono(13))
                }
                Text("Calculé sur les bougies médianes de toutes les sources ; la confiance tient compte de l'accord entre indicateurs et unités de temps.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
    }

    // MARK: Zones

    @ViewBuilder private var zonesCard: some View {
        switch zones {
        case .idle, .loading:
            Card(title: "Zones d'achat (Fibonacci)") { ProgressView().frame(maxWidth: .infinity) }
        case let .failed(m):
            Card(title: "Zones d'achat (Fibonacci)") { ErrorView(message: m) { Task { await loadStatic() } } }
        case let .loaded(z):
            Card(title: "Zones d'achat (Fibonacci)", glow: Theme.violet) {
                Text("Retracement 38,2 % – 65 % du dernier mouvement haussier ; « zone d'or » 61,8 – 65 %.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
                ForEach(z.zones) { ZoneRow(zone: $0) }
            }
        }
    }

    // MARK: Guard

    @ViewBuilder private var guardCard: some View {
        switch guardReport {
        case .idle, .loading:
            Card(title: "Garde-fou marché") { ProgressView().frame(maxWidth: .infinity) }
        case let .failed(m):
            Card(title: "Garde-fou marché") { ErrorView(message: m) { Task { await loadStatic() } } }
        case let .loaded(g):
            GuardCard(report: g)
        }
    }

    private func newsCard(_ news: [GuardReport.Headline]) -> some View {
        Card(title: "Actualités (24 h)") {
            ForEach(news.prefix(5)) { h in
                VStack(alignment: .leading, spacing: 2) {
                    Text(h.title).font(.footnote).foregroundStyle(.white.opacity(0.9))
                    Text(Format.date(h.time, time: true)).font(.caption2).foregroundStyle(Theme.textSecondary)
                }
            }
        }
    }

    // MARK: Loading

    private func loadStatic() async {
        guard let client = model.client else { return }
        if zones.value == nil { zones = .loading }
        if guardReport.value == nil { guardReport = .loading }
        async let z = client.zones(asset)
        async let g = client.guardReport(asset)
        do { zones = .loaded(try await z) } catch { zones = .failed(error.localizedDescription) }
        do { guardReport = .loaded(try await g) } catch { guardReport = .failed(error.localizedDescription) }
        model.persistSession()
    }

    private func loadChart() async {
        guard let client = model.client else { return }
        snapshot = .loading
        async let r = try? client.radar([asset], interval: interval)
        do {
            snapshot = .loaded(try await client.candles(asset, interval: interval))
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch {
            snapshot = .failed(error.localizedDescription)
        }
        signal = (await r)?.first
    }
}

struct PriceChart: View {
    var candles: [Candle]
    var zone: FibZone?

    var body: some View {
        let lows = candles.map(\.low) + (zone?.zone.map { [$0.from] } ?? [])
        let highs = candles.map(\.high) + (zone?.zone.map { [$0.to] } ?? [])
        let lo = lows.min() ?? 0
        let hi = highs.max() ?? 1
        let pad = (hi - lo) * 0.05
        let first = candles.first?.date ?? .now
        let last = candles.last?.date ?? .now
        Chart {
            if let band = zone?.zone {
                RectangleMark(xStart: .value("début", first), xEnd: .value("fin", last), yStart: .value("bas", band.from), yEnd: .value("haut", band.to))
                    .foregroundStyle(Theme.violet.opacity(0.18))
            }
            if let g = zone?.golden {
                RectangleMark(xStart: .value("début", first), xEnd: .value("fin", last), yStart: .value("bas", g.from), yEnd: .value("haut", g.to))
                    .foregroundStyle(Theme.warning.opacity(0.22))
            }
            ForEach(candles) { c in
                LineMark(x: .value("date", c.date), y: .value("prix", c.close))
                    .interpolationMethod(.monotone)
                    .lineStyle(StrokeStyle(lineWidth: 2))
                    .foregroundStyle(Theme.cyan)
            }
        }
        .chartYScale(domain: (lo - pad)...(hi + pad))
        .chartXAxis {
            AxisMarks(values: .automatic(desiredCount: 4)) { _ in
                AxisGridLine().foregroundStyle(.white.opacity(0.06))
                AxisValueLabel()
            }
        }
        .chartYAxis {
            AxisMarks(position: .trailing, values: .automatic(desiredCount: 4)) { _ in
                AxisGridLine().foregroundStyle(.white.opacity(0.06))
                AxisValueLabel()
            }
        }
        .accessibilityLabel("Graphique du prix, de \(Format.price(candles.first?.close)) à \(Format.price(candles.last?.close))")
    }
}

struct ZoneRow: View {
    var zone: FibZone

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Divider().overlay(Color.white.opacity(0.1))
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(zone.label).font(.subheadline.bold())
                    Text("bougies \(zone.unit) · détention \(zone.holding)").font(.caption).foregroundStyle(Theme.textSecondary)
                }
                Spacer()
                Badge(text: zone.statusLabel.uppercased(), tone: zone.tone)
            }
            if let z = zone.zone { KeyValue(key: "Zone d'achat", value: "\(Format.price(z.to)) – \(Format.price(z.from))") }
            if let g = zone.golden { KeyValue(key: "Zone d'or", value: "\(Format.price(g.to)) – \(Format.price(g.from))", tone: .warn) }
            if let inv = zone.invalidation { KeyValue(key: "Invalidée sous", value: Format.price(inv), tone: .bad) }
            if !zone.targets.isEmpty { KeyValue(key: "Objectifs", value: zone.targets.prefix(3).map { Format.price($0) }.joined(separator: "\n"), tone: .good) }
            Text(zone.text).font(.footnote).foregroundStyle(.white.opacity(0.9))
            if let note = zone.macroNote { Notice(text: note, tone: .warn) }
            Text(evidenceText).font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    /// Same wording and thresholds as the web app (weigh(): 20 cases minimum, lift ≥ 1.1 to count).
    private var evidenceText: String {
        guard let e = zone.evidence else {
            return "Historique : aucun repli comparable sur cet actif pour cet horizon, zone non vérifiée."
        }
        let verdict = e.samples < 20 ? "trop peu de cas pour conclure"
            : e.lift < 1.1 ? "ces zones n'ont pas fait mieux qu'une entrée au hasard sur cet actif"
            : "ces zones ont mieux tenu qu'une entrée au hasard sur cet actif"
        return "Historique : sur \(e.samples) repli\(e.samples > 1 ? "s" : "") dans la zone, \(Int(e.rate.rounded())) % sont remontés au plus haut avant de casser le plus bas, contre \(Int(e.base.rounded())) % pour une entrée au hasard : \(verdict)."
    }
}

struct GuardCard: View {
    var report: GuardReport

    var body: some View {
        Card(title: "Garde-fou marché", glow: Theme.color(report.shockTone)) {
            KeyValue(key: "Tendance de fond", value: report.trendLabel, tone: report.regime.trend == "up" ? .good : report.regime.trend == "down" ? .bad : .neutral)
            Text(report.regime.text).font(.footnote).foregroundStyle(.white.opacity(0.85))
            Meter(label: "Risque de choc (\(report.shockLabel))", value: report.shock.score, tone: report.shockTone)
            factors(report.shock.factors)
            Meter(label: "Risque de retournement" + (report.reversal.direction == "up" ? " (à la hausse)" : report.reversal.direction == "down" ? " (à la baisse)" : ""),
                  value: report.reversal.score, tone: report.reversal.score >= 50 ? .bad : report.reversal.score >= 25 ? .warn : .good)
            factors(report.reversal.factors)
            KeyValue(key: "Trading court terme", value: report.policyLabel, tone: report.policy.scalping == "ok" ? .good : report.policy.scalping == "pause" ? .bad : .warn)
            if report.policy.sizeMultiplier < 1 {
                KeyValue(key: "Taille conseillée", value: "× \(Format.plain(report.policy.sizeMultiplier))", tone: .warn)
            }
            ForEach(report.policy.notes, id: \.self) { Text("• \($0)").font(.footnote).foregroundStyle(.white.opacity(0.85)) }
        }
    }

    private func detail(_ f: GuardFactor) -> String {
        var text = "+\(Int(f.points.rounded())) pts"
        if let status = f.statusText { text += " · \(status)" }
        if let e = f.evidence, e.samples > 0 {
            text += " (\(Int(e.rate.rounded())) % des \(e.samples) cas passés contre \(Int(e.base.rounded())) % d'habitude)"
        }
        return text
    }

    @ViewBuilder private func factors(_ list: [GuardFactor]) -> some View {
        if list.isEmpty {
            Text("Aucun signal.").font(.caption).foregroundStyle(Theme.textSecondary)
        } else {
            ForEach(list) { f in
                VStack(alignment: .leading, spacing: 2) {
                    Text(f.text).font(.footnote).foregroundStyle(f.status == "rejected" ? Theme.textSecondary : .white.opacity(0.9))
                    Text(detail(f)).font(.caption2).foregroundStyle(Theme.textSecondary)
                }
            }
        }
    }
}

struct MacroCard: View {
    var macro: MacroInfo

    var body: some View {
        Card(title: "Contexte macro et géopolitique", glow: Theme.color(macro.tone)) {
            HStack {
                Text(macro.levelLabel).font(.subheadline.bold()).foregroundStyle(Theme.color(macro.tone))
                Spacer()
                Text("\(Int(macro.score))/100").font(Theme.mono(13))
            }
            ForEach(macro.factors) { Text("• \($0.text)").font(.footnote) }
            ForEach(MacroInfo.series, id: \.key) { s in
                if let v = macro.values[s.key] {
                    KeyValue(key: s.label, value: "\(Format.plain(v.value)) (\(Format.percent(v.change5d, digits: 1)) 5 j)")
                }
            }
            if !macro.themes.isEmpty {
                Text("Actualité mondiale (titres des dernières 48 h)").font(.caption.bold()).padding(.top, 4)
                ForEach(macro.themes) { t in
                    Text("\(t.label) : \(t.count) titres").font(.caption).foregroundStyle(Theme.textSecondary)
                }
            }
            if macro.factors.isEmpty {
                Text("Aucun signe de stress sur la peur (VIX), le S&P 500, le pétrole, l'or, le dollar ni les taux.").font(.caption).foregroundStyle(Theme.textSecondary)
            }
            if let e = macro.evidence, e.samples >= 20 {
                Text("Sur cet actif, les jours de stress macro ont été suivis d'une forte baisse dans \(Int(e.rate.rounded())) % des cas en 5 jours, contre \(Int(e.base.rounded())) % d'habitude" + (e.lift >= 1.1 ? " : à prendre au sérieux." : " : pas d'effet mesurable ici."))
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
    }
}
