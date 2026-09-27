import SwiftUI
import AltimCore

/// Soldes des courtiers connectés et journal des ordres.
struct PortfolioView: View {
    @Environment(AppSettings.self) private var settings
    @Environment(AppServices.self) private var services
    @State private var sections: [(title: String, balances: [Balance], error: String?)] = []
    @State private var loading = false

    var body: some View {
        NavigationStack {
            ZStack {
                CyberGridBackground()
                ScrollView {
                    VStack(spacing: 16) {
                        HStack {
                            SectionTitle(text: "Soldes")
                            if loading { ProgressView().tint(Theme.cyan) }
                        }
                        ForEach(sections.indices, id: \.self) { i in
                            let section = sections[i]
                            VStack(alignment: .leading, spacing: 10) {
                                Text(section.title).font(.headline)
                                if let error = section.error {
                                    Text(error).font(.caption).foregroundStyle(Theme.warning)
                                } else if section.balances.isEmpty {
                                    Text("Aucun actif.").font(.caption).foregroundStyle(Theme.textSecondary)
                                }
                                ForEach(section.balances) { b in
                                    HStack {
                                        Text(b.asset).font(Theme.mono(14))
                                        Spacer()
                                        VStack(alignment: .trailing) {
                                            Text(Format.quantity(b.total)).font(Theme.mono(14))
                                            if b.locked > 0 {
                                                Text("dont \(Format.quantity(b.locked)) en ordres")
                                                    .font(.caption2).foregroundStyle(Theme.textSecondary)
                                            }
                                        }
                                    }
                                }
                            }
                            .glassCard()
                        }

                        SectionTitle(text: "Journal des ordres")
                        if services.journal.entries.isEmpty {
                            Text("Aucun ordre pour l'instant.").font(.caption).foregroundStyle(Theme.textSecondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        ForEach(services.journal.entries.prefix(50)) { e in
                            HStack {
                                Text(e.side.label.uppercased())
                                    .font(Theme.mono(11, weight: .heavy))
                                    .foregroundStyle(e.side == .buy ? Theme.buy : Theme.sell)
                                    .frame(width: 56, alignment: .leading)
                                VStack(alignment: .leading) {
                                    Text(e.symbol).font(.subheadline.bold())
                                    Text(e.date.formatted(date: .abbreviated, time: .shortened))
                                        .font(.caption2).foregroundStyle(Theme.textSecondary)
                                }
                                Spacer()
                                VStack(alignment: .trailing) {
                                    Text("\(e.quantity.formatted()) @ \(Format.price(e.price))").font(Theme.mono(12))
                                    Text(e.simulated ? "démo" : (e.environment == .live ? "réel" : "test"))
                                        .font(.caption2).foregroundStyle(e.environment == .live && !e.simulated ? Theme.sell : Theme.textSecondary)
                                }
                            }
                            .glassCard(glow: e.side == .buy ? Theme.buy : Theme.sell)
                        }
                    }
                    .padding()
                }
                .refreshable { await load() }
            }
            .navigationTitle("Portefeuille")
            .task(id: settings.demoMode) { await load() }
        }
    }

    private func load() async {
        loading = true
        defer { loading = false }
        var result: [(String, [Balance], String?)] = []
        let crypto = Asset.defaults[0]
        let stock = Asset.defaults[4]
        let brokers: [(String, Broker)] = settings.demoMode
            ? [("Compte démo", services.paperBroker)]
            : [("Binance · \(settings.cryptoEnvironment.label)", services.broker(for: crypto, settings: settings)),
               ("Alpaca · \(settings.stockEnvironment.label)", services.broker(for: stock, settings: settings))]
        for (title, broker) in brokers {
            do {
                let balances = try await broker.balances().sorted { $0.total > $1.total }
                result.append((title, balances, nil))
            } catch {
                result.append((title, [], error.localizedDescription))
            }
        }
        sections = result.map { (title: $0.0, balances: $0.1, error: $0.2) }
    }
}
