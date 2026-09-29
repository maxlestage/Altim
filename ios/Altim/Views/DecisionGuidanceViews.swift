import SwiftUI
import AltimKit

// Guidance of the decision card (web DecisionCard.tsx): when not to trade, action zones (vertical ladder), counter-
// argument, why the signal changed, and the watched scenarios' conditions. Stacked rows only: nothing scrolls
// sideways; the meaning is always written out, never carried by the colour alone.

/// Compact banner near the top: the headline and the reasons' names (the detail is in its section).
struct NoTradeBanner: View {
    let noTrade: Decision.NoTrade

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(noTrade.headline).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            WrapLayout(spacing: 4) {
                ForEach(noTrade.reasons) { TagChip(text: $0.label, color: Theme.warning) }
            }
            .accessibilityLabel("Raisons : " + noTrade.reasons.map(\.label).joined(separator: ", "))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.warning.opacity(0.1)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Theme.warning.opacity(0.45), lineWidth: 1))
        .accessibilityElement(children: .combine)
    }
}

/// Content of the "Quand ne pas trader" section.
struct NoTradeList: View {
    let noTrade: Decision.NoTrade

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if noTrade.reasons.isEmpty {
                Text(Decision.NoTrade.empty).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            } else {
                ForEach(noTrade.reasons) { r in
                    (Text("• ") + Text(r.label).bold() + Text(" — \(r.detail)"))
                        .font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
            }
            if let u = noTrade.uncheckedText {
                Text(u).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            Text(Decision.NoTrade.note).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Vertical ladder, highest price at the top; bands in the kind's colour, the price marked where it sits.
struct ActionLadderView: View {
    let zones: Decision.ActionZones

    static func color(_ k: Decision.ActionZoneKind) -> Color {
        switch k {
        case .invalidation: return Theme.sell
        case .exit: return Color.orange
        case .buy: return Theme.buy
        case .wait, .unknown: return Color(red: 0.78, green: 0.82, blue: 0.9)
        case .profit: return Theme.cyan
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Zones d'action").font(.subheadline.weight(.semibold)).foregroundStyle(.white).padding(.top, 4)
            Text(zones.hereText).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(DecisionGuidance.ladder(zones).enumerated()), id: \.offset) { item in
                    switch item.element {
                    case let .zone(r, here): zoneRow(r, here: here)
                    case let .marker(text): markerRow(text)
                    }
                }
            }
            .padding(.leading, 4)
            .overlay(alignment: .leading) { Rectangle().fill(Color.white.opacity(0.14)).frame(width: 2) }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Zones d'action, du prix le plus haut au plus bas")
        }
    }

    private func zoneRow(_ r: Decision.ActionZone, here: Bool) -> some View {
        let c = Self.color(r.kind)
        return HStack(alignment: .top, spacing: 10) {
            RoundedRectangle(cornerRadius: 3).fill(c.opacity(0.8))
                .frame(width: 6, height: r.kind == .invalidation ? 2 : nil)
                .frame(maxHeight: r.kind == .invalidation ? nil : .infinity, alignment: .center)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(r.label).font(.subheadline.weight(.semibold)).foregroundStyle(c)
                Text(DecisionGuidance.range(r)).font(.caption.monospacedDigit()).foregroundStyle(.white)
                if here {
                    Text(DecisionGuidance.hereMark(zones.price)).font(.caption.monospacedDigit().weight(.bold)).foregroundStyle(Theme.warning)
                }
                Text(r.note).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .fixedSize(horizontal: false, vertical: true)
        .padding(.vertical, 6)
        .padding(.leading, 6)
        .background(here ? Color.white.opacity(0.05) : Color.clear, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(here ? .isSelected : [])
    }

    private func markerRow(_ text: String) -> some View {
        Text(text).font(.caption.monospacedDigit().weight(.bold)).foregroundStyle(Theme.warning)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.vertical, 4).padding(.leading, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .top) { DashedLine().stroke(Theme.warning, style: StrokeStyle(lineWidth: 1, dash: [3, 3])).frame(height: 1) }
            .overlay(alignment: .bottom) { DashedLine().stroke(Theme.warning, style: StrokeStyle(lineWidth: 1, dash: [3, 3])).frame(height: 1) }
    }
}

private struct DashedLine: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.minX, y: rect.midY))
        p.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
        return p
    }
}

/// "Contre-argument": favourable / unfavourable reasons and what would prove the scenario wrong.
struct CounterBlockView: View {
    let counter: Decision.CounterArgument

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Contre-argument").font(.subheadline.weight(.semibold)).foregroundStyle(.white).padding(.top, 4)
            WrapLayout(spacing: 16) {
                (Text("🟢 Raisons favorables : ") + Text("\(counter.favourable)").bold()).font(.footnote).foregroundStyle(.white)
                (Text("🔴 Raisons défavorables : ") + Text("\(counter.unfavourable)").bold()).font(.footnote).foregroundStyle(.white)
            }
            Text(counter.familiesText).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if !counter.invalidators.isEmpty {
                Text("Points qui pourraient invalider le scénario").font(.footnote.weight(.semibold)).foregroundStyle(.white)
                ForEach(Array(counter.invalidators.enumerated()), id: \.offset) { item in
                    Text("• \(item.element.text)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

/// "Pourquoi le signal a changé depuis le 28/09 à 14:02": what the measurements say changed.
struct ChangeBlockView: View {
    let transition: ConfigTransition

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(DecisionGuidance.changeTitle(transition)).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            Text(DecisionGuidance.changeSubtitle(transition)).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if transition.changes.isEmpty {
                Text(DecisionGuidance.changeUnknown).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            } else {
                ForEach(transition.changes, id: \.self) { c in
                    Text("• \(c)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.cyan.opacity(0.04)))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Theme.cyan.opacity(0.35), lineWidth: 1))
        .accessibilityElement(children: .combine)
    }
}

/// The conditions of a watched scenario, each checked on the latest data.
struct ScenarioChecksView: View {
    let checks: [Decision.ScenarioCheck]

    private func color(_ s: Decision.CheckState) -> Color {
        switch s {
        case .met: return Theme.buy
        case .unmet: return Color.orange
        case .unknown: return Theme.textSecondary
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(checks.enumerated()), id: \.offset) { item in
                let c = item.element
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(c.state.icon) \(c.state.label)").font(.caption.weight(.semibold)).foregroundStyle(color(c.state))
                    Text(c.text).font(.caption).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                    Text(c.detail).font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                .padding(.top, 4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .overlay(alignment: .top) { Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1) }
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// « Preuve du modèle »: the cross-asset validation of the signal on this asset's class, under the confidence, with the
/// link to « Validation du modèle » (same texts as the web's `EvidenceLine`). The chip and the texts wrap: nothing
/// scrolls sideways. The edge's colour repeats what the text says, never carries the meaning alone.
struct EvidenceBlockView: View {
    let evidence: ModelEvidence

    private var edgeColor: Color {
        switch evidence.tone {
        case .na: return Theme.textSecondary
        case .weak: return Color(red: 1.0, green: 0x9F / 255, blue: 0x43 / 255)
        case .unproven: return Theme.warning
        case .edge: return Theme.buy
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            WrapLayout(spacing: 8) {
                Text(ModelEvidence.title).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                if let chip = evidence.beatHoldChip { TagChip(text: chip, color: .white.opacity(0.9)) }
            }
            Text(evidence.text).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            NavigationLink {
                ValidationView()
            } label: {
                (Text(ModelEvidence.linkText).foregroundStyle(Theme.cyan)
                    + Text(evidence.asOfText ?? "").foregroundStyle(Theme.textSecondary))
                    .font(.caption)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, 8)
        .padding(.leading, 13)
        .padding(.trailing, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.03))
        .overlay(alignment: .leading) { Rectangle().fill(edgeColor).frame(width: 3) }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.white.opacity(0.12), lineWidth: 1))
    }
}
