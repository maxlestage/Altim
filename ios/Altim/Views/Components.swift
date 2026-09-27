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

struct ActionBadge: View {
    var action: Action
    var body: some View { Badge(text: action.label, tone: action.tone) }
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
