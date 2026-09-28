import SwiftUI
import Charts
import AltimKit

/// "If I had invested 100 $ every month": regular purchases replayed on the real daily closes of the asset.
struct DcaCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @AppStorage("dca.amount") private var amountText = "100"
    @AppStorage("dca.every") private var every = 30
    @AppStorage("dca.days") private var days = 365
    @State private var closes: [(Double, Double)]?
    @State private var error: String?

    private var amount: Double {
        Double(amountText.replacingOccurrences(of: " ", with: "").replacingOccurrences(of: "\u{202F}", with: "").replacingOccurrences(of: ",", with: ".")) ?? 0
    }

    var body: some View {
        Card(title: "Si j'avais investi régulièrement") {
            HStack {
                Text("Montant par achat").foregroundStyle(Theme.textSecondary)
                Spacer()
                TextField("100", text: $amountText).keyboardType(.decimalPad).multilineTextAlignment(.trailing).font(Theme.mono(16)).frame(maxWidth: 120)
                Text("$").foregroundStyle(Theme.textSecondary)
            }
            Picker("Fréquence", selection: $every) {
                Text("Chaque semaine").tag(7)
                Text("Chaque mois").tag(30)
            }
            .pickerStyle(.segmented)
            Picker("Depuis", selection: $days) {
                Text("1 an").tag(365)
                Text("2 ans").tag(730)
            }
            .pickerStyle(.segmented)
            if let error {
                Notice(text: error, tone: .bad)
            } else if let closes {
                if let r = amount > 0 ? DcaResult.simulate(closes, amount: amount, everyDays: every, days: days) : nil {
                    DcaBody(r: r)
                } else {
                    Text(amount > 0 ? "Pas assez d'historique pour \(asset.symbol) sur cette période." : "Indiquez un montant.")
                        .font(.footnote).foregroundStyle(Theme.textSecondary)
                }
            } else {
                ProgressView("Chargement de l'historique…").frame(maxWidth: .infinity)
            }
        }
        .task(id: "\(asset.id)|\(days)") {
            guard let client = model.client else { return }
            closes = nil
            do {
                closes = try await client.history([asset], days: days).byId[asset.id] ?? []
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

private func signed(_ v: Double) -> String {
    "\(v >= 0 ? "+" : "−")\(abs(v).formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "fr_FR")))) %"
}

private func dollars(_ v: Double) -> String { "\(v.formatted(.number.precision(.fractionLength(0)).locale(Locale(identifier: "fr_FR")))) $" }

private struct DcaBody: View {
    let r: DcaResult

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            KeyValue(key: "\(r.buys) achats · \(dollars(r.invested)) investis", value: "\(dollars(r.value)) (\(signed(r.gain)))", tone: r.gain >= 0 ? .good : .bad)
            Chart {
                ForEach(r.path) { s in
                    LineMark(x: .value("Jour", s.t), y: .value("Montant", s.invested), series: .value("Série", "Somme investie"))
                        .foregroundStyle(.white.opacity(0.45))
                        .lineStyle(StrokeStyle(lineWidth: 1.5, dash: [4, 3]))
                    LineMark(x: .value("Jour", s.t), y: .value("Montant", s.value), series: .value("Série", "Valeur"))
                        .foregroundStyle(Theme.allocCrypto)
                        .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                }
            }
            .chartXAxis(.hidden)
            .chartYAxis(.hidden)
            .frame(height: 110)
            .accessibilityLabel("\(dollars(r.invested)) investis, valeur \(dollars(r.value))")
            Text("Trait plein : valeur · pointillés : somme investie").font(.caption).foregroundStyle(Theme.textSecondary)
            KeyValue(key: "Tout investi le \(Format.date(r.first))", value: "\(dollars(r.lumpValue)) (\(signed(r.lumpGain)))", tone: r.lumpGain >= 0 ? .good : .bad)
            KeyValue(key: "Prix moyen payé", value: Format.price(r.averagePrice))
            KeyValue(key: "Prix à la dernière clôture", value: Format.price(r.lastPrice))
            Text(verdict + " Rejoué sur les vraies clôtures journalières, sans frais ni impôts ; le passé ne dit pas ce qui arrivera.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private var verdict: String {
        if r.lumpGain > r.gain { return "Sur cette période, tout acheter le premier jour a mieux rendu : le prix a surtout monté." }
        if r.lumpGain < r.gain { return "Sur cette période, étaler les achats a mieux rendu : ils ont profité des baisses." }
        return "Sur cette période, les deux façons d'investir reviennent au même."
    }
}
