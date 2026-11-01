import SwiftUI
import Charts
import AltimKit

/// Everything about one asset, computed on the iPhone: live price, Altim's verdict, chart, time machine, Fibonacci buy
/// zones by horizon, market guard, macro, news, note, position, DCA.
struct AssetDetailView: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    /// Shared with the Radar's chooser (the Radar has no 1 h).
    @AppStorage(Timeframe.storageKey) private var intervalRaw = Timeframe.standard.rawValue
    @State private var snapshot: Loadable<Snapshot> = .idle
    @State private var signal: RadarRow?
    @State private var zones: Loadable<ZonesReport> = .idle
    @State private var guardReport: Loadable<GuardReport> = .idle
    @State private var targetSheet = false

    private var timeframe: Timeframe { Timeframe.saved(intervalRaw) }
    private var interval: String { timeframe.rawValue }
    private var timeframeBinding: Binding<Timeframe> {
        Binding(get: { timeframe }, set: { intervalRaw = $0.rawValue })
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                VerdictCard(asset: asset)
                chartCard
                TimeMachineCard(asset: asset)
                zonesCard
                guardCard
                if asset.kind == .stock { EarningsCard(symbols: [asset.symbol], showsEmpty: true) }
                if let macro = guardReport.value?.macro ?? zones.value?.macro { MacroCard(macro: macro) }
                if let news = guardReport.value?.inputs?.headlines, !news.isEmpty { newsCard(news) }
                NoteCard(asset: asset)
                PositionCard(asset: asset, price: model.live.price(asset)?.price ?? signal?.price ?? zones.value?.price, zones: zones.value?.zones ?? [])
                DcaCard(asset: asset)
                Text("Altim ne passe aucun ordre : ces analyses sont des probabilités, à confronter à votre propre jugement.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle(asset.symbol)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $targetSheet) {
            PriceTargetSheet(asset: asset, current: model.live.price(asset)?.price ?? signal?.price ?? zones.value?.price)
                .environment(model)
        }
        .toolbar {
            Button {
                targetSheet = true
            } label: {
                Image(systemName: "bell.badge")
            }
            .accessibilityLabel("Alerte de prix")
            if model.liveActivityEnabled && LiveActivities.shared.available {
                Button {
                    Task { await toggleLiveActivity() }
                } label: {
                    Image(systemName: model.activityAsset?.id == asset.id ? "livephoto.slash" : "livephoto")
                }
                .accessibilityLabel(model.activityAsset?.id == asset.id ? "Arrêter le suivi en direct" : "Suivre sur l'écran verrouillé et la Dynamic Island")
            }
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
                .lineLimit(1).minimumScaleFactor(0.5)
                .liveTick(tick?.price ?? signal?.price ?? zones.value?.price)
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
            FittingPicker(title: "Unité de temps", selection: timeframeBinding) {
                ForEach(Timeframe.allCases) { Text($0.label).tag($0) }
            }
            switch snapshot {
            case .idle, .loading:
                ProgressView().frame(maxWidth: .infinity, minHeight: 220)
            case let .failed(m):
                ErrorView(message: m) { Task { await loadChart() } }
            case let .loaded(s):
                PriceChart(candles: Array(s.candles.suffix(timeframe.chartCandles)), zone: chartZone)
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
        zones.value?.zones.first { $0.horizon == timeframe.zoneHorizon && $0.zone != nil }
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

    // MARK: Live Activity

    private func toggleLiveActivity() async {
        if model.activityAsset?.id == asset.id {
            await LiveActivities.shared.stopAll()
            model.activityAsset = nil
            return
        }
        let tick = model.live.price(asset)
        guard let price = tick?.price ?? signal?.price ?? zones.value?.price else { return }
        let alert = model.lastAlerts.first { $0.id == asset.id }
        await LiveActivities.shared.start(asset, price: price, change: tick?.change ?? signal?.change, alert: alert)
        model.activityAsset = LiveActivities.shared.current
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
    }

    private func loadChart() async {
        guard let client = model.client else { return }
        snapshot = .loading
        async let r = try? client.radar([asset], interval: interval)
        do {
            snapshot = .loaded(try await client.candles(asset, interval: interval))
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
        // Dollar prices drawn in the display currency (the axis has no symbol: it must match the price above).
        let k = Money.toDisplay(1)
        let lows = candles.map { $0.low * k } + (zone?.zone.map { [$0.from * k] } ?? [])
        let highs = candles.map { $0.high * k } + (zone?.zone.map { [$0.to * k] } ?? [])
        let lo = lows.min() ?? 0
        let hi = highs.max() ?? 1
        let pad = (hi - lo) * 0.05
        let first = candles.first?.date ?? .now
        let last = candles.last?.date ?? .now
        Chart {
            if let band = zone?.zone {
                RectangleMark(xStart: .value("début", first), xEnd: .value("fin", last), yStart: .value("bas", band.from * k), yEnd: .value("haut", band.to * k))
                    .foregroundStyle(Theme.violet.opacity(0.18))
            }
            if let g = zone?.golden {
                RectangleMark(xStart: .value("début", first), xEnd: .value("fin", last), yStart: .value("bas", g.from * k), yEnd: .value("haut", g.to * k))
                    .foregroundStyle(Theme.warning.opacity(0.22))
            }
            ForEach(candles) { c in
                LineMark(x: .value("date", c.date), y: .value("prix", c.close * k))
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
            if let r = macro.regime { RegimeLine(regime: r) }
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

/// « La machine à remonter le temps » : the technical signal replayed day by day on the asset's history, computed on
/// the iPhone (AltimKit Backtest), against simply holding the asset. Honest by design: it says so when holding won.
struct TimeMachineCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @State private var report: Loadable<BacktestReport> = .idle
    @State private var showTrades = false

    var body: some View {
        Card(title: "Machine à remonter le temps", glow: Theme.magenta) {
            Text("Et si vous aviez suivi Altim sur \(asset.symbol) ? Investi seulement quand la tendance de fond est haussière et le momentum positif, sorti sinon. Rejoué jour par jour, sans jamais voir le lendemain, avec 0,1 % de frais par ordre.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            switch report {
            case .idle, .loading:
                ProgressView("Voyage dans le temps…").frame(maxWidth: .infinity, minHeight: 80).tint(Theme.magenta)
            case let .failed(m):
                ErrorView(message: m) { Task { await load() } }
            case let .loaded(r):
                content(r)
            }
        }
        .task(id: asset.id) { await load() }
    }

    @ViewBuilder private func content(_ r: BacktestReport) -> some View {
        if let e = r.error {
            Notice(text: e, tone: .warn)
        } else {
            Badge(text: r.verdict.uppercased(), tone: r.tone)
            Text("Du \(Format.date(r.from)) au \(Format.date(r.to)) · \(r.days) jours")
                .font(.caption).foregroundStyle(Theme.textSecondary)
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) { tiles(r) }
                VStack(spacing: 10) { tiles(r) }
            }
            TimeMachineChart(points: r.curve)
                .frame(height: 170)
            HStack(spacing: 14) {
                legend(Theme.magenta, "Suivre Altim", dashed: false)
                legend(.white.opacity(0.6), "Garder", dashed: true)
            }
            .font(.caption2)
            KeyValue(key: "Achats déclenchés", value: "\(r.trades.count)")
            if let w = r.winRate { KeyValue(key: "Trades gagnants", value: "\(Int(w.rounded())) %", tone: w >= 50 ? .good : .warn) }
            KeyValue(key: "Temps investi", value: "\(Int(r.exposure.rounded())) %")
            if !r.trades.isEmpty {
                Button(showTrades ? "Masquer les trades" : "Voir les derniers trades") {
                    withAnimation(.snappy) { showTrades.toggle() }
                }
                .font(.footnote).buttonStyle(.borderless)
                if showTrades {
                    ForEach(r.trades.suffix(8).reversed()) { t in tradeRow(t) }
                        .transition(.opacity.combined(with: .move(edge: .top)))
                }
            }
            Text("Le passé ne garantit rien. La règle protège surtout dans les marchés baissiers ; en forte hausse, garder peut faire mieux.")
                .font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
    }

    @ViewBuilder private func tiles(_ r: BacktestReport) -> some View {
        tile("Suivre Altim", r.strategyReturn, drawdown: r.strategyDrawdown, color: Theme.magenta)
        tile("Garder", r.holdReturn, drawdown: r.holdDrawdown, color: .white)
    }

    private func tile(_ title: String, _ ret: Double, drawdown: Double, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption.weight(.semibold)).foregroundStyle(color)
            Text(Self.signed(ret)).font(Theme.mono(22, weight: .bold)).foregroundStyle(Theme.color(forChange: ret))
                .lineLimit(1).minimumScaleFactor(0.6)
            Text("pire creux \(Self.signed(drawdown))").font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.05)))
        .accessibilityElement(children: .combine)
    }

    private func legend(_ color: Color, _ text: String, dashed: Bool) -> some View {
        HStack(spacing: 6) {
            Capsule().fill(color).frame(width: 16, height: 3).opacity(dashed ? 0.7 : 1)
            Text(text).foregroundStyle(Theme.textSecondary)
        }
    }

    private func tradeRow(_ t: BacktestReport.Trade) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("\(Format.date(t.entryTime)) → \(t.exit == "open" ? "en cours" : Format.date(t.exitTime))").font(.caption)
                Text(Self.exitLabel(t.exit)).font(.caption2).foregroundStyle(Theme.textSecondary)
            }
            Spacer()
            Text(Self.signed(t.result)).font(Theme.mono(13)).foregroundStyle(Theme.color(forChange: t.result))
        }
        .accessibilityElement(children: .combine)
    }

    static func exitLabel(_ e: String) -> String {
        switch e {
        case "stop": return "Sorti au stop"
        case "target": return "Objectif atteint"
        case "signal": return "Signal de vente"
        default: return "Toujours détenu"
        }
    }

    static func signed(_ v: Double) -> String { "\(v >= 0 ? "+" : "−")\(Format.plain(abs(v), digits: abs(v) >= 100 ? 0 : 1)) %" }

    private func load() async {
        guard let client = model.client else { return }
        if report.value == nil { report = .loading }
        do { report = .loaded(try await client.backtest(asset)) } catch { report = .failed(error.localizedDescription) }
    }
}

/// Value of 100 invested: following the signal (magenta, filled) and holding (white, dashed). Drawn once, left to
/// right, when it appears (none with Reduce Motion).
struct TimeMachineChart: View {
    var points: [BacktestReport.Point]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = 0.0

    var body: some View {
        let visible = Array(points.prefix(max(2, Int((Double(points.count) * shown).rounded()))))
        let values = points.flatMap { [$0.strategy, $0.hold] }
        let lo = (values.min() ?? 90) * 0.97
        let hi = (values.max() ?? 110) * 1.03
        Chart {
            RuleMark(y: .value("départ", 100)).foregroundStyle(.white.opacity(0.15)).lineStyle(StrokeStyle(lineWidth: 1, dash: [2, 3]))
            ForEach(visible, id: \.time) { p in
                AreaMark(x: .value("date", Date(timeIntervalSince1970: p.time / 1000)), yStart: .value("bas", lo), yEnd: .value("Altim", p.strategy))
                    .foregroundStyle(LinearGradient(colors: [Theme.magenta.opacity(0.35), .clear], startPoint: .top, endPoint: .bottom))
                    .interpolationMethod(.monotone)
                LineMark(x: .value("date", Date(timeIntervalSince1970: p.time / 1000)), y: .value("Altim", p.strategy), series: .value("courbe", "Altim"))
                    .foregroundStyle(Theme.magenta)
                    .lineStyle(StrokeStyle(lineWidth: 2.5))
                    .interpolationMethod(.monotone)
                LineMark(x: .value("date", Date(timeIntervalSince1970: p.time / 1000)), y: .value("Garder", p.hold), series: .value("courbe", "Garder"))
                    .foregroundStyle(.white.opacity(0.6))
                    .lineStyle(StrokeStyle(lineWidth: 1.5, dash: [4, 3]))
                    .interpolationMethod(.monotone)
            }
        }
        .chartYScale(domain: lo...hi)
        .chartXScale(domain: Date(timeIntervalSince1970: (points.first?.time ?? 0) / 1000)...Date(timeIntervalSince1970: (points.last?.time ?? 1) / 1000))
        .chartXAxis {
            AxisMarks(values: .automatic(desiredCount: 3)) { _ in
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
        .accessibilityLabel("100 investis deviennent \(Int((points.last?.strategy ?? 100).rounded())) en suivant Altim, \(Int((points.last?.hold ?? 100).rounded())) en gardant")
        // The curve is traced in 30 steps (a chart's point count does not animate by itself).
        .task(id: points.count) {
            guard !reduceMotion else { return shown = 1 }
            for step in 1...30 {
                shown = Double(step) / 30
                try? await Task.sleep(nanoseconds: 40_000_000)
                if Task.isCancelled { return shown = 1 }
            }
        }
    }
}

/// « Verdict Altim » (standalone): the daily rule chosen on measured results (AltimKit TrendEngine) — in only while the
/// background trend is up and the momentum positive, best entered on a short pull-back.
struct VerdictCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @State private var verdict: Loadable<TrendEngine.Verdict> = .idle
    @State private var simulating = false

    var body: some View {
        Card(title: "Verdict Altim", glow: tone.map { Theme.color($0) } ?? Theme.cyan) {
            switch verdict {
            case .idle, .loading:
                ProgressView().frame(maxWidth: .infinity, minHeight: 60)
            case let .failed(m):
                ErrorView(message: m) { Task { await load() } }
            case let .loaded(v):
                HStack(spacing: 8) {
                    Badge(text: v.label, tone: v.action.tone)
                    if let m = v.momentum {
                        Text("momentum \(m >= 0 ? "+" : "−")\(Format.plain(abs(m) * 100, digits: 1)) %").font(Theme.mono(12)).foregroundStyle(Theme.textSecondary)
                    }
                }
                Text(v.text).font(.subheadline).fixedSize(horizontal: false, vertical: true)
                if let e = v.ema200 {
                    KeyValue(key: "Moyenne 200 jours", value: Format.price(e))
                }
                if v.bot != nil || v.botRisk != nil {
                    BotsBlock(verdict: v)
                }
                Text("La règle : investi seulement quand la tendance de fond est haussière (prix au-dessus de la moyenne 200 jours) et le momentum positif (\(asset.kind == .crypto ? "60 jours" : "3 mois")) ; et seulement quand son marché l'est aussi (\(asset.kind == .crypto ? "le Bitcoin" : "le S&P 500, via SPY")) ; un repli de quelques jours a donné en moyenne une meilleure entrée sur la semaine suivante, sans avantage net à un mois. Choisie parce qu'elle a le mieux résisté aux tests sur 26 actifs : voir la machine à remonter le temps ci-dessous.")
                    .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                Button { simulating = true } label: {
                    Label("Simuler cet achat", systemImage: "testtube.2").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .accessibilityHint("Achat fictif dans le portefeuille simulé, sans argent réel")
            }
        }
        .task(id: asset.id) { await load() }
        .sheet(isPresented: $simulating) {
            if let v = verdict.value { PaperBuySheet(asset: asset, verdict: v) }
        }
    }

    private var tone: Tone? { verdict.value.map { $0.action.tone } }

    private func load() async {
        guard let client = model.client else { return }
        if verdict.value == nil { verdict = .loading }
        do {
            let v = try await client.verdict(asset)
            verdict = .loaded(v)
            // Compared with the last verdict seen for this asset (Radar → Changements de configuration).
            model.recordVerdict(v, asset: asset)
        } catch {
            verdict = .failed(error.localizedDescription)
        }
    }
}

/// « Prochains résultats »: the quarterly results of these stocks within 3 weeks (Nasdaq's public calendar, read by
/// the iPhone). Around results the price often moves a lot, both ways: said plainly, never a reason to buy.
struct EarningsCard: View {
    @Environment(AppModel.self) private var model
    let symbols: [String]
    /// The asset page says when nothing is announced; Mes avoirs shows nothing then.
    var showsEmpty = false
    @State private var events: [EarningsEvent]?

    var body: some View {
        Group {
            if let events, !events.isEmpty {
                Card(title: "Prochains résultats", glow: Theme.warning) {
                    ForEach(events) { e in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(alignment: .firstTextBaseline) {
                                Text(e.symbol).font(Theme.mono(14, weight: .bold))
                                Text(e.dayText).font(.subheadline)
                                Spacer(minLength: 4)
                                Badge(text: Self.when(e), tone: e.daysAway() <= 3 ? .warn : .neutral)
                            }
                            if let f = e.epsForecast {
                                Text("Bénéfice par action attendu \(f)\(e.lastYearEPS.map { " (un an plus tôt : \($0))" } ?? "")")
                                    .font(.caption).foregroundStyle(Theme.textSecondary)
                            }
                        }
                        .accessibilityElement(children: .combine)
                    }
                    Text("Le cours bouge souvent fortement le lendemain des résultats, dans un sens ou dans l'autre : taille prudente autour de ces dates.")
                        .font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
            } else if showsEmpty, events != nil {
                Card(title: "Prochains résultats") {
                    Text("Aucun résultat annoncé dans les 3 prochaines semaines (calendrier Nasdaq).")
                        .font(.caption).foregroundStyle(Theme.textSecondary)
                }
            }
        }
        .task(id: symbols.joined(separator: ",")) {
            guard let client = model.client, !symbols.isEmpty else { return }
            events = (try? await client.earnings(symbols)) ?? []
        }
    }

    static func when(_ e: EarningsEvent) -> String {
        switch e.daysAway() {
        case ..<1: return "AUJOURD'HUI"
        case 1: return "DEMAIN"
        case let d: return "DANS \(d) J"
        }
    }
}

/// The trained bots' readings for this asset, each with what the lab measured about it.
struct BotsBlock: View {
    let verdict: TrendEngine.Verdict
    @State private var details = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Bots entraînés sur l'iPhone").font(.subheadline.weight(.semibold))
            if let b = verdict.bot {
                KeyValue(key: "Bot Tendance · 20 jours", value: pct(b))
            }
            if let r = verdict.botRisk {
                KeyValue(key: "Bot Risque · pire baisse 20 jours", value: pct(r))
                Text(riskText(r)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            if let line = trainedLine {
                Text(line).font(.caption2).foregroundStyle(Theme.textSecondary)
            }
            DisclosureGroup(isExpanded: $details) {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(BotEngine.bots, id: \.name) { bot in
                        Text("\(bot.name) — prévoit \(bot.predicts). Rôle : \(bot.use). Mesuré sans regarder le futur : \(bot.measured).")
                            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                    Text("Ils apprennent, sur l'historique des actifs de leur marché, ce qui a suivi chaque journée, et sont réentraînés toutes les 12 heures. Testés et écartés : un bot linéaire et un bot à 5 jours, sans avantage démontré. Des prévisions moyennes, jamais des certitudes.")
                        .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                .padding(.top, 4)
            } label: {
                Text("Comment ils sont entraînés et ce qu'ils valent").font(.caption)
            }
        }
        .padding(.top, 4)
    }

    /// "Entraînés il y a 3 h sur 13 480 journées" (nil before the bots exist).
    private var trainedLine: String? {
        guard let at = verdict.botTrainedAt, let n = verdict.botExamples else { return nil }
        let hours = max(0, Int((Date().timeIntervalSince1970 * 1000 - at) / 3_600_000))
        let when = hours == 0 ? "il y a moins d'une heure" : "il y a \(hours) h"
        return "Entraînés \(when) sur \(Format.plain(Double(n), digits: 0)) journées de l'historique de leur marché"
    }

    private func pct(_ x: Double) -> String { "\(x >= 0 ? "+" : "−")\(Format.plain(abs(x) * 100, digits: 1)) %" }

    /// Size that keeps the typical fall near the accepted risk (Réglages) of the amount planned.
    private func riskText(_ r: Double) -> String {
        let budget = BotEngine.riskBudget
        let size = BotEngine.size(forFall: r, budget: budget) ?? 100
        return "Baisse typique attendue au pire moment des 20 prochains jours. Pour ne risquer qu'environ \(Int(budget * 100)) % de la somme prévue (Réglages), n'en engager qu'environ \(size) %."
    }
}
