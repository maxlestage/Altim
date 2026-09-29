import SwiftUI
import AltimKit

/// « Validation du modèle »: the signal's backtest (the decision's own track record, same costs) on a basket fixed in
/// advance, pooled by asset class, by market regime and overall (GET /api/validation). Same content and texts as the
/// web (Validation.tsx). Stacked cards, chips that wrap, rows stacked: nothing scrolls sideways. The per-asset list
/// follows the basket's order by default, never ranked by performance.
struct ValidationView: View {
    @Environment(AppModel.self) private var model
    @State private var report: ValidationReport?
    @State private var error: String?
    @State private var pending = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Le signal d'Altim rejoué jour par jour sur un panier d'actions, de bitcoin, d'ether et d'altcoins choisi à l'avance, avec les mêmes réglages et les mêmes coûts que l'historique de la carte Décision. Un test du passé, pas une promesse.")
                    .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                if let error { Notice(text: error, tone: .warn) }
                if report == nil && error == nil {
                    Card {
                        Text(pending ? ModelValidation.pendingText : "Chargement…").font(.footnote).foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                        ProgressView().frame(maxWidth: .infinity)
                    }
                }
                if let report { ValidationContent(report: report) }
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle("Validation du modèle")
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
        .refreshable { await load() }
    }

    /// The first computation takes ≈ 30 s: the server answers "pending" meanwhile, asked again every 5 seconds.
    private func load() async {
        guard let client = model.client else { return }
        error = nil
        while !Task.isCancelled {
            do {
                switch try await client.validation() {
                case .pending:
                    pending = true
                    try? await Task.sleep(for: .seconds(5))
                    continue
                case let .ready(r):
                    pending = false
                    report = r
                    model.persistSession()
                }
            } catch AltimError.unauthorized {
                model.sessionLost()
            } catch is CancellationError {
            } catch {
                if report == nil { self.error = error.localizedDescription }
            }
            return
        }
    }
}

/// The report itself: headline, classes, regimes (filtered by class), assets, biases.
private struct ValidationContent: View {
    let report: ValidationReport
    @State private var regimeGroup = "all"
    @State private var sort: ModelValidation.SortKey = .byClass

    var body: some View {
        let r = report
        let groups = r.groups
        let shown = groups.first { $0.id == regimeGroup } ?? r.overall
        VStack(alignment: .leading, spacing: 16) {
            headline(r)

            SectionLabel(text: "Par classe d'actifs")
            ForEach(r.classes) { g in ValidationGroupCard(group: g, taxRate: r.parameters.taxRatePct) }

            SectionLabel(text: "Par régime de marché")
            regimes(r, groups: groups, shown: shown)

            SectionLabel(text: "Actif par actif")
            assets(r)

            Card(title: "Protections contre les biais") {
                bullets(r.protections)
                (Text("Hors échantillon ? ").bold() + Text(r.outOfSample.note))
                    .font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }

            Card(title: "Biais et limites") {
                bullets(r.limits)
                caption(ModelValidation.costsText(r))
            }
        }
    }

    /// Chips "Tous · Actions · Bitcoin · Ethereum · Altcoins", then the regimes of the chosen group.
    @ViewBuilder private func regimes(_ r: ValidationReport, groups: [ValidationGroup], shown: ValidationGroup) -> some View {
        WrapLayout(spacing: 6) {
            ForEach(groups) { g in
                let on = g.id == regimeGroup
                Button { regimeGroup = g.id } label: {
                    TagChip(text: g.assetClass?.short ?? "Tous", color: .white, selected: on)
                }
                .buttonStyle(.borderless)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Actifs pris en compte")
        ForEach(ModelValidation.regimesOf(shown), id: \.regime) { g in ValidationRegimeCard(group: g) }
        caption(ModelValidation.regimeFooter(r))
    }

    /// Order chips, then one stacked row per asset (basket order by default).
    @ViewBuilder private func assets(_ r: ValidationReport) -> some View {
        WrapLayout(spacing: 6) {
            ForEach(ModelValidation.SortKey.allCases, id: \.self) { key in
                let on = key == sort
                Button { sort = key } label: {
                    TagChip(text: key.label, color: .white, selected: on)
                }
                .buttonStyle(.borderless)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Ordre des actifs")
        let rows = ModelValidation.sortAssets(r.assets, sort)
        Card {
            ForEach(Array(rows.enumerated()), id: \.element.id) { i, a in
                if i > 0 { Divider().overlay(Color.white.opacity(0.08)) }
                ValidationAssetRow(asset: a)
            }
        }
    }

    private func headline(_ r: ValidationReport) -> some View {
        Card {
            Text(r.headline).font(.subheadline.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            WrapLayout(spacing: 8) {
                tile("Actifs testés", ModelValidation.testedTile(r))
                tile("Trades", String(r.overall.pooled.trades))
                tile("Historique", ModelValidation.historyTile(r))
            }
            BeatMeter(beat: r.overall.beatHold, n: r.overall.assets, share: r.overall.beatShare)
            VerdictChip(verdict: r.overall.verdict, label: r.overall.verdictLabel)
            caption("Tous les trades ensemble : \(ModelValidation.pooledText(r.overall.pooled))")
            caption("Période : \(ModelValidation.periodText(r)) (bougies journalières). Calculé le \(Self.computed(r.asOf)), mis à jour toutes les 12 h.")
            if !r.failures.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text(ModelValidation.failuresTitle(r.failures.count)).font(.footnote.weight(.semibold)).foregroundStyle(Theme.warning)
                    ForEach(r.failures, id: \.symbol) { f in
                        (Text("• ") + Text(f.symbol).bold() + Text(" — \(f.error)"))
                            .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.warning.opacity(0.1)))
            }
        }
    }

    private func tile(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(Theme.textSecondary)
            Text(value).font(.subheadline.monospacedDigit().weight(.bold)).foregroundStyle(.white)
        }
        .padding(.horizontal, 10).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.white.opacity(0.06)))
        .accessibilityElement(children: .combine)
    }

    private func bullets(_ lines: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(lines, id: \.self) { l in
                Text("• \(l)").font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func caption(_ text: String) -> some View {
        Text(text).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    /// "29/09/2026 07:42" (local time, like the web's short date and time).
    static func computed(_ ms: Double?) -> String {
        guard let ms else { return "?" }
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "dd/MM/yyyy HH:mm"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }
}

private struct SectionLabel: View {
    let text: String
    var body: some View {
        Text(text).font(.headline).foregroundStyle(.white).padding(.top, 4).accessibilityAddTraits(.isHeader)
    }
}

/// Share of the assets where the signal beat buy-and-hold: one thin bar, the number written next to it.
private struct BeatMeter: View {
    let beat: Int
    let n: Int
    let share: Double?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            DecisionRow(key: "A battu la simple détention", value: ModelValidation.beatText(beat, n, share))
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.08))
                    Capsule().fill(Theme.cyan).frame(width: geo.size.width * CGFloat(min(100, max(0, share ?? 0))) / 100)
                }
            }
            .frame(height: 6)
            .accessibilityHidden(true)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("A battu la détention sur \(beat) actifs sur \(n)")
    }
}

/// The verdict, written out (the colour only repeats it).
private struct VerdictChip: View {
    let verdict: ValidationVerdict
    let label: String

    var body: some View {
        let tone = ModelValidation.verdictTone(verdict)
        let color = tone == .neutral ? Theme.cyan : Theme.color(tone)
        Text(label).font(.caption.weight(.semibold)).foregroundStyle(color)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().fill(color.opacity(0.12)))
            .overlay(Capsule().strokeBorder(color.opacity(0.5), lineWidth: 1))
    }
}

private struct ValidationGroupCard: View {
    let group: ValidationGroup
    let taxRate: Double

    var body: some View {
        let g = group
        Card(title: g.label) {
            VerdictChip(verdict: g.verdict, label: g.verdictLabel)
            Text(ModelValidation.groupSubtitle(g)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            Text(ModelValidation.pooledText(g.pooled)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            BeatMeter(beat: g.beatHold, n: g.assets, share: g.beatShare)
            ForEach(Array(ModelValidation.groupRows(g, taxRate: taxRate).enumerated()), id: \.offset) { _, row in
                DecisionRow(key: row.label, value: row.value, tone: row.tone.map { $0 >= 0 ? Tone.good : Tone.bad })
            }
        }
    }
}

private struct ValidationRegimeCard: View {
    let group: ValidationRegimeGroup

    var body: some View {
        let g = group
        Card(title: g.label) {
            VerdictChip(verdict: g.verdict, label: g.verdictLabel)
            Text(ModelValidation.pooledText(g.pooled)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            Text(ModelValidation.regimeDays(g)).font(.caption).foregroundStyle(Theme.textSecondary)
            if g.regime != .unknown {
                Text(ModelValidation.regimeDaysText(g)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

private struct ValidationAssetRow: View {
    let asset: ValidationAsset

    var body: some View {
        let a = asset
        VStack(alignment: .leading, spacing: 5) {
            NavigationLink {
                AssetDetailView(asset: a.asset)
            } label: {
                (Text(a.symbol).bold().foregroundStyle(.white) + Text("  \(a.name)").font(.caption).foregroundStyle(Theme.textSecondary))
                    .font(.subheadline)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .buttonStyle(.plain)
            WrapLayout(spacing: 6) {
                TagChip(text: ModelValidation.assetTag(a))
                if a.lowSample { TagChip(text: "échantillon trop faible") }
            }
            (Text("Signal ") + Text(ModelValidation.signedPct(a.totalReturn)).bold().foregroundStyle(a.totalReturn >= 0 ? Theme.buy : Theme.sell)
             + Text(" · détention ") + Text(ModelValidation.signedPct(a.buyAndHold)).bold().foregroundStyle(a.buyAndHold >= 0 ? Theme.buy : Theme.sell)
             + Text(" \(ModelValidation.assetGapText(a))").foregroundStyle(Theme.textSecondary))
                .font(.footnote).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            Text(ModelValidation.assetDetails(a)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// « Validation du modèle : le même test sur 34 actifs → » at the bottom of the track details.
struct ValidationLink: View {
    var body: some View {
        NavigationLink {
            ValidationView()
        } label: {
            Text("Validation du modèle : le même test sur 34 actifs →")
                .font(.caption.weight(.semibold)).foregroundStyle(Theme.cyan)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
        }
        .buttonStyle(.plain)
    }
}
