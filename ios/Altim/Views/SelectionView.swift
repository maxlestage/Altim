import SwiftUI
import AltimKit

/// Which stocks or cryptos to buy, for 8 holding durations: ranked by what was measured to work on the past,
/// each finalist checked, with an entry plan, a stop, a target and an amount.
struct SelectionView: View {
    @Environment(AppModel.self) private var model
    @State private var report: SelectionReport?
    @State private var pending = false
    @State private var error: String?
    @State private var budgetText = ""
    /// The text put in the field when the screen opens (not a typing: its currency is kept).
    @State private var budgetInitial = ""
    /// Currency of the field: the display currency, or the saved one when no rate allows the conversion.
    @State private var budgetCurrency: Currency = .usd
    @FocusState private var budgetFocused: Bool

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 8), count: 4)

    var body: some View {
        @Bindable var model = model
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                OpportunitiesLink()

                FittingPicker(title: "Marché", selection: $model.selectionMarket) {
                    Text("Actions").tag(Kind.stock)
                    Text("Cryptos").tag(Kind.crypto)
                }

                VStack(alignment: .leading, spacing: 8) {
                    Text("Durée de détention").font(.caption.bold()).foregroundStyle(Theme.textSecondary)
                    LazyVGrid(columns: columns, spacing: 8) {
                        ForEach(Horizon.allCases) { h in
                            Button { model.selectionHorizon = h } label: {
                                Text(h.label)
                                    .font(.system(size: 14, weight: .semibold, design: .rounded))
                                    .frame(maxWidth: .infinity, minHeight: 40)
                                    .foregroundStyle(model.selectionHorizon == h ? Color.black : .white)
                                    .background(RoundedRectangle(cornerRadius: 10).fill(model.selectionHorizon == h ? Theme.cyan : Color.white.opacity(0.07)))
                            }
                            .accessibilityAddTraits(model.selectionHorizon == h ? .isSelected : [])
                        }
                    }
                }

                if let error { ErrorView(message: error) { Task { await load() } } }

                if let report {
                    method(report)
                    if report.marketClosed {
                        Notice(text: "Bourse de New York fermée : ce classement vient de la dernière séance ; il changera à la réouverture.")
                    }
                    if let v = report.validation { ValidationCard(v: v, report: report) }
                    budgetCard
                    // Allocated in dollars (the prices' currency), shown in the display currency.
                    let amounts = report.allocate(budget: model.budgetUsd)
                    Text("À acheter · \(report.buy.count)").font(.headline)
                    ForEach(report.buy) { c in
                        PickCard(c: c, report: report, amount: amounts[c.symbol])
                    }
                    if !report.watch.isEmpty {
                        Text("À surveiller · \(report.watch.count)").font(.headline)
                        ForEach(report.watch) { c in
                            NavigationLink(value: Asset(symbol: c.symbol, kind: report.market, name: c.name)) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text("\(c.name) ").bold() + Text(c.symbol).foregroundStyle(Theme.textSecondary)
                                    if let r = c.reason { Text("⚠ \(r)").font(.footnote).foregroundStyle(Theme.warning) }
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .glassCard(glow: Theme.warning)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    if !report.setAside.isEmpty {
                        DisclosureGroup("Écartées faute de données fiables · \(report.setAside.count)") {
                            ForEach(report.setAside) { x in
                                Text("\(x.symbol) : \(x.reason)").font(.caption).frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                        .glassCard()
                    }
                    Text("Sélection calculée le \(Format.date(report.asOf, time: true)), prix en direct. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.")
                        .font(.caption).foregroundStyle(Theme.textSecondary)
                } else if error == nil {
                    VStack(spacing: 12) {
                        ProgressView()
                        Text(pending
                             ? "Analyse des \(model.selectionMarket == .crypto ? "120 cryptos" : "150 actions") en cours (environ 30 secondes la première fois)…"
                             : "Chargement de la sélection…")
                            .font(.footnote).foregroundStyle(Theme.textSecondary).multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity, minHeight: 160)
                }
            }
            .padding(16)
        }
        .scrollDismissesKeyboard(.interactively)
        .altimScreen()
        .navigationTitle("Sélection")
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .task(id: "\(model.selectionMarket.rawValue)|\(model.selectionHorizon.rawValue)") { await load() }
        .refreshable { await load() }
        .onAppear {
            if budgetText.isEmpty {
                // The saved budget converted to the display currency (as typed when no rate allows it).
                let shown = Money.convert(model.budget, from: model.budgetCurrency, to: Money.displayCurrency)
                budgetCurrency = shown.isFinite ? Money.displayCurrency : model.budgetCurrency
                if model.budget > 0 { budgetText = Format.plain(shown.isFinite ? shown : model.budget, digits: 0) }
                budgetInitial = budgetText
            }
            if let r = report { model.selectionAssets = r.buy.map { Asset(symbol: $0.symbol, kind: r.market, name: $0.name) } }
        }
        .onDisappear { model.selectionAssets = [] }
        .toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button("OK") { budgetFocused = false }
            }
        }
    }

    private func method(_ r: SelectionReport) -> some View {
        Card(title: "Comment la sélection est faite") {
            Text("Classement : \(r.rankText)").font(.subheadline.bold())
            Text(r.evidence).font(.footnote).foregroundStyle(.white.opacity(0.85))
            Text("\(r.scanned) \(r.market == .crypto ? "cryptos" : "actions") analysées. Chaque finaliste est vérifié : prix recoupés sur plusieurs sources, garde-fou marché, tendance de fond. Plan pour une détention de \(r.holdText) : entrée (zone d'achat Fibonacci), stop selon la volatilité, objectif à 2 fois le risque.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private var budgetCard: some View {
        Card(title: "Budget à investir") {
            HStack {
                TextField("10 000", text: $budgetText)
                    .keyboardType(.decimalPad)
                    .focused($budgetFocused)
                    .font(Theme.mono(18))
                    .onChange(of: budgetText) { _, t in
                        // Typed in the display currency and saved with it.
                        guard t != budgetInitial else { return }
                        budgetCurrency = Money.displayCurrency
                        model.budget = Money.parse(t) ?? 0
                        model.budgetCurrency = budgetCurrency
                    }
                Text(budgetCurrency.symbol).foregroundStyle(Theme.textSecondary)
            }
            .padding(10)
            .background(RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.06)))
            Text("Réparti pour que chaque ligne risque la même somme si son stop est touché (une valeur volatile reçoit moins), sans dépasser 20 % du budget par ligne.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        let market = model.selectionMarket, horizon = model.selectionHorizon
        report = nil
        error = nil
        pending = false
        // The first computation takes ≈ 30 s: the server answers "pending", we come back every 8 s (at most 3 min).
        for _ in 0..<24 {
            do {
                switch try await client.selection(horizon, kind: market) {
                case let .ready(r):
                    guard market == model.selectionMarket, horizon == model.selectionHorizon else { return }
                    report = r
                    pending = false
                    model.selectionAssets = r.buy.map { Asset(symbol: $0.symbol, kind: r.market, name: $0.name) }
                    model.persistSession()
                    return
                case .pending:
                    pending = true
                    try await Task.sleep(nanoseconds: 8_000_000_000)
                }
            } catch is CancellationError {
                return
            } catch AltimError.unauthorized {
                model.sessionLost()
                return
            } catch {
                if Task.isCancelled { return }
                self.error = error.localizedDescription
                return
            }
        }
        error = "Le calcul prend plus de temps que prévu. Tirez vers le bas pour réessayer."
    }
}

struct ValidationCard: View {
    var v: Validation
    var report: SelectionReport

    var body: some View {
        Card(title: "Ce que cette méthode aurait donné", glow: v.edge == "clear" ? Theme.buy : Theme.warning) {
            if v.edge == "none" {
                Notice(text: "Pas d'avance mesurée pour \(report.horizon.label). Une fois les frais payés (\(Format.plain(v.cost)) % l'aller-retour), ce classement n'a pas fait mieux que de choisir au hasard. Il est affiché à titre indicatif : ne misez pas dessus.", tone: .bad)
            } else if v.edge == "weak" {
                Notice(text: "Avance faible et irrégulière. En moyenne la sélection a fait mieux, mais seulement environ une fois sur deux : quelques très bons choix tirent la moyenne. Une durée plus longue est plus fiable.", tone: .warn)
            }
            Text(summary).font(.footnote)
            if report.market == .crypto && v.top < 0 {
                Notice(text: "Sur cette période, la sélection a perdu moins que les autres cryptos, mais elle a quand même perdu : quand presque toutes les cryptos baissent, bien choisir limite la casse sans l'éviter.", tone: .warn)
            }
            Text(limits).font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    private var summary: String {
        var s = "Rejouée \(v.periods) fois"
        if let from = v.from { s += " depuis le \(Format.date(from))" }
        s += " (sélection de \(v.topN), gardée \(report.holdText)) : \(Format.percent(v.top, digits: 1)) en moyenne pour la sélection contre \(Format.percent(v.universe, digits: 1)) pour l'ensemble des \(report.scanned) \(report.market == .crypto ? "cryptos" : "actions")"
        if let b = v.benchmark { s += " et \(Format.percent(b, digits: 1)) pour le simple achat de Bitcoin" }
        s += " ; la sélection a fait mieux que l'ensemble \(Int(v.beatRate.rounded())) % du temps."
        return s
    }

    private var limits: String {
        if report.horizon.isIntraday {
            return "Durées courtes : rejouées sur \(report.market == .crypto ? "quelques jours à 3 semaines" : "60 jours") seulement ; à ces échelles les prix sont surtout du bruit et les frais pèsent lourd. Ce n'est pas une garantie."
        }
        if report.market == .crypto {
            return "Limites honnêtes : l'historique ne couvre qu'environ 2 ans et demi (les plateformes gardent 1 000 jours), et la liste est celle des cryptos qui existent encore aujourd'hui, ce qui embellit les chiffres. Les cryptos restent très risquées. Ce n'est pas une garantie."
        }
        return "Limite honnête : la liste est celle des plus grandes sociétés d'aujourd'hui, qui ont par définition réussi, ce qui gonfle ces chiffres. Entre fin 2021 et 2023, la force relative n'a presque rien apporté ; l'essentiel de l'avance vient de 2023–2026. Ce n'est pas une garantie."
    }
}

struct PickCard: View {
    @Environment(AppModel.self) private var model
    var c: Candidate
    var report: SelectionReport
    var amount: Double?
    @State private var open: Bool?

    var body: some View {
        let asset = Asset(symbol: c.symbol, kind: report.market, name: c.name)
        let price = model.live.price(asset)?.price ?? c.price
        let buyAt = c.plan?.limit ?? price
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 10) {
                Text("\(c.rank)").font(Theme.mono(16, weight: .bold)).foregroundStyle(.black)
                    .frame(width: 30, height: 30).background(Circle().fill(Theme.cyan))
                    .accessibilityLabel("Rang \(c.rank)")
                VStack(alignment: .leading, spacing: 2) {
                    Text(c.name).font(.subheadline.bold()).lineLimit(2)
                    Text("\(c.symbol) · \(c.sector)").font(.caption).foregroundStyle(Theme.textSecondary)
                }
                Spacer(minLength: 6)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(Format.price(price)).font(Theme.mono(14))
                        .contentTransition(.numericText()).animation(.default, value: price)
                    Text("\(rankLabel) \(rankScore)/100").font(.caption2).foregroundStyle(Theme.textSecondary)
                }
            }

            if let plan = c.plan {
                VStack(spacing: 6) {
                    KeyValue(key: "Entrée", value: plan.limit.map { "ordre limite \(Format.price($0))" } ?? "maintenant ≈ \(Format.price(price))")
                    if plan.limit != nil { KeyValue(key: "ou tout de suite", value: "≈ \(Format.price(price))") }
                    KeyValue(key: "Stop", value: "\(Format.price(plan.stop)) (\(Format.percent((plan.stop / buyAt - 1) * 100, digits: 1)))", tone: .bad)
                    KeyValue(key: "Objectif", value: "\(Format.price(plan.target)) (\(Format.percent((plan.target / buyAt - 1) * 100, digits: 1)))", tone: .good)
                    if let amount, amount > 0 { amountView(amount, buyAt: buyAt, stop: plan.stop) }
                }
            }

            Button(isOpen ? "Masquer le détail" : "Pourquoi celle-ci ? Le détail") { open = !isOpen }
                .font(.footnote.bold())

            if isOpen {
                ForEach(report.orderedCriteria, id: \.self) { k in criterion(k) }
                ForEach(c.checks) { x in
                    Label {
                        Text("\(x.label) : ").bold() + Text(x.detail)
                    } icon: {
                        Image(systemName: x.ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill").foregroundStyle(x.ok ? Theme.buy : Theme.warning)
                    }
                    .font(.caption)
                }
                if let t = c.track {
                    Label {
                        Text("Signaux d'Altim sur ce titre : ").bold() + Text("\(t.trades) achats passés, \(Int(t.winRate.rounded())) % gagnants, \(Format.percent(t.avgReturn, digits: 1)) en moyenne")
                    } icon: { Image(systemName: "info.circle").foregroundStyle(Theme.cyan) }
                        .font(.caption)
                }
                HStack {
                    NavigationLink("Voir la fiche complète", value: asset).font(.footnote.bold())
                    Spacer()
                    if !model.isWatched(asset) {
                        Button("+ Ajouter au radar") { model.watch(asset) }.font(.footnote)
                    }
                }
            }
        }
        .glassCard(glow: c.rank <= 3 ? Theme.cyan : Theme.violet)
    }

    private var isOpen: Bool { open ?? (c.rank <= 3) }

    private var rankLabel: String {
        switch report.rankBy {
        case .signal: return "signal"
        case .momentum: return report.rankRule == "reversal" ? "rebond" : "force"
        case .risk: return "calme"
        case .trend: return "tendance"
        case .zone: return "zone"
        }
    }

    private var rankScore: Int {
        let s = report.rankRule == "reversal" ? 100 - (c.scores["momentum"] ?? 0) : (c.scores[report.rankBy.rawValue] ?? 0)
        return Int(s.rounded())
    }

    private func amountView(_ amount: Double, buyAt: Double, stop: Double) -> some View {
        // Whole shares for a stock; cryptos are divisible.
        let qty = buyAt > 0 ? (report.market == .stock ? (amount / buyAt).rounded(.down) : amount / buyAt) : 0
        let detail: String
        if qty > 0 {
            let units = report.market == .stock ? "\(Int(qty)) action\(qty > 1 ? "s" : "")" : "\(Format.quantity(qty)) \(c.symbol)"
            detail = "\(units) · perte max ≈ \(Format.money(qty * (buyAt - stop))) au stop"
        } else {
            detail = "moins d'une action : fractionnée chez votre courtier"
        }
        return VStack(alignment: .leading, spacing: 2) {
            KeyValue(key: "Montant suggéré", value: Format.money(amount), tone: .neutral)
            Text(detail).font(.caption).foregroundStyle(Theme.textSecondary).frame(maxWidth: .infinity, alignment: .trailing)
        }
    }

    private func criterion(_ k: Criterion) -> some View {
        let score = c.scores[k.rawValue] ?? 0
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(report.criteria[k.rawValue] ?? k.rawValue).font(.caption.bold())
                    .foregroundStyle(k == report.rankBy ? Theme.cyan : .white)
                Text(report.roles[k.rawValue] ?? "").font(.caption2).foregroundStyle(Theme.textSecondary)
                Spacer()
                Text("\(Int(score.rounded()))").font(Theme.mono(12))
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.08))
                    Capsule().fill(score >= 70 ? Theme.buy : score >= 40 ? Theme.warning : Theme.sell)
                        .frame(width: max(3, geo.size.width * min(100, score) / 100))
                }
            }
            .frame(height: 5)
            if let why = c.why[k.rawValue] { Text(why).font(.caption2).foregroundStyle(Theme.textSecondary) }
        }
    }
}

/// « Sélection Altim » (standalone): each week, the 3 strongest assets of the market among those in an up background
/// trend, and the rule replayed on the same assets against holding them all (AltimKit Rotation).
struct RotationView: View {
    @Environment(AppModel.self) private var model
    @State private var report: Loadable<RotationReport> = .idle

    var body: some View {
        @Bindable var model = model
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                FittingPicker(title: "Marché", selection: $model.selectionMarket) {
                    Text("Actions").tag(Kind.stock)
                    Text("Cryptos").tag(Kind.crypto)
                }
                switch report {
                case .idle, .loading:
                    Card(title: "Sélection Altim") {
                        ProgressView("Analyse de \(Rotation.universe[model.selectionMarket]?.count ?? 0) actifs…")
                            .frame(maxWidth: .infinity, minHeight: 100)
                    }
                case let .failed(m):
                    ErrorView(message: m) { Task { await load() } }
                case let .loaded(r):
                    content(r)
                }
                Text("Altim ne passe aucun ordre : une règle testée sur le passé, jamais une certitude.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle("Sélection")
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .refreshable { await load() }
        .task(id: model.selectionMarket) { await load() }
        .onDisappear { model.selectionAssets = [] }
    }

    @ViewBuilder private func content(_ r: RotationReport) -> some View {
        Card(title: "À détenir maintenant · \(r.selected.count) sur \(r.scanned)", glow: Theme.buy) {
            Text("Les \(r.kind == .crypto ? "cryptos" : "actions") en tendance de fond haussière avec un momentum positif (\(r.kind == .crypto ? "\(r.lookback) jours" : "3 mois")), chacune pour 1/\(r.top) du budget ; le reste (\(Int((Double(r.top - r.selected.count) / Double(max(1, r.top)) * 100).rounded())) %) en liquidités. Vérifié chaque jour.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if r.selected.isEmpty {
                Notice(text: "Aucun actif de ce marché n'est en tendance haussière : la règle reste entièrement en liquidités.", tone: .warn)
            }
            ForEach(r.selected) { p in NavigationLink(value: p.asset) { row(p) }.buttonStyle(.plain) }
        }
        if let t = r.test { testCard(t, r) }
        Card(title: "Les autres, du plus fort au plus faible") {
            ForEach(r.picks.filter { !$0.selected }) { p in NavigationLink(value: p.asset) { row(p) }.buttonStyle(.plain) }
        }
        Card(title: "Comment ça marche") {
            ForEach(r.notes, id: \.self) { n in
                Text("• \(n)").font(.caption).foregroundStyle(.white.opacity(0.85)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func row(_ p: RotationReport.Pick) -> some View {
        let tick = model.live.price(p.asset)
        return VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text("\(p.rank)").font(Theme.mono(12)).foregroundStyle(Theme.textSecondary).frame(width: 22, alignment: .leading)
                VStack(alignment: .leading, spacing: 2) {
                    Text(p.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                    Text(p.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(Format.price(tick?.price ?? p.price)).font(Theme.mono(13)).lineLimit(1).minimumScaleFactor(0.6)
                    if let m = p.momentum {
                        Text(Format.percent(m * 100, digits: 1)).font(Theme.mono(12)).foregroundStyle(Theme.color(forChange: m))
                    }
                }
                if p.selected { Image(systemName: "checkmark.seal.fill").foregroundStyle(Theme.buy).accessibilityLabel("Sélectionné") }
            }
            Text(p.reason).font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    private func testCard(_ t: RotationReport.Test, _ r: RotationReport) -> some View {
        let better = t.ret > t.basket
        return Card(title: "Rejoué sur le passé", glow: better ? Theme.buy : Theme.warning) {
            Text("Du \(Format.date(t.from)) au \(Format.date(t.to)), sur ces actifs : la règle (une part fixe par actif, investie seulement en tendance) contre tout garder. 0,1 % de frais par ordre.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 10) {
                tile("Parts Altim", t.ret, t.drawdown, Theme.magenta)
                tile("Tout garder", t.basket, t.basketDrawdown, .white)
            }
            TimeMachineChart(points: t.curve).frame(height: 160)
            KeyValue(key: "1re moitié", value: "\(TimeMachineCard.signed(t.firstHalf)) contre \(TimeMachineCard.signed(t.firstHalfBasket))",
                     tone: t.firstHalf > t.firstHalfBasket ? .good : .warn)
            KeyValue(key: "2de moitié", value: "\(TimeMachineCard.signed(t.secondHalf)) contre \(TimeMachineCard.signed(t.secondHalfBasket))",
                     tone: t.secondHalf > t.secondHalfBasket ? .good : .warn)
        }
    }

    private func tile(_ title: String, _ ret: Double, _ dd: Double, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption.weight(.semibold)).foregroundStyle(color)
            Text(TimeMachineCard.signed(ret)).font(Theme.mono(20, weight: .bold)).foregroundStyle(Theme.color(forChange: ret))
                .lineLimit(1).minimumScaleFactor(0.6)
            Text("pire creux \(TimeMachineCard.signed(dd))").font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.05)))
        .accessibilityElement(children: .combine)
    }

    private func load() async {
        guard let client = model.client else { return }
        report = .loading
        do {
            let r = try await client.rotation(model.selectionMarket)
            report = .loaded(r)
            model.selectionAssets = r.selected.map(\.asset)
        } catch {
            report = .failed(error.localizedDescription)
        }
    }
}
