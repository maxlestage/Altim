import SwiftUI
import AltimCore

/// Passage d'ordre : taille suggérée par le gestionnaire de risque, vérifications, confirmation biométrique.
struct TradeSheet: View {
    let asset: Asset
    let side: OrderSide
    let signal: Signal?
    let price: Double

    @Environment(AppSettings.self) private var settings
    @Environment(AppServices.self) private var services
    @Environment(\.dismiss) private var dismiss
    @State private var model: TradeViewModel?

    var body: some View {
        NavigationStack {
            ZStack {
                Theme.background.ignoresSafeArea()
                if let model {
                    TradeForm(model: model, demo: settings.demoMode, close: { dismiss() })
                } else {
                    ProgressView().tint(Theme.cyan)
                }
            }
            .navigationTitle("\(side.label) \(asset.name)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Annuler") { dismiss() } } }
        }
        .task {
            let vm = TradeViewModel(asset: asset, side: side, signal: signal, price: price, settings: settings, services: services)
            model = vm
            await vm.prepare()
        }
    }
}

private struct TradeForm: View {
    @Bindable var model: TradeViewModel
    let demo: Bool
    let close: () -> Void

    private var sideColor: Color { model.side == .buy ? Theme.buy : Theme.sell }

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                HStack {
                    Text(model.brokerName).font(Theme.mono(13)).foregroundStyle(Theme.textSecondary)
                    Spacer()
                    EnvironmentBadge(isLive: model.isLive, demo: demo)
                }

                switch model.phase {
                case .loading:
                    ProgressView("Connexion au courtier…").tint(Theme.cyan).padding(40)
                case .failed(let message):
                    failure(message)
                case .done:
                    success
                case .ready, .executing:
                    form
                }
            }
            .padding()
        }
    }

    // MARK: Formulaire

    private var form: some View {
        VStack(spacing: 16) {
            if let signal = model.signal {
                HStack {
                    Text("Signal actuel").foregroundStyle(Theme.textSecondary)
                    Spacer()
                    ActionBadge(action: signal.action, compact: true)
                }
                .glassCard()
                if (model.side == .buy && signal.action.isSell) || (model.side == .sell && signal.action.isBuy) {
                    Label("Cet ordre va à l'encontre du signal Altim.", systemImage: "exclamationmark.triangle.fill")
                        .font(.subheadline).foregroundStyle(Theme.warning)
                }
            }

            VStack(alignment: .leading, spacing: 12) {
                SectionTitle(text: "Ordre")
                Picker("Type", selection: $model.orderType) {
                    ForEach(OrderType.allCases, id: \.self) { Text($0.label).tag($0) }
                }
                .pickerStyle(.segmented)

                field("Quantité (\(model.rules?.baseAsset ?? model.asset.base))", text: $model.quantityText)
                if model.orderType == .limit {
                    field("Prix limite (\(model.rules?.quoteAsset ?? model.asset.quote))", text: $model.limitPriceText)
                }
                HStack {
                    Text("Disponible").foregroundStyle(Theme.textSecondary)
                    Spacer()
                    Text(model.side == .buy
                         ? "\(Format.quantity(model.quoteBalance)) \(model.rules?.quoteAsset ?? "")"
                         : "\(Format.quantity(model.baseBalance)) \(model.rules?.baseAsset ?? "")")
                        .font(Theme.mono(13))
                }
                .font(.caption)
                if let quantity = TradeViewModel.decimal(model.quantityText) {
                    HStack {
                        Text("Montant estimé").foregroundStyle(Theme.textSecondary)
                        Spacer()
                        Text("≈ \(Format.price(quantity.doubleValue * model.referencePrice)) \(model.rules?.quoteAsset ?? model.asset.quote)")
                            .font(Theme.mono(13))
                    }
                    .font(.caption)
                }
            }
            .glassCard()

            if model.side == .buy {
                VStack(alignment: .leading, spacing: 12) {
                    Toggle("Stop et objectif automatiques", isOn: $model.protect).tint(Theme.cyan)
                    if model.protect {
                        field("Stop (vente si le prix descend à)", text: $model.stopText)
                        field("Objectif (vente si le prix monte à)", text: $model.targetText)
                    } else {
                        Text("Sans stop, une chute brutale n'est pas limitée.")
                            .font(.caption).foregroundStyle(Theme.warning)
                    }
                    if let s = model.suggestion {
                        Text(String(format: "Taille suggérée : risque %.2f (%.1f %% du capital si le stop est touché), position %.1f %% du capital%@.",
                                    s.riskAmount, s.riskAmount / max(model.quoteBalance.doubleValue, 1) * 100,
                                    s.percentOfEquity, s.capped ? ", plafonnée" : ""))
                            .font(.caption).foregroundStyle(Theme.textSecondary)
                    }
                }
                .glassCard(glow: Theme.violet)
            }

            if model.isLive {
                Toggle(isOn: $model.liveConfirmed) {
                    Text("Je confirme passer un ordre avec de l'ARGENT RÉEL.").font(.subheadline.bold())
                }
                .tint(Theme.sell)
                .glassCard(glow: Theme.sell)
            }

            if !model.issues.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(model.issues, id: \.self) { issue in
                        Label(issue, systemImage: "xmark.octagon.fill").font(.caption).foregroundStyle(Theme.sell)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Button {
                Task { await model.execute() }
            } label: {
                if model.phase == .executing {
                    ProgressView().tint(.black)
                } else {
                    Label(model.side == .buy ? "CONFIRMER L'ACHAT" : "CONFIRMER LA VENTE", systemImage: "faceid")
                }
            }
            .buttonStyle(NeonButtonStyle(color: sideColor))
            .disabled(model.phase == .executing)

            Text("L'ordre est d'abord validé par le courtier (ordre test), puis exécuté après Face ID / code.")
                .font(.caption2).foregroundStyle(Theme.textSecondary).multilineTextAlignment(.center)
        }
        .onChange(of: model.quantityText) { _, _ in _ = model.buildOrder() }
        .onChange(of: model.stopText) { _, _ in _ = model.buildOrder() }
        .onChange(of: model.targetText) { _, _ in _ = model.buildOrder() }
        .onChange(of: model.orderType) { _, _ in _ = model.buildOrder() }
        .onChange(of: model.protect) { _, _ in _ = model.buildOrder() }
        .onChange(of: model.liveConfirmed) { _, _ in _ = model.buildOrder() }
    }

    private func field(_ title: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.caption).foregroundStyle(Theme.textSecondary)
            TextField("0", text: text)
                .keyboardType(.decimalPad)
                .font(Theme.mono(18))
                .padding(12)
                .background(RoundedRectangle(cornerRadius: 12).fill(Color.white.opacity(0.05)))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.cyan.opacity(0.3)))
        }
    }

    // MARK: Résultats

    private var success: some View {
        VStack(spacing: 16) {
            Image(systemName: "checkmark.seal.fill")
                .font(.system(size: 64))
                .foregroundStyle(sideColor)
                .neonGlow(sideColor, radius: 16)
            if let r = model.result {
                Text("Ordre \(r.status)").font(.title2.bold())
                VStack(spacing: 8) {
                    row("Référence", r.orderId)
                    row("Quantité exécutée", Format.quantity(r.executedQuantity))
                    if let avg = r.averagePrice { row("Prix moyen", Format.price(avg.doubleValue)) }
                }
                .glassCard(glow: sideColor)
                ForEach(r.notes, id: \.self) { note in
                    Text(note).font(.caption)
                        .foregroundStyle(note.hasPrefix("⚠️") ? Theme.warning : Theme.textSecondary)
                }
            }
            Button("TERMINER", action: close).buttonStyle(NeonButtonStyle())
        }
    }

    private func failure(_ message: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: "xmark.octagon.fill").font(.system(size: 56)).foregroundStyle(Theme.sell)
            Text(message).multilineTextAlignment(.center)
            Button("RÉESSAYER") {
                if model.rules == nil { Task { await model.prepare() } } else { model.reset() }
            }
            .buttonStyle(NeonButtonStyle(filled: false))
        }
        .padding(.top, 30)
    }

    private func row(_ title: String, _ value: String) -> some View {
        HStack {
            Text(title).foregroundStyle(Theme.textSecondary)
            Spacer()
            Text(value).font(Theme.mono(13)).lineLimit(1).minimumScaleFactor(0.6)
        }
    }
}
