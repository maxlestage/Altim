import SwiftUI
import AltimCore

struct SettingsView: View {
    @Environment(AppSettings.self) private var settings
    @Environment(AppServices.self) private var services
    @State private var alpacaKey = ""
    @State private var alpacaSecret = ""
    @State private var twelveDataKey = ""
    @State private var polygonKey = ""
    @State private var finnhubKey = ""
    @State private var saved = false
    @State private var showSourcesTest = false

    var body: some View {
        @Bindable var settings = settings
        NavigationStack {
            Form {
                Section {
                    Stepper(value: $settings.risk.riskPerTradePercent, in: 0.25...5, step: 0.25) {
                        LabeledContent("Risque accepté par idée", value: String(format: "%.2f %%", settings.risk.riskPerTradePercent))
                    }
                    Stepper(value: $settings.risk.maxPositionPercent, in: 5...100, step: 5) {
                        LabeledContent("Taille max d'une ligne", value: String(format: "%.0f %%", settings.risk.maxPositionPercent))
                    }
                } header: {
                    Text("Prudence des conseils")
                } footer: {
                    Text("Ces réglages déterminent les montants conseillés : la part de votre patrimoine qu'un conseil d'achat accepte de risquer si le stop est touché, et la taille maximale d'une ligne. Règle professionnelle : 1 à 2 % par idée.")
                }

                Section("Alertes") {
                    Toggle("Me prévenir quand un conseil change", isOn: Binding(
                        get: { settings.notificationsEnabled },
                        set: { newValue in
                            if newValue {
                                Task { settings.notificationsEnabled = await services.requestNotificationPermission() }
                            } else {
                                settings.notificationsEnabled = false
                            }
                        }))
                }

                Section {
                    SecureField("Twelve Data (gratuit)", text: $twelveDataKey)
                    SecureField("Polygon.io (gratuit)", text: $polygonKey)
                    SecureField("Finnhub (gratuit)", text: $finnhubKey)
                    SecureField("Alpaca Key ID (données IEX)", text: $alpacaKey)
                    SecureField("Alpaca Secret (données IEX)", text: $alpacaSecret)
                    Button(saved ? "Clés enregistrées ✓" : "Enregistrer les clés") { saveKeys() }
                    Button("Tester toutes les sources") {
                        saveKeys()
                        showSourcesTest = true
                    }
                } header: {
                    Text("Sources de données supplémentaires")
                } footer: {
                    Text("Facultatif. Sans clé, Altim recoupe déjà jusqu'à 24 sources par crypto (Binance, OKX, Coinbase, Kraken, Bitstamp, Gemini…) et 8 par action (Yahoo Finance, Nasdaq, Robinhood, Cboe, StockAnalysis, Webull, TradingView, Zacks). Ces clés gratuites (lecture de cours uniquement) ajoutent des sources indépendantes pour les actions. Elles restent chiffrées dans le trousseau de cet iPhone.")
                }

                Section("À propos") {
                    LabeledContent("Données crypto", value: "jusqu'à 24 sources recoupées")
                    LabeledContent("Données actions", value: "8 sources recoupées")
                    LabeledContent("Vos avoirs", value: "Base SQLite sur l'iPhone")
                    LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0")
                    Text("Altim est un conseiller : il ne passe aucun ordre et n'accède à aucun compte. Ses conseils sont indicatifs et ne constituent pas une recommandation d'investissement personnalisée.")
                        .font(.caption).foregroundStyle(.secondary)
                    Text("Conception & développement : Maxime Nathan Lestage")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            .scrollContentBackground(.hidden)
            .background(CyberGridBackground())
            .navigationTitle("Réglages")
            .onAppear(perform: loadKeys)
            .sheet(isPresented: $showSourcesTest) { SourcesTestView() }
        }
    }

    private func loadKeys() {
        alpacaKey = KeychainStore.get(.alpacaKey)
        alpacaSecret = KeychainStore.get(.alpacaSecret)
        twelveDataKey = KeychainStore.get(.twelveDataKey)
        polygonKey = KeychainStore.get(.polygonKey)
        finnhubKey = KeychainStore.get(.finnhubKey)
        saved = false
    }

    private func saveKeys() {
        KeychainStore.set(alpacaKey, for: .alpacaKey)
        KeychainStore.set(alpacaSecret, for: .alpacaSecret)
        KeychainStore.set(twelveDataKey, for: .twelveDataKey)
        KeychainStore.set(polygonKey, for: .polygonKey)
        KeychainStore.set(finnhubKey, for: .finnhubKey)
        saved = true
    }
}

/// Live diagnostic of every source (crypto and stocks), to check reliability from the iPhone.
struct SourcesTestView: View {
    @Environment(AppServices.self) private var services
    @Environment(\.dismiss) private var dismiss
    @State private var results: [(title: String, snapshot: MarketSnapshot?, error: String?)] = []
    @State private var running = true

    private let targets: [(Asset, Timeframe)] = [(Asset.defaults[0], .h1), (Asset.defaults[4], .d1)]

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    if running { ProgressView("Interrogation de toutes les sources…").tint(Theme.cyan).padding() }
                    ForEach(results.indices, id: \.self) { i in
                        let r = results[i]
                        VStack(alignment: .leading, spacing: 10) {
                            Text(r.title).font(.headline)
                            if let s = r.snapshot {
                                ReliabilityBadge(level: s.reliability)
                                Text(s.summary).font(.caption).foregroundStyle(Theme.textSecondary)
                                ForEach(s.checks) { SourceRow(check: $0) }
                            } else if let e = r.error {
                                Text(e).font(.caption).foregroundStyle(Theme.sell)
                            }
                        }
                        .glassCard()
                    }
                }
                .padding()
            }
            .background(Theme.background)
            .navigationTitle("Test des sources")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("OK") { dismiss() } } }
        }
        .task { await run() }
    }

    private func run() async {
        running = true
        // Every source is queried (not just the first three) for a full diagnostic.
        var market = services.market
        market.targetSources = market.sources.count
        for (asset, tf) in targets {
            let title = "\(asset.name) · \(tf.label)"
            do {
                let snapshot = try await market.snapshot(for: asset, timeframe: tf)
                results.append((title: title, snapshot: snapshot, error: nil))
            } catch {
                results.append((title: title, snapshot: nil, error: error.localizedDescription))
            }
        }
        running = false
    }
}
