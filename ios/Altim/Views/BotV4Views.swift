import SwiftUI
import AltimKit

/// « Bots sélectifs » (v4) on the Bot screen (same content and texts as the Yew front's `app/bot/v4.rs`): each bot's
/// avis today (or « pas d'avis »), its measured precision with the Wilson interval, its signals per year and its status
/// (prouvé, en attente, contredit, non prouvé). Stacked cards and rows, chips that wrap: nothing is wider than the
/// screen.
struct BotV4Section: View {
    let v4: BotV4Report

    var body: some View {
        let req = v4.tRequired
        VStack(alignment: .leading, spacing: 16) {
            Text(ModelBotV4.sectionTitle).font(.headline).foregroundStyle(.white).padding(.top, 4)
                .accessibilityAddTraits(.isHeader)
            Card(title: ModelBotV4.title(v4)) {
                Text(v4.headline).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                WrapLayout(spacing: 8) {
                    BotV4Tile(label: "Seuil corrigé", value: "t ≥ \(ModelValidation.plain(req, digits: 2))")
                    BotV4Tile(label: "Tests comptés (v1 à v4)", value: "\(v4.k?.total ?? 0)")
                    BotV4Tile(label: "Bots précis sur le passé", value: ModelBotV4.preciseCount(v4))
                }
                BotV4Caption(text: ModelBotV4.intro)
                if !v4.afterPrereg.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Modifié après le pré-enregistrement :").font(.footnote.weight(.semibold)).foregroundStyle(Theme.warning)
                        ForEach(Array(v4.afterPrereg.enumerated()), id: \.offset) { _, l in BotV4Caption(text: "• \(l)") }
                    }
                }
            }
            ForEach(Array(ModelBotV4.cards(v4).enumerated()), id: \.offset) { _, card in
                Card(title: card.title) {
                    ForEach(card.bots) { b in BotV4Row(bot: b, required: req) }
                }
            }
            Card(title: ModelBotV4.forwardTitle) {
                Text(v4.forwardHeadline).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                BotV4Caption(text: "Un bot précis sur le passé ne compte dans les décisions qu'après \(v4.minSignals) signaux sur l'avenir qui le confirment.")
            }
            Card(title: ModelBotV4.methodTitle) {
                ForEach(Array(v4.method.enumerated()), id: \.offset) { _, l in
                    Text("• \(l)").font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
                DisclosureGroup {
                    ForEach(Array(v4.limits.enumerated()), id: \.offset) { _, l in BotV4Caption(text: "• \(l)") }
                } label: {
                    Text("Limites").font(.footnote).foregroundStyle(Theme.cyan)
                }
                .tint(Theme.cyan)
            }
        }
    }
}

/// One bot: its status, today's avis, precision ± interval, reference, signals per year, t vs the required one.
private struct BotV4Row: View {
    let bot: BotV4Bot
    let required: Double

    var body: some View {
        let s = bot.main
        VStack(alignment: .leading, spacing: 4) {
            Divider().overlay(Color.white.opacity(0.08))
            WrapLayout(spacing: 6) {
                Text(bot.side.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                BotV4StatusChip(status: bot.status)
            }
            BotV4Caption(text: "Avis du jour : \(ModelBotV4.todayText(bot))")
            BotV4Kv(key: "Précision mesurée", value: ModelBotV4.precisionShort(s))
            BotV4Kv(key: "Hasard (référence)", value: ModelBotV4.pct(s.reference))
            BotV4Kv(key: "Signaux", value: ModelBotV4.rhythm(s))
            BotV4Kv(key: "t par jour", value: ModelBotV4.tText(s, required))
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 4) {
                    BotV4Caption(text: "Réussite : \(bot.hit).")
                    BotV4Caption(text: "\(bot.models).")
                    BotV4Caption(text: bot.text)
                }
            } label: {
                Text("Détails").font(.caption).foregroundStyle(Theme.cyan)
            }
            .tint(Theme.cyan)
        }
    }
}

struct BotV4StatusChip: View {
    let status: BotV4Status

    var body: some View {
        let color = status == .proven ? Theme.buy : status == .contradicted ? Theme.warning : Theme.cyan
        Text(status.label).font(.caption.weight(.semibold)).foregroundStyle(color)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().fill(color.opacity(0.12)))
            .overlay(Capsule().strokeBorder(color.opacity(0.5), lineWidth: 1))
    }
}

private struct BotV4Tile: View {
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

private struct BotV4Caption: View {
    let text: String
    var body: some View {
        Text(text).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }
}

private struct BotV4Kv: View {
    let key: String
    let value: String
    var body: some View {
        WrapLayout(spacing: 6) {
            Text(key).font(.caption).foregroundStyle(Theme.textSecondary)
            Text(value).font(.caption.weight(.semibold)).foregroundStyle(.white)
        }
        .accessibilityElement(children: .combine)
    }
}

/// "Bots sélectifs : Hausse 20 j, …" of an asset of the basket.
struct BotV4AssetNowView: View {
    let now: BotV4AssetNow

    var body: some View {
        Text("Bots sélectifs : \(now.bots.isEmpty ? "pas d'avis" : now.bots.map(ModelBotV4.shortLabel).joined(separator: ", "))")
            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }
}

/// The group's selective bots today in a view (decision line, watched asset): those speaking, and whether they count.
struct BotV4AvisView: View {
    let view: BotV4View

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if view.available {
                Text("Bots sélectifs : \(ModelBotV4.viewHead(view))").font(.caption).foregroundStyle(.white.opacity(0.9))
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(Array(view.speaking.enumerated()), id: \.offset) { _, s in
                    WrapLayout(spacing: 6) {
                        Text(s.label).font(.caption.weight(.semibold)).foregroundStyle(.white)
                        BotV4StatusChip(status: s.status)
                    }
                    BotV4Caption(text: ModelBotV4.signalText(s))
                }
            }
            BotV4Caption(text: view.note)
        }
    }
}
