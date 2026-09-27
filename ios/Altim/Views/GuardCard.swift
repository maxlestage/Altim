import SwiftUI
import AltimCore

/// Market guard: background regime, shock risk, counter-trend reversal risk, and what a short-term bot should do.
struct GuardCard: View {
    let result: MarketGuard.Result
    let headlines: [MarketGuard.NewsItem]
    let asset: Asset
    @State private var showNews = false

    private func trendLabel(_ t: MarketGuard.Trend) -> String {
        switch t { case .up: "Haussière"; case .down: "Baissière"; case .range: "Sans direction" }
    }
    private func levelLabel(_ l: MarketGuard.ShockLevel) -> String {
        switch l { case .calm: "Calme"; case .agitated: "Agité"; case .shock: "Choc" }
    }
    private func policyLabel(_ p: MarketGuard.Scalping) -> String {
        switch p { case .ok: "Autorisé"; case .reduce: "Taille réduite"; case .pause: "Suspendu" }
    }
    private func statusLabel(_ s: MarketGuard.FactorStatus) -> String {
        switch s {
        case .verified: "vérifié sur cet actif"
        case .unproven: "peu d'historique : compté à moitié"
        case .rejected: "jamais prédictif ici : ignoré"
        case .unverifiable: "sans historique : non vérifié"
        }
    }
    private func color(_ score: Int, warn: Int, bad: Int) -> Color {
        score >= bad ? Theme.sell : score >= warn ? Theme.warning : Theme.buy
    }

    var body: some View {
        let shockColor = color(result.shock.score, warn: MarketGuard.shockAgitated, bad: MarketGuard.shockLevel)
        let revColor = color(result.reversal.score, warn: 25, bad: MarketGuard.reversalHigh)
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: "Garde-fou marché")
            HStack {
                Text("Tendance de fond").foregroundStyle(Theme.textSecondary)
                Spacer()
                Text("\(trendLabel(result.regime.trend)) · force \(result.regime.strength)/100").bold()
            }
            .font(.subheadline)
            Text(result.regime.text).font(.caption).foregroundStyle(Theme.textSecondary)

            meter("Risque de choc · \(levelLabel(result.shock.level))", result.shock.score, shockColor)
            factors(result.shock.factors)
            let dir = result.reversal.direction.map { $0 == .down ? " à la baisse" : " à la hausse" } ?? ""
            meter("Risque de retournement\(dir)", result.reversal.score, revColor)
            factors(result.reversal.factors)

            Divider().overlay(Color.white.opacity(0.1))
            HStack {
                Text("Trading court terme (bots)").foregroundStyle(Theme.textSecondary)
                Spacer()
                Text(policyLabel(result.policy.scalping)).bold()
            }
            .font(.subheadline)
            HStack {
                Text("Taille × \(result.policy.sizeMultiplier.formatted())")
                Spacer()
                Text("Stop × \(result.policy.stopMultiplier.formatted())")
            }
            .font(Theme.mono(12, weight: .regular)).foregroundStyle(Theme.textSecondary)
            ForEach(result.policy.notes, id: \.self) { n in
                Label(n, systemImage: "arrow.right.circle").font(.caption).foregroundStyle(.white.opacity(0.85))
            }
            if !headlines.isEmpty {
                DisclosureGroup("Dernières actualités (\(headlines.count))", isExpanded: $showNews) {
                    ForEach(headlines, id: \.self) { h in
                        Text("\(h.title) · \(h.time.formatted(date: .omitted, time: .shortened))")
                            .font(.caption2).foregroundStyle(Theme.textSecondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .font(.caption)
            }
            Text("Aucun outil ne prévoit une vraie surprise. Le garde-fou mesure les conditions où les grands mouvements et les retournements sont plus probables, et chaque signal technique est vérifié sur l'historique de l'actif.")
                .font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .glassCard(glow: shockColor)
    }

    private func meter(_ label: String, _ value: Int, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(label).font(.subheadline)
                Spacer()
                Text("\(value)/100").font(Theme.mono(13, weight: .bold)).foregroundStyle(color)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.07))
                    Capsule().fill(color).frame(width: max(4, geo.size.width * Double(value) / 100))
                }
            }
            .frame(height: 6)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(label) : \(value) sur 100")
    }

    @ViewBuilder
    private func factors(_ list: [MarketGuard.Factor]) -> some View {
        if list.isEmpty {
            Text("Aucun signal.").font(.caption).foregroundStyle(Theme.textSecondary)
        } else {
            ForEach(list) { f in
                VStack(alignment: .leading, spacing: 2) {
                    Text(f.text).font(.caption).foregroundStyle(f.status == .rejected ? Theme.textSecondary : .white)
                    Text("+\(f.points) · \(statusLabel(f.status))" + (f.evidence.map { e in e.samples > 0 ? " (\(Int(e.rate.rounded())) % des \(e.samples) cas passés contre \(Int(e.base.rounded())) % d'habitude)" : "" } ?? ""))
                        .font(.caption2).foregroundStyle(Theme.textSecondary)
                }
            }
        }
    }
}
