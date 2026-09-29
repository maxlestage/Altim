import SwiftUI
import Charts
import AltimKit

// Categorical palette (fixed order, validated on the dark surface). The 4th is close to the 1st for deuteranopes
// (ΔE 6,4, allowed with a second cue): its line is dashed.
private let series: [Color] = [Theme.allocCrypto, Theme.allocStock, Theme.allocCash, Color(red: 0xC2 / 255, green: 0x4E / 255, blue: 0xC9 / 255)]
private let fr = Locale(identifier: "fr_FR")

private func signed(_ v: Double) -> String { "\(v >= 0 ? "+" : "−")\(abs(v).formatted(.number.precision(.fractionLength(0...1)).locale(fr))) %" }
/// A dollar amount in the display currency, whole: "1 250 €".
private func dollars(_ v: Double) -> String { Money.money(v, min: 0, max: 0, sep: " ") }
private func number(_ s: String) -> Double? { Money.parse(s) }
/// A typed amount (display currency) in dollars, nil when not a number.
private func typedUsd(_ s: String) -> Double? { number(s).map(Money.fromDisplay).flatMap { $0.isFinite ? $0 : nil } }

// MARK: Comparison

/// 2 to 4 assets of the radar over the same days: change, volatility, worst fall, correlation.
struct CompareCard: View {
    @Environment(AppModel.self) private var model
    @State private var picked: [String] = []
    @State private var days = 90
    @State private var data: [String: [(Double, Double)]]?
    @State private var error: String?

    private var comparison: Tools.Comparison? { data.flatMap { Tools.compare($0, ids: picked, days: days) } }

    var body: some View {
        Card(title: "Comparer") {
            Text("Choisissez 2 à 4 actifs de votre radar.").font(.caption).foregroundStyle(Theme.textSecondary)
            WrapLayout(spacing: 6) { chips }
            Picker("Période", selection: $days) {
                Text("30 j").tag(30)
                Text("90 j").tag(90)
                Text("1 an").tag(365)
            }
            .pickerStyle(.segmented)
            if picked.count < 2 {
                Text("Sélectionnez au moins 2 actifs.").font(.footnote).foregroundStyle(Theme.textSecondary)
            } else if let error {
                Notice(text: error, tone: .bad)
            } else if data == nil {
                ProgressView("Chargement de l'historique…").frame(maxWidth: .infinity)
            } else if let c = comparison {
                CompareBody(c: c, picked: picked)
            } else {
                Text("Pas assez d'historique commun.").font(.footnote).foregroundStyle(Theme.textSecondary)
            }
        }
        .onAppear { if picked.isEmpty { picked = Array(model.watchlist.prefix(2).map(\.id)) } }
        .task(id: "\(picked.sorted().joined(separator: ","))|\(days)") {
            guard picked.count >= 2, let client = model.client else { return }
            data = nil
            do {
                data = try await client.history(model.watchlist.filter { picked.contains($0.id) }, days: days).byId
                error = nil
            } catch AltimError.unauthorized {
                model.sessionLost()
            } catch is CancellationError {
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    @ViewBuilder private var chips: some View {
        ForEach(model.watchlist) { a in
            let i = picked.firstIndex(of: a.id)
            Button {
                if let i { picked.remove(at: i) } else if picked.count < 4 { picked.append(a.id) }
            } label: {
                Text(a.symbol).font(.footnote.weight(.semibold))
                    .foregroundStyle(i != nil ? .white : Theme.textSecondary)
                    .padding(.horizontal, 12).padding(.vertical, 6)
                    .overlay(Capsule().stroke(i.map { series[$0] } ?? .white.opacity(0.15), lineWidth: i != nil ? 2 : 1))
            }
            .buttonStyle(.borderless)
            .accessibilityAddTraits(i != nil ? .isSelected : [])
        }
    }
}

private struct CompareBody: View {
    let c: Tools.Comparison
    let picked: [String]

    private struct Row: Identifiable { let id = UUID(); let i: Int; let name: String; let pct: Double }

    private func color(_ id: String) -> Color { series[min(3, picked.firstIndex(of: id) ?? 0)] }
    private func name(_ id: String) -> String { id.components(separatedBy: ":").last ?? id }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Chart {
                RuleMark(y: .value("Départ", 0)).foregroundStyle(.white.opacity(0.18)).lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                ForEach(c.stats) { s in
                    ForEach(Array(s.pct.enumerated()), id: \.offset) { e in
                        LineMark(x: .value("Jour", e.offset), y: .value("Variation", e.element), series: .value("Actif", s.id))
                            .foregroundStyle(color(s.id))
                            .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round, dash: picked.firstIndex(of: s.id) == 3 ? [6, 4] : []))
                    }
                }
            }
            .chartXAxis(.hidden)
            .chartYAxis { AxisMarks(position: .leading, values: .automatic(desiredCount: 3)) { v in
                AxisGridLine().foregroundStyle(.white.opacity(0.06))
                AxisValueLabel { if let p = v.as(Double.self) { Text(signed(p)) } }
            } }
            .frame(height: 140)
            .accessibilityLabel(c.stats.map { "\(name($0.id)) \(signed($0.change))" }.joined(separator: ", "))
            Grid(alignment: .trailing, horizontalSpacing: 8, verticalSpacing: 4) {
                GridRow {
                    Text("Actif").gridColumnAlignment(.leading)
                    Text("Variation")
                    Text("Volatilité")
                    Text("Pire recul")
                }
                .font(.caption).foregroundStyle(Theme.textSecondary)
                ForEach(c.stats) { s in
                    GridRow {
                        HStack(spacing: 6) {
                            Circle().fill(color(s.id)).frame(width: 8, height: 8)
                            Text(name(s.id)).font(.footnote.weight(.semibold))
                        }
                        Text(signed(s.change)).foregroundStyle(s.change >= 0 ? Theme.buy : Theme.sell)
                        Text("\(Int(s.volatility.rounded())) %/an")
                        Text(signed(s.maxDrawdown)).foregroundStyle(Theme.sell)
                    }
                    .font(Theme.mono(13))
                }
            }
            Text(correlationText).font(.caption).foregroundStyle(Theme.textSecondary)
            Text("Mêmes jours pour tous. Volatilité = écart type annualisé des variations journalières. Le passé ne dit pas ce qui arrivera.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private var correlationText: String {
        var parts: [String] = []
        for a in c.stats.indices {
            for b in c.stats.indices where b > a {
                let v = c.correlation[a][b].map { $0.formatted(.number.precision(.fractionLength(2)).locale(fr)) } ?? "—"
                parts.append("\(name(c.stats[a].id))/\(name(c.stats[b].id)) \(v)")
            }
        }
        return "Corrélation : " + parts.joined(separator: " · ") + ". Proche de 1 : ils montent et baissent ensemble (peu de diversification) ; proche de 0 : indépendants."
    }
}

// MARK: Position size

/// How much to buy so that hitting the stop costs the chosen share of the capital.
struct PositionCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    let price: Double?
    let zones: [FibZone]
    /// Capital, stop and target are shown and typed in the display currency, sized in dollars. The saved capital keeps
    /// the currency it was typed in (older versions: dollars).
    @AppStorage("position.capital") private var capitalText = ""
    @AppStorage("position.capitalCurrency") private var capitalCurrency = Currency.usd.rawValue
    @AppStorage("position.risk") private var riskText = "1"
    @State private var stopText = ""
    @State private var targetText = ""

    /// Proposed stop: the low that invalidates the nearest buy zone below the price, else 5 % under the price.
    private var zone: FibZone? { zones.first { z in (z.invalidation ?? 0) > 0 && price.map { z.invalidation! < $0 } == true } }

    var body: some View {
        let p = price.flatMap { Tools.positionSize(capital: typedUsd(capitalText) ?? 0, riskPct: number(riskText) ?? 0, entry: $0, stop: typedUsd(stopText) ?? 0, target: typedUsd(targetText)) }
        let sym = Money.symbol()
        Card(title: "Taille de position") {
            HStack {
                field("Capital", $capitalText, sym)
                field("Risque accepté", $riskText, "%")
            }
            HStack {
                field("Stop", $stopText, sym)
                field("Objectif", $targetText, sym)
            }
            Text("Entrée au prix actuel \(price.map { Format.price($0) } ?? "…") ; stop proposé : \(zone != nil ? "plus bas qui invalide la zone d'achat" : "5 % sous le prix (à ajuster)").")
                .font(.caption).foregroundStyle(Theme.textSecondary)
            if let p {
                KeyValue(key: "Acheter", value: "\(Format.plain(p.quantity, digits: p.quantity >= 1 ? 2 : 6)) \(asset.symbol) · \(dollars(p.amount))")
                KeyValue(key: "Part du capital", value: "\(Format.plain(p.capitalShare, digits: 1)) %")
                KeyValue(key: "Perte si le stop est touché (\(signed(-p.stopDistance)))", value: "−\(dollars(p.risk))", tone: .bad)
                if let reward = p.reward, let ratio = p.ratio {
                    KeyValue(key: "Gain à l'objectif · gain/risque", value: "+\(dollars(reward)) · \(Format.plain(ratio, digits: 1)) R", tone: .good)
                }
                if p.capped { Notice(text: "Stop très proche : la taille est limitée à votre capital, la perte au stop reste sous le risque choisi.", tone: .warn) }
                if let ratio = p.ratio, ratio < 1.5 { Notice(text: "Rapport gain/risque sous 1,5 : l'idée rapporte peu au regard du risque.", tone: .warn) }
                Text("Calcul, pas conseil : un écart de prix (gap) peut faire perdre plus que prévu au stop. Altim ne passe aucun ordre.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            } else if price != nil {
                Text("Le stop doit être sous le prix d'entrée, et le capital et le risque positifs.").font(.footnote).foregroundStyle(Theme.textSecondary)
            }
        }
        .onChange(of: price, initial: true) { _, price in
            guard let price else { return }
            let shown = Money.toDisplay(price)
            if stopText.isEmpty { stopText = Format.plain(Money.toDisplay(zone?.invalidation ?? price * 0.95), digits: shown >= 1 ? 2 : 6) }
            if targetText.isEmpty, let t = zone?.targets.first(where: { $0 > price }) { targetText = Format.plain(Money.toDisplay(t), digits: shown >= 1 ? 2 : 6) }
            // A capital saved in the other currency is shown converted at today's rate.
            let saved = Currency(rawValue: capitalCurrency) ?? .usd
            if saved != Money.displayCurrency, let v = number(capitalText) {
                let c = Money.convert(v, from: saved, to: Money.displayCurrency)
                if c.isFinite {
                    capitalText = String(Int(c.rounded()))
                    capitalCurrency = Money.displayCurrency.rawValue
                }
            }
            if capitalText.isEmpty {
                let usd = model.usdHoldings.holdings
                let total = Portfolio(holdings: usd, prices: Dictionary(usd.compactMap { h in model.live.price(h.asset).map { (h.asset.id, $0.price) } }, uniquingKeysWith: { a, _ in a })).total
                if total > 0 {
                    capitalText = String(Int(Money.toDisplay(total).rounded()))
                    capitalCurrency = Money.displayCurrency.rawValue
                }
            }
        }
        .onChange(of: capitalText) { _, _ in capitalCurrency = Money.displayCurrency.rawValue }
    }

    private func field(_ label: String, _ text: Binding<String>, _ unit: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(Theme.textSecondary)
            HStack(spacing: 4) {
                TextField(label, text: text).keyboardType(.decimalPad).font(Theme.mono(15))
                Text(unit).foregroundStyle(Theme.textSecondary)
            }
            .padding(8)
            .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: Sale after fees and tax

/// What selling would leave once the fees and the flat tax on the gains are paid.
struct SaleCard: View {
    let portfolio: Portfolio
    @State private var taxText = "30"
    @State private var feeText = "0,1"

    var body: some View {
        let lines = portfolio.lines.compactMap { l in l.value.map { (id: l.holding.asset.symbol, value: $0, cost: l.cost) } }
        let t = Tools.saleTotal(lines, taxPct: number(taxText) ?? 0, feePct: number(feeText) ?? 0)
        if !lines.isEmpty {
            Card(title: "Si je vendais") {
                HStack {
                    field("Impôt sur la plus-value", $taxText)
                    field("Frais de vente", $feeText)
                }
                // By position: two lines of the same asset share a symbol.
                ForEach(Array(t.lines.enumerated()), id: \.offset) { e in
                    let s = e.element
                    KeyValue(key: "\(s.id) · \(s.gain.map { "\($0 >= 0 ? "+" : "−")\(dollars(abs($0)))" } ?? "prix d'achat inconnu")\(s.tax > 0 ? " · impôt −\(dollars(s.tax))" : "")",
                             value: dollars(s.net))
                }
                KeyValue(key: "Tout vendre : vous garderiez", value: dollars(t.net), tone: .good)
                Text("Frais \(dollars(t.fees)) · impôt \(dollars(t.tax))\(t.gain.map { " · plus-value nette \($0 >= 0 ? "+" : "−")\(dollars(abs($0)))" } ?? "")")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
                Text(note(t)).font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
    }

    private func note(_ t: Tools.SaleTotal) -> String {
        let unknown = t.unknownCost > 0 ? " \(t.unknownCost) ligne(s) sans prix d'achat : plus-value non calculée." : ""
        return "30 % = prélèvement forfaitaire unique en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux) ; les pertes de l'année compensent les gains. "
            + "Cryptos : l'impôt se calcule sur l'ensemble du portefeuille à chaque cession et les cessions de moins de 305 € par an sont exonérées, donc ce calcul ligne par ligne est une estimation."
            + unknown + " Vérifiez votre situation (PEA, assurance-vie, option barème…) ; Altim ne passe aucun ordre."
    }

    private func field(_ label: String, _ text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(Theme.textSecondary)
            HStack(spacing: 4) {
                TextField(label, text: text).keyboardType(.decimalPad).font(Theme.mono(15))
                Text("%").foregroundStyle(Theme.textSecondary)
            }
            .padding(8)
            .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: Projection

/// What the portfolio plus a monthly contribution would become, under three yearly returns (hypotheses).
struct ProjectionCard: View {
    let start: Double
    @State private var monthlyText = "0"
    @State private var years = 10

    var body: some View {
        // Typed in the display currency, projected in dollars like the portfolio.
        let monthly = typedUsd(monthlyText) ?? 0
        let runs = Tools.projectionRates.map { (rate: $0, end: Tools.projection(start: start, monthly: monthly, years: years, ratePct: $0).last!) }
        let paid = runs[0].end.paid
        Card(title: "Projection") {
            HStack {
                Text("Versement chaque mois, facultatif").foregroundStyle(Theme.textSecondary)
                Spacer()
                TextField("200", text: $monthlyText).keyboardType(.decimalPad).multilineTextAlignment(.trailing).font(Theme.mono(16)).frame(maxWidth: 120)
                Text(Money.symbol()).foregroundStyle(Theme.textSecondary)
            }
            Picker("Durée", selection: $years) {
                Text("5 ans").tag(5)
                Text("10 ans").tag(10)
                Text("20 ans").tag(20)
            }
            .pickerStyle(.segmented)
            KeyValue(key: monthly > 0 ? "Aujourd'hui \(dollars(start)) + versements" : "Vos avoirs aujourd'hui, sans rien ajouter", value: dollars(paid) + (monthly > 0 ? " versés" : ""))
            ForEach(runs, id: \.rate) { run in
                let gain = run.end.value - paid
                KeyValue(key: "Si \(Int(run.rate)) % par an", value: "\(dollars(run.end.value)) (\(gain >= 0 ? "+" : "−")\(dollars(abs(gain))))", tone: gain > 0 ? .good : nil)
            }
            Text("Trois hypothèses de rendement à comparer, pas des prévisions : une année peut perdre 30 % ou plus (cryptos : davantage), et l'inflation réduit ce que ces montants achèteront. Sans frais ni impôts.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }
}

// MARK: Rebalancing

/// Buys and sells to reach a target split between cryptos and stocks.
struct RebalanceCard: View {
    let portfolio: Portfolio
    @AppStorage("rebalance.crypto") private var target = 40.0

    var body: some View {
        let r = Tools.rebalance(portfolio.lines.compactMap { l in l.value.map { (id: l.holding.asset.id, kind: l.holding.asset.kind, value: $0) } }, targetCrypto: target)
        let small = { (v: Double) in abs(v) < max(10, (r?.total ?? 0) * 0.01) }
        Card(title: "Rééquilibrer") {
            Text("Cible : \(Int(target)) % cryptos · \(100 - Int(target)) % actions").font(.subheadline)
            Slider(value: $target, in: 0...100, step: 5).tint(Theme.cyan)
                .accessibilityLabel("Part des cryptos visée")
                .accessibilityValue("\(Int(target)) %")
            if let r {
                ForEach([Kind.crypto, Kind.stock], id: \.self) { k in
                    let m = r.moves[k] ?? 0
                    let label = k == .crypto ? "Cryptos" : "Actions"
                    let goal = k == .crypto ? target : 100 - target
                    let tone: Tone? = small(m) ? nil : (m > 0 ? .good : .bad)
                    KeyValue(key: "\(label) : \(Int((r.current[k] ?? 0).rounded())) % → \(Int(goal)) %",
                             value: small(m) ? "rien à faire" : "\(m > 0 ? "acheter" : "vendre") \(dollars(abs(m)))",
                             tone: tone)
                }
                ForEach(r.lines.filter { !small($0.amount) }, id: \.id) { l in
                    Text("\(l.amount > 0 ? "Acheter" : "Vendre") \(dollars(abs(l.amount))) de \(l.id.components(separatedBy: ":").last ?? l.id)")
                        .font(.footnote)
                }
                Text("Réparti au prorata de vos lignes actuelles. Avant de vendre, pensez aux frais et à l'impôt sur les plus-values. Altim ne passe aucun ordre.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            } else {
                Text("Ajoutez des avoirs avec un prix pour calculer.").font(.footnote).foregroundStyle(Theme.textSecondary)
            }
        }
    }
}
