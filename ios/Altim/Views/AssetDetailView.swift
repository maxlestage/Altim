import SwiftUI
import Charts
import AltimCore

struct AssetDetailView: View {
    @Environment(AppServices.self) private var services
    @State private var model: AssetViewModel
    @State private var trade: TradeIntent?

    struct TradeIntent: Identifiable {
        let side: OrderSide
        var id: String { side.rawValue }
    }

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

                    if let signal = model.signal { SignalCard(signal: signal) }
                    if let backtest = model.backtest { BacktestCard(result: backtest, timeframe: model.timeframe) }

                    HStack(spacing: 12) {
                        Button("ACHETER") { trade = TradeIntent(side: .buy) }
                            .buttonStyle(NeonButtonStyle(color: Theme.buy))
                        Button("VENDRE") { trade = TradeIntent(side: .sell) }
                            .buttonStyle(NeonButtonStyle(color: Theme.sell, filled: false))
                    }
                    .disabled(model.signal == nil)
                    .padding(.top, 4)
                }
                .padding()
            }
            .refreshable { await model.load(services: services) }
        }
        .navigationTitle(model.asset.name)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: model.timeframe) { await model.load(services: services) }
        .sheet(item: $trade) { intent in
            TradeSheet(asset: model.asset, side: intent.side, signal: model.signal,
                       price: model.quote?.price ?? model.signal?.price ?? 0)
        }
    }

    private var priceHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(model.asset.symbol).font(Theme.mono(13)).foregroundStyle(Theme.textSecondary)
                if let price = model.quote?.price ?? model.signal?.price {
                    Text(Format.price(price))
                        .font(Theme.mono(34, weight: .bold))
                        .neonGlow(Theme.cyan, radius: 6)
                        .contentTransition(.numericText())
                }
            }
            Spacer()
            if let change = model.quote?.changePercent24h {
                Text(Format.percent(change))
                    .font(Theme.mono(15))
                    .foregroundStyle(Theme.color(forChange: change))
                    .padding(8)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Theme.color(forChange: change).opacity(0.12)))
            }
        }
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
