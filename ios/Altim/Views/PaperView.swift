import SwiftUI
import AltimKit

/// "Réel / Simulation / Journal" at the top of Mes avoirs.
struct PortfolioModePicker: View {
    @Binding var mode: Int

    var body: some View {
        Picker("Portefeuille", selection: $mode) {
            Text("Réel").tag(0)
            Text("Simulation").tag(1)
            Text("Journal").tag(2)
        }
        .pickerStyle(.segmented)
    }
}

/// Dollars with cents when there are some: "10 509,12 $", "10 000 $".
enum PaperFormat {
    static func usd(_ v: Double) -> String { "\(Format.plain(v, digits: 2)) $" }
    static func signed(_ v: Double) -> String { "\(v >= 0 ? "+" : "−")\(usd(abs(v)))" }
    static func tone(_ v: Double) -> Tone { v > 0 ? .good : v < 0 ? .bad : .neutral }

    /// Text typed in a French field ("1 000,50") → number.
    static func number(_ s: String) -> Double? {
        let t = s.replacingOccurrences(of: "\u{202F}", with: "").replacingOccurrences(of: "\u{00A0}", with: "")
            .replacingOccurrences(of: " ", with: "").replacingOccurrences(of: ",", with: ".").replacingOccurrences(of: "$", with: "")
        guard let v = Double(t), v.isFinite else { return nil }
        return v
    }

    static func decisionLine(_ d: PaperDecision?) -> String {
        guard let d else { return "Ouverte sans décision affichée" }
        return "Décision à l'achat : \(d.label) · confiance \(Int(d.confidence.rounded())) % · \(Format.date(d.asOf))"
    }
}

/// Simulated portfolio (paper trading): follows Altim's decisions with no real money and no order placed, to see
/// whether "signal → exécution → résultat" holds. Everything stays on this iPhone.
struct PaperView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Binding var mode: Int
    @State private var quotes: [String: Double] = [:]
    @State private var selling: PaperPosition?
    @State private var resetting = false
    @State private var error: String?

    var body: some View {
        let state = model.paper
        let v = Paper.valuation(state, prices: prices)
        let stats = Paper.stats(state)
        List {
            Section { PortfolioModePicker(mode: $mode) }.listRowBackground(Color.clear)
            Section { summary(v) }.listRowBackground(Color.clear)
            if !model.paperJustClosed.isEmpty { Section { justClosed }.listRowBackground(Color.clear) }
            if let error { Section { Notice(text: error, tone: .bad) }.listRowBackground(Color.clear) }
            Section {
                if v.lines.isEmpty {
                    Text("Aucune position ouverte. Sur la page d'un actif, le bouton « Simuler cet achat » de la carte Décision ouvre une position fictive.")
                        .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                } else {
                    ForEach(v.lines) { positionCard($0) }
                }
            } header: {
                HStack { Text("Positions ouvertes"); Spacer(); if !v.lines.isEmpty { LiveBadge() } }
            }
            .listRowBackground(Color.clear)
            Section {
                if state.trades.isEmpty {
                    Text("Aucune position clôturée pour l'instant.").font(.footnote).foregroundStyle(Theme.textSecondary)
                } else {
                    ForEach(state.trades.reversed()) { tradeRow($0) }
                }
            } header: {
                Text("Journal des ventes simulées")
            }
            .listRowBackground(Theme.surface.opacity(0.6))
            Section { statsCard(stats) }.listRowBackground(Color.clear)
            Section { howCard }.listRowBackground(Color.clear)
        }
        .listStyle(.insetGrouped)
        .task(id: state.positions.map(\.id).joined(separator: ",")) { await load() }
        .refreshable { await load() }
        .confirmationDialog("Vendre (simulé) ?", isPresented: Binding(get: { selling != nil }, set: { if !$0 { selling = nil } }),
                            titleVisibility: .visible, presenting: selling) { p in
            Button("Vendre \(p.symbol) (simulé)", role: .destructive) { sell(p) }
        } message: { p in
            Text("Vente fictive de toute la position \(p.symbol) au prix actuel (\(Format.price(prices[p.key]))), moins le glissement de 0,05 % et les frais de 0,1 %. Aucun ordre réel n'est passé.")
        }
        .sheet(isPresented: $resetting) { PaperResetSheet().environment(model) }
    }

    /// Live price when available, otherwise the consensus quote.
    private var prices: [String: Double] {
        var p = quotes
        for pos in model.paper.positions { if let t = model.live.price(pos.asset) { p[pos.key] = t.price } }
        return p
    }

    // MARK: Summary

    private func summary(_ v: PaperValuation) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Portefeuille simulé — aucun argent réel, aucun ordre passé", systemImage: "testtube.2")
                .font(.subheadline.weight(.semibold)).foregroundStyle(Theme.warning)
                .fixedSize(horizontal: false, vertical: true)
            Text("Valeur simulée").font(.caption).foregroundStyle(Theme.textSecondary)
            Text(PaperFormat.usd(v.equity)).font(Theme.mono(30, weight: .bold))
                .minimumScaleFactor(0.6).lineLimit(1)
                .accessibilityLabel("Valeur simulée : \(PaperFormat.usd(v.equity))")
            DecisionRow(key: "Résultat", value: "\(PaperFormat.signed(v.pnl)) (\(Format.percent(v.pnlPct)))", tone: PaperFormat.tone(v.pnl))
            DecisionRow(key: "Capital de départ", value: PaperFormat.usd(model.paper.startCapital))
            DecisionRow(key: "Liquidités", value: PaperFormat.usd(v.cash))
            DecisionRow(key: "Positions (si vendues maintenant)", value: PaperFormat.usd(v.positionsValue))
            Text("Depuis le \(Format.date(model.paper.startedAt)).").font(.caption).foregroundStyle(Theme.textSecondary)
            if v.unpriced > 0 {
                Notice(text: v.unpriced == 1 ? "1 position sans prix pour l'instant : comptée à son coût." : "\(v.unpriced) positions sans prix pour l'instant : comptées à leur coût.", tone: .warn)
            }
            Button("Recommencer") { resetting = true }
                .buttonStyle(NeonButtonStyle(color: Theme.violet, filled: false))
                .accessibilityHint("Efface la simulation et repart d'un capital à choisir")
        }
        .glassCard(glow: Theme.warning)
    }

    private var justClosed: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(model.paperJustClosed) { t in
                Notice(text: "Clôturée automatiquement : \(t.symbol), \(t.reason.label) le \(Format.date(t.closedAt)) à \(Format.price(t.exit)) → \(PaperFormat.signed(t.pnl)) (\(Format.percent(t.pnlPct))).",
                       tone: t.pnl > 0 ? .good : .warn)
            }
            Button("J'ai vu") { model.paperJustClosed = [] }
                .buttonStyle(NeonButtonStyle(filled: false))
        }
    }

    // MARK: Positions

    private func positionCard(_ line: PaperLine) -> some View {
        let p = line.position
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(p.symbol).font(Theme.mono(16, weight: .bold)).foregroundStyle(.white)
                    Text(p.name).font(.caption).foregroundStyle(Theme.textSecondary)
                }
                Spacer(minLength: 6)
                if let d = p.decision { Badge(text: d.label, tone: tone(d.verdict)) }
            }
            .accessibilityElement(children: .combine)
            Text(PaperFormat.decisionLine(p.decision))
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            Group {
                DecisionRow(key: "Entrée (glissement inclus)", value: Format.price(p.entry))
                DecisionRow(key: "Prix actuel", value: Format.price(line.price))
                DecisionRow(key: "Quantité", value: Format.quantity(p.quantity))
                DecisionRow(key: "Investi (frais inclus)", value: PaperFormat.usd(p.invested))
                DecisionRow(key: "Valeur si vendue", value: line.value.map(PaperFormat.usd) ?? "sans prix : au coût")
                if let pnl = line.pnl, let pct = line.pnlPct {
                    DecisionRow(key: "Résultat", value: "\(PaperFormat.signed(pnl)) (\(Format.percent(pct)))", tone: PaperFormat.tone(pnl))
                }
                DecisionRow(key: "Stop", value: p.stop.map { Format.price($0) } ?? "aucun", tone: p.stop == nil ? nil : .bad)
                DecisionRow(key: "Objectif", value: p.target.map { Format.price($0) } ?? "aucun", tone: p.target == nil ? nil : .good)
            }
            Text("Ouverte le \(Format.date(p.openedAt, time: true)).").font(.caption2).foregroundStyle(Theme.textSecondary)
            Button("Vendre (simulé)") { selling = p }
                .buttonStyle(NeonButtonStyle(color: Theme.sell, filled: false))
                .accessibilityLabel("Vendre \(p.symbol) (simulé)")
                .accessibilityHint("Vente fictive de toute la position, après confirmation")
        }
        .glassCard(glow: Theme.violet)
    }

    private func sell(_ p: PaperPosition) {
        error = model.paperSell(p.id, price: prices[p.key])
        selling = nil
    }

    // MARK: Journal

    private func tradeRow(_ t: PaperTrade) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline) {
                Text(t.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                Text(t.reason.label).font(.caption.weight(.semibold)).foregroundStyle(t.reason == .stop ? Theme.sell : t.reason == .target ? Theme.buy : Theme.cyan)
                Spacer(minLength: 6)
                Text(PaperFormat.signed(t.pnl)).font(Theme.mono(14)).foregroundStyle(Theme.color(PaperFormat.tone(t.pnl)))
            }
            Text("\(Format.price(t.entry)) → \(Format.price(t.exit)) · \(Format.percent(t.pnlPct)) · vendue le \(Format.date(t.closedAt))")
                .font(.caption.monospacedDigit()).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            Text(PaperFormat.decisionLine(t.decision)).font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    // MARK: Statistics

    private func statsCard(_ s: PaperStats) -> some View {
        Card(title: "Statistiques", glow: Theme.cyan) {
            if s.trades == 0 {
                Text("Les statistiques apparaissent après la première vente (manuelle, stop ou objectif).")
                    .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            } else {
                if s.trades < 20 {
                    Notice(text: "Seulement \(s.trades) position\(s.trades > 1 ? "s" : "") clôturée\(s.trades > 1 ? "s" : "") : en dessous d'une vingtaine, ces chiffres ne veulent pas dire grand-chose (le hasard pèse plus que la méthode).", tone: .warn)
                }
                Group {
                    DecisionRow(key: "Positions clôturées", value: "\(s.trades)")
                    DecisionRow(key: "Gagnantes", value: "\(s.wins) (\(Format.plain(s.winRate, digits: 1)) %)")
                    DecisionRow(key: "Gain moyen", value: s.avgWinPct.map { Format.percent($0) } ?? "—", tone: s.avgWinPct == nil ? nil : .good)
                    DecisionRow(key: "Perte moyenne", value: s.avgLossPct.map { Format.percent($0) } ?? "—", tone: s.avgLossPct == nil ? nil : .bad)
                    DecisionRow(key: "Profit factor", value: s.profitFactor.map { Format.plain($0, digits: 2) } ?? "— (aucune perte)")
                    DecisionRow(key: "Résultat réalisé", value: PaperFormat.signed(s.realizedPnl), tone: PaperFormat.tone(s.realizedPnl))
                    DecisionRow(key: "Pire recul du capital", value: Format.percent(s.maxDrawdownPct), tone: s.maxDrawdownPct < 0 ? .bad : nil)
                    DecisionRow(key: "Sorties", value: "objectif \(s.byReason.target) · stop \(s.byReason.stop) · manuelle \(s.byReason.manual)")
                    if let b = s.best { DecisionRow(key: "Meilleure", value: "\(b.symbol) \(Format.percent(b.pnlPct))", tone: PaperFormat.tone(b.pnlPct)) }
                    if let w = s.worst, s.trades > 1 { DecisionRow(key: "Pire", value: "\(w.symbol) \(Format.percent(w.pnlPct))", tone: PaperFormat.tone(w.pnlPct)) }
                }
                Text("Résultats par décision affichée à l'achat").font(.subheadline.weight(.semibold)).foregroundStyle(.white).padding(.top, 4)
                verdictTable(s.byVerdict)
                Text("Profit factor = somme des gains ÷ somme des pertes. Pire recul = plus forte baisse du capital réalisé, vente après vente.")
                    .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    @ViewBuilder private func verdictTable(_ rows: [PaperVerdictStats]) -> some View {
        if dynamicTypeSize.isAccessibilitySize {
            ForEach(rows, id: \.verdict) { r in
                VStack(alignment: .leading, spacing: 2) {
                    Text(r.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                    Text("\(r.trades) position\(r.trades > 1 ? "s" : "") · \(Format.plain(r.winRate, digits: 0)) % gagnantes · moyenne \(Format.percent(r.avgPnlPct))")
                        .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
            }
        } else {
            Grid(alignment: .trailing, horizontalSpacing: 8, verticalSpacing: 4) {
                GridRow {
                    Text("Décision").gridColumnAlignment(.leading)
                    Text("Nombre")
                    Text("Gagnantes")
                    Text("Moyenne")
                }
                .font(.caption).foregroundStyle(Theme.textSecondary)
                ForEach(rows, id: \.verdict) { r in
                    GridRow {
                        Text(r.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                        Text("\(r.trades)")
                        Text("\(Format.plain(r.winRate, digits: 0)) %")
                        Text(Format.percent(r.avgPnlPct)).foregroundStyle(Theme.color(PaperFormat.tone(r.avgPnlPct)))
                    }
                    .font(Theme.mono(13))
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(rows.map { r in
                "\(r.label) : \(r.trades) position\(r.trades > 1 ? "s" : ""), \(Format.plain(r.winRate, digits: 0)) % gagnantes, résultat moyen \(Format.percent(r.avgPnlPct))"
            }.joined(separator: " ; "))
        }
    }

    private var howCard: some View {
        Card(title: "Comment c'est calculé", glow: Theme.violet) {
            ForEach(Self.rules, id: \.self) { rule in
                Text("• \(rule)").font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private static let rules = [
        "Frais de 0,1 % à l'achat et à la vente, comme le suivi des signaux.",
        "Glissement de 0,05 % contre vous à chaque ordre : achat un peu plus cher, vente un peu moins cher que le prix affiché.",
        "Stop et objectif vérifiés sur les bougies journalières à partir du lendemain de l'achat : la bougie du jour d'achat n'est pas utilisée (son plus bas peut être antérieur à l'achat).",
        "Une bougie qui touche à la fois le stop et l'objectif compte comme le stop (le pire cas : l'ordre dans la journée n'est pas connu).",
        "Ouverture sous le stop (écart) : vente au prix d'ouverture ; objectif : vente à l'objectif, jamais mieux que prévu.",
        "Valeur des positions = ce que rapporterait une vente maintenant (glissement et frais déduits) ; sans prix, au coût.",
        "Tout reste sur cet iPhone. Aucun argent réel, aucun ordre n'est jamais passé.",
    ]

    private func tone(_ verdict: String) -> Tone {
        switch verdict {
        case "buy", "buyZone": return .good
        case "trim", "sell": return .bad
        case "wait", "noPosition": return .warn
        default: return .neutral
        }
    }

    // MARK: Loading

    private func load() async {
        // Automatic exits first (stop / target reached on the daily candles), then current prices.
        await model.paperCheckExits()
        guard let client = model.client else { return }
        var seen = Set<String>()
        let assets = model.paper.positions.filter { seen.insert($0.key).inserted }.map(\.asset)
        guard !assets.isEmpty else { return }
        do {
            let q = try await client.quotes(assets)
            quotes = Dictionary(q.map { ("\($0.kind.rawValue):\($0.symbol)", $0.price) }, uniquingKeysWith: { a, _ in a })
            error = nil
            model.persistSession()
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// "Recommencer": starting capital, then a confirmation.
struct PaperResetSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var capital = Format.plain(Paper.defaultCapital, digits: 0)
    @State private var confirming = false

    private var value: Double? { PaperFormat.number(capital).flatMap { $0 > 0 ? $0 : nil } }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Capital de départ en $", text: $capital).keyboardType(.decimalPad)
                        .accessibilityLabel("Capital de départ en dollars")
                } header: {
                    Text("Capital de départ")
                } footer: {
                    Text("Les positions ouvertes et le journal simulés seront effacés. Aucun argent réel n'est en jeu.")
                }
            }
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle("Recommencer la simulation")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Recommencer") { confirming = true }.disabled(value == nil) }
            }
            .confirmationDialog("Effacer la simulation ?", isPresented: $confirming, titleVisibility: .visible) {
                Button("Recommencer avec \(PaperFormat.usd(value ?? Paper.defaultCapital))", role: .destructive) {
                    model.paperReset(capital: value ?? Paper.defaultCapital)
                    dismiss()
                }
            } message: {
                Text("Toutes les positions et ventes simulées seront effacées.")
            }
        }
        .presentationDetents([.medium, .large])
    }
}

/// "Simuler cet achat" from the Décision card: amount, stop and target (the plan's, editable), current price.
/// The decision shown now is kept with the position, to see later which verdicts actually worked.
struct PaperBuySheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let decision: Decision
    @State private var amount = ""
    @State private var stop = ""
    @State private var target = ""
    @State private var note = ""
    @State private var error: String?

    private var asset: Asset { Asset(symbol: decision.symbol, kind: decision.kind, name: decision.name) }
    private var price: Double? { model.live.price(asset)?.price ?? decision.price }
    private var label: String { decision.label.isEmpty ? decision.verdict.label : decision.label }
    private var against: Bool { decision.verdict != .buy && decision.verdict != .buyZone }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    DecisionRow(key: "\(decision.symbol) · \(decision.name)", value: Format.price(price))
                    DecisionRow(key: "Décision affichée", value: "\(label) · confiance \(Int(decision.confidence.rounded())) %")
                    DecisionRow(key: "Liquidités simulées", value: PaperFormat.usd(model.paper.cash))
                } footer: {
                    Text("Achat fictif dans le portefeuille simulé (Mes avoirs › Simulation) : aucun argent réel, aucun ordre passé.")
                }
                if against {
                    Section {
                        Notice(text: "La décision actuelle est « \(label) » : vous simulez contre la décision. C'est permis ; le résultat sera classé sous « \(label) » dans les statistiques.", tone: .warn)
                    }
                    .listRowBackground(Color.clear)
                }
                Section {
                    TextField("Montant en $ (frais inclus)", text: $amount).keyboardType(.decimalPad)
                        .accessibilityLabel("Montant en dollars, frais inclus")
                } header: {
                    Text("Montant")
                } footer: {
                    Text("Frais de 0,1 % et glissement de 0,05 % déduits, comme pour un vrai ordre.")
                }
                Section {
                    TextField("Stop en $ (facultatif)", text: $stop).keyboardType(.decimalPad)
                        .accessibilityLabel("Stop en dollars, facultatif")
                    TextField("Objectif en $ (facultatif)", text: $target).keyboardType(.decimalPad)
                        .accessibilityLabel("Objectif en dollars, facultatif")
                } header: {
                    Text("Sortie automatique")
                } footer: {
                    Text("Pré-remplis avec le stop et l'objectif 1 du plan. Vérifiés sur les bougies journalières à partir du lendemain ; un stop au-dessus du prix d'achat ou un objectif en dessous est ignoré.")
                }
                Section {
                    JournalNoteField(text: $note)
                }
                if let error {
                    Section { Notice(text: error, tone: .bad) }.listRowBackground(Color.clear)
                }
            }
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle("Simuler cet achat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Simuler", action: buy) }
            }
            .onAppear(perform: prefill)
        }
        .presentationDetents([.large])
    }

    private func prefill() {
        guard amount.isEmpty else { return }
        var prices: [String: Double] = [:]
        for p in model.paper.positions { if let t = model.live.price(p.asset) { prices[p.key] = t.price } }
        let equity = Paper.valuation(model.paper, prices: prices).equity
        let a = (min(equity * 0.1, model.paper.cash) * 100).rounded(.down) / 100
        amount = a > 0 ? Format.plain(a, digits: 2) : ""
        if let plan = decision.plan {
            stop = Format.quantity(plan.stop)
            target = Format.quantity(plan.target1)
        }
    }

    /// Optional field: empty → nil; a text that is not a number is refused.
    private func parseOptional(_ s: String, _ name: String) -> (Double?, String?) {
        let t = s.trimmingCharacters(in: .whitespaces)
        if t.isEmpty { return (nil, nil) }
        guard let v = PaperFormat.number(t) else { return (nil, "\(name) invalide.") }
        return (v, nil)
    }

    private func buy() {
        let (s, e1) = parseOptional(stop, "Stop")
        let (t, e2) = parseOptional(target, "Objectif")
        if let e = e1 ?? e2 { return error = e }
        let order = PaperOrder(
            id: UUID().uuidString, symbol: decision.symbol, kind: decision.kind, name: decision.name,
            price: price ?? 0, amount: PaperFormat.number(amount) ?? .nan, stop: s, target: t,
            decision: PaperDecision(verdict: decision.verdict.rawValue, label: label, confidence: decision.confidence, asOf: decision.asOf)
        )
        if let e = model.paperBuy(order) {
            error = e
        } else {
            if let pos = model.paper.positions.last {
                model.recordTrade(source: .paper, side: .buy, asset: asset, price: pos.entry, quantity: pos.quantity, amount: pos.invested,
                                  stop: pos.stop, targets: [pos.target], note: note, refId: pos.id, decision: decision)
            }
            dismiss()
        }
    }
}
