import SwiftUI
import AltimKit

/// Decision on one asset (GET /api/decision): verdict and level, confidence, the evidence families, the entry plan,
/// what would change the decision, then the details (vetoes, setup, scenarios, fundamentals, track record…).
/// The level is always written out, never shown by its colour alone.
struct DecisionCard: View {
    let decision: Decision
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(AppModel.self) private var model
    @State private var simulating = false

    private var nd: String { "non disponible" }

    var body: some View {
        Card(title: "Décision", glow: DecisionStyle.color(headlineLevel)) {
            verdict
            if let t = ConfigChanges.latestChange(model.configChanges.transitions, decision) { ChangeBlockView(transition: t) }
            Meter(label: "Confiance", value: decision.confidence, tone: decision.confidence >= 65 ? .good : decision.confidence >= 40 ? .warn : .bad)
            Text(decision.confidenceText).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if let e = decision.modelEvidence { EvidenceBlockView(evidence: e) }
            lights
            modeLine
            if let r = decision.marketRegime { RegimeLine(regime: r) }
            if let s = decision.score { ScoreBlock(score: s) }
            if let plan = decision.plan { planView(plan) }
            if let z = decision.actionZones { ActionLadderView(zones: z) }
            if !decision.whyWait.isEmpty {
                section("Pourquoi attendre ?")
                bullets(decision.whyWait)
            }
            conditions("Pour passer en ACHAT", decision.toBuy)
            conditions("Pour passer en VENTE", decision.toSell)
            if let c = decision.counterArgument { CounterBlockView(counter: c) }
            if let p = decision.position { positionView(p) }
            if let e = decision.exposure { exposureView(e) }
            details
            simulateButton
            Text("Calculé le \(Format.date(decision.asOf, time: true)).")
                .font(.caption2).foregroundStyle(Theme.textSecondary)
            Text(decision.disclaimer)
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .sheet(isPresented: $simulating) { PaperBuySheet(decision: decision).environment(model) }
    }

    /// Simulated purchase (paper trading): no real money, no order placed.
    private var simulateButton: some View {
        Button {
            simulating = true
        } label: {
            Label("Simuler cet achat", systemImage: "testtube.2")
        }
        .buttonStyle(NeonButtonStyle(color: Theme.violet, filled: false))
        .accessibilityHint("Achat fictif dans le portefeuille simulé : aucun argent réel, aucun ordre passé")
    }

    // MARK: Verdict

    /// Level whose colour the headline takes: the rating's when there is one.
    private var headlineLevel: Decision.Level { decision.rating?.level ?? decision.level }

    private var verdictLabel: String { decision.label.isEmpty ? decision.verdict.label : decision.label }

    private var verdict: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let rating = decision.rating {
                let label = decision.ratingLabel.flatMap { $0.isEmpty ? nil : $0 } ?? rating.label
                Text("\(rating.emoji) \(label)")
                    .font(.title2.weight(.heavy))
                    .foregroundStyle(DecisionStyle.color(rating.level))
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                Text(verdictLabel)
                    .font(.title2.weight(.heavy))
                    .foregroundStyle(DecisionStyle.color(decision.level))
                    .fixedSize(horizontal: false, vertical: true)
            }
            Text("\(decision.level.emoji) \(decision.levelLabel.isEmpty ? decision.level.label : decision.levelLabel)")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
            if decision.rating != nil {
                (Text("Verdict du plan : ").foregroundStyle(.white.opacity(0.9)) + Text(verdictLabel).bold().foregroundStyle(.white)
                    + Text(" · la note résume verdict, niveau et confiance").foregroundStyle(Theme.textSecondary))
                    .font(.footnote).fixedSize(horizontal: false, vertical: true)
            }
            if let why = decision.ratingReason, !why.isEmpty {
                Text(why).font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            if let dg = decision.degraded, dg.active { DegradedBanner(degraded: dg) }
            if let nt = decision.noTrade, nt.active { NoTradeBanner(noTrade: nt) }
            Text(decision.headline).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if decision.blocked {
                Notice(text: "Achat interdit pour l'instant : " + decision.vetoes.filter(\.active).map(\.label).joined(separator: ", ") + ".", tone: .bad)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var lights: some View {
        let columns = [GridItem(.adaptive(minimum: dynamicTypeSize.isAccessibilitySize ? 260 : 130), spacing: 8, alignment: .leading)]
        return LazyVGrid(columns: columns, alignment: .leading, spacing: 8) {
            ForEach(decision.families) { FamilyLight(family: $0) }
        }
    }

    private var modeLine: some View {
        HStack(alignment: .top, spacing: 6) {
            Image(systemName: decision.isPersonal ? "person.crop.circle" : "info.circle").foregroundStyle(Theme.cyan)
            Text(decision.isPersonal
                 ? "Mode personnel : votre prix d'achat moyen et la part de chaque ligne dans votre portefeuille (en %, jamais les quantités ni les montants) sont transmis pour ce calcul et jamais conservés."
                 : "Mode informationnel : données de marché uniquement, sans vos avoirs.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
    }

    // MARK: Plan

    private func planView(_ p: Decision.Plan) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            section("Plan · \(p.horizon)")
            DecisionRow(key: "Zone d'achat", value: "\(Format.price(p.zoneFrom)) – \(Format.price(p.zoneTo))")
            DecisionRow(key: "Stop / invalidation", value: "\(Format.price(p.stop)) (\(Format.percent(-abs(p.riskPct), digits: 1)))", tone: .bad)
            DecisionRow(key: "Objectif 1", value: "\(Format.price(p.target1)) (\(Format.percent(p.reward1Pct, digits: 1)))", tone: .good)
            if let t2 = p.target2 {
                DecisionRow(key: "Objectif 2", value: "\(Format.price(t2))" + (p.reward2Pct.map { " (\(Format.percent($0, digits: 1)))" } ?? ""), tone: .good)
            }
            if let t3 = p.target3 {
                DecisionRow(key: "Objectif 3", value: "\(Format.price(t3))" + (p.reward3Pct.map { " (\(Format.percent($0, digits: 1)))" } ?? ""), tone: .good)
            } else if decision.rating != nil {
                DecisionRow(key: "Objectif 3", value: "aucun")
            }
            DecisionRow(key: "Gain/risque", value: "\(Format.plain(p.riskReward, digits: 1)) (minimum \(Format.plain(p.minRiskReward, digits: 1)))",
                        tone: p.acceptable ? .good : .bad)
            Text("Calculé depuis \(Format.price(p.entry)) : " + (p.acceptable ? "rapport suffisant." : "rapport insuffisant, pas d'entrée à ce prix."))
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if let h = decision.horizon {
                (Text("Horizon : ").foregroundStyle(Theme.textSecondary) + Text(h.label).bold().foregroundStyle(.white)
                    + Text(" — \(h.detail)").foregroundStyle(Theme.textSecondary))
                    .font(.caption).fixedSize(horizontal: false, vertical: true)
            }
            if let src = p.target3Source, !src.isEmpty {
                Text(DecisionText.target3Source(src)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    @ViewBuilder private func conditions(_ title: String, _ list: [Decision.Condition]) -> some View {
        if !list.isEmpty {
            section(title)
            ForEach(Array(list.enumerated()), id: \.offset) { item in
                let c = item.element
                Text("• \(c.text)" + (c.level.map { " — niveau \(Format.price($0))" } ?? ""))
                    .font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    // MARK: Position and exposure (personal mode)

    private func positionView(_ p: Decision.Position) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            section("Votre position")
            DecisionRow(key: "Prix d'achat moyen", value: Format.price(p.cost))
            if let pnl = p.pnlPct { DecisionRow(key: "Plus ou moins-value", value: Format.percent(pnl, digits: 1), tone: pnl >= 0 ? .good : .bad) }
            Text(p.advice).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if !p.exits.isEmpty {
                Text("Sorties progressives").font(.subheadline.weight(.semibold)).padding(.top, 2)
                ForEach(Array(p.exits.enumerated()), id: \.offset) { item in exitRow(item.element) }
            }
        }
    }

    private func exitRow(_ e: Decision.Exit) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline) {
                Text("Vendre \(Format.plain(e.share, digits: 0)) % si \(DecisionStyle.lowerFirst(e.trigger))")
                    .font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 6)
                if e.now { Badge(text: "MAINTENANT", tone: .warn) }
            }
            Text(e.kind.label + (e.price.map { " · \(Format.price($0))" } ?? ""))
                .font(.caption.monospacedDigit()).foregroundStyle(Theme.textSecondary)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(e.now ? 0.08 : 0.04), in: RoundedRectangle(cornerRadius: 10))
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private func exposureView(_ e: Decision.Exposure) -> some View {
        if let w = e.warning {
            Notice(text: w, tone: .warn)
        }
        Text("Exposition au facteur \(e.factor) : \(Format.plain(e.weight, digits: 0)) % du portefeuille (\(e.assets.joined(separator: ", ")))"
             + (e.correlation.map { " ; corrélation de cet actif : \(Format.plain($0, digits: 2))" } ?? "") + ".")
            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    // MARK: Details

    private var familiesTitle: String {
        let available = decision.families.filter { $0.status != .unavailable && $0.status != .unknown }.count
        return "Familles · \(available)/\(decision.families.count) disponibles"
    }
    private var vetoesTitle: String { "Interdictions d'achat · \(decision.vetoes.filter(\.active).count) active(s)" }
    private var setupTitle: String { "Setup : \(decision.setup.name) · \(decision.setup.met)/\(decision.setup.total) étapes" }
    private var sourcesTitle: String { "Sources · \(decision.sources.filter(\.ok).count)/\(decision.sources.count) en ligne" }

    private func scoreText(_ f: Decision.Family) -> String {
        guard let s = f.score else { return "\(f.status.label) · \(nd)" }
        return "\(f.status.label) · \(s >= 0 ? "+" : "−")\(Int(abs(s).rounded()))"
    }

    @ViewBuilder private var details: some View {
        if let nt = decision.noTrade {
            DisclosureGroup("Quand ne pas trader · \(nt.badge)") {
                NoTradeList(noTrade: nt).padding(.top, 6)
            }
            .tint(.white)
        }
        DisclosureGroup(familiesTitle) {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(decision.families) { familyDetail($0) }
            }
            .padding(.top, 6)
        }
        .tint(.white)
        if let st = decision.structure {
            DisclosureGroup("Structure technique · \(st.score.map { "direction \(DecisionText.signedScore($0))" } ?? "non disponible")") {
                StructureList(structure: st).padding(.top, 6)
            }
            .tint(.white)
        }
        if decision.knowsEvents {
            DisclosureGroup("Agenda (7 jours) · \(DecisionText.eventsBadge(decision.events))") {
                VStack(alignment: .leading, spacing: 10) {
                    if let note = DecisionText.eventsNote(decision.events, kind: decision.kind) {
                        Text(note).font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(Array((decision.events ?? []).enumerated()), id: \.offset) { item in
                        let e = item.element
                        EventRow(event: e, timeLabel: Agenda.dayLabel(e.day) + (e.time.map { " · \($0)" } ?? ""))
                        Divider().opacity(0.3)
                    }
                }
                .padding(.top, 6)
            }
            .tint(.white)
        }
        DisclosureGroup(vetoesTitle) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(decision.sortedVetoes) { vetoRow($0) }
            }
            .padding(.top, 6)
        }
        .tint(.white)
        DisclosureGroup(setupTitle) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(decision.setup.steps.enumerated()), id: \.offset) { item in stepRow(item.offset + 1, item.element) }
            }
            .padding(.top, 6)
        }
        .tint(.white)
        if !decision.scenarios.isEmpty {
            DisclosureGroup("Scénarios" + (DecisionGuidance.scenariosBadge(decision).map { " · \($0)" } ?? "")) {
                VStack(alignment: .leading, spacing: 8) {
                    if let u = decision.unfolding {
                        Text(u.text).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(Array(decision.scenarios.enumerated()), id: \.offset) { item in scenarioRow(item.element) }
                }
                .padding(.top, 6)
            }
            .tint(.white)
        }
        if !decision.pros.isEmpty || !decision.cons.isEmpty {
            DisclosureGroup("Points favorables / défavorables") {
                VStack(alignment: .leading, spacing: 4) {
                    if !decision.pros.isEmpty { Text("Favorables").font(.caption.weight(.semibold)).foregroundStyle(Theme.buy) }
                    bullets(decision.pros, mark: "+")
                    if !decision.cons.isEmpty { Text("Défavorables").font(.caption.weight(.semibold)).foregroundStyle(Theme.sell).padding(.top, 4) }
                    bullets(decision.cons, mark: "−")
                }
                .padding(.top, 6)
            }
            .tint(.white)
        }
        DisclosureGroup("Pourquoi pas ?") {
            VStack(alignment: .leading, spacing: 4) {
                Text("Ce qui pourrait rendre cette décision fausse · incertitude \(decision.whyNot.uncertainty.label)")
                    .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                bullets(decision.whyNot.risks)
                if !decision.whyNot.invalidation.isEmpty {
                    Text("Invalidation").font(.caption.weight(.semibold)).padding(.top, 4)
                    bullets(decision.whyNot.invalidation)
                }
            }
            .padding(.top, 6)
        }
        .tint(.white)
        if let f = decision.fundamentals {
            DisclosureGroup("Fondamentaux") {
                VStack(alignment: .leading, spacing: 6) { fundamentals(f) }.padding(.top, 6)
            }
            .tint(.white)
        }
        if let l = decision.liquidity {
            DisclosureGroup("Liquidité") {
                VStack(alignment: .leading, spacing: 6) {
                    DecisionRow(key: "Écart achat/vente", value: l.spreadPct.map { "\(Format.plain($0, digits: 4)) %" } ?? nd)
                    DecisionRow(key: "Échangé par jour (20 j)", value: l.dailyValue.map { Format.large($0) } ?? nd)
                    DecisionRow(key: "Volume relatif", value: l.relativeVolume.map { Format.plain($0, digits: 1) } ?? nd)
                    source(l.source)
                }
                .padding(.top, 6)
            }
            .tint(.white)
        }
        if let t = decision.track {
            DisclosureGroup("Historique du signal") {
                VStack(alignment: .leading, spacing: 6) { trackView(t) }.padding(.top, 6)
            }
            .tint(.white)
        }
        if !decision.sources.isEmpty {
            DisclosureGroup(sourcesTitle) {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(decision.sources.enumerated()), id: \.offset) { item in
                        let s = item.element
                        Text(s.ok ? "✔ \(s.name) · \(s.detail)" : "✕ \(s.name) · \(s.detail)")
                            .font(.caption).foregroundStyle(s.ok ? .white : Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(.top, 6)
            }
            .tint(.white)
        }
    }

    private func familyDetail(_ f: Decision.Family) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline) {
                Text(f.label).font(.subheadline.weight(.semibold))
                Spacer(minLength: 6)
                Text(scoreText(f))
                    .font(.caption.monospacedDigit().weight(.semibold)).foregroundStyle(DecisionStyle.color(f.status))
                    .multilineTextAlignment(.trailing)
            }
            Text(f.summary).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            bullets(f.points)
            source(f.source)
        }
        .accessibilityElement(children: .combine)
    }

    private func vetoRow(_ v: Decision.Veto) -> some View {
        let state = !v.verifiable ? "non vérifiable" : v.active ? "ACTIVE" : "non active"
        let tone: Tone = !v.verifiable ? .neutral : v.active ? .bad : .good
        return VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Image(systemName: !v.verifiable ? "questionmark.circle" : v.active ? "xmark.octagon.fill" : "checkmark.circle")
                    .foregroundStyle(Theme.color(tone))
                Text(v.label).font(.footnote.weight(v.active ? .semibold : .regular)).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 6)
                Text(state).font(.caption.weight(.semibold)).foregroundStyle(Theme.color(tone))
            }
            Text(v.detail).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
    }

    private func stepRow(_ n: Int, _ s: Decision.Step) -> some View {
        let state: (icon: String, text: String, tone: Tone)
        switch s.state {
        case .ok: state = ("checkmark.circle.fill", "fait", .good)
        case .no: state = ("xmark.circle", "pas encore", .bad)
        case .unknown: state = ("questionmark.circle", "inconnu", .neutral)
        }
        return VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Image(systemName: state.icon).foregroundStyle(Theme.color(state.tone))
                Text("\(n). \(s.label)").font(.footnote).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 6)
                Text(state.text).font(.caption.weight(.semibold)).foregroundStyle(Theme.color(state.tone))
            }
            if !s.detail.isEmpty && s.detail != "—" {
                Text(s.detail).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func scenarioRow(_ s: Decision.Scenario) -> some View {
        let tone: Tone = s.kind == .bull ? .good : s.kind == .bear ? .bad : .warn
        let unfolding = s.unfolding == true
        return VStack(alignment: .leading, spacing: 2) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline) {
                    Text("\(s.kind.icon) \(s.title)").font(.footnote.weight(.semibold)).foregroundStyle(Theme.color(tone))
                    Spacer(minLength: 6)
                    if let count = DecisionGuidance.scenarioCount(s) { Text(count).font(.caption.monospacedDigit()).foregroundStyle(unfolding ? Theme.cyan : Theme.textSecondary) }
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(s.kind.icon) \(s.title)").font(.footnote.weight(.semibold)).foregroundStyle(Theme.color(tone))
                    if let count = DecisionGuidance.scenarioCount(s) { Text(count).font(.caption.monospacedDigit()).foregroundStyle(unfolding ? Theme.cyan : Theme.textSecondary) }
                }
            }
            Text("Si \(DecisionStyle.lowerFirst(s.condition)) → \(s.consequence)")
                .font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if let l = s.level { Text("Niveau à surveiller : \(Format.price(l))").font(.caption.monospacedDigit()).foregroundStyle(Theme.textSecondary) }
            if let checks = s.conditions, !checks.isEmpty { ScenarioChecksView(checks: checks) }
        }
        .padding(unfolding ? 8 : 0)
        .background(unfolding ? Theme.cyan.opacity(0.05) : Color.clear, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(unfolding ? Theme.cyan : Color.clear, lineWidth: 1))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(unfolding ? .isSelected : [])
    }

    // MARK: Fundamentals

    @ViewBuilder private func fundamentals(_ f: Decision.Fundamentals) -> some View {
        switch f {
        case let .stock(s): stockView(s)
        case let .crypto(c): cryptoView(c)
        case .unknown: Text("Fondamentaux non disponibles dans cette version de l'app.").font(.footnote).foregroundStyle(Theme.textSecondary)
        }
    }

    private func pct(_ v: Double?, digits: Int = 1) -> String { v.map { "\(Format.plain($0, digits: digits)) %" } ?? nd }
    private func signed(_ v: Double?) -> String { v.map { Format.percent($0, digits: 1) } ?? nd }
    private func ratio(_ v: Double?, digits: Int = 1) -> String { v.map { Format.plain($0, digits: digits) } ?? nd }
    private func amount(_ v: Double?) -> String { v.map { Format.large($0) } ?? nd }
    private func units(_ v: Double?) -> String { v.map { Format.large($0, unit: "") } ?? nd }

    @ViewBuilder private func stockView(_ s: Decision.StockFundamentals) -> some View {
        Text(s.period).font(.caption).foregroundStyle(Theme.textSecondary)
        if let end = s.periodEnd, let filed = s.filedAt {
            note("Comptes arrêtés au \(DecisionText.nyDate(end)), déposés à la SEC le \(DecisionText.nyDate(filed)).")
        }
        if let sector = s.sector {
            Text("Secteur : \(sector.label) · \(sector.sicDescription) (code SIC \(sector.sic))")
                .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        }
        DecisionRow(key: "Chiffre d'affaires", value: "\(amount(s.revenue)) (\(signed(s.revenueGrowth)))")
        DecisionRow(key: "Bénéfice net", value: amount(s.netIncome))
        DecisionRow(key: "Bénéfice par action", value: "\(s.eps.map { Format.price($0) } ?? nd) (\(signed(s.epsGrowth)))")
        DecisionRow(key: "Marge brute · opérationnelle · nette", value: "\(pct(s.grossMargin)) · \(pct(s.operatingMargin)) · \(pct(s.netMargin))")
        DecisionRow(key: "Flux de trésorerie libre", value: "\(amount(s.freeCashFlow)) (marge \(pct(s.fcfMargin)))")
        DecisionRow(key: "Dette · trésorerie", value: "\(amount(s.debt)) · \(amount(s.cash))")
        DecisionRow(key: "Dette nette", value: amount(s.netDebt))
        DecisionRow(key: "Rentabilité des capitaux propres", value: pct(s.roe))
        DecisionRow(key: "PER · PEG · EV/EBITDA", value: "\(ratio(s.per)) · \(ratio(s.peg, digits: 2)) · \(ratio(s.evEbitda))")
        if s.guidance != nil || s.ps != nil || s.pb != nil || s.roic != nil {
            DecisionRow(key: "P/S (capitalisation ÷ ventes)", value: ratio(s.ps))
            DecisionRow(key: "P/B (capitalisation ÷ fonds propres)", value: ratio(s.pb))
            DecisionRow(key: "ROIC (rentabilité du capital investi)", value: DecisionText.roic(s) ?? nd)
        }
        DecisionRow(key: "Rendement du dividende", value: pct(s.dividendYield, digits: 2))
        DecisionRow(key: "Nombre d'actions sur 1 an", value: signed(s.shareChange))
        DecisionRow(key: "Prochains résultats",
                    value: s.nextEarnings.map { "\(Format.date($0.date))" + ($0.estimated ? " (date estimée)" : " (date annoncée)") } ?? nd)
        if !s.surprises.isEmpty {
            Text("Résultats publiés vs attendus").font(.caption.weight(.semibold)).padding(.top, 2)
            ForEach(Array(s.surprises.enumerated()), id: \.offset) { item in
                let x = item.element
                Text("\(x.quarter) : BPA \(Format.price(x.eps)) contre \(Format.price(x.consensus)) attendu (\(Format.percent(x.surprisePct, digits: 1)))")
                    .font(.caption.monospacedDigit()).fixedSize(horizontal: false, vertical: true)
            }
        }
        if let r = s.revisions {
            Text("Révisions : BPA attendu de l'année \(Format.price(r.monthAgo)) il y a un mois, \(Format.price(r.now)) aujourd'hui (\(Format.percent(r.changePct, digits: 1))).")
                .font(.caption.monospacedDigit()).fixedSize(horizontal: false, vertical: true)
        }
        if let h = s.valuationHistory {
            block("Valorisation par rapport à sa propre histoire")
            if let v = s.valuationVerdict { note("\(v).", strong: true) }
            ForEach(Array((DecisionText.historyRows("PER", h.per) + DecisionText.historyRows("P/S", h.ps)).enumerated()), id: \.offset) { item in
                DecisionRow(key: item.element.0, value: item.element.1)
            }
            note("\(h.method). Source : \(h.source).")
        }
        if let c = s.peers {
            block("Comparaison sectorielle")
            note(s.sectorNote, strong: true)
            ForEach(Array(c.peers.enumerated()), id: \.offset) { item in
                Text("• \(DecisionText.peer(item.element))").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
            note("Cours du \(DecisionText.nyDate(c.date)). Source : \(c.source).")
        } else {
            Text(s.sectorNote).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        if let g = s.guidance, !g.isEmpty { note(g) }
        source(s.source)
    }

    @ViewBuilder private func cryptoView(_ c: Decision.CryptoFundamentals) -> some View {
        DecisionRow(key: "Capitalisation", value: amount(c.marketCap))
        DecisionRow(key: "Valeur totale diluée (FDV)", value: amount(c.fdv))
        DecisionRow(key: "Capitalisation / FDV", value: ratio(c.mcFdv, digits: 2))
        DecisionRow(key: "Offre en circulation", value: "\(units(c.circulatingSupply)) (\(pct(c.circulatingPct)) du maximum)")
        DecisionRow(key: "Offre totale · maximale", value: "\(units(c.totalSupply)) · \(units(c.maxSupply))")
        DecisionRow(key: "Valeur bloquée (TVL)", value: amount(c.tvl))
        DecisionRow(key: "Frais sur 30 jours", value: amount(c.fees30d))
        DecisionRow(key: "Dominance du bitcoin", value: pct(c.btcDominance))
        DecisionRow(key: "Financement (funding)", value: c.fundingRate.map { "\(Format.percent($0 * 100, digits: 4)) par 8 h" } ?? nd)
        DecisionRow(key: "Positions ouvertes (OI)", value: amount(c.openInterest))
        if let tx = c.txPerDay { DecisionRow(key: "Transactions par jour", value: Format.large(tx, unit: "")) }
        if let h = c.hashRate { DecisionRow(key: "Taux de hachage", value: "\(Format.plain(h / 1e18, digits: 0)) EH/s") }
        Text("Déblocages de jetons : \(c.unlocks)").font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        if c.stablecoins != nil || c.chainStablecoins != nil {
            block("Flux de stablecoins")
            let rows = DecisionText.stableRows(c.stablecoins, label: "tous réseaux")
                + DecisionText.stableRows(c.chainStablecoins, label: "réseau \(c.chainStablecoins?.scope ?? "")")
            ForEach(Array(rows.enumerated()), id: \.offset) { item in
                DecisionRow(key: item.element.0, value: item.element.1 ?? nd)
            }
            note("Liquidité disponible sur le marché crypto. Source : \((c.stablecoins ?? c.chainStablecoins)?.source ?? nd).")
        }
        if c.knowsDevActivity {
            block("Activité de développement")
            if let dev = c.devActivity {
                DecisionRow(key: "Commits sur 4 semaines", value: dev.commits4w.map { DecisionText.count($0) } ?? nd)
                DecisionRow(key: "Lignes ajoutées / supprimées (4 semaines)",
                            value: dev.additions4w.flatMap { a in dev.deletions4w.map { "+\(DecisionText.count(a)) / −\(DecisionText.count($0))" } } ?? nd)
                DecisionRow(key: "Pull requests intégrées (total)", value: dev.pullRequestsMerged.map { DecisionText.count($0) } ?? nd)
                DecisionRow(key: "Contributeurs", value: dev.contributors.map { DecisionText.count($0) } ?? nd)
                DecisionRow(key: "Étoiles", value: dev.stars.map { DecisionText.count($0) } ?? nd)
                source(dev.source)
            } else {
                note("Non disponible (CoinGecko ne la publie plus et le dépôt GitHub du projet n'a pas répondu).")
            }
        }
        if let n = c.notCovered, !n.isEmpty { note("\(n).") }
        source(c.source)
    }

    // MARK: Track record

    @ViewBuilder private func trackView(_ t: Decision.Track) -> some View {
        Text(t.period).font(.caption).foregroundStyle(Theme.textSecondary)
        DecisionRow(key: "Trades", value: "\(t.trades)")
        DecisionRow(key: "Gagnants", value: pct(t.winRate))
        DecisionRow(key: "Gain moyen · perte moyenne", value: "\(signed(t.avgWin)) · \(signed(t.avgLoss))")
        DecisionRow(key: "Profit factor", value: ratio(t.profitFactor, digits: 2))
        DecisionRow(key: "Sharpe · Sortino", value: "\(ratio(t.sharpe, digits: 2)) · \(ratio(t.sortino, digits: 2))")
        DecisionRow(key: "Pire recul (drawdown)", value: Format.percent(t.maxDrawdown, digits: 1), tone: .bad)
        DecisionRow(key: "Rendement du signal", value: Format.percent(t.totalReturn, digits: 1), tone: t.totalReturn >= 0 ? .good : .bad)
        DecisionRow(key: "Simple détention", value: Format.percent(t.buyAndHold, digits: 1))
        DecisionRow(key: "Frais · glissement par ordre", value: "\(pct(t.feesPct, digits: 2)) · \(pct(t.slippagePct, digits: 2))")
        DecisionRow(key: "Pire série de pertes", value: "\(t.losingStreak) trade\(t.losingStreak > 1 ? "s" : "")")
        Text(t.note).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        if t.hasDetails { TrackDetails(track: t) }
        ValidationLink().padding(.top, 2)
    }

    // MARK: Helpers

    /// Title of a block inside a disclosure (valuation history, peers, stablecoins, developer activity).
    private func block(_ title: String) -> some View {
        Text(title).font(.caption.weight(.semibold)).foregroundStyle(.white).padding(.top, 6).fixedSize(horizontal: false, vertical: true)
    }

    private func note(_ text: String, strong: Bool = false) -> some View {
        Text(text).font(.caption).foregroundStyle(strong ? Color.white.opacity(0.9) : Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    private func section(_ title: String) -> some View {
        Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.white).padding(.top, 4).fixedSize(horizontal: false, vertical: true)
    }

    private func bullets(_ items: [String], mark: String = "•") -> some View {
        ForEach(Array(items.enumerated()), id: \.offset) { item in
            Text("\(mark) \(item.element)").font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        }
    }

    private func source(_ s: String) -> some View {
        Text("Source : " + (s.isEmpty || s == "—" ? nd : s)).font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }
}

/// One family of evidence: its status written out, an icon that differs by status, then the colour.
private struct FamilyLight: View {
    let family: Decision.Family

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: DecisionStyle.icon(family.status)).foregroundStyle(DecisionStyle.color(family.status))
            VStack(alignment: .leading, spacing: 0) {
                Text(family.label).font(.caption.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                Text(family.status.label).font(.caption2).foregroundStyle(DecisionStyle.color(family.status))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(family.label) : \(family.status.label)" + (family.score.map { ", score \(Int($0.rounded())) sur une échelle de −100 à +100" } ?? ""))
    }
}

/// Key and value on one line, or stacked when the text is too large (Dynamic Type): the number is never cut.
struct DecisionRow: View {
    var key: String
    var value: String
    var tone: Tone?

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline) {
                keyText
                Spacer(minLength: 12)
                valueText.multilineTextAlignment(.trailing)
            }
            VStack(alignment: .leading, spacing: 2) {
                keyText.fixedSize(horizontal: false, vertical: true)
                valueText.fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var keyText: some View { Text(key).font(.subheadline).foregroundStyle(Theme.textSecondary) }
    private var valueText: some View {
        Text(value).font(.subheadline.monospacedDigit().weight(.semibold)).foregroundStyle(tone.map { Theme.color($0) } ?? .white)
    }
}

enum DecisionStyle {
    /// 🟢 strong · 🟡 moderate · ⚪ waiting · 🟠 highRisk · 🔴 exit
    static func color(_ level: Decision.Level) -> Color {
        switch level {
        case .strong: return Theme.buy
        case .moderate: return Theme.warning
        case .waiting, .unknown: return Color.white.opacity(0.8)
        case .highRisk: return Color.orange
        case .exit: return Theme.sell
        }
    }

    static func color(_ status: Decision.Status) -> Color {
        switch status {
        case .positive: return Theme.buy
        case .neutral: return Theme.warning
        case .negative: return Theme.sell
        case .unavailable, .unknown: return Theme.textSecondary
        }
    }

    static func icon(_ status: Decision.Status) -> String {
        switch status {
        case .positive: return "arrow.up.circle.fill"
        case .neutral: return "minus.circle.fill"
        case .negative: return "arrow.down.circle.fill"
        case .unavailable, .unknown: return "questionmark.circle"
        }
    }

    /// "Objectif 1 atteint" → "objectif 1 atteint" (kept as is when it starts with an acronym such as "VIX").
    static func lowerFirst(_ s: String) -> String {
        guard let first = s.first, s.count > 1 else { return s }
        let second = s[s.index(after: s.startIndex)]
        return second.isUppercase ? s : first.lowercased() + String(s.dropFirst())
    }
}
