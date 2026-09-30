import SwiftUI
import AltimKit

/// « Bot Altim » v3 on the Bot screen (same content and texts as the web's BotV3.tsx): what changed, the
/// pre-registration date, the corrected threshold (K tests), the 4 headline results (2 families × horizons 20 / 60)
/// with raw and required t, the forward test, each configuration alone for information, the volatility-managed trend
/// vs holding, and the time / memory of the computation. Stacked cards and rows, chips that wrap: nothing is wider
/// than the screen.
struct BotV3Section: View {
    let v3: BotV3Report
    let timing: BotTiming?

    var body: some View {
        let req = v3.tRequired
        VStack(alignment: .leading, spacing: 16) {
            Card(title: ModelBot.v3Title(v3)) {
                WrapLayout(spacing: 8) {
                    BotV3Tile(label: "Seuil corrigé", value: "t ≥ \(ModelValidation.plain(req, digits: 2))")
                    BotV3Tile(label: "Tests comptés (v1 à v3)", value: "\(v3.k.total)")
                    BotV3Tile(label: "Test sur l'avenir", value: ModelBot.signalsCount(ModelBot.forwardSignals(v3)))
                }
                BotV3Caption(text: ModelBot.v3ProtocolText(v3))
                if !v3.afterPrereg.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(ModelBot.afterPreregTitle).font(.footnote.weight(.semibold)).foregroundStyle(Theme.warning)
                            .fixedSize(horizontal: false, vertical: true)
                        BotV3Bullets(lines: v3.afterPrereg)
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.warning.opacity(0.1)))
                }
                BotV3Disclosure(title: ModelBot.v3ChangesTitle) { BotV3Bullets(lines: v3.changes) }
            }

            BotV3SectionLabel(text: ModelBot.v3ResultsTitle)
            BotV3Caption(text: ModelBot.v3ResultsIntro(v3))
            ForEach(v3.groups) { g in BotV3GroupCard(group: g, required: req) }

            BotV3SectionLabel(text: ModelBot.forwardTitle(v3))
            Card {
                Text(v3.forwardHeadline).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                BotV3Caption(text: ModelBot.forwardIntro(v3))
                ForEach(v3.groups) { g in
                    ForEach(Array(ModelBot.headlineConfigs(g).enumerated()), id: \.offset) { _, h in
                        VStack(alignment: .leading, spacing: 3) {
                            Divider().overlay(Color.white.opacity(0.08))
                            (Text(g.label).bold().foregroundStyle(.white)
                                + Text(" · \(h.config.family.label) · \(h.horizon) jours").foregroundStyle(.white.opacity(0.9)))
                                .font(.footnote).fixedSize(horizontal: false, vertical: true)
                            BotV3Caption(text: ModelBot.forwardText(h.config.forward, req))
                        }
                    }
                }
            }

            BotV3SectionLabel(text: ModelBot.configsTitle)
            BotV3Caption(text: ModelBot.configsIntro)
            ForEach(v3.groups) { g in
                ForEach(g.horizons) { h in
                    BotV3ConfigsCard(title: "\(g.label) · \(h.horizon) jours", configs: h.configs, required: req)
                }
            }

            if v3.groups.contains(where: { $0.volManaged != nil }) {
                BotV3SectionLabel(text: ModelBot.volTitle)
                BotV3Caption(text: ModelBot.volIntro)
                ForEach(v3.groups) { g in
                    if let v = g.volManaged { BotV3VolCard(label: g.label, vol: v) }
                }
            }

            Card(title: ModelBot.v3MethodTitle) {
                BotV3Bullets(lines: v3.method)
                BotV3Disclosure(title: ModelBot.candidatesCount(v3)) {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(v3.candidates) { c in
                            (Text("• ") + Text(c.label).bold() + Text(" (\(c.family.label)) — \(c.description)"))
                                .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                BotV3Disclosure(title: ModelBot.v3LimitsTitle) { BotV3Bullets(lines: v3.limits) }
                if let line = ModelBot.v3ComputeLine(v3, timing) { BotV3Caption(text: line) }
            }
        }
    }
}

/// A group (stocks or cryptos): each horizon's two headline configurations.
private struct BotV3GroupCard: View {
    let group: BotV3Group
    let required: Double

    var body: some View {
        let g = group
        Card(title: g.label) {
            BotV3Caption(text: ModelBot.v3GroupSubtitle(g))
            ForEach(g.horizons) { h in
                VStack(alignment: .leading, spacing: 8) {
                    Divider().overlay(Color.white.opacity(0.08))
                    (Text(ModelBot.horizonTitle(h)).bold().foregroundStyle(.white)
                        + Text(" · \(ModelBot.horizonSubtitle(h))").foregroundStyle(Theme.textSecondary))
                        .font(.footnote).fixedSize(horizontal: false, vertical: true)
                    ForEach(h.configs.filter(\.headline)) { c in
                        BotV3HeadlineConfig(config: c, required: required,
                                            runs: ModelBot.choiceRuns(h.selection, family: c.family == .peers ? .peers : .absolute))
                    }
                }
            }
        }
    }
}

/// One headline configuration: both sides with their verdict chips, family B's control, the « fragile » note, the
/// model chosen at each retraining.
private struct BotV3HeadlineConfig: View {
    let config: BotV3Config
    let required: Double
    let runs: [ModelBot.ChoiceRun]

    var body: some View {
        let c = config
        let m = c.main
        VStack(alignment: .leading, spacing: 6) {
            Text(c.family.label).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            ForEach(BotSide.allCases, id: \.self) { side in
                Text(ModelBot.v3SideText(c.family, side, m[side], required)).font(.footnote).foregroundStyle(.white.opacity(0.9))
                    .fixedSize(horizontal: false, vertical: true)
                WrapLayout(spacing: 6) {
                    Text(ModelBot.sideHead(m, side)).font(.caption).foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                    BotV3VerdictChip(stats: m[side], required: required)
                }
                if let control = m.vsMean(side) {
                    VStack(alignment: .leading, spacing: 4) {
                        BotV3Caption(text: ModelBot.controlText(control, required))
                        BotV3VerdictChip(stats: control, required: required)
                        Text(ModelBot.controlVerdict(ModelBot.proven(m, side))).font(.caption.weight(.semibold)).foregroundStyle(.white)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.leading, 10)
                    .overlay(alignment: .leading) { Rectangle().fill(Color.white.opacity(0.15)).frame(width: 2) }
                }
            }
            ForEach(BotSide.allCases, id: \.self) { side in
                if let p = ModelBot.pendingNote(c, side, required) { Notice(text: p, tone: .warn) }
            }
            if m.holdAssets > 0 {
                DecisionRow(key: "Achats cumulés / détention (médianes)", value: ModelBot.holdValue(m))
            }
            BotV3Disclosure(title: "Modèle retenu à chaque réentraînement") { BotV3Caption(text: ModelBot.choiceRunsText(runs)) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Every configuration of a group and horizon (headline included), compact stacked rows.
private struct BotV3ConfigsCard: View {
    let title: String
    let configs: [BotV3Config]
    let required: Double

    var body: some View {
        Card(title: title) {
            ForEach(Array(configs.enumerated()), id: \.offset) { i, c in
                if i > 0 { Divider().overlay(Color.white.opacity(0.08)) }
                VStack(alignment: .leading, spacing: 5) {
                    WrapLayout(spacing: 6) {
                        Text(c.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                            .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                        if c.headline { TagChip(text: "principal", color: .white) }
                        if let chosen = ModelBot.v3ChosenText(c) { TagChip(text: chosen) }
                    }
                    ForEach(Array(ModelBot.configRows(c, required).enumerated()), id: \.offset) { _, row in
                        DecisionRow(key: row.label, value: row.value)
                    }
                    WrapLayout(spacing: 6) {
                        BotV3VerdictChip(stats: c.main.buy, required: required)
                        BotV3VerdictChip(stats: c.main.sell, required: required)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

/// The volatility-managed trend vs holding: one stacked row per measure.
private struct BotV3VolCard: View {
    let label: String
    let vol: BotV3VolManaged

    var body: some View {
        Card(title: label) {
            BotV3Caption(text: ModelBot.volSubtitle(vol))
            ForEach(ModelBot.volRows(vol), id: \.label) { r in
                VStack(alignment: .leading, spacing: 2) {
                    Text(r.label).font(.footnote).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                    (Text("gérée ").foregroundStyle(.white.opacity(0.9)) + Text(r.managed).bold().foregroundStyle(.white)
                        + Text(" · détention \(r.hold)").foregroundStyle(Theme.textSecondary))
                        .font(.footnote.monospacedDigit()).fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
            }
            Text(vol.text).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Verdict at the corrected threshold, written out; the colour only repeats it.
private struct BotV3VerdictChip: View {
    let stats: BotV3SideStats
    let required: Double

    var body: some View {
        let tone = stats.verdict.map(ModelValidation.verdictTone) ?? .neutral
        let color = tone == .neutral ? Theme.cyan : Theme.color(tone)
        Text(ModelBot.v3VerdictLabel(stats, required)).font(.caption.weight(.semibold)).foregroundStyle(color)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().fill(color.opacity(0.12)))
            .overlay(Capsule().strokeBorder(color.opacity(0.5), lineWidth: 1))
    }
}

private struct BotV3Tile: View {
    let label: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            Text(value).font(.subheadline.monospacedDigit().weight(.bold)).foregroundStyle(.white)
        }
        .padding(.horizontal, 10).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.white.opacity(0.06)))
        .accessibilityElement(children: .combine)
    }
}

private struct BotV3SectionLabel: View {
    let text: String
    var body: some View {
        Text(text).font(.headline).foregroundStyle(.white).padding(.top, 4)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityAddTraits(.isHeader)
    }
}

private struct BotV3Caption: View {
    let text: String
    var body: some View {
        Text(text).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }
}

private struct BotV3Bullets: View {
    let lines: [String]
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, l in
                Text("• \(l)").font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

private struct BotV3Disclosure<Content: View>: View {
    let title: String
    @ViewBuilder var content: Content

    var body: some View {
        DisclosureGroup {
            content.padding(.top, 4)
        } label: {
            Text(title).font(.footnote).foregroundStyle(Theme.cyan)
                .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
        }
        .tint(Theme.cyan)
    }
}

/// Today's actions of the 4 headline configurations of an asset row: "v3 : hausse/baisse 20 j [ATTENDRE] 60 j […] ·
/// entre pairs 20 j […] 60 j […]", wrapping.
struct BotV3AssetNowView: View {
    let now: BotV3AssetNow

    var body: some View {
        WrapLayout(spacing: 6) {
            label("v3 : hausse/baisse 20 j")
            BotActionChip(action: now.absolute20)
            label("60 j")
            BotActionChip(action: now.absolute60)
            label("· entre pairs 20 j")
            BotActionChip(action: now.peers20)
            label("60 j")
            BotActionChip(action: now.peers60)
        }
    }

    private func label(_ s: String) -> some View { Text(s).font(.caption).foregroundStyle(Theme.textSecondary) }
}

/// A view's v3 signals: "v3 : hausse/baisse 20 j [ATTENDRE] entre pairs 20 j [ACHETER] …", wrapping.
struct BotV3SignalsView: View {
    let view: BotV3View

    var body: some View {
        WrapLayout(spacing: 6) {
            Text("v3 :").font(.caption).foregroundStyle(Theme.textSecondary)
            ForEach(Array(view.signals.enumerated()), id: \.offset) { _, s in
                Text(ModelBot.v3SignalLabel(s)).font(.caption).foregroundStyle(Theme.textSecondary)
                BotActionChip(action: s.action)
            }
        }
    }
}

/// « Bot Altim » on the Radar (same as the web's BotRadarCard): the report's headline (first sentence, read from
/// /api/bot, never written here), the forward test's counter and a link to the Bot screen. Loading, pending (first
/// training, asked again every 15 seconds) and error states; one compact card.
struct BotRadarCard: View {
    @Environment(AppModel.self) private var model
    @State private var report: BotReport?
    @State private var state: LoadState = .loading

    enum LoadState { case loading, pending, error, ready }

    var body: some View {
        Card(title: ModelBot.radarTitle) {
            switch state {
            case .loading: caption(ModelBot.radarLoading)
            case .pending: caption(ModelBot.radarPending)
            case .error: caption(ModelBot.radarError)
            case .ready:
                if let report {
                    Text(ModelBot.radarHeadline(report)).font(.footnote).foregroundStyle(.white.opacity(0.9))
                        .fixedSize(horizontal: false, vertical: true)
                    if let fwd = ModelBot.radarForward(report) {
                        Text(fwd).font(.footnote.weight(.bold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            NavigationLink {
                BotScreen()
            } label: {
                Text(ModelBot.radarLink).font(.caption.weight(.semibold)).foregroundStyle(Theme.cyan)
                    .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
            }
            .buttonStyle(.borderless)
        }
        .task { await load() }
    }

    private func caption(_ s: String) -> some View {
        Text(s).font(.footnote).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    private func load() async {
        while !Task.isCancelled {
            guard let client = model.client else { return }
            do {
                switch try await client.bot() {
                case .pending:
                    if report == nil { state = .pending }
                    try? await Task.sleep(for: .seconds(15))
                    continue
                case let .ready(r):
                    report = r
                    state = .ready
                }
            } catch AltimError.unauthorized {
                model.sessionLost()
            } catch is CancellationError {
            } catch {
                if report == nil { state = .error }
            }
            return
        }
    }
}
