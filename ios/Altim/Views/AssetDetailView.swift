import SwiftUI
import Charts
import AltimCore

struct AssetDetailView: View {
    @Environment(AppServices.self) private var services
    @Environment(AppSettings.self) private var settings
    @State private var model: AssetViewModel

    init(asset: Asset, timeframe: Timeframe) {
        _model = State(initialValue: AssetViewModel(asset: asset, timeframe: timeframe))
    }

    var body: some View {
        ZStack {
            CyberGridBackground()
            ScrollView {
                VStack(spacing: 16) {
                    priceHeader
                    Picker("Unité de temps", selection: $model.timeframe) {
                        ForEach(Timeframe.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)

                    if let error = model.error {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .font(.subheadline).foregroundStyle(Theme.warning)
                            .glassCard(glow: Theme.warning)
                    }

                    if !model.candles.isEmpty {
                        PriceChart(candles: model.candles, plan: model.signal?.plan,
                                   trades: model.backtest?.trades ?? [])
                            .frame(height: 260)
                            .glassCard()
                    } else if model.isLoading {
                        ProgressView("Analyse en cours…").tint(Theme.cyan).frame(height: 260)
                    }

                    if let advice = model.advice { AdviceCard(advice: advice, held: model.heldLine != nil) }
                    if let g = model.guardResult { GuardCard(result: g, headlines: model.headlines, asset: model.asset) }
                    if let signal = model.signal { SignalCard(signal: signal) }
                    if let snapshot = model.snapshot { ReliabilityCard(snapshot: snapshot) }
                    if model.fearGreed != nil || model.social?.bullishPercent != nil {
                        SentimentCard(fearGreed: model.fearGreed, social: model.social)
                    }
                    if let backtest = model.backtest { BacktestCard(result: backtest, timeframe: model.timeframe) }
                }
                .padding()
            }
            .refreshable { await reload() }
        }
        .navigationTitle(model.asset.name)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: model.timeframe) { await reload() }
        .task(id: model.asset.id) { await services.live.watch([model.asset]) }
    }

    private func reload() async {
        async let guardLoad: Void = model.loadGuard(services: services)
        await model.load(services: services)
        await model.loadAdvice(services: services, risk: settings.risk)
        await guardLoad
        // The advice takes the market guard into account once it is known.
        await model.loadAdvice(services: services, risk: settings.risk)
    }

    private var live: LiveTick? { services.live.ticks[model.asset.id] }

    private var priceHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(model.asset.symbol).font(Theme.mono(13)).foregroundStyle(Theme.textSecondary)
                if let price = live?.price ?? model.analysis?.price {
                    LivePriceText(price: price, size: 34, weight: .bold)
                        .neonGlow(Theme.cyan, radius: 6)
                }
                LiveBadge(lastTick: live?.time)
                if let live {
                    Text(live.marketOpen == false ? "Bourse fermée · \(live.agreeing)/\(live.total) sources" : "\(live.agreeing)/\(live.total) sources en direct")
                        .font(Theme.mono(10, weight: .regular)).foregroundStyle(live.marketOpen == false ? Theme.warning : Theme.textSecondary)
                }
            }
            Spacer()
            if let change = live?.change ?? model.analysis?.change24h {
                Text(Format.percent(change))
                    .font(Theme.mono(15))
                    .foregroundStyle(Theme.color(forChange: change))
                    .padding(8)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Theme.color(forChange: change).opacity(0.12)))
            }
        }
    }
}

// MARK: - Conseil

/// Altim's advice in plain language: Altim never places orders.
struct AdviceCard: View {
    let advice: Advisor.Advice
    let held: Bool

    private var color: Color {
        switch advice.tone {
        case .buy: return Theme.buy
        case .sell: return Theme.sell
        case .hold: return Theme.cyan
        case .unknown: return Theme.warning
        }
    }

    private var icon: String {
        switch advice.tone {
        case .buy: return "arrow.up.right.circle.fill"
        case .sell: return "shield.lefthalf.filled"
        case .hold: return "pause.circle.fill"
        case .unknown: return "questionmark.circle.fill"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: held ? "Le conseil d'Altim · vous en détenez" : "Le conseil d'Altim")
            Label(advice.title, systemImage: icon)
                .font(.title3.bold())
                .foregroundStyle(color)
            VStack(alignment: .leading, spacing: 8) {
                ForEach(advice.points, id: \.self) { point in
                    HStack(alignment: .top, spacing: 8) {
                        Circle().fill(color).frame(width: 5, height: 5).padding(.top, 7)
                        Text(point).font(.subheadline).foregroundStyle(.white.opacity(0.88))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            if let amount = advice.amount {
                HStack {
                    Text("Montant prudent").font(.caption).foregroundStyle(Theme.textSecondary)
                    Spacer()
                    Text(Format.price(amount)).font(Theme.mono(15, weight: .bold)).foregroundStyle(color)
                }
            }
            Text("Conseil indicatif : Altim ne passe aucun ordre. Vous décidez, chez votre courtier habituel.")
                .font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard(glow: color)
    }
}

// MARK: - Graphique

struct PriceChart: View {
    let candles: [Candle]
    let plan: TradePlan?
    let trades: [Backtester.Trade]

    private struct Point: Identifiable {
        let time: Date
        let close: Double
        let ema20: Double?
        let ema50: Double?
        var id: Date { time }
    }

    var body: some View {
        let closes = candles.closes
        let e20 = Indicators.ema(closes, period: 20)
        let e50 = Indicators.ema(closes, period: 50)
        let start = max(0, candles.count - 150)
        let points = (start..<candles.count).map { Point(time: candles[$0].time, close: closes[$0], ema20: e20[$0], ema50: e50[$0]) }
        let firstTime = points.first?.time ?? .distantPast
        let visibleTrades = trades.filter { $0.entryTime >= firstTime }
        let lows = points.map(\.close) + [plan?.stopLoss].compactMap { $0 }
        let highs = points.map(\.close) + [plan?.takeProfit].compactMap { $0 }
        let lo = (lows.min() ?? 0) * 0.995
        let hi = max((highs.max() ?? 1) * 1.005, lo + 1e-9)

        Chart {
            ForEach(points) { p in
                AreaMark(x: .value("Date", p.time), yStart: .value("Bas", lo), yEnd: .value("Prix", p.close))
                    .foregroundStyle(LinearGradient(colors: [Theme.cyan.opacity(0.28), .clear], startPoint: .top, endPoint: .bottom))
                LineMark(x: .value("Date", p.time), y: .value("Prix", p.close), series: .value("Série", "Prix"))
                    .foregroundStyle(Theme.cyan)
                    .lineStyle(StrokeStyle(lineWidth: 2))
                if let v = p.ema20 {
                    LineMark(x: .value("Date", p.time), y: .value("EMA 20", v), series: .value("Série", "EMA 20"))
                        .foregroundStyle(Theme.magenta.opacity(0.8))
                        .lineStyle(StrokeStyle(lineWidth: 1))
                }
                if let v = p.ema50 {
                    LineMark(x: .value("Date", p.time), y: .value("EMA 50", v), series: .value("Série", "EMA 50"))
                        .foregroundStyle(Theme.violet.opacity(0.9))
                        .lineStyle(StrokeStyle(lineWidth: 1))
                }
            }
            ForEach(visibleTrades) { t in
                PointMark(x: .value("Date", t.entryTime), y: .value("Entrée", t.entryPrice))
                    .symbol(.triangle)
                    .foregroundStyle(Theme.buy)
                    .symbolSize(40)
            }
            if let plan {
                RuleMark(y: .value("Stop", plan.stopLoss))
                    .foregroundStyle(Theme.sell)
                    .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 4]))
                    .annotation(position: .bottom, alignment: .leading) {
                        Text("STOP \(Format.price(plan.stopLoss))").font(Theme.mono(9)).foregroundStyle(Theme.sell)
                    }
                RuleMark(y: .value("Objectif", plan.takeProfit))
                    .foregroundStyle(Theme.buy)
                    .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 4]))
                    .annotation(position: .top, alignment: .leading) {
                        Text("OBJECTIF \(Format.price(plan.takeProfit))").font(Theme.mono(9)).foregroundStyle(Theme.buy)
                    }
            }
        }
        .chartYScale(domain: lo...hi)
        .chartXAxis {
            AxisMarks(values: .automatic(desiredCount: 4)) { _ in
                AxisGridLine().foregroundStyle(Color.white.opacity(0.06))
                AxisValueLabel().foregroundStyle(Theme.textSecondary)
            }
        }
        .chartYAxis {
            AxisMarks(position: .trailing) { _ in
                AxisGridLine().foregroundStyle(Color.white.opacity(0.06))
                AxisValueLabel().foregroundStyle(Theme.textSecondary)
            }
        }
    }
}

// MARK: - Carte signal

struct SignalCard: View {
    let signal: Signal

    var body: some View {
        let color = Theme.color(for: signal.action)
        VStack(spacing: 14) {
            SectionTitle(text: "Signal Altim")
            HStack(alignment: .center) {
                ScoreGauge(score: signal.score, confidence: signal.confidence)
                Spacer()
                VStack(alignment: .trailing, spacing: 8) {
                    ActionBadge(action: signal.action)
                    Text("Bougie du \(signal.time.formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption2).foregroundStyle(Theme.textSecondary)
                }
            }

            VStack(spacing: 10) {
                ForEach(signal.factors) { factor in
                    FactorRow(factor: factor)
                }
            }

            if let plan = signal.plan, signal.action != .hold {
                HStack {
                    planCell("ENTRÉE", plan.entry, .white)
                    planCell("STOP", plan.stopLoss, Theme.sell)
                    planCell("OBJECTIF", plan.takeProfit, Theme.buy)
                }
                Text(String(format: "Ratio gain/risque 1 : %.1f", plan.riskReward))
                    .font(Theme.mono(11)).foregroundStyle(Theme.textSecondary)
            }

            ForEach(signal.warnings, id: \.self) { warning in
                Label(warning, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(Theme.warning)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .glassCard(glow: color)
    }

    private func planCell(_ title: String, _ value: Double, _ color: Color) -> some View {
        VStack(spacing: 4) {
            Text(title).font(Theme.mono(10)).foregroundStyle(Theme.textSecondary)
            Text(Format.price(value)).font(Theme.mono(13)).foregroundStyle(color)
                .minimumScaleFactor(0.6).lineLimit(1)
        }
        .frame(maxWidth: .infinity)
    }
}

struct FactorRow: View {
    let factor: SignalFactor

    var body: some View {
        let color = factor.score > 0 ? Theme.buy : factor.score < 0 ? Theme.sell : Theme.textSecondary
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(factor.name).font(.subheadline.weight(.semibold))
                Spacer()
                Text(factor.detail).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
            }
            GeometryReader { geo in
                let half = geo.size.width / 2
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.07))
                    Rectangle().fill(Color.white.opacity(0.25)).frame(width: 1).offset(x: half)
                    Capsule()
                        .fill(color)
                        .frame(width: max(2, half * abs(factor.score)))
                        .offset(x: factor.score >= 0 ? half : half - half * abs(factor.score))
                        .neonGlow(color, radius: 4)
                }
            }
            .frame(height: 5)
        }
    }
}

// MARK: - Carte backtest

struct BacktestCard: View {
    let result: Backtester.Result
    let timeframe: Timeframe

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: "Backtest · \(timeframe.label) · historique chargé")
            HStack {
                metric("Stratégie", Format.percent(result.totalReturnPercent), Theme.color(forChange: result.totalReturnPercent))
                metric("Achat-conservation", Format.percent(result.buyAndHoldPercent), Theme.color(forChange: result.buyAndHoldPercent))
            }
            HStack {
                metric("Trades", "\(result.trades.count)", .white)
                metric("Réussite", Format.percent(result.winRatePercent, signed: false), .white)
                metric("Drawdown max", Format.percent(-result.maxDrawdownPercent), Theme.sell)
            }
            Text(verdict)
                .font(.caption)
                .foregroundStyle(Theme.textSecondary)
        }
        .glassCard(glow: Theme.violet)
    }

    private var verdict: String {
        if result.trades.count < 5 {
            return "Trop peu de trades pour conclure : prudence."
        }
        if result.beatsBuyAndHold {
            return "La stratégie a fait mieux que l'achat-conservation sur cette période (frais de 0,1 % inclus). Les performances passées ne préjugent pas des performances futures."
        }
        return "Sur cette période, conserver l'actif a mieux rapporté que suivre les signaux. La stratégie sert surtout à limiter les pertes en marché baissier."
    }

    private func metric(_ title: String, _ value: String, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption2).foregroundStyle(Theme.textSecondary)
            Text(value).font(Theme.mono(15)).foregroundStyle(color)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - Reliability

struct ReliabilityBadge: View {
    let level: ReliabilityLevel

    var color: Color {
        switch level {
        case .high: return Theme.buy
        case .medium: return Theme.warning
        case .low: return Theme.sell
        }
    }

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: level == .high ? "checkmark.shield.fill" : level == .medium ? "exclamationmark.shield.fill" : "xmark.shield.fill")
            Text(level.label)
        }
        .font(.system(size: 11, weight: .bold, design: .rounded))
        .foregroundStyle(color)
    }
}

struct ReliabilityCard: View {
    let snapshot: MarketSnapshot
    @State private var expanded = false

    var body: some View {
        let badge = ReliabilityBadge(level: snapshot.reliability)
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: "Fiabilité des données")
            HStack {
                badge
                Spacer()
                Text("\(Int(snapshot.reliabilityScore))/100").font(Theme.mono(15)).foregroundStyle(badge.color)
            }
            Text(snapshot.summary).font(.caption).foregroundStyle(Theme.textSecondary)
            Text("Analyse basée sur \(snapshot.primarySource), recoupée avec les autres sources.")
                .font(.caption).foregroundStyle(Theme.textSecondary)

            Button {
                withAnimation(.spring(response: 0.3)) { expanded.toggle() }
            } label: {
                Label(expanded ? "Masquer les sources" : "Voir les \(snapshot.checks.count) sources",
                      systemImage: expanded ? "chevron.up" : "chevron.down")
                    .font(.caption.bold())
            }

            if expanded {
                VStack(spacing: 8) {
                    ForEach(snapshot.checks) { check in
                        SourceRow(check: check)
                    }
                }
                ForEach(snapshot.quality.issues, id: \.self) { issue in
                    Label(issue, systemImage: "exclamationmark.triangle")
                        .font(.caption).foregroundStyle(Theme.warning)
                }
            }
        }
        .glassCard(glow: badge.color)
    }
}

struct SourceRow: View {
    let check: SourceCheck

    var body: some View {
        let (icon, color, text): (String, Color, String) = {
            switch check.status {
            case .primary: return ("star.circle.fill", Theme.cyan, "principale")
            case .agrees: return ("checkmark.circle.fill", Theme.buy, "concordante")
            case .diverges: return ("xmark.circle.fill", Theme.sell, "écartée")
            case .failed(let m): return ("wifi.exclamationmark", Theme.warning, m)
            case .skipped: return ("circle.dotted", Theme.textSecondary, "en réserve")
            }
        }()
        HStack(alignment: .top) {
            Image(systemName: icon).foregroundStyle(color)
            VStack(alignment: .leading, spacing: 2) {
                Text(check.name).font(.subheadline.weight(.semibold))
                Text(text).font(.caption2).foregroundStyle(Theme.textSecondary).lineLimit(2)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                if let p = check.lastPrice { Text(Format.price(p)).font(Theme.mono(12)) }
                if let d = check.deviationPercent {
                    Text(String(format: "écart %.3f %%", d)).font(Theme.mono(10)).foregroundStyle(Theme.textSecondary)
                }
            }
        }
    }
}

struct SentimentCard: View {
    let fearGreed: FearGreedIndex?
    let social: SocialSentiment?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: "Sentiment du marché")
            if let fg = fearGreed {
                HStack {
                    Text("Fear & Greed crypto").font(.subheadline)
                    Spacer()
                    Text("\(fg.value) · \(fg.label)").font(Theme.mono(13))
                        .foregroundStyle(fg.value < 45 ? Theme.sell : fg.value > 55 ? Theme.buy : Theme.cyan)
                }
            }
            if let s = social, let bull = s.bullishPercent {
                HStack {
                    Text("StockTwits").font(.subheadline)
                    Spacer()
                    Text(String(format: "%.0f %% haussier (%d avis)", bull, s.sampleSize)).font(Theme.mono(13))
                        .foregroundStyle(bull >= 50 ? Theme.buy : Theme.sell)
                }
            }
            Text("Information de contexte, non intégrée au score : la foule se trompe souvent aux extrêmes.")
                .font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .glassCard(glow: Theme.magenta)
    }
}
