import SwiftUI
import Charts
import AltimKit

/// "Comparer les stratégies" (asset page): textbook strategies with fixed parameters on this asset's daily history
/// (/api/strategies). Chips to pick them, one single-axis chart (value of 100 invested) or its list view, then one
/// stacked card of metrics per strategy and how the test avoids flattering itself. Same content as the web
/// (StrategiesCard.tsx): one colour per strategy, buy and hold as the dashed neutral reference, a legend and direct
/// labels (up to 4 coloured curves), readout on touch. Nothing scrolls sideways.
struct StrategiesCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @State private var report: StrategiesReport?
    @State private var error: String?
    @State private var selected: [StrategyID] = Strategies.defaultSelection
    @State private var asList = false

    var body: some View {
        Card(title: "Comparer les stratégies") {
            Text("Comment des stratégies classiques, avec leurs réglages de manuel, se seraient comportées sur l'historique de \(asset.symbol).")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if let error { Notice(text: error, tone: .warn) }
            if report == nil && error == nil {
                ProgressView().frame(maxWidth: .infinity, minHeight: 80).accessibilityLabel("Chargement de la comparaison")
            }
            if let report { content(report) }
        }
        .task(id: asset.id) { await load() }
    }

    @ViewBuilder private func content(_ report: StrategiesReport) -> some View {
        let series = Strategies.drawable(report, selected: selected)
        WrapLayout(spacing: 6) {
            ForEach(Strategies.order.filter { report.strategy($0) != nil }, id: \.self) { id in
                let on = selected.contains(id)
                Button {
                    selected = Strategies.toggle(id, in: selected)
                } label: {
                    HStack(spacing: 6) {
                        StrategySwatch(id: id)
                        Text(Strategies.shortName(id)).font(.caption).foregroundStyle(on ? .white : Theme.textSecondary)
                    }
                    .padding(.horizontal, 10).padding(.vertical, 7)
                    .background(Color.white.opacity(on ? 0.1 : 0.04), in: Capsule())
                    .overlay(Capsule().strokeBorder(on ? StrategyStyle.color(id) : Color.clear, lineWidth: 1))
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(report.strategy(id)?.name ?? Strategies.shortName(id))
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        (Text("Période testée").bold().foregroundStyle(.white) + Text(" : \(report.period) · source \(report.source)").foregroundStyle(Theme.textSecondary))
            .font(.caption).fixedSize(horizontal: false, vertical: true)
        if series.isEmpty {
            Text("Choisissez au moins une stratégie disponible.").font(.caption).foregroundStyle(Theme.textSecondary)
        } else if asList {
            StrategyListView(series: series)
        } else {
            StrategyChart(series: series)
        }
        if !series.isEmpty {
            Button(asList ? "Voir le graphique" : "Voir en liste") { asList.toggle() }
                .font(.footnote)
                .buttonStyle(.borderless)
        }
        ForEach(Strategies.chosen(report, selected: selected)) { StrategyBlock(strategy: $0) }
        Text("Comment ce test évite de se flatter").font(.footnote.weight(.semibold)).foregroundStyle(.white)
        ForEach(report.notes, id: \.self) { n in
            Text("• \(n)").font(.caption).foregroundStyle(.white.opacity(0.85)).fixedSize(horizontal: false, vertical: true)
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        do {
            report = try await client.strategies(asset)
            error = nil
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            if report == nil { self.error = error.localizedDescription }
        }
    }
}

enum StrategyStyle {
    static func color(_ id: StrategyID) -> Color {
        let hex = Strategies.colorHex[id] ?? 0x9AA0B4
        return Color(red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }

    static func stroke(_ id: StrategyID, width: CGFloat = 1.8) -> StrokeStyle {
        id == Strategies.reference ? StrokeStyle(lineWidth: 2, lineCap: .round, dash: [5, 4]) : StrokeStyle(lineWidth: width, lineCap: .round, lineJoin: .round)
    }
}

/// Short line in the strategy's colour, dashed for the buy-and-hold reference (legend, chips, readout).
struct StrategySwatch: View {
    let id: StrategyID

    private struct Line: Shape {
        func path(in rect: CGRect) -> Path {
            var p = Path()
            p.move(to: CGPoint(x: rect.minX, y: rect.midY))
            p.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
            return p
        }
    }

    var body: some View {
        Line()
            .stroke(StrategyStyle.color(id), style: StrokeStyle(lineWidth: 3, lineCap: .round, dash: id == Strategies.reference ? [3, 3] : []))
            .frame(width: 14, height: 3)
            .accessibilityHidden(true)
    }
}

/// One axis (value of 100 invested), 100 always inside; direct labels at the end of the curves up to 4 coloured ones.
private struct StrategyChart: View {
    let series: [StrategyResult]
    @State private var selected: Int?

    private struct Point: Identifiable {
        let id: String
        let i: Int
        let v: Double
        let strategy: StrategyID
        let name: String
    }

    private var points: [Point] {
        series.flatMap { s in s.equity.enumerated().map { Point(id: "\(s.id.rawValue)-\($0.offset)", i: $0.offset, v: $0.element.v, strategy: s.id, name: s.name) } }
    }

    private var coloured: Int { series.filter { $0.id != Strategies.reference }.count }
    private var labeled: Bool { coloured <= Strategies.maxLabeled }

    private func value(_ s: StrategyResult, _ i: Int) -> Double { s.equity[min(i, s.equity.count - 1)].v }

    var body: some View {
        let n = series.map(\.equity.count).max() ?? 0
        let g = Strategies.geometry(series, box: Strategies.Box(w: 1, h: 1, l: 0, r: 0, t: 0, b: 0))
        if let g, n > 1, let first = series.first {
            let at = min(max(selected ?? n - 1, 0), n - 1)
            VStack(alignment: .leading, spacing: 8) {
                // Readout of the touched date (the last one by default), above the chart so the finger does not hide it.
                WrapLayout(spacing: 8) {
                    Text("\(Strategies.shortDate(first.equity[min(at, first.equity.count - 1)].t)) · valeur de 100 investis :")
                        .font(.caption.weight(.semibold)).foregroundStyle(.white)
                    ForEach(series) { s in
                        HStack(spacing: 4) {
                            StrategySwatch(id: s.id)
                            Text(Strategies.shortName(s.id)).font(.caption).foregroundStyle(Theme.textSecondary)
                            Text(Strategies.value100(value(s, at))).font(.caption.monospacedDigit().weight(.bold)).foregroundStyle(.white)
                        }
                    }
                }
                .accessibilityElement(children: .combine)
                chart(g: g, n: n)
                HStack {
                    Text(Strategies.shortDate(first.equity[0].t))
                    Spacer()
                    Text(Strategies.shortDate(first.equity[first.equity.count - 1].t))
                }
                .font(.caption2).foregroundStyle(Theme.textSecondary)
                .padding(.trailing, labeled ? 72 : 0)
                ForEach(series) { s in
                    HStack(spacing: 8) {
                        StrategySwatch(id: s.id)
                        Text(s.name).font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 6)
                        Text(Strategies.signedPct(s.metrics?.totalReturn)).font(.footnote.monospacedDigit().weight(.semibold)).foregroundStyle(.white)
                    }
                    .accessibilityElement(children: .combine)
                }
                if !labeled {
                    Text("Plus de \(Strategies.maxLabeled) courbes : touchez le graphique pour lire les valeurs, ou passez en liste.")
                        .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func chart(g: Strategies.Geometry, n: Int) -> some View {
        Chart {
            RuleMark(y: .value("Investi", 100)).foregroundStyle(.white.opacity(0.25)).lineStyle(StrokeStyle(lineWidth: 1))
            ForEach(points) { p in
                LineMark(x: .value("Jour", p.i), y: .value("Valeur de 100 investis", p.v), series: .value("Stratégie", p.name))
                    .foregroundStyle(by: .value("Stratégie", p.name))
                    .lineStyle(StrategyStyle.stroke(p.strategy))
            }
            if let selected {
                let i = min(max(selected, 0), n - 1)
                RuleMark(x: .value("Jour", i)).foregroundStyle(.white.opacity(0.35))
                ForEach(series) { s in
                    PointMark(x: .value("Jour", min(i, s.equity.count - 1)), y: .value("Valeur de 100 investis", value(s, i)))
                        .foregroundStyle(StrategyStyle.color(s.id))
                        .symbolSize(36)
                }
            }
        }
        .chartForegroundStyleScale(domain: series.map(\.name), range: series.map { StrategyStyle.color($0.id) })
        .chartLegend(.hidden)
        .chartXScale(domain: 0...(n - 1), range: .plotDimension(startPadding: 0, endPadding: labeled ? 72 : 4))
        .chartYScale(domain: g.lo...g.hi)
        .chartXAxis(.hidden)
        .chartYAxis {
            AxisMarks(position: .leading, values: [g.lo, 100, g.hi]) { v in
                AxisGridLine().foregroundStyle(.white.opacity(0.06))
                AxisValueLabel { if let d = v.as(Double.self) { Text(Strategies.value100(d)) } }
            }
        }
        .chartXSelection(value: $selected)
        .chartOverlay { proxy in
            GeometryReader { geo in
                if labeled, let anchor = proxy.plotFrame {
                    let plot = geo[anchor]
                    let x = plot.minX + (proxy.position(forX: n - 1) ?? plot.width) + 6
                    let ends = series.map { plot.minY + (proxy.position(forY: $0.equity[$0.equity.count - 1].v) ?? 0) }
                    let ys = Strategies.placeLabels(ends.map { Double($0) }, gap: 13, top: Double(plot.minY) + 6, bottom: Double(plot.maxY) - 6)
                    ForEach(Array(series.enumerated()), id: \.offset) { item in
                        HStack(spacing: 3) {
                            StrategySwatch(id: item.element.id)
                            Text(Strategies.shortName(item.element.id)).font(.system(size: 10)).foregroundStyle(Theme.textSecondary).lineLimit(1)
                        }
                        .frame(width: 66, alignment: .leading)
                        .position(x: x + 33, y: ys[item.offset])
                    }
                }
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
        .frame(height: 220)
        .accessibilityElement()
        .accessibilityLabel("Valeur de 100 investis à la fin de la période : "
                            + series.map { "\($0.name) \(Strategies.value100($0.equity[$0.equity.count - 1].v))" }.joined(separator: ", "))
    }
}

/// Stacked rows: the value of 100 invested at five dates of the period, per strategy.
private struct StrategyListView: View {
    let series: [StrategyResult]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(series) { s in
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) {
                        StrategySwatch(id: s.id)
                        (Text(s.name).bold().foregroundStyle(.white) + Text(" · \(Strategies.signedPct(s.metrics?.totalReturn))").foregroundStyle(.white.opacity(0.9)))
                            .font(.footnote).fixedSize(horizontal: false, vertical: true)
                    }
                    WrapLayout(spacing: 12) {
                        ForEach(Strategies.checkpoints(s), id: \.t) { c in
                            VStack(alignment: .leading, spacing: 1) {
                                Text(Strategies.shortDate(c.t)).font(.caption2).foregroundStyle(Theme.textSecondary)
                                Text(Strategies.value100(c.v)).font(.caption.monospacedDigit().weight(.bold)).foregroundStyle(.white)
                            }
                            .frame(minWidth: 84, alignment: .leading)
                        }
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.white.opacity(0.08), lineWidth: 1))
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// One strategy: rule, fixed parameters, metrics in a stacked card, results by market regime, note.
private struct StrategyBlock: View {
    let strategy: StrategyResult

    var body: some View {
        let s = strategy
        VStack(alignment: .leading, spacing: 5) {
            WrapLayout(spacing: 6) {
                HStack(spacing: 6) {
                    StrategySwatch(id: s.id)
                    Text(s.name).font(.footnote.weight(.bold)).foregroundStyle(.white)
                }
                if let low = Strategies.lowSampleText(s) { TagChip(text: low) }
            }
            Text(s.rule).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            Text("Paramètres fixes : \(s.params)").font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if !s.available || s.metrics == nil {
                Notice(text: s.unavailable ?? "Non couvert.", tone: .neutral)
            } else {
                ForEach(Array(Strategies.metricRows(s).enumerated()), id: \.offset) { item in
                    let r = item.element
                    DecisionRow(key: r.label, value: r.value, tone: r.tone > 0 ? .good : r.tone < 0 ? .bad : nil)
                }
                if !s.regimes.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(Array(s.regimes.enumerated()), id: \.offset) { item in
                            let g = Strategies.regimeRow(item.element, dca: s.id == .dca)
                            DecisionRow(key: g.label, value: g.value)
                            if g.lowSample { TagChip(text: "échantillon trop faible") }
                        }
                    }
                    .padding(.top, 4)
                }
            }
            if let note = s.note {
                Text(note).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.white.opacity(0.02)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.white.opacity(0.1), lineWidth: 1))
    }
}
