import SwiftUI
import AltimKit

/// News tab: every feed in one place, stories told by several sources merged, the user's assets first.
struct NewsView: View {
    @Environment(AppModel.self) private var model
    @State private var report: NewsReport?
    @State private var error: String?
    @AppStorage("news.filter") private var filter = "all"
    @AppStorage("news.frenchOnly") private var frenchOnly = false
    /// "articles" or "agenda", remembered like the web's Actu sub-tab.
    @AppStorage("news.view") private var view = "articles"
    /// Asset opened from the summary's chips.
    @State private var opened: Asset?

    private static let filters = [("all", "Tout"), ("actifs", "Mes actifs"), ("monde", "Monde"), ("marches", "Marchés"), ("crypto", "Crypto")]

    private var assets: [Asset] {
        let all = model.watchlist + model.holdings.map(\.asset)
        var seen = Set<String>()
        return Array(all.filter { seen.insert($0.id).inserted }.prefix(20))
    }

    private var shown: [NewsItem] {
        (report?.items ?? []).filter { (filter == "all" || $0.category == filter) && (!frenchOnly || $0.lang == "fr") }
    }

    var body: some View {
        Group {
            if view == "agenda" {
                AgendaView(view: $view)
            } else {
                articles
            }
        }
        .altimScreen()
        .navigationTitle("Actualités")
        .navigationDestination(item: $opened) { AssetDetailView(asset: $0) }
    }

    private var articles: some View {
        List {
            Section { NewsViewPicker(view: $view) }.listRowBackground(Color.clear)
            Section {
                Text(report.map { r in "\(r.items.count) articles de \(r.sources.filter(\.ok).count) sources sur 48 h, mis à jour \(NewsItem.ago(r.asOf))." } ?? "Chargement des sources…")
                    .font(.footnote).foregroundStyle(Theme.textSecondary)
                if let error { Text(error).foregroundStyle(Theme.sell) }
            }
            .listRowBackground(Color.clear)

            if let summary = report?.summary {
                Section { NewsSummaryCard(list: summary) { opened = $0 } }.listRowBackground(Color.clear)
            }

            if let report, filter == "all" {
                let top = report.topItems
                if !top.isEmpty {
                    Section {
                        ForEach(top) { StoryRow(item: $0, featured: true) }
                    } header: {
                        Text("À la une")
                    } footer: {
                        Text("Les sujets repris par plusieurs sources, et toute escalade grave (guerre, panique bancaire…).")
                    }
                    .listRowBackground(Theme.surface.opacity(0.6))
                }
                if report.digest.total > 0 {
                    Section("Ce qui domine (24 h)") { DigestView(digest: report.digest) }
                        .listRowBackground(Theme.surface.opacity(0.6))
                }
            }

            Section {
                Picker("Rubrique", selection: $filter) {
                    ForEach(Self.filters, id: \.0) { Text($0.1).tag($0.0) }
                }
                .pickerStyle(.menu)
                Toggle("Articles en français seulement", isOn: $frenchOnly).tint(Theme.cyan)
            }
            .listRowBackground(Theme.surface.opacity(0.6))

            Section {
                if report != nil && shown.isEmpty {
                    Text("Aucun article dans cette rubrique pour le moment.").font(.footnote).foregroundStyle(Theme.textSecondary)
                }
                ForEach(shown) { StoryRow(item: $0, featured: false) }
            }
            .listRowBackground(Theme.surface.opacity(0.6))

            if let report {
                Section {
                    DisclosureGroup("Sources · \(report.sources.filter(\.ok).count)/\(report.sources.count) en ligne") {
                        ForEach(report.sources) { s in
                            Text(s.ok ? "✔ \(s.name) · \(s.count)" : "✕ \(s.name) · \(s.error ?? "indisponible")")
                                .font(.caption).foregroundStyle(s.ok ? .white : Theme.textSecondary)
                        }
                    }
                } footer: {
                    Text("Les titres sont affichés tels que publiés (non traduits) ; les liens ouvrent l'article chez sa source.")
                }
                .listRowBackground(Theme.surface.opacity(0.6))
            }
        }
        .listStyle(.insetGrouped)
        .overlay { if report == nil && error == nil { ProgressView("Lecture d'une vingtaine de sources…") } }
        .refreshable { await load() }
        .task(id: assets.map(\.id).joined(separator: ",")) {
            // Refreshed every 5 minutes while the tab is on screen.
            while !Task.isCancelled {
                await load()
                try? await Task.sleep(for: .seconds(300))
            }
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        do {
            report = try await client.news(assets)
            error = nil
            model.persistSession()
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// "Articles / Agenda" at the top of the Actu tab.
struct NewsViewPicker: View {
    @Binding var view: String

    var body: some View {
        FittingPicker(title: "Vue", selection: $view) {
            Text("Articles").tag("articles")
            Text("Agenda").tag("agenda")
        }
    }
}

private struct StoryRow: View {
    var item: NewsItem
    var featured: Bool
    @Environment(\.openURL) private var openURL

    var body: some View {
        Button {
            if let url = item.url { openURL(url) }
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                Text(item.title).font(featured ? .subheadline.weight(.semibold) : .footnote.weight(.semibold)).foregroundStyle(.white)
                if featured, let summary = item.summary {
                    Text(summary).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(3)
                }
                meta
                if item.alert || !item.assets.isEmpty || !item.themes.isEmpty { tags }
            }
            .padding(.vertical, 2)
        }
        .accessibilityHint("Ouvre l'article chez \(item.source)")
    }

    private var meta: some View {
        HStack(spacing: 8) {
            Text(item.source).foregroundStyle(.white.opacity(0.85))
            Text(item.age()).foregroundStyle(Theme.textSecondary)
            if !item.alsoIn.isEmpty { Text("+\(item.alsoIn.count) source\(item.alsoIn.count > 1 ? "s" : "")").foregroundStyle(Theme.textSecondary) }
            if item.lang == "en" { Text("EN").foregroundStyle(Theme.textSecondary) }
            if item.tone == "negative" { Text("▼ négatif").foregroundStyle(Theme.sell) }
            if item.tone == "positive" { Text("▲ positif").foregroundStyle(Theme.buy) }
        }
        .font(.caption2)
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }

    private var tags: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 6) { tagViews }
            VStack(alignment: .leading, spacing: 4) { tagViews }
        }
    }

    @ViewBuilder private var tagViews: some View {
        if item.alert { Badge(text: "ALERTE", tone: .bad) }
        ForEach(item.assets, id: \.self) { Chip(text: String($0.split(separator: ":").last ?? ""), color: .white) }
        ForEach(item.themes, id: \.self) { Chip(text: NewsItem.themeLabels[$0] ?? $0, color: Theme.textSecondary) }
    }
}

private struct Chip: View {
    var text: String
    var color: Color

    var body: some View {
        Text(text).font(.caption2).foregroundStyle(color)
            .padding(.horizontal, 8).padding(.vertical, 2)
            .background(Color.white.opacity(0.08), in: Capsule())
    }
}

private struct DigestView: View {
    var digest: NewsReport.Digest

    private var label: String {
        "Ton des titres : \(digest.tone.negative) négatifs, \(digest.tone.neutral) neutres, \(digest.tone.positive) positifs"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 6) { themes }
                VStack(alignment: .leading, spacing: 4) { themes }
            }
            GeometryReader { geo in
                let t = digest.tone
                let total = max(1, t.negative + t.neutral + t.positive)
                HStack(spacing: 0) {
                    Theme.sell.frame(width: geo.size.width * CGFloat(t.negative) / CGFloat(total))
                    Theme.textSecondary.opacity(0.5).frame(width: geo.size.width * CGFloat(t.neutral) / CGFloat(total))
                    Theme.buy.frame(width: geo.size.width * CGFloat(t.positive) / CGFloat(total))
                }
            }
            .frame(height: 8)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .accessibilityElement()
            .accessibilityLabel(label)
            Text("\(label) (repérage par mots-clés, indicatif).").font(.caption).foregroundStyle(Theme.textSecondary)
        }
    }

    @ViewBuilder private var themes: some View {
        ForEach(digest.themes) { Chip(text: "\($0.label) · \($0.count)", color: .white) }
    }
}
