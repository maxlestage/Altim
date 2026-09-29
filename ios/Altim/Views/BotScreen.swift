import SwiftUI
import AltimKit

/// « Bot Altim » v2: candidate models trained on long histories of the validation's basket and an extra universe
/// (GET /api/bot), chosen at each retraining on an inner validation and tested walk-forward on periods they had not
/// seen, saying ACHETER / ATTENDRE / VENDRE; today's view of the watched assets (GET /api/bot/views). Same content and
/// texts as the web (Bot.tsx); a v1 answer shows without the v2 parts. Stacked cards, chips that wrap, rows stacked:
/// nothing scrolls sideways. The per-asset list keeps the basket's order, never ranked by performance; each candidate
/// is shown alone for information only.
struct BotScreen: View {
    @Environment(AppModel.self) private var model
    @State private var report: BotReport?
    @State private var error: String?
    @State private var pending = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text(ModelBot.intro)
                    .font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                if let error { Notice(text: error, tone: .warn) }
                if report == nil && error == nil {
                    Card {
                        Text(pending ? ModelBot.pendingText : "Chargement…").font(.footnote).foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                        ProgressView().frame(maxWidth: .infinity)
                    }
                }
                if let report { BotReportContent(report: report) }
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle("Bot Altim")
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
        .refreshable { await load() }
    }

    /// The first training takes one to two minutes: the server answers "pending" meanwhile, asked again every 5 seconds.
    private func load() async {
        guard let client = model.client else { return }
        error = nil
        while !Task.isCancelled {
            do {
                switch try await client.bot() {
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
                if report == nil { self.error = error.localizedDescription.isEmpty ? "Bot indisponible" : error.localizedDescription }
            }
            return
        }
    }
}

/// The report: headline, changes, today's view of the watched assets, method, results by group, each candidate alone,
/// last year and extra universe, calibration, assets, limits.
private struct BotReportContent: View {
    let report: BotReport

    var body: some View {
        let r = report
        VStack(alignment: .leading, spacing: 16) {
            headline(r)

            if !r.changes.isEmpty {
                Card(title: ModelBot.changesTitle) { bullets(r.changes) }
            }

            BotWatchedViews()

            Card(title: "Comment il apprend et comment il est jugé") {
                bullets(r.method)
                DisclosureGroup {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(r.features, id: \.id) { f in
                            (Text("• ") + Text(f.label).bold() + Text(" — \(f.help)"))
                                .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .padding(.top, 4)
                } label: {
                    Text("Les \(r.features.count) mesures lues à chaque clôture").font(.footnote).foregroundStyle(Theme.cyan)
                        .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                }
                .tint(Theme.cyan)
            }

            BotSectionLabel(text: "Résultats hors échantillon")
            caption(ModelBot.resultsIntro)
            ForEach(r.groups) { g in BotGroupCard(group: g) }

            let withCandidates = ModelBot.withCandidates(r)
            if !withCandidates.isEmpty {
                BotSectionLabel(text: ModelBot.candidatesTitle)
                caption(ModelBot.candidatesIntro)
                ForEach(withCandidates) { g in BotCandidatesCard(group: g) }
            }

            if ModelBot.hasSubResults(r) {
                BotSectionLabel(text: ModelBot.subResultsTitle)
                ForEach(r.groups) { g in
                    if let h = g.holdout {
                        BotSubResultCard(title: ModelBot.holdoutTitle(g), note: ModelBot.holdoutNote(h), stats: h.stats)
                    }
                    if let x = g.extra {
                        BotSubResultCard(title: ModelBot.extraTitle(g), note: ModelBot.extraNote(g), stats: x)
                    }
                }
            }

            BotSectionLabel(text: "Calibration")
            caption(ModelBot.calibrationIntro)
            ForEach(r.groups) { g in
                BotCalibrationCard(title: ModelBot.calibrationTitle(g, up: true), buckets: g.stats.calibrationUp, skill: g.stats.brierSkillUp)
                BotCalibrationCard(title: ModelBot.calibrationTitle(g, up: false), buckets: g.stats.calibrationDown, skill: g.stats.brierSkillDown)
            }

            BotSectionLabel(text: "Actif par actif")
            caption(ModelBot.assetsIntro)
            Card {
                ForEach(Array(r.assets.enumerated()), id: \.element.id) { i, a in
                    if i > 0 { Divider().overlay(Color.white.opacity(0.08)) }
                    BotAssetRowView(asset: a)
                }
            }

            Card(title: "Limites") {
                bullets(r.limits)
                caption(ModelBot.footer(r))
                NavigationLink {
                    ValidationView()
                } label: {
                    Text("Voir la validation du signal →")
                        .font(.caption.weight(.semibold)).foregroundStyle(Theme.cyan)
                        .multilineTextAlignment(.leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func headline(_ r: BotReport) -> some View {
        Card {
            Text(r.headline).font(.subheadline.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            WrapLayout(spacing: 8) {
                tile("Actifs testés", ModelBot.assetsTile(r))
                tile("Jours testés", ModelBot.count(r.overall.labelled))
                tile("Achats / ventes", ModelBot.signalsTile(r))
            }
            caption("\(ModelBot.parametersText(r).dropLast()). Calculé le \(Self.computed(r.asOf)), réentraîné toutes les 12 h.")
            if !r.failures.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text(ModelBot.failuresTitle(r.failures.count)).font(.footnote.weight(.semibold)).foregroundStyle(Theme.warning)
                    ForEach(r.failures, id: \.symbol) { f in
                        (Text("• ") + Text(f.symbol).bold() + Text(" — \(f.error)"))
                            .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.warning.opacity(0.1)))
            }
            if let x = ModelBot.extraFailuresText(r) { caption(x) }
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

    /// "29/09/2026 13:36" (local time, like the web's short date and time).
    static func computed(_ ms: Double?) -> String {
        guard let ms else { return "?" }
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "dd/MM/yyyy HH:mm"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }
}

private struct BotSectionLabel: View {
    let text: String
    var body: some View {
        Text(text).font(.headline).foregroundStyle(.white).padding(.top, 4).accessibilityAddTraits(.isHeader)
    }
}

/// ACHETER (green), ATTENDRE (yellow), VENDRE (red): the word is always written, the colour only repeats it.
struct BotActionChip: View {
    let action: BotAction?

    private var color: Color {
        switch action {
        case .buy: return Theme.buy
        case .sell: return Theme.sell
        case .wait: return Theme.warning
        case nil: return Theme.textSecondary
        }
    }

    var body: some View {
        Text(action?.label ?? "pas d'avis")
            .font(.caption.weight(action == nil ? .regular : .bold))
            .tracking(action == nil ? 0 : 0.5)
            .foregroundStyle(color)
            .lineLimit(1)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Color.white.opacity(0.06), in: Capsule())
            .overlay(Capsule().strokeBorder(color.opacity(action == nil ? 0 : 0.55), lineWidth: 1))
    }
}

/// The out-of-sample verdict of one side, written out.
private struct BotVerdictChip: View {
    let verdict: ValidationVerdict?
    let label: String

    var body: some View {
        let tone = verdict.map(ModelValidation.verdictTone) ?? .neutral
        let color = tone == .neutral ? Theme.cyan : Theme.color(tone)
        Text(label).font(.caption.weight(.semibold)).foregroundStyle(color)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().fill(color.opacity(0.12)))
            .overlay(Capsule().strokeBorder(color.opacity(0.5), lineWidth: 1))
    }
}

/// Stocks or cryptos: ACHETER, VENDRE and ATTENDRE, each against a random day; v2: data span, t by day, sell exits and
/// the model chosen at each retraining.
private struct BotGroupCard: View {
    let group: BotGroupStat

    var body: some View {
        let g = group
        Card(title: g.label) {
            Text(ModelBot.groupSubtitle(g)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if let data = ModelBot.dataText(g) {
                Text("Données : \(data).").font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            side(.buy, verdict: g.buy.verdict, label: g.buy.verdictLabel, text: ModelBot.buyText(g.buy),
                 clustered: g.buy.clustered.map { ModelBot.clusteredText($0, g.buy.tStat) }, rows: ModelBot.buyRows(g.buy), after: nil)
            side(.sell, verdict: g.sell.verdict, label: g.sell.verdictLabel, text: ModelBot.sellText(g.sell),
                 clustered: g.sell.clustered.map { ModelBot.clusteredText($0, g.sell.tStat) }, rows: ModelBot.sellRows(g.sell),
                 after: ModelBot.exitText(g.sell.exit))
            side(.wait, verdict: nil, label: nil, text: ModelBot.waitText(g.wait), clustered: nil, rows: [], after: nil)
            if !g.selection.isEmpty { selection(g) }
        }
    }

    private func side(_ action: BotAction, verdict: ValidationVerdict?, label: String?, text: String, clustered: String?,
                      rows: [(label: String, value: String)], after: String?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Divider().overlay(Color.white.opacity(0.08))
            WrapLayout(spacing: 6) {
                BotActionChip(action: action)
                if let label { BotVerdictChip(verdict: verdict, label: label) }
            }
            Text(text).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if let clustered {
                Text(clustered).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                DecisionRow(key: row.label, value: row.value)
            }
            if let after {
                Text(after).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// « Modèle retenu à chaque réentraînement »: consecutive identical choices grouped, one stacked line per run.
    private func selection(_ g: BotGroupStat) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Divider().overlay(Color.white.opacity(0.08))
            Text(ModelBot.selectionTitle).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                .fixedSize(horizontal: false, vertical: true)
            ForEach(Array(ModelBot.selectionRuns(g.selection).enumerated()), id: \.offset) { _, x in
                (Text(ModelBot.runPeriod(x)).foregroundStyle(Theme.textSecondary) + Text(" ") + Text(ModelBot.runChoice(x)).bold().foregroundStyle(.white))
                    .font(.footnote).fixedSize(horizontal: false, vertical: true)
            }
            if let live = ModelBot.liveModelText(g) {
                Text(live).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// Each candidate's own out-of-sample result (for information, never used to choose): stacked rows, no table.
private struct BotCandidatesCard: View {
    let group: BotGroupStat

    var body: some View {
        Card(title: group.label) {
            ForEach(Array(group.candidates.enumerated()), id: \.offset) { i, c in
                if i > 0 { Divider().overlay(Color.white.opacity(0.08)) }
                row(c)
            }
        }
    }

    private func row(_ c: BotCandidateStat) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            WrapLayout(spacing: 6) {
                Text(c.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                    .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                TagChip(text: ModelBot.chosenText(c), color: Theme.textSecondary)
            }
            if !c.description.isEmpty {
                Text(c.description).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            ForEach(Array(ModelBot.candidateRows(c).enumerated()), id: \.offset) { _, row in
                DecisionRow(key: row.label, value: row.value)
            }
            WrapLayout(spacing: 6) {
                BotVerdictChip(verdict: c.buy.verdict, label: ModelBot.sideVerdictLabel("Achats", c.buy.verdictLabel))
                BotVerdictChip(verdict: c.sell.verdict, label: ModelBot.sideVerdictLabel("Ventes", c.sell.verdictLabel))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The last 12 months, or the extra training assets: the same figures, shorter.
private struct BotSubResultCard: View {
    let title: String
    let note: String
    let stats: BotStats

    var body: some View {
        let s = stats
        Card(title: title) {
            Text(note).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            WrapLayout(spacing: 6) {
                BotActionChip(action: .buy)
                BotVerdictChip(verdict: s.buy.verdict, label: s.buy.verdictLabel)
            }
            Text(ModelBot.buyText(s.buy)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            WrapLayout(spacing: 6) {
                BotActionChip(action: .sell)
                BotVerdictChip(verdict: s.sell.verdict, label: s.sell.verdictLabel)
            }
            Text(ModelBot.sellText(s.sell)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if let exit = ModelBot.exitText(s.sell.exit) {
                Text(exit).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// Predicted vs realised per probability bucket: two thin bars per row, the numbers written next to them.
private struct BotCalibrationCard: View {
    let title: String
    let buckets: [BotBucket]
    let skill: Double?

    var body: some View {
        Card(title: title) {
            Text(ModelBot.precisionText(skill)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            ForEach(ModelBot.calibrationRows(buckets), id: \.label) { b in
                VStack(alignment: .leading, spacing: 3) {
                    (Text(b.label).bold().foregroundStyle(.white) + Text(" · \(ModelBot.bucketDays(b))").foregroundStyle(Theme.textSecondary))
                        .font(.footnote).fixedSize(horizontal: false, vertical: true)
                    Text(ModelBot.bucketText(b)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                    bar(b.predicted, Theme.cyan)
                    bar(b.realised, Theme.warning)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(b.label), \(ModelBot.bucketDays(b)) : \(ModelBot.bucketText(b))")
            }
            HStack(spacing: 6) {
                key(Theme.cyan)
                Text("prévu").font(.caption).foregroundStyle(Theme.textSecondary)
                Text("·").font(.caption).foregroundStyle(Theme.textSecondary)
                key(Theme.warning)
                Text("observé").font(.caption).foregroundStyle(Theme.textSecondary)
            }
            .accessibilityHidden(true)
        }
    }

    private func bar(_ value: Double?, _ color: Color) -> some View {
        GeometryReader { geo in
            Capsule().fill(color).frame(width: geo.size.width * CGFloat(min(100, max(0, value ?? 0))) / 100)
        }
        .frame(height: 5)
    }

    private func key(_ color: Color) -> some View {
        Capsule().fill(color).frame(width: 14, height: 5)
    }
}

private struct BotAssetRowView: View {
    let asset: BotAssetRow

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
                TagChip(text: a.assetClass.short)
                BotActionChip(action: a.now.action)
            }
            Text(ModelBot.todayText(a)).font(.footnote).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            Text(ModelBot.assetTestText(a)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// « Vos actifs aujourd'hui »: today's view of the watched assets (cached report only; nothing is trained here).
private struct BotWatchedViews: View {
    @Environment(AppModel.self) private var model
    @State private var data: BotViews?
    @State private var error: String?

    var body: some View {
        let watchlist = model.watchlist
        if !watchlist.isEmpty {
            Card(title: "Vos actifs aujourd'hui") {
                if let error {
                    Text("Avis indisponibles (\(error)).").font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                } else if let data {
                    ForEach(Array(data.views.enumerated()), id: \.offset) { i, v in
                        if i > 0 { Divider().overlay(Color.white.opacity(0.08)) }
                        BotViewRow(view: v)
                    }
                } else {
                    Text("Chargement…").font(.caption).foregroundStyle(Theme.textSecondary)
                }
            }
            .task(id: watchlist.map(\.id).joined(separator: ",")) { await load(watchlist) }
        }
    }

    private func load(_ assets: [Asset]) async {
        guard let client = model.client else { return }
        do {
            data = try await client.botViews(assets)
            error = nil
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            if data == nil { self.error = error.localizedDescription.isEmpty ? "indisponible" : error.localizedDescription }
        }
    }
}

private struct BotViewRow: View {
    let view: BotView

    var body: some View {
        let v = view
        VStack(alignment: .leading, spacing: 5) {
            WrapLayout(spacing: 6) {
                if let symbol = v.symbol, let kind = v.kind {
                    NavigationLink {
                        AssetDetailView(asset: Asset(symbol: symbol, kind: kind, name: symbol))
                    } label: {
                        Text(symbol).font(.subheadline.bold()).foregroundStyle(.white)
                    }
                    .buttonStyle(.plain)
                }
                BotActionChip(action: v.action)
                TagChip(text: v.countsLabel, color: v.counts ? .white : Theme.textSecondary)
            }
            Text(ModelBot.viewText(v)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// « Bot Altim » in the decision card: the learned model's action, its probabilities and whether it counts in this
/// decision, with the link to the bot's screen (same texts as the web's `BotLine`). Everything wraps: nothing scrolls
/// sideways. The edge's colour repeats what the text says, never carries the meaning alone.
struct BotBlockView: View {
    let bot: BotView

    private var edgeColor: Color {
        switch bot.tone {
        case .na: return Theme.textSecondary
        case .unproven: return Theme.warning
        case .edge: return Theme.buy
        }
    }

    var body: some View {
        let b = bot
        VStack(alignment: .leading, spacing: 4) {
            WrapLayout(spacing: 8) {
                Text("Bot Altim").font(.footnote.weight(.semibold)).foregroundStyle(.white)
                if b.hasAction {
                    BotActionChip(action: b.action)
                    TagChip(text: b.countsLabel, color: .white.opacity(0.9))
                }
            }
            if b.hasAction {
                (Text(ModelBot.probabilitiesText(b)).foregroundStyle(.white.opacity(0.9))
                    + Text(ModelBot.modelSuffix(b) ?? "").foregroundStyle(Theme.textSecondary)
                    + Text(ModelBot.outOfBasketText(b) ?? "").foregroundStyle(Theme.textSecondary))
                    .font(.caption).fixedSize(horizontal: false, vertical: true)
            } else {
                Text(b.text).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
            if let c = ModelBot.contributionsText(b) {
                Text(c).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            if !b.note.isEmpty {
                Text(b.note).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
            NavigationLink {
                BotScreen()
            } label: {
                (Text(ModelBot.linkText).foregroundStyle(Theme.cyan)
                    + Text(ModelBot.trainedText(b) ?? "").foregroundStyle(Theme.textSecondary))
                    .font(.caption)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, 8)
        .padding(.leading, 13)
        .padding(.trailing, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.03))
        .overlay(alignment: .leading) { Rectangle().fill(edgeColor).frame(width: 3) }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.white.opacity(0.12), lineWidth: 1))
    }
}
