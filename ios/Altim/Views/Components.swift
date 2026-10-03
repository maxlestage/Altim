import SwiftUI
import Charts
import AltimKit

/// Card with title (same structure as the web app's cards).
struct Card<Content: View>: View {
    var title: String?
    var glow: Color = Theme.cyan
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let title {
                Text(title).font(.headline).foregroundStyle(.white)
            }
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard(glow: glow)
    }
}

struct Badge: View {
    var text: String
    var tone: Tone

    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .bold, design: .rounded))
            .tracking(0.5)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundStyle(Theme.color(tone))
            .background(Capsule().fill(Theme.color(tone).opacity(0.14)))
            .overlay(Capsule().strokeBorder(Theme.color(tone).opacity(0.5), lineWidth: 1))
            .fixedSize()
    }
}

/// The verdict of an asset outside its page: the full decision (the same as its Décision card), seen on this iPhone
/// less than 12 h ago; "Décision…" until then. The 4 h technical signal is only one of its inputs (web DecisionBadge).
struct DecisionBadge: View {
    @Environment(AppModel.self) private var model
    var asset: Asset

    var body: some View {
        if let d = DecisionDigests.fresh(model.decisionDigests, asset, now: Date().timeIntervalSince1970 * 1000) {
            Badge(text: d.badgeLabel, tone: d.tone)
                .accessibilityLabel("Décision Altim : \(d.badgeLabel), confiance \(Int(d.confidence.rounded()))")
        } else {
            Badge(text: DecisionDigests.pendingLabel, tone: .neutral)
                .opacity(0.7)
                .accessibilityLabel("Décision en cours de calcul")
        }
    }
}

/// The decision's short reason under the Radar's chip (« zone d'achat 67 653,51 € (−10,1 %) », « veto : … »), when the
/// chip reads ATTENDRE or AUCUNE POSITION; nothing otherwise (web DecisionNote).
struct DecisionNoteLine: View {
    @Environment(AppModel.self) private var model
    var asset: Asset

    var body: some View {
        if let n = DecisionDigests.fresh(model.decisionDigests, asset, now: Date().timeIntervalSince1970 * 1000)?.noteLine {
            Text(n)
                .font(.caption2)
                .foregroundStyle(Theme.textSecondary)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// The technical signal as a small direction line, never a verdict: "technique 4 h : haussier" (web `technicalText`).
struct TechnicalLine: View {
    var action: Action
    var interval: String

    var body: some View {
        Text("technique \(interval) : \(action.technicalText)")
            .font(.caption2)
            .foregroundStyle(Theme.textSecondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityLabel("Signal technique sur bougies de \(interval) : \(action.technicalText), un indice parmi d'autres de la décision")
    }
}

struct ChangeText: View {
    var value: Double?
    var body: some View {
        Text(Format.percent(value))
            .font(Theme.mono(13))
            .foregroundStyle(value == nil ? Theme.textSecondary : Theme.color(forChange: value))
    }
}

/// Short line of recent closes (radar).
struct Sparkline: View {
    var values: [Double]

    var body: some View {
        let up = (values.last ?? 0) >= (values.first ?? 0)
        Chart(Array(values.enumerated()), id: \.offset) { item in
            LineMark(x: .value("t", item.offset), y: .value("prix", item.element))
                .interpolationMethod(.monotone)
                .lineStyle(StrokeStyle(lineWidth: 1.5))
                .foregroundStyle(up ? Theme.buy : Theme.sell)
        }
        .chartXAxis(.hidden)
        .chartYAxis(.hidden)
        .chartYScale(domain: (values.min() ?? 0)...(values.max() ?? 1))
        .accessibilityHidden(true)
    }
}

/// "EN DIRECT" while ticks keep coming, otherwise the reconnection state.
struct LiveBadge: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        TimelineView(.periodic(from: .now, by: 2)) { ctx in
            let fresh = model.live.lastTick.map { ctx.date.timeIntervalSince($0) < 20 } ?? false
            HStack(spacing: 6) {
                Circle().fill(fresh ? Theme.buy : Theme.warning).frame(width: 7, height: 7).neonGlow(fresh ? Theme.buy : Theme.warning, radius: 4)
                Text(fresh ? "EN DIRECT" : "CONNEXION…").font(.system(size: 10, weight: .bold, design: .rounded)).tracking(1)
                    .foregroundStyle(fresh ? Theme.buy : Theme.warning)
            }
            .accessibilityElement(children: .combine)
        }
    }
}

struct KeyValue: View {
    var key: String
    var value: String
    var tone: Tone?

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(key).foregroundStyle(Theme.textSecondary)
            Spacer(minLength: 12)
            Text(value).font(Theme.mono(14)).foregroundStyle(tone.map { Theme.color($0) } ?? .white).multilineTextAlignment(.trailing)
        }
        .font(.subheadline)
    }
}

struct Notice: View {
    var text: String
    var tone: Tone = .warn

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: tone == .bad ? "exclamationmark.octagon.fill" : tone == .good ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                .foregroundStyle(Theme.color(tone))
            Text(text).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.color(tone).opacity(0.1)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Theme.color(tone).opacity(0.4), lineWidth: 1))
    }
}

/// Meter 0–100 (guard).
struct Meter: View {
    var label: String
    var value: Double
    var tone: Tone

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(label).font(.subheadline).foregroundStyle(Theme.textSecondary)
                Spacer()
                Text("\(Int(value.rounded()))/100").font(Theme.mono(13))
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.08))
                    Capsule().fill(Theme.color(tone)).frame(width: max(4, geo.size.width * min(100, max(0, value)) / 100))
                }
            }
            .frame(height: 6)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) : \(Int(value.rounded())) sur 100")
    }
}

/// Loading / error / content of an asynchronous call.
enum Loadable<T> {
    case idle, loading, failed(String), loaded(T)
    var value: T? { if case let .loaded(v) = self { return v }; return nil }
}

struct ErrorView: View {
    var message: String
    var retry: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            Notice(text: message, tone: .bad)
            Button("Réessayer", action: retry).buttonStyle(NeonButtonStyle(filled: false))
        }
    }
}

extension View {
    /// Screen background + dark navigation bar.
    func altimScreen() -> some View {
        background(AppBackground())
            .scrollContentBackground(.hidden)
            .toolbarBackground(Theme.background.opacity(0.9), for: .navigationBar)
    }
}

/// Small label in a capsule (tags of an event, filters): wraps inside a WrapLayout, never scrolls sideways.
struct TagChip: View {
    var text: String
    var color: Color = Theme.textSecondary
    var selected = false

    var body: some View {
        Text(text).font(.caption).foregroundStyle(selected ? Theme.cyan : color)
            .lineLimit(2)
            .multilineTextAlignment(.leading)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Color.white.opacity(selected ? 0.1 : 0.06), in: Capsule())
            .overlay(Capsule().strokeBorder(selected ? Theme.cyan : Color.clear, lineWidth: 1))
    }
}

/// A finding with its icon, a bold title and the detail (risk limits, crisis scenarios, configuration changes).
/// The meaning is written out and carried by the icon, never by the colour alone.
struct InsightRow<Extra: View>: View {
    var icon: String
    var tone: Tone
    var title: String
    var detail: String?
    @ViewBuilder var extra: Extra

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon).foregroundStyle(Theme.color(tone)).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                if let detail {
                    Text(detail).font(.footnote).foregroundStyle(.white.opacity(0.85)).fixedSize(horizontal: false, vertical: true)
                }
                extra
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.color(tone).opacity(0.08)))
        .accessibilityElement(children: .combine)
    }
}

extension InsightRow where Extra == EmptyView {
    init(icon: String, tone: Tone, title: String, detail: String?) {
        self.init(icon: icon, tone: tone, title: title, detail: detail) { EmptyView() }
    }
}

/// A choice among a few options that never runs off the screen: a segmented control while all its labels fit the
/// width at their full size, otherwise (iPhone SE at 320 pt, large Dynamic Type) a menu showing the current choice.
struct FittingPicker<Value: Hashable, Options: View>: View {
    var title: String
    @Binding var selection: Value
    @ViewBuilder var options: () -> Options
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        if dynamicTypeSize.isAccessibilitySize {
            menu
        } else {
            ViewThatFits(in: .horizontal) {
                Picker(title, selection: $selection, content: options).pickerStyle(.segmented)
                menu
            }
        }
    }

    private var menu: some View {
        Picker(title, selection: $selection, content: options).pickerStyle(.menu)
    }
}

/// Lays its children out left to right and wraps to a new line when the width runs out (never a horizontal scroll).
struct WrapLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
        let width = rows.map { $0.width }.max() ?? 0
        let height = rows.reduce(0) { $0 + $1.height } + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: proposal.width.map { min($0, width) } ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for i in row.items {
                let size = subviews[i].sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
                subviews[i].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: min(size.width, bounds.width), height: size.height))
                x += min(size.width, bounds.width) + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row { var items: [Int] = []; var width: CGFloat = 0; var height: CGFloat = 0 }

    private func arrange(width: CGFloat, subviews: Subviews) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for i in subviews.indices {
            let size = subviews[i].sizeThatFits(ProposedViewSize(width: width, height: nil))
            let w = min(size.width, width)
            if !row.items.isEmpty && row.width + spacing + w > width {
                rows.append(row)
                row = Row()
            }
            row.width += (row.items.isEmpty ? 0 : spacing) + w
            row.height = max(row.height, size.height)
            row.items.append(i)
        }
        if !row.items.isEmpty { rows.append(row) }
        return rows
    }
}

/// Discreet line under amounts: "1 $ = 0,8819 € · Yahoo Finance, 17:15", or why they are still in dollars (euros
/// asked but no rate: never a made-up conversion). Nothing when dollars are chosen.
struct FxNote: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        // Read here so that the line follows the rate and the setting.
        let _ = (model.fx, model.currency)
        if let text = Money.note() {
            Text(text).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}
