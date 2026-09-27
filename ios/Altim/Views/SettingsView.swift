import SwiftUI
import AltimCore

struct SettingsView: View {
    @Environment(AppSettings.self) private var settings
    @Environment(AppServices.self) private var services
    @State private var binanceKey = ""
    @State private var binanceSecret = ""
    @State private var alpacaKey = ""
    @State private var alpacaSecret = ""
    @State private var twelveDataKey = ""
    @State private var polygonKey = ""
    @State private var finnhubKey = ""
    @State private var showSourcesTest = false
    @State private var saved = false
    @State private var confirmLive: LiveTarget?

    enum LiveTarget: String, Identifiable {
        case crypto, stock
        var id: String { rawValue }
    }

    var body: some View {
        @Bindable var settings = settings
        NavigationStack {
            Form {
                Section {
                    Toggle("Mode démo (argent fictif)", isOn: $settings.demoMode)
                } footer: {
                    Text("En mode démo, les ordres sont simulés localement avec 10 000 USDT fictifs. Désactivez-le pour utiliser vos comptes Binance / Alpaca.")
                }

                Section("Binance · crypto") {
                    SecureField("Clé API", text: $binanceKey)
                    SecureField("Clé secrète", text: $binanceSecret)
                    environmentPicker(value: settings.cryptoEnvironment, target: .crypto)
                }

                Section {
                    SecureField("Key ID", text: $alpacaKey)
                    SecureField("Secret key", text: $alpacaSecret)
                    environmentPicker(value: settings.stockEnvironment, target: .stock)
                } header: {
                    Text("Alpaca · actions US")
                } footer: {
                    Text("Clés stockées chiffrées dans le trousseau de cet iPhone uniquement. Créez des clés SANS permission de retrait et, si possible, restreintes à une IP.")
                }

                Section {
                    SecureField("Twelve Data (gratuit)", text: $twelveDataKey)
                    SecureField("Polygon.io (gratuit)", text: $polygonKey)
                    SecureField("Finnhub (gratuit)", text: $finnhubKey)
                    Button("Tester toutes les sources") {
                        saveKeys()
                        showSourcesTest = true
                    }
                } header: {
                    Text("Sources de données supplémentaires")
                } footer: {
                    Text("Sans clé, Altim recoupe déjà Binance, OKX, Coinbase, Kraken, KuCoin, Gate.io, Bitfinex, Binance.US, CoinGecko, Yahoo Finance, Nasdaq et Cboe. Ces clés gratuites ajoutent des sources indépendantes pour les actions.")
                }

                Section {
                    Button(saved ? "Clés enregistrées ✓" : "Enregistrer les clés") { saveKeys() }
                    Button("Effacer toutes les clés", role: .destructive) {
                        KeychainStore.Key.allCases.forEach { KeychainStore.set("", for: $0) }
                        loadKeys()
                    }
                }

                Section {
                    Stepper(value: $settings.risk.riskPerTradePercent, in: 0.25...5, step: 0.25) {
                        LabeledContent("Risque par trade", value: String(format: "%.2f %%", settings.risk.riskPerTradePercent))
                    }
                    Stepper(value: $settings.risk.maxPositionPercent, in: 5...100, step: 5) {
                        LabeledContent("Position max", value: String(format: "%.0f %%", settings.risk.maxPositionPercent))
                    }
                    Stepper(value: $settings.risk.dailyLossLimitPercent, in: 1...20, step: 0.5) {
                        LabeledContent("Perte max / jour", value: String(format: "%.1f %%", settings.risk.dailyLossLimitPercent))
                    }
                    Stepper(value: $settings.risk.minRiskReward, in: 1...5, step: 0.25) {
                        LabeledContent("Gain/risque min", value: String(format: "%.2f", settings.risk.minRiskReward))
                    }
                } header: {
                    Text("Gestion du risque")
                } footer: {
                    Text("Règle professionnelle : ne jamais risquer plus de 1 à 2 % du capital par position.")
                }

                Section("Alertes") {
                    Toggle("Notifier les changements de signal", isOn: Binding(
                        get: { settings.notificationsEnabled },
                        set: { newValue in
                            if newValue {
                                Task { settings.notificationsEnabled = await services.requestNotificationPermission() }
                            } else {
                                settings.notificationsEnabled = false
                            }
                        }))
                }

                Section("À propos") {
                    LabeledContent("Données crypto", value: "9 sources recoupées")
                    LabeledContent("Données actions", value: "3 à 6 sources recoupées")
                    LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0")
                    Text("Altim est un outil d'aide à la décision. Il ne constitue pas un conseil en investissement.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            .scrollContentBackground(.hidden)
            .background(CyberGridBackground())
            .navigationTitle("Réglages")
            .onAppear(perform: loadKeys)
            .sheet(isPresented: $showSourcesTest) { SourcesTestView() }
            .alert(item: $confirmLive) { target in
                Alert(
                    title: Text("Passer en argent réel ?"),
                    message: Text("Les ordres seront exécutés sur votre compte réel. Vous pouvez perdre de l'argent. Vérifiez d'abord votre stratégie en test."),
                    primaryButton: .destructive(Text("Activer le réel")) {
                        if target == .crypto { settings.cryptoEnvironment = .live } else { settings.stockEnvironment = .live }
                    },
                    secondaryButton: .cancel(Text("Rester en test"))
                )
            }
        }
    }

    private func environmentPicker(value: BrokerEnvironment, target: LiveTarget) -> some View {
        Picker("Environnement", selection: Binding(
            get: { value },
            set: { newValue in
                if newValue == .live {
                    confirmLive = target
                } else if target == .crypto {
                    settings.cryptoEnvironment = .test
                } else {
                    settings.stockEnvironment = .test
                }
            })) {
            Text("Test").tag(BrokerEnvironment.test)
            Text("Réel").tag(BrokerEnvironment.live)
        }
        .pickerStyle(.segmented)
    }

    private func loadKeys() {
        binanceKey = KeychainStore.get(.binanceKey)
        binanceSecret = KeychainStore.get(.binanceSecret)
        alpacaKey = KeychainStore.get(.alpacaKey)
        alpacaSecret = KeychainStore.get(.alpacaSecret)
        twelveDataKey = KeychainStore.get(.twelveDataKey)
        polygonKey = KeychainStore.get(.polygonKey)
        finnhubKey = KeychainStore.get(.finnhubKey)
        saved = false
    }

    private func saveKeys() {
        KeychainStore.set(binanceKey, for: .binanceKey)
        KeychainStore.set(binanceSecret, for: .binanceSecret)
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
