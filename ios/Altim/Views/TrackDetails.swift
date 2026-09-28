import SwiftUI
import AltimKit

/// More of the signal's track record (inside "Historique du signal"): spread cost, expectancy, R multiples, results
/// by market regime, how the test avoids flattering itself, and the tax assumption. Same content as the web
/// (web/src/webapp/TrackDetails.tsx); older answers show nothing more.
struct TrackDetails: View {
    let track: Decision.Track

    var body: some View {
        let t = track
        VStack(alignment: .leading, spacing: 6) {
            DecisionRow(key: "Espérance par trade (coûts inclus)", value: DecisionText.pct(t.expectancy, digits: 2, sign: true),
                        tone: (t.expectancy ?? 0) >= 0 ? .good : .bad)
            DecisionRow(key: "Multiple de R moyen (gain ÷ risque jusqu'au stop)", value: t.avgR.map { "\(DecisionText.num($0, digits: 2)) R" } ?? "—")
            if let spread = t.spreadPct {
                DecisionRow(key: "Écart achat/vente \(t.spreadMeasured == true ? "mesuré" : "supposé")", value: DecisionText.pct(spread, digits: 3))
            }
            if let n = t.spreadNote, !n.isEmpty { note(n) }

            if let regimes = t.regimes, !regimes.isEmpty {
                Text("Selon le régime de marché").font(.caption.weight(.semibold)).padding(.top, 4)
                ForEach(regimes, id: \.regime) { g in regimeRow(g) }
                note("Haussier : clôture au-dessus d'une moyenne 200 jours qui monte (sur 20 jours) ; baissier : sous une moyenne qui baisse ; crise : plus de 30 % sous le plus haut de l'année. Régime lu à la date du signal, sans données futures.")
            }

            if let notes = t.biasNotes, !notes.isEmpty {
                Text("Comment ce test évite de se flatter").font(.caption.weight(.semibold)).padding(.top, 4)
                ForEach(notes, id: \.self) { n in
                    Text("• \(n)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
            }

            note("Impôt (hypothèse : flat tax de \(Int(Decision.Track.flatTax)) % sur le gain net, payée à la fin, pertes compensées) : rendement du signal après impôt ≈ \(DecisionText.pct(t.afterTaxReturn, digits: 1, sign: true)). Votre situation fiscale peut différer.")
        }
        .padding(.top, 4)
    }

    private func regimeRow(_ g: Decision.RegimeStat) -> some View {
        let tone: Tone = g.lowSample ? .neutral : (g.avgReturn ?? 0) >= 0 ? .good : .warn
        let icon = g.regime == "bull" ? "arrow.up.right" : g.regime == "bear" ? "arrow.down.right" : g.regime == "crisis" ? "exclamationmark.triangle" : "arrow.right"
        var detail = "\(g.trades) trade\(g.trades > 1 ? "s" : "")"
        if g.trades > 0 {
            detail += " · réussite \(DecisionText.pct(g.winRate, digits: 0)) · moyenne \(DecisionText.pct(g.avgReturn, digits: 1, sign: true))"
        }
        if g.lowSample { detail += " · échantillon trop faible" }
        return InsightRow(icon: icon, tone: tone, title: g.label, detail: detail)
    }

    private func note(_ text: String) -> some View {
        Text(text).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }
}
