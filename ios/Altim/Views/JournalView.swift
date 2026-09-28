import SwiftUI
import AltimKit

/// Journal (Mes avoirs › Journal): every entry with its automatic review, and the profile of the results. Same content
/// as the web (Journal.tsx); stored on this iPhone only. Stacked cards: nothing scrolls sideways.
struct JournalView: View {
    @Environment(AppModel.self) private var model
    @Binding var mode: Int
    /// Daily candles per asset ("kind:SYMBOL"); a missing key is loading, an empty array failed.
    @State private var candles: [String: [Candle]] = [:]
    @State private var failed: Set<String> = []
    @State private var horizon = 10
    @State private var editing: TradeJournalEntry?
    @State private var deleting: TradeJournalEntry?

    private var entries: [TradeJournalEntry] { model.tradeJournal.entries }

    private var assetsKey: String { Set(entries.map(\.key)).sorted().joined(separator: ",") }

    private var closedById: [String: TradeJournal.ClosedInfo] {
        Dictionary(model.paper.trades.map { ($0.id, TradeJournal.ClosedInfo(at: $0.closedAt, price: $0.exit, reason: $0.reason.label)) }, uniquingKeysWith: { a, _ in a })
    }

    var body: some View {
        let now = Date().timeIntervalSince1970 * 1000
        let closed = closedById
        let reviews = entries.map { e in
            TradeJournal.review(e, candles: candles[e.key] ?? [], now: now, closed: e.source == .paper ? e.refId.flatMap { closed[$0] } : nil)
        }
        let profile = TradeJournal.profile(reviews, horizon: horizon)
        let newest = reviews.sorted { $0.entry.createdAt > $1.entry.createdAt }
        List {
            Section { PortfolioModePicker(mode: $mode) }.listRowBackground(Color.clear)
            Section {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Enregistré uniquement sur cet iPhone · \(entries.count) entrée\(entries.count > 1 ? "s" : "")")
                        .font(.caption).foregroundStyle(Theme.textSecondary)
                    Text("Chaque achat simulé et chaque achat ou vente réel enregistré dans « Mes avoirs » est noté ici avec la décision affichée à ce moment-là, puis revu automatiquement à 3, 10 et 30 jours sur les bougies journalières suivantes. Des faits, pas des jugements : un bon trade peut perdre, un mauvais peut gagner.")
                        .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    if let e = model.tradeJournalError { Notice(text: e, tone: .warn) }
                }
            }
            .listRowBackground(Color.clear)
            if entries.isEmpty {
                Section {
                    Card(title: "Aucune entrée pour l'instant") {
                        Text("Simulez un achat depuis la carte « Décision » d'un actif, ou enregistrez un achat ou une vente dans Mes avoirs : l'entrée apparaîtra ici.")
                            .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                        Button("Mes avoirs réels") { mode = 0 }.buttonStyle(.borderless).font(.footnote)
                    }
                }
                .listRowBackground(Color.clear)
            } else {
                Section { ProfileCard(profile: profile, horizon: $horizon) }.listRowBackground(Color.clear)
                Section {
                    ForEach(newest) { r in
                        EntryCard(review: r, loading: candles[r.entry.key] == nil && !failed.contains(r.entry.key), failed: failed.contains(r.entry.key),
                                  edit: { editing = r.entry }, delete: { deleting = r.entry })
                    }
                } header: {
                    Text("Entrées")
                }
                .listRowBackground(Color.clear)
            }
        }
        .listStyle(.insetGrouped)
        .task(id: assetsKey) { await load() }
        .refreshable { await load(force: true) }
        .sheet(item: $editing) { JournalNoteSheet(entry: $0).environment(model) }
        .confirmationDialog("Supprimer cette entrée du journal (\(deleting?.name ?? "")) ?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                            titleVisibility: .visible) {
            Button("Supprimer", role: .destructive) {
                if let d = deleting { model.deleteJournalEntry(d.id) }
                deleting = nil
            }
        }
    }

    /// Daily candles of every asset of the journal, once per opening (cached by the server).
    private func load(force: Bool = false) async {
        guard let client = model.client else { return }
        let assets = Dictionary(entries.map { ($0.key, $0.asset) }, uniquingKeysWith: { a, _ in a })
        for (key, asset) in assets where force || (candles[key] == nil && !failed.contains(key)) {
            do {
                let c = try await client.candles(asset, interval: "1d").candles
                candles[key] = c
                failed.remove(key)
            } catch AltimError.unauthorized {
                return model.sessionLost()
            } catch is CancellationError {
                return
            } catch {
                failed.insert(key)
            }
        }
    }
}

/// "Pourquoi j'entre (facultatif, pour le journal)" in the order sheets and the holding form.
struct JournalNoteField: View {
    @Binding var text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Pourquoi j'entre (facultatif, pour le journal)").font(.caption).foregroundStyle(Theme.textSecondary)
            TextField("ex. rebond sur le support, objectif 1 visé", text: $text, axis: .vertical)
                .lineLimit(2...6)
                .onChange(of: text) { _, t in if t.count > 1000 { text = String(t.prefix(1000)) } }
        }
    }
}

private struct JournalNoteSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let entry: TradeJournalEntry
    @State private var note = ""

    var body: some View {
        NavigationStack {
            Form {
                Section { JournalNoteField(text: $note) }
            }
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle(entry.side == .buy ? "Pourquoi je suis entré" : "Pourquoi j'ai vendu")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Enregistrer") {
                        model.setJournalNote(entry.id, note)
                        dismiss()
                    }
                }
            }
            .onAppear { note = entry.note }
        }
        .presentationDetents([.medium])
    }
}

private enum JournalText {
    static func date(_ ms: Double, time: Bool = false) -> String {
        // Midnight UTC: a candle day, read as such (the web's frDate).
        if ms.truncatingRemainder(dividingBy: 86_400_000) == 0 {
            let f = DateFormatter()
            f.locale = Locale(identifier: "fr_FR")
            f.timeZone = TimeZone(identifier: "UTC")
            f.dateFormat = "d MMMM yyyy"
            return f.string(from: Date(timeIntervalSince1970: ms / 1000))
        }
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.timeZone = TimeZone(identifier: "Europe/Paris")
        f.dateFormat = time ? "d MMMM yyyy 'à' HH:mm" : "d MMMM yyyy"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }

    static func fr(_ v: Double, _ d: Int = 1) -> String { Format.plain(v, digits: d) }
}

private struct ProfileCard: View {
    let profile: TradeJournal.Profile
    @Binding var horizon: Int

    var body: some View {
        let p = profile
        Card(title: "Votre profil") {
            WrapLayout(spacing: 6) {
                ForEach(TradeJournal.reviewDays, id: \.self) { d in
                    Button { horizon = d } label: { TagChip(text: "À \(d) jours", color: .white, selected: d == horizon) }
                        .buttonStyle(.borderless)
                        .accessibilityAddTraits(d == horizon ? .isSelected : [])
                }
            }
            Text("\(p.reviewed) achat\(p.reviewed > 1 ? "s" : "") sur \(p.purchases) revu\(p.reviewed > 1 ? "s" : "") à \(horizon) jours"
                 + (p.pending > 0 ? " · \(p.pending) pas encore à \(horizon) jours" : "")
                 + ". Résultats en R (multiples du risque pris, entrée − stop) et en pire recul : un gain obtenu en prenant plus de risque ne compte pas davantage. Sortie supposée au premier niveau touché (stop ou objectif 1), sinon au dernier cours.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if let all = p.all {
                groups("Ensemble", [all])
                groups("Par note à l'entrée", p.byRating)
                groups("Plan respecté ou non", p.byPlan)
                groups("Par régime de marché", p.byRegime)
            } else {
                Text("Aucun achat n'a encore \(horizon) jours d'historique après l'entrée.").font(.caption).foregroundStyle(Theme.textSecondary)
            }
        }
    }

    @ViewBuilder private func groups(_ title: String, _ list: [TradeJournal.GroupStat]) -> some View {
        if !list.isEmpty {
            Text(title).font(.footnote.weight(.semibold)).foregroundStyle(.white).padding(.top, 4)
            ForEach(list) { g in
                VStack(alignment: .leading, spacing: 4) {
                    WrapLayout(spacing: 6) {
                        Text(g.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                        Text("\(g.n) achat\(g.n > 1 ? "s" : "")\(g.withStop < g.n ? " (\(g.withStop) avec stop)" : "")").font(.caption).foregroundStyle(Theme.textSecondary)
                        if g.lowSample { TagChip(text: "échantillon trop faible (< \(TradeJournal.minSample))") }
                    }
                    let avg = g.avgR ?? 0
                    DecisionRow(key: "Résultat moyen", value: TradeJournal.rValue(g.avgR), tone: avg > 0 ? .good : avg < 0 ? .bad : nil)
                    DecisionRow(key: "Médiane", value: TradeJournal.rValue(g.medianR))
                    DecisionRow(key: "Positifs", value: g.winRate.map { "\(JournalText.fr($0, 0)) %" } ?? "—")
                    DecisionRow(key: "Recul moyen", value: g.avgMaeR != nil ? TradeJournal.rValue(g.avgMaeR) : TradeJournal.pctValue(g.avgMaePct))
                    DecisionRow(key: "Pire recul", value: TradeJournal.rValue(g.worstMaeR))
                    DecisionRow(key: "Variation moy.", value: TradeJournal.pctValue(g.avgReturnPct))
                }
                .padding(10)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.04)))
            }
        }
    }
}

private struct EntryCard: View {
    let review: TradeJournal.EntryReview
    let loading: Bool
    let failed: Bool
    var edit: () -> Void
    var delete: () -> Void
    @State private var details = false

    private static let stepIcon = ["ok": "✓", "no": "✕", "unknown": "?"]

    var body: some View {
        let r = review
        let e = r.entry
        let buy = e.side == .buy
        let glow: Color = r.coherent == nil ? Theme.cyan : r.coherent == true ? Theme.buy : Theme.warning
        Card(glow: glow) {
            NavigationLink(value: e.asset) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(buy ? "Achat" : "Vente") · \(e.name)").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                            .fixedSize(horizontal: false, vertical: true)
                        Text("\(e.symbol) · \(JournalText.date(e.createdAt, time: true))").font(.caption.monospacedDigit()).foregroundStyle(Theme.textSecondary)
                    }
                    Spacer(minLength: 6)
                    TagChip(text: e.source == .paper ? "simulé" : "réel", color: e.source == .paper ? Theme.violet : Theme.textSecondary)
                }
            }
            .buttonStyle(.plain)
            DecisionRow(key: "Prix", value: Format.price(e.price))
            if buy {
                DecisionRow(key: "Stop\(r.levels.stopSource == .plan ? " (plan)" : "")", value: r.levels.stop.map { Format.price($0) } ?? "aucun")
                DecisionRow(key: "Objectifs\(r.levels.targetsSource == .plan ? " (plan)" : "")",
                            value: r.levels.targets.isEmpty ? "aucun" : r.levels.targets.map { Format.price($0) }.joined(separator: " · "))
                DecisionRow(key: "Gain visé", value: TradeJournal.rValue(r.planR))
            }
            (Text("Signal utilisé : ").foregroundStyle(Theme.textSecondary) + Text(e.signal).foregroundStyle(.white.opacity(0.9)))
                .font(.caption).fixedSize(horizontal: false, vertical: true)
            DisclosureGroup("Pourquoi, et le contexte de marché", isExpanded: $details) { context(e) }
                .font(.footnote)
                .tint(.white)
            HStack(alignment: .firstTextBaseline) {
                (Text("\(buy ? "Pourquoi je suis entré" : "Pourquoi j'ai vendu") : ").foregroundStyle(Theme.textSecondary)
                 + (e.note.isEmpty ? Text("pas de note").italic().foregroundStyle(Theme.textSecondary) : Text(e.note).foregroundStyle(.white)))
                    .font(.caption).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 6)
                Button(e.note.isEmpty ? "Ajouter" : "Modifier", action: edit).font(.caption).buttonStyle(.borderless)
            }
            reviewView(r, buy: buy)
            Button("Supprimer", role: .destructive, action: delete).font(.caption).buttonStyle(.borderless)
        }
    }

    @ViewBuilder private func context(_ e: TradeJournalEntry) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let d = e.decision {
                (Text(d.ratingLabel ?? d.label).bold()
                 + Text(" · confiance \(Int(d.confidence.rounded()))/100\(d.score.map { " · score \(Int($0.rounded()))/100" } ?? "") · décision du \(JournalText.date(d.asOf, time: true))"))
                    .font(.caption).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                Text(d.headline).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                if d.degraded { Notice(text: "Signal dégradé : \(d.degradedHeadline ?? "")", tone: .warn) }
                if !d.vetoes.isEmpty { Notice(text: "Interdictions d'achat actives : \(d.vetoes.joined(separator: ", "))", tone: .bad) }
                ForEach(d.pros, id: \.self) { Text("＋ \($0)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true) }
                ForEach(d.cons, id: \.self) { Text("－ \($0)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true) }
                Text("Configuration « \(d.setup.name) » : \(d.setup.met)/\(d.setup.total)").font(.caption).foregroundStyle(Theme.textSecondary)
                WrapLayout(spacing: 4) {
                    ForEach(Array(d.setup.steps.enumerated()), id: \.offset) { item in
                        TagChip(text: "\(Self.stepIcon[item.element.state] ?? "?") \(item.element.label)")
                    }
                }
                if let p = d.plan {
                    Text("Plan : zone \(Format.price(min(p.zoneFrom, p.zoneTo))) – \(Format.price(max(p.zoneFrom, p.zoneTo))), stop \(Format.price(p.stop)), objectif 1 \(Format.price(p.target1))"
                         + (p.target2.map { ", objectif 2 \(Format.price($0))" } ?? "") + ", rapport gain / risque \(JournalText.fr(p.riskReward)).")
                        .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
            } else {
                Text("Aucune décision chargée pour cet actif à ce moment-là.").font(.caption).foregroundStyle(Theme.textSecondary)
            }
            let m = e.market
            WrapLayout(spacing: 4) {
                TagChip(text: "Régime : \(m.regimeLabel ?? "inconnu")")
                TagChip(text: "Stress macro : \(m.macroScore.map { "\(Int($0.rounded()))/100 (\(TradeJournal.macroLabels[m.macroLevel ?? ""] ?? m.macroLevel ?? ""))" } ?? "inconnu")")
                TagChip(text: "ATR : \(m.atrPct.map { "\(JournalText.fr($0, 2)) %/jour" } ?? "inconnu")")
                TagChip(text: "Volume relatif : \(m.relativeVolume.map { "\(JournalText.fr($0, 2))×" } ?? "inconnu")")
                TagChip(text: "Événements à 7 j : \(m.events.map { "\($0)" } ?? "inconnu")")
            }
        }
        .padding(.top, 6)
    }

    @ViewBuilder private func reviewView(_ r: TradeJournal.EntryReview, buy: Bool) -> some View {
        Text("Revue automatique").font(.footnote.weight(.semibold)).foregroundStyle(.white).padding(.top, 4)
        if failed { Notice(text: "Cours journaliers indisponibles pour l'instant : revue non calculée.", tone: .warn) }
        if loading { Text("Chargement des cours…").font(.caption).foregroundStyle(Theme.textSecondary) }
        ForEach(r.horizons, id: \.days) { h in
            Text("• " + TradeJournal.horizonText(h, buy: buy, pendingDate: { JournalText.date($0) }))
                .font(.caption).foregroundStyle(h.status == .ready ? Color.white.opacity(0.9) : Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        if let c = r.closed {
            Text("Clôturée le \(JournalText.date(c.at)) (\(c.reason)) à \(Format.price(c.price))\(r.realizedR.map { " : \(TradeJournal.rValue($0)) réalisé" } ?? "").")
                .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        }
        if !r.worked.isEmpty {
            Text("Qu'est-ce qui a fonctionné ?").font(.caption.weight(.semibold)).foregroundStyle(.white)
            ForEach(r.worked, id: \.self) { InsightRow(icon: "checkmark.seal.fill", tone: .good, title: $0, detail: nil) }
        }
        if !r.failed.isEmpty {
            Text("Qu'est-ce qui n'a pas fonctionné ?").font(.caption.weight(.semibold)).foregroundStyle(.white)
            ForEach(r.failed, id: \.self) { InsightRow(icon: "exclamationmark.triangle.fill", tone: .warn, title: $0, detail: nil) }
        }
        Text("Le signal était-il cohérent avec les données disponibles à ce moment-là ?").font(.caption.weight(.semibold)).foregroundStyle(.white)
            .fixedSize(horizontal: false, vertical: true)
        (Text(r.coherent == nil ? "Non vérifiable" : r.coherent == true ? "Oui" : "Non").bold()
         + Text(r.coherent == false ? " : au moins un point ci-dessous ne l'était pas." : ""))
            .font(.caption).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
        ForEach(r.coherence) { c in
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(c.ok == nil ? "?" : c.ok == true ? "✓" : "✕").font(.caption.weight(.bold))
                    .foregroundStyle(c.ok == nil ? Theme.textSecondary : c.ok == true ? Theme.buy : Theme.sell)
                (Text(c.label).bold() + Text(" — \(c.detail)")).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
            .accessibilityElement(children: .combine)
        }
    }
}
