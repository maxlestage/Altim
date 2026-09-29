import SwiftUI
import AltimKit

// Risk of the real portfolio (Mes avoirs) and what changed on the watched assets (Radar), same content as the web
// (RiskCards.tsx, MyHoldings.tsx, Radar.tsx). Stacked rows only: nothing scrolls sideways.

extension LimitLevel {
    var tone: Tone {
        switch self {
        case .danger: return .bad
        case .warning: return .warn
        case .ok: return .good
        case .na: return .neutral
        }
    }

    var icon: String {
        switch self {
        case .danger: return "xmark.octagon.fill"
        case .warning: return "exclamationmark.triangle.fill"
        case .ok: return "checkmark.circle.fill"
        case .na: return "questionmark.circle"
        }
    }
}

/// The portfolio against the user's own limits (Réglages → Prudence des conseils).
struct LimitsCard: View {
    let checks: [LimitCheck]

    var body: some View {
        Card(title: "Vos limites de risque") {
            ForEach(checks) { c in
                InsightRow(icon: c.level.icon, tone: c.level.tone, title: c.label, detail: c.detail)
            }
            Text("Limites réglables dans Réglages → Prudence des conseils. Hypothèse : stop que vous avez saisi, sinon stop de protection à 2 × la volatilité journalière ; variation du jour mesurée depuis la clôture de la veille (journée UTC pour les cryptos, dernière séance pour les actions).")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Market shocks passed through each line's beta: loss in $ and %, worst line.
struct StressCard: View {
    let portfolio: RiskPortfolio
    let results: [StressResult]
    let betas: [String: BetaEstimate]

    var body: some View {
        Card(title: "Scénarios de crise") {
            Text("Ce que perdrait votre portefeuille si les marchés chutaient d'un coup : chaque ligne bouge selon son bêta face à sa référence (\(RiskEngine.benchmark(.crypto).label) pour les cryptos, \(RiskEngine.benchmark(.stock).label) pour les actions)."
                 + (portfolio.cash > 0 ? " Vos liquidités (\(RiskText.usd(portfolio.cash))) ne bougent pas." : " Les liquidités ne sont pas saisies sur l'iPhone : pourcentages calculés sur vos lignes."))
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            ForEach(results, id: \.scenario.key) { r in
                InsightRow(icon: r.loss > 0 ? "arrow.down.right" : "arrow.right",
                           tone: r.lossPercent >= 10 ? .bad : r.loss > 0 ? .warn : .good,
                           title: r.scenario.label, detail: RiskText.stressLine(r, cash: portfolio.cash)) {
                    if let w = r.worst {
                        Text(RiskText.worst(w)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            Text(RiskText.stressNote(portfolio, betas: betas))
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Positions that became dangerous (stop broken or close, loss beyond the risk per idea).
struct DangerNotice: View {
    let dangers: [Danger]
    /// On the Radar: with the reasons and when it was measured.
    var measuredAt: Double?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label {
                Text("\(dangers.count > 1 ? "Positions devenues dangereuses" : "Position devenue dangereuse")\(measuredAt == nil ? "" : " dans vos avoirs") : \(dangers.map(\.symbol).joined(separator: ", "))")
                    .font(.footnote.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
            } icon: {
                Image(systemName: "exclamationmark.octagon.fill").foregroundStyle(Theme.sell)
            }
            if let measuredAt {
                ForEach(dangers) { d in
                    Text("\(d.symbol) : \(d.reasons.map(\.text).joined(separator: " "))")
                        .font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
                Text("Mesuré le \(Format.date(measuredAt, time: true)) sur Mes avoirs (ouvrez-le pour actualiser).")
                    .font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            } else {
                Text("Détail sur chaque ligne ci-dessous. Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque).")
                    .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.sell.opacity(0.1)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Theme.sell.opacity(0.4), lineWidth: 1))
        .accessibilityElement(children: .combine)
    }
}

/// Configuration changes of the watched assets (verdict or level changed since the last decision seen here).
struct ConfigChangesCard: View {
    @Environment(AppModel.self) private var model
    @State private var showAll = false
    @State private var confirmClear = false

    var body: some View {
        let all = model.configChanges.transitions
        Card(title: "Changements de configuration · \(all.count)") {
            ForEach(showAll ? all : Array(all.prefix(5))) { t in
                NavigationLink(value: t.asset) { row(t) }
                    .buttonStyle(.plain)
            }
            HStack(spacing: 16) {
                if all.count > 5 {
                    Button(showAll ? "Voir moins" : "Voir les \(all.count)") { showAll.toggle() }
                }
                Button("Effacer", role: .destructive) { confirmClear = true }
            }
            .font(.footnote)
            .buttonStyle(.borderless)
            Text("Comparaison avec la dernière décision vue sur cet iPhone. Nouvelle analyse toutes les 15 minutes tant que le radar est ouvert, à chaque ouverture d'une fiche\(model.configAlertsEnabled ? ", et en arrière-plan quand iOS le permet (notification « Changements de configuration »)" : ""). 50 derniers changements conservés ici uniquement.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .confirmationDialog("Effacer l'historique des changements ?", isPresented: $confirmClear, titleVisibility: .visible) {
            Button("Effacer", role: .destructive) { model.clearTransitions() }
        }
    }

    private func row(_ t: ConfigTransition) -> some View {
        InsightRow(icon: "arrow.triangle.2.circlepath", tone: t.tone, title: t.title,
                   detail: "\(Format.date(t.at, time: true)) · niveau \(t.from.levelLabel) → \(t.to.levelLabel) · configuration précédente vue le \(Format.date(t.since, time: true))\(t.personal ? " · mode personnel" : "")") {
            if !t.changes.isEmpty {
                Text("Pourquoi le signal a changé : \(t.changes.joined(separator: " ; ")).").font(.caption).foregroundStyle(.white.opacity(0.85))
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !t.missing.isEmpty {
                Text("Conditions manquantes : \(t.missing.joined(separator: " ; ")).").font(.caption).foregroundStyle(.white.opacity(0.85))
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !t.triggers.isEmpty {
                Text("Ce qui changerait la décision : \(t.triggers.joined(separator: " ; ")).").font(.caption).foregroundStyle(.white.opacity(0.85))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
