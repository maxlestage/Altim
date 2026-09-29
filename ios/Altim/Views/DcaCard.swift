import SwiftUI
import Charts
import AltimKit

/// One purchase repeats nothing: its "next purchase" is far beyond any period.
private let once = 100_000

/// "If I had invested 1 000 €": one purchase by default (not everybody wants to spend every month), regular purchases
/// as an option, replayed on the real daily closes of the asset. New storage keys: the former default was monthly.
struct DcaCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @AppStorage("dca.v2.amount") private var amountText = "1000"
    @AppStorage("dca.v2.every") private var every = once
    @AppStorage("dca.v2.days") private var days = 365
    @State private var closes: [(Double, Double)]?
    @State private var error: String?

    /// Typed in the display currency, replayed in dollars on the dollar closes.
    private var amount: Double {
        let v = Money.fromDisplay(Money.parse(amountText) ?? 0)
        return v.isFinite ? v : 0
    }

    var body: some View {
        Card(title: "Si j'avais investi") {
            HStack {
                Text(every == once ? "Montant investi" : "Montant par achat").foregroundStyle(Theme.textSecondary)
                Spacer()
                TextField("100", text: $amountText).keyboardType(.decimalPad).multilineTextAlignment(.trailing).font(Theme.mono(16)).frame(maxWidth: 120)
                Text(Money.symbol()).foregroundStyle(Theme.textSecondary)
            }
            Picker("Achat", selection: $every) {
                Text("Une fois").tag(once)
                Text("Chaque semaine").tag(7)
                Text("Chaque mois").tag(30)
            }
            .pickerStyle(.segmented)
            Picker("Depuis", selection: $days) {
                Text("6 mois").tag(182)
                Text("1 an").tag(365)
                Text("2 ans").tag(730)
            }
            .pickerStyle(.segmented)
            if let error {
                Notice(text: error, tone: .bad)
            } else if let closes {
                if let r = amount > 0 ? DcaResult.simulate(closes, amount: amount, everyDays: every, days: days) : nil {
                    DcaBody(r: r, single: every == once)
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
                closes = try await client.history([asset], days: days == 730 ? 730 : 365).byId[asset.id] ?? []
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

/// A dollar amount in the display currency, whole: "1 000 €".
private func dollars(_ v: Double) -> String { Money.money(v, min: 0, max: 0, sep: " ") }

private struct DcaBody: View {
    let r: DcaResult
    let single: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            KeyValue(key: single ? "\(dollars(r.invested)) investis le \(Format.date(r.first))" : "\(r.buys) achats · \(dollars(r.invested)) investis", value: "\(dollars(r.value)) (\(signed(r.gain)))", tone: r.gain >= 0 ? .good : .bad)
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
            if !single {
                KeyValue(key: "Tout investi le \(Format.date(r.first))", value: "\(dollars(r.lumpValue)) (\(signed(r.lumpGain)))", tone: r.lumpGain >= 0 ? .good : .bad)
            }
            KeyValue(key: single ? "Prix d'achat" : "Prix moyen payé", value: Format.price(r.averagePrice))
            KeyValue(key: "Prix à la dernière clôture", value: Format.price(r.lastPrice))
            if Money.displayCurrency == .eur {
                Text("Rejoué en $ sur les cours en dollars, puis converti au taux du jour : l'effet de change passé (EUR/USD) n'est pas compté.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
            Text(verdict + " Rejoué sur les vraies clôtures journalières, sans frais ni impôts ; le passé ne dit pas ce qui arrivera.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private var verdict: String {
        if single { return "Un seul achat, à la clôture de ce jour-là." }
        if r.lumpGain > r.gain { return "Sur cette période, tout acheter le premier jour a mieux rendu : le prix a surtout monté." }
        if r.lumpGain < r.gain { return "Sur cette période, étaler les achats a mieux rendu : ils ont profité des baisses." }
        return "Sur cette période, les deux façons d'investir reviennent au même."
    }
}
