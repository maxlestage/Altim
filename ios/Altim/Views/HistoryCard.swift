import SwiftUI
import Charts
import AltimKit

/// "How did what I own now behave": value of today's lines over the period, against Bitcoin and the S&P 500.
struct HistoryCard: View {
    @Environment(AppModel.self) private var model
    @AppStorage("history.days") private var days = 90
    @State private var data: HistoryResponse?
    @State private var error: String?

    private var assets: [Asset] {
        var seen = Set<String>()
        return model.holdings.map(\.asset).filter { seen.insert($0.id).inserted }
    }

    private var history: PortfolioHistory? {
        guard let data else { return nil }
        let lines = Dictionary(model.holdings.map { ($0.asset.id, $0.quantity) }, uniquingKeysWith: +).map { ($0.key, $0.value) }
        return PortfolioHistory.compute(lines, series: data.byId, days: days)
    }

    var body: some View {
        Card(title: "Évolution de mes lignes") {
            Picker("Période", selection: $days) {
                Text("30 j").tag(30)
                Text("90 j").tag(90)
                Text("1 an").tag(365)
            }
            .pickerStyle(.segmented)
            if let error {
                Notice(text: error, tone: .bad)
            } else if data == nil {
                ProgressView("Chargement de l'historique…").frame(maxWidth: .infinity)
            } else if let h = history {
                HistoryBody(h: h)
            } else {
                Text("Pas assez d'historique pour vos lignes sur cette période.").font(.footnote).foregroundStyle(Theme.textSecondary)
            }
        }
        .task(id: "\(days)|\(assets.map(\.id).joined(separator: ","))") {
            guard let client = model.client else { return }
            data = nil
            do {
                data = try await client.history(assets, days: days)
                error = nil
            } catch AltimError.unauthorized {
                model.sessionLost()
            } catch is CancellationError {
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

// Categorical palette validated for the allocation (dark surface, colour-blind readers): same entity, same colour.
private let mineColor = Theme.allocCrypto
private let benchColors: [String: Color] = ["crypto:BTC": Theme.allocStock, "stock:SPY": Theme.allocCash]

private func pct(_ v: Double) -> String {
    let s = abs(v).formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "fr_FR")))
    return "\(v >= 0 ? "+" : "−")\(s) %"
}

/// A dollar value in the display currency, whole: "12 480 €".
private func usd(_ v: Double) -> String { Money.money(v, min: 0, max: 0, sep: " ") }

private func day(_ t: Double, year: Bool = false) -> String {
    let f = DateFormatter()
    f.locale = Locale(identifier: "fr_FR")
    f.timeZone = TimeZone(identifier: "UTC")
    f.dateFormat = year ? "d MMM yyyy" : "d MMM"
    return f.string(from: Date(timeIntervalSince1970: t / 1000))
}

private struct HistoryBody: View {
    let h: PortfolioHistory
    @State private var selected: Int?

    private struct Row: Identifiable { let id = UUID(); let i: Int; let series: String; let pct: Double }

    private var rows: [Row] {
        let mine = h.pct
        return mine.indices.map { Row(i: $0, series: "Mes lignes", pct: mine[$0]) }
            + h.benchmarks.flatMap { b in b.pct.indices.map { Row(i: $0, series: b.label, pct: b.pct[$0]) } }
    }

    var body: some View {
        let mine = h.pct
        let at = min(max(selected ?? mine.count - 1, 0), mine.count - 1)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("\(usd(h.points.first!.value)) → \(usd(h.points.last!.value))").font(.subheadline).foregroundStyle(Theme.textSecondary)
                Spacer()
                Text(pct(h.change)).font(Theme.mono(16, weight: .bold)).foregroundStyle(h.change >= 0 ? Theme.buy : Theme.sell)
            }
            // Readout of the touched day (the last one by default), above the chart so the finger does not hide it.
            Text("\(day(h.points[at].t)) · Mes lignes \(usd(h.points[at].value)) (\(pct(mine[at])))"
                 + h.benchmarks.map { " · \($0.label.components(separatedBy: " (")[0]) \(pct($0.pct[at]))" }.joined())
                .font(.caption).foregroundStyle(Theme.textSecondary)
            Chart {
                RuleMark(y: .value("Départ", 0)).foregroundStyle(.white.opacity(0.18)).lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                ForEach(rows) { r in
                    LineMark(x: .value("Jour", r.i), y: .value("Variation", r.pct), series: .value("Série", r.series))
                        .foregroundStyle(by: .value("Série", r.series))
                        .lineStyle(StrokeStyle(lineWidth: r.series == "Mes lignes" ? 2 : 1.5, lineCap: .round, lineJoin: .round))
                }
                if let selected {
                    RuleMark(x: .value("Jour", selected)).foregroundStyle(.white.opacity(0.35))
                }
            }
            .chartForegroundStyleScale(domain: ["Mes lignes"] + h.benchmarks.map(\.label),
                                       range: [mineColor] + h.benchmarks.map { benchColors[$0.id] ?? .gray })
            .chartLegend(.hidden)
            .chartXAxis {
                AxisMarks(values: [0, mine.count - 1]) { v in
                    AxisValueLabel { if let i = v.as(Int.self) { Text(day(h.points[i].t, year: h.points.count > 200)) } }
                }
            }
            .chartYAxis { AxisMarks(position: .leading, values: .automatic(desiredCount: 3)) { v in
                AxisGridLine().foregroundStyle(.white.opacity(0.06))
                AxisValueLabel { if let p = v.as(Double.self) { Text(pct(p)) } }
            } }
            .chartXSelection(value: $selected)
            .frame(height: 170)
            .accessibilityLabel("Mes lignes \(pct(h.change))" + h.benchmarks.map { ", \($0.label) \(pct($0.change))" }.joined())

            legend(mineColor, "Mes lignes", h.change)
            ForEach(h.benchmarks) { legend(benchColors[$0.id] ?? .gray, $0.label, $0.change) }
            KeyValue(key: "Pire recul depuis un sommet", value: pct(h.maxDrawdown), tone: .bad)
            if let b = h.best { KeyValue(key: "Meilleure journée (\(day(b.t)))", value: pct(b.change), tone: .good) }
            if let w = h.worst { KeyValue(key: "Pire journée (\(day(w.t)))", value: pct(w.change), tone: .bad) }
            Text("Valeur chaque jour des quantités que vous détenez aujourd'hui (cours de clôture) : vos achats et ventes passés ne sont pas connus, ce n'est donc pas la performance de votre compte."
                 + (h.shortened ? " La courbe commence plus tard : une de vos lignes a un historique plus court." : "")
                 + (h.missing.isEmpty ? "" : " Sans historique : \(h.missing.map { $0.components(separatedBy: ":").last ?? $0 }.joined(separator: ", ")).")
                 + (Money.displayCurrency == .eur ? " Cours en dollars convertis au taux du jour, pas au taux de chaque date : l'effet de change passé n'est pas compté (les % restent exacts en dollars)." : "")
            )
            .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private func legend(_ color: Color, _ label: String, _ change: Double) -> some View {
        HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 2).fill(color).frame(width: 14, height: 3)
            Text(label).font(.footnote).foregroundStyle(Theme.textSecondary)
            Spacer()
            Text(pct(change)).font(Theme.mono(13))
        }
        .accessibilityElement(children: .combine)
    }
}
