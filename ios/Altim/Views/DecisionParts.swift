import SwiftUI
import AltimKit

// Parts of the decision card added with the rating (web DecisionCard.tsx): market regime, degraded signal, composite
// score with one thin bar per factor, technical structure. Stacked rows: nothing scrolls sideways.

/// "Régime de marché : 🟢 Risk-on (appétit pour le risque) — reasons".
struct RegimeLine: View {
    let regime: Decision.MarketRegime

    var body: some View {
        (Text("Régime de marché : ").foregroundStyle(Theme.textSecondary)
            + Text("\(regime.kind.emoji) \(regime.kind.label)").bold().foregroundStyle(.white)
            + Text(regime.reasons.isEmpty ? "" : " — \(regime.reasons.joined(separator: " · "))").foregroundStyle(Theme.textSecondary))
            .font(.caption)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// "⚠️ Signal dégradé — …": the server's headline as sent (it names the actual cause), then its reasons.
struct DegradedBanner: View {
    let degraded: Decision.Degraded

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(degraded.headline).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            ForEach(degraded.reasons, id: \.self) { r in
                Text("• \(r)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.sell.opacity(0.12)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Theme.sell.opacity(0.5), lineWidth: 1))
        .accessibilityElement(children: .combine)
    }
}

/// Score −100 … +100 on one axis centred on 0.
struct ScoreBar: View {
    let value: Double
    let label: String

    var body: some View {
        GeometryReader { geo in
            let half = geo.size.width / 2
            let w = half * min(100, abs(value)) / 100
            ZStack(alignment: .leading) {
                Capsule().fill(Color.white.opacity(0.08))
                Rectangle().fill(Color.white.opacity(0.25)).frame(width: 1).offset(x: half)
                Capsule().fill(value >= 0 ? Theme.buy : Theme.sell)
                    .frame(width: max(2, w))
                    .offset(x: value >= 0 ? half : half - w)
            }
        }
        .frame(height: 4)
        .accessibilityElement()
        .accessibilityLabel("\(label) : \(DecisionText.signedScore(value)) sur une échelle de −100 à +100")
    }
}

struct ScoreBlock: View {
    let score: Decision.CompositeScore

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let v = score.value {
                DecisionRow(key: "Score composite", value: "\(DecisionText.signedScore(v)) · \(score.label)", tone: v > 0 ? Tone.good : v < 0 ? Tone.bad : nil)
                ScoreBar(value: v, label: "Score composite")
                ForEach(score.factors) { f in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(alignment: .firstTextBaseline) {
                            (Text(f.label).foregroundStyle(.white)
                                + Text(f.value == nil ? "  non mesuré" : "  poids \(DecisionText.num(f.applied, digits: 1)) %").font(.caption2).foregroundStyle(Theme.textSecondary))
                                .font(.caption)
                                .fixedSize(horizontal: false, vertical: true)
                            Spacer(minLength: 6)
                            Text(f.value.map { DecisionText.signedScore($0) } ?? "—").font(Theme.mono(12, weight: .regular))
                        }
                        if let fv = f.value { ScoreBar(value: fv, label: f.label) }
                    }
                    .accessibilityElement(children: .combine)
                }
            }
            Text(score.text).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Every item of the technical structure, measured or not (a missing one says why rather than disappearing).
struct StructureList: View {
    let structure: Decision.Structure

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(DecisionText.structureHeader(structure)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            ForEach(Array(DecisionText.structureRows(structure).enumerated()), id: \.offset) { item in
                row(item.element)
            }
        }
    }

    private func row(_ r: DecisionText.StructureRow) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline) {
                    Text(r.name).font(.subheadline.weight(.semibold))
                    Spacer(minLength: 6)
                    if let b = r.bias { biasTag(b) }
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(r.name).font(.subheadline.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                    if let b = r.bias { biasTag(b) }
                }
            }
            Text(r.reading).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if let extra = r.extra {
                Text(extra).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            ForEach(Array(r.list.enumerated()), id: \.offset) { item in
                let line = item.element
                Text("• \(line)").font(.caption.monospacedDigit()).foregroundStyle(.white.opacity(0.85)).fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// Direction: an arrow and a word, never colour alone.
    private func biasTag(_ b: Decision.Bias) -> some View {
        Text("\(b.arrow) \(b.label)").font(.caption.weight(.semibold)).foregroundStyle(Theme.color(b.tone))
    }
}
