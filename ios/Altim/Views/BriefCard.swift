import SwiftUI
import AltimKit

/// "Point du jour": the day in a few lines for the radar and the holdings, refreshed every 5 minutes.
struct BriefCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var brief: Brief?

    private var assets: [Asset] {
        var seen = Set<String>()
        return Array((model.watchlist + model.holdings.map(\.asset)).filter { seen.insert($0.id).inserted }.prefix(20))
    }

    var body: some View {
        Group {
            if let b = brief {
                Card(title: "Point du jour") {
                    Text(b.headline).font(.subheadline.weight(.semibold))
                    if let themes = b.market?.themes, !themes.isEmpty {
                        Text("Sujets du moment : \(themes.joined(separator: ", ").lowercased()).").font(.caption).foregroundStyle(Theme.textSecondary)
                    }
                    let moves = b.movers.filter { abs($0.change) >= 0.05 }.prefix(4)
                    if !moves.isEmpty {
                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: 6) { chips(Array(moves)) }
                            VStack(alignment: .leading, spacing: 6) { chips(Array(moves)) }
                        }
                    }
                    ForEach(b.news) { n in
                        Button {
                            if let url = n.url { openURL(url) }
                        } label: {
                            Text(line(n))
                                .font(.footnote).multilineTextAlignment(.leading)
                                .foregroundStyle(n.alert ? Theme.sell : .white.opacity(0.9))
                        }
                        .buttonStyle(.borderless)
                        .accessibilityHint("Ouvre l'article chez \(n.source)")
                    }
                    Text("Achetable = signal 4 h à l'achat ou prix dans une zone d'achat Fibonacci, sans blocage (la règle des notifications). Variations depuis la dernière clôture journalière. Conseil indicatif : Altim ne passe aucun ordre.")
                        .font(.caption).foregroundStyle(Theme.textSecondary)
                }
            }
        }
        .task(id: assets.map(\.id).joined(separator: ",")) {
            while !Task.isCancelled {
                guard let client = model.client else { return }
                do {
                    brief = try await client.brief(assets)
                } catch AltimError.unauthorized {
                    model.sessionLost()
                    return
                } catch {
                    // The rest of the radar still shows; the brief comes back at the next attempt.
                }
                try? await Task.sleep(for: .seconds(300))
            }
        }
    }

    private func line(_ n: NewsItem) -> String {
        let prefix = n.alert ? "ALERTE · " : ""
        let others = n.alsoIn.isEmpty ? "" : " +\(n.alsoIn.count)"
        return prefix + n.title + " · " + n.source + others
    }

    @ViewBuilder private func chips(_ moves: [Brief.Mover]) -> some View {
        ForEach(moves) { m in
            NavigationLink(value: m.asset) {
                Text("\(m.symbol) \(m.change >= 0 ? "+" : "−")\(abs(m.change).formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "fr_FR")))) %")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(m.change >= 0 ? Theme.buy : Theme.sell)
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .background(Color.white.opacity(0.08), in: Capsule())
            }
            // Several links in one list row: each one opens on its own.
            .buttonStyle(.borderless)
        }
    }
}
