import SwiftUI
import AltimKit

/// "N événements importants aujourd'hui" at the top of Actu (the `summary` of /api/news): impact potentiel (rule or
/// measured), assets concerned, consensus of the sources, moves since publication, reading against the technical trend,
/// and every source's headline. Same texts as the web (News.tsx `Summary`). Stacked rows, chips wrap: nothing scrolls
/// sideways.
struct NewsSummaryCard: View {
    let list: [StorySummary]
    /// Opens an asset's page (the chips of the assets concerned).
    var open: (Asset) -> Void

    var body: some View {
        let head = NewsSummary.heading(list)
        Card(title: head.title) {
            if let others = head.others {
                Text(others).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
            Text(NewsSummary.intro).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            ForEach(list) { s in
                SummaryEventView(story: s, open: open)
                Divider().opacity(0.3)
            }
            DisclosureGroup("Comment l'impact est estimé") {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach([NewsSummary.howRule, NewsSummary.howMeasured, NewsSummary.howConsensus], id: \.self) { t in
                        Text(t).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(.top, 4)
            }
            .font(.footnote)
            .tint(.white)
        }
    }
}

private struct SummaryEventView: View {
    let story: StorySummary
    var open: (Asset) -> Void
    @Environment(\.openURL) private var openURL

    private var impactColor: Color {
        switch story.impact {
        case .high: return Theme.sell
        case .medium: return Theme.warning
        case .low, .unknown: return Theme.textSecondary
        }
    }

    var body: some View {
        let s = story
        VStack(alignment: .leading, spacing: 6) {
            Button {
                if let url = s.url { openURL(url) }
            } label: {
                HStack(alignment: .top, spacing: 6) {
                    if s.alert { Badge(text: "ALERTE", tone: .bad) }
                    Text(s.title).font(.footnote.weight(.semibold)).foregroundStyle(.white).multilineTextAlignment(.leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .buttonStyle(.borderless)
            .accessibilityHint("Ouvre l'article chez \(s.source)")
            WrapLayout(spacing: 4) {
                TagChip(text: "Impact potentiel \(s.impact.label)", color: impactColor)
                TagChip(text: NewsSummary.basisLabel(s))
            }
            if !s.assets.isEmpty {
                WrapLayout(spacing: 4) {
                    Text("Actifs concernés").font(.caption).foregroundStyle(Theme.textSecondary).padding(.vertical, 4)
                    ForEach(s.assets, id: \.self) { id in
                        let a = NewsSummary.assetLink(id)
                        if let asset = a.asset {
                            Button { open(asset) } label: { TagChip(text: a.symbol, color: Theme.cyan) }
                                .buttonStyle(.borderless)
                                .accessibilityHint("Ouvre la fiche de \(a.symbol)")
                        } else {
                            TagChip(text: a.symbol, color: .white)
                        }
                    }
                }
            }
            (Text("Sources ").foregroundStyle(Theme.textSecondary) + Text(NewsSummary.consensusText(s)).foregroundStyle(.white.opacity(0.9)))
                .font(.caption).fixedSize(horizontal: false, vertical: true)
            ForEach(s.moves, id: \.asset) { m in
                (Text(NewsSummary.moveText(m)).foregroundStyle(Theme.color(forChange: m.changePct))
                    + Text(" (clôtures horaires, pas forcément dues à cette actualité)").foregroundStyle(Theme.textSecondary))
                    .font(.caption).fixedSize(horizontal: false, vertical: true)
            }
            ForEach(s.technical, id: \.asset) { t in
                (Text("Impact sur le signal technique\(s.technical.count > 1 ? " (\(NewsSummary.assetLink(t.asset).symbol))" : "") : ").foregroundStyle(Theme.textSecondary)
                    + Text(t.text).foregroundStyle(.white.opacity(0.9)))
                    .font(.caption).fixedSize(horizontal: false, vertical: true)
            }
            DisclosureGroup("Sources et calcul") {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(s.links.enumerated()), id: \.offset) { item in
                        let l = item.element
                        Button {
                            if let url = l.url { openURL(url) }
                        } label: {
                            (Text(l.tone == "negative" ? "▼ " : l.tone == "positive" ? "▲ " : "· ")
                                .foregroundStyle(l.tone == "negative" ? Theme.sell : l.tone == "positive" ? Theme.buy : Theme.textSecondary)
                             + Text(l.source).foregroundStyle(Theme.cyan)
                             + Text(" · \(l.title)").foregroundStyle(Theme.textSecondary))
                                .font(.caption).multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                        }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("\(l.source), ton \(l.tone == "negative" ? "négatif" : l.tone == "positive" ? "positif" : "neutre") : \(l.title)")
                    }
                    Text(NewsSummary.ruleText(s)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                .padding(.top, 4)
            }
            .font(.caption)
            .tint(.white)
        }
        .padding(.vertical, 2)
        .overlay(alignment: .leading) {
            if s.alert { Rectangle().fill(Theme.sell.opacity(0.5)).frame(width: 2).offset(x: -8) }
        }
    }
}
