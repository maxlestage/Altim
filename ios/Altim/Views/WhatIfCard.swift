import SwiftUI
import AltimKit

/// « Et si… ? » on Mes avoirs: how the portfolio would move if one market factor (Nasdaq-100, S&P 500, Bitcoin) fell by
/// a given shock, each line through its beta to that factor (one year of shared sessions). `daily`: the daily candles
/// already loaded ("kind:SYMBOL"); the factor's own candles are fetched when missing. Same texts as the web
/// (WhatIfCard.tsx); stacked rows, chips wrap.
struct WhatIfCard: View {
    @Environment(AppModel.self) private var model
    let portfolio: RiskPortfolio
    let daily: [String: [Candle]]
    @State private var factor: FactorKey = .qqq
    @State private var shock: Double = -10
    @State private var custom = ""
    @State private var amountText = ""
    @State private var fetched: [String: [Candle]] = [:]
    @State private var failed: Set<String> = []

    private var factorCandles: [Candle]? {
        let key = factor.asset.id
        if let c = daily[key], !c.isEmpty { return c }
        return fetched[key]
    }

    private var effectiveShock: Double {
        let t = custom.trimmingCharacters(in: .whitespaces)
        guard !t.isEmpty else { return shock }
        let raw = t.hasPrefix("−") || t.hasPrefix("-") ? String(t.dropFirst()) : t
        guard let v = PaperFormat.number(raw), v > 0, v <= 100 else { return shock }
        return -v
    }

    private var amount: Double? { PaperFormat.number(amountText).flatMap { $0.isFinite ? $0 : nil } }

    private func betas(_ factorCandles: [Candle]) -> [String: FactorBeta] {
        var b: [String: FactorBeta] = [:]
        for l in portfolio.lines where b[l.key] == nil {
            b[l.key] = WhatIf.factorBeta(daily[l.key] ?? [], factorCandles, days: WhatIf.betaDays)
        }
        return b
    }

    var body: some View {
        let eff = effectiveShock
        let f = factor
        Card(title: "Et si… ?") {
            Text(WhatIf.intro(f, shock: eff)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            Text("Marché").font(.caption).foregroundStyle(Theme.textSecondary)
            WrapLayout(spacing: 6) {
                ForEach(FactorKey.allCases, id: \.self) { k in
                    Button { factor = k } label: { TagChip(text: k.label, color: .white, selected: k == factor) }
                        .buttonStyle(.borderless)
                        .accessibilityAddTraits(k == factor ? .isSelected : [])
                }
            }
            Text("Choc").font(.caption).foregroundStyle(Theme.textSecondary)
            WrapLayout(spacing: 6) {
                ForEach(WhatIf.shocks, id: \.self) { s in
                    let on = custom.trimmingCharacters(in: .whitespaces).isEmpty && s == shock
                    Button {
                        shock = s
                        custom = ""
                    } label: { TagChip(text: WhatIf.signedPct(s), color: .white, selected: on) }
                        .buttonStyle(.borderless)
                        .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
            field("Autre baisse (%)", text: $custom, placeholder: "ex. 15")
            field("Montant simulé (USD, facultatif)", text: $amountText, placeholder: WhatIf.usd(portfolio.total))
            if let candles = factorCandles {
                result(WhatIf.run(portfolio, factor: f, shock: eff, betas: betas(candles), amount: amount))
            } else if failed.contains(f.asset.id) {
                Notice(text: "Cours journaliers de \(f.label) indisponibles : simulation non couverte pour l'instant.", tone: .warn)
            } else {
                Text("Chargement des cours de \(f.label)…").font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
        .task(id: factor) { await loadFactor() }
    }

    private func field(_ label: String, text: Binding<String>, placeholder: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.caption).foregroundStyle(Theme.textSecondary)
            TextField(placeholder, text: text)
                .keyboardType(.decimalPad)
                .textFieldStyle(.roundedBorder)
                .accessibilityLabel(label)
        }
    }

    @ViewBuilder private func result(_ r: WhatIfResult) -> some View {
        if r.scaled {
            Text("Simulé sur \(WhatIf.usd(r.base)) répartis selon les poids actuels (liquidités comprises : \(WhatIf.usd(r.cash))).")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Text("Perte estimée").font(.subheadline).foregroundStyle(Theme.textSecondary)
                Spacer(minLength: 8)
                Text(WhatIf.signedUsd(r.loss)).font(Theme.mono(20, weight: .bold)).foregroundStyle(r.loss > 0 ? Theme.sell : Theme.buy)
            }
            Text("soit \(WhatIf.signedPct(-r.lossPercent)) de \(r.scaled ? "ce montant" : "votre patrimoine")")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
        .accessibilityElement(children: .combine)
        if let w = r.worst, let move = w.movePercent, let loss = w.loss {
            (Text("Ligne la plus touchée : ") + Text(w.symbol).bold() + Text(" (\(WhatIf.signedPct(move)), \(WhatIf.signedUsd(loss))).")).font(.footnote)
                .foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        }
        ForEach(r.lines) { l in
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(l.symbol).font(Theme.mono(14, weight: .bold)).foregroundStyle(.white)
                    Spacer(minLength: 8)
                    Text(l.loss.map { WhatIf.signedUsd($0) } ?? "non couvert").font(Theme.mono(13))
                        .foregroundStyle(l.loss == nil ? Theme.textSecondary : (l.loss ?? 0) > 0 ? Theme.sell : Theme.buy)
                }
                Text(WhatIf.lineDetail(l, factor: r.factor)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            .padding(.vertical, 4)
            .accessibilityElement(children: .combine)
        }
        if !r.uncovered.isEmpty {
            Text("Non couvert : \(r.uncovered.joined(separator: ", ")) (\(WhatIf.usd(r.uncoveredValue))) — laissé hors du total, jamais estimé.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        Text(WhatIf.footnote(r)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    /// The factor's daily candles when the holdings screen has not loaded them (a failure is said, never guessed).
    private func loadFactor() async {
        let a = factor.asset
        if let c = daily[a.id], !c.isEmpty { return }
        guard fetched[a.id] == nil, !failed.contains(a.id), let client = model.client else { return }
        do {
            let c = try await client.candles(a, interval: "1d").candles
            if c.isEmpty { failed.insert(a.id) } else { fetched[a.id] = c }
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            failed.insert(a.id)
        }
    }
}
