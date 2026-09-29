import ActivityKit
import SwiftUI
import WidgetKit
import AltimKit

@main
struct AltimWidgetsBundle: WidgetBundle {
    var body: some Widget {
        PriceLiveActivity()
    }
}

private enum Palette {
    static let background = Color(red: 0.02, green: 0.03, blue: 0.07)
    static let cyan = Color(red: 0.0, green: 0.94, blue: 1.0)
    static let buy = Color(red: 0.22, green: 1.0, blue: 0.53)
    static let sell = Color(red: 1.0, green: 0.23, blue: 0.36)
    static let muted = Color.white.opacity(0.6)
    static func change(_ v: Double?) -> Color { (v ?? 0) >= 0 ? buy : sell }
}

/// Price of one asset on the lock screen and in the Dynamic Island, with Altim's buy verdict.
struct PriceLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: PriceActivityAttributes.self) { context in
            LockScreenView(attributes: context.attributes, state: context.state)
                .activityBackgroundTint(Palette.background.opacity(0.92))
                .activitySystemActionForegroundColor(Palette.cyan)
        } dynamicIsland: { context in
            let s = context.state
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(context.attributes.symbol).font(.headline.monospaced()).foregroundStyle(.white)
                        Text(context.attributes.name).font(.caption2).foregroundStyle(Palette.muted).lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    VStack(alignment: .trailing, spacing: 2) {
                        Text(s.priceText ?? Format.price(s.price)).font(.headline.monospaced()).foregroundStyle(.white).minimumScaleFactor(0.6)
                        Text(Format.percent(s.change)).font(.caption.monospaced()).foregroundStyle(Palette.change(s.change))
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack(alignment: .top, spacing: 8) {
                        Verdict(buy: s.buy, signal: s.signal)
                        Text(s.note).font(.caption2).foregroundStyle(Palette.muted).lineLimit(2)
                        Spacer(minLength: 0)
                    }
                }
            } compactLeading: {
                HStack(spacing: 4) {
                    Circle().fill(s.buy ? Palette.buy : Palette.muted).frame(width: 6, height: 6)
                    Text(context.attributes.symbol).font(.caption2.monospaced().bold()).foregroundStyle(.white)
                }
            } compactTrailing: {
                Text(s.priceText ?? Format.price(s.price)).font(.caption2.monospaced()).foregroundStyle(Palette.change(s.change)).minimumScaleFactor(0.5)
            } minimal: {
                Image(systemName: s.buy ? "arrow.up.circle.fill" : "chart.line.uptrend.xyaxis")
                    .foregroundStyle(s.buy ? Palette.buy : Palette.cyan)
            }
            .keylineTint(Palette.cyan)
        }
    }
}

private struct Verdict: View {
    var buy: Bool
    var signal: String?

    var body: some View {
        Text(buy ? "ACHAT POSSIBLE" : (signal ?? "PAS D'ACHAT"))
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .foregroundStyle(buy ? Palette.buy : Palette.cyan)
            .background(Capsule().fill((buy ? Palette.buy : Palette.cyan).opacity(0.15)))
    }
}

private struct LockScreenView: View {
    var attributes: PriceActivityAttributes
    var state: PriceActivityAttributes.ContentState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(attributes.symbol).font(.headline.monospaced()).foregroundStyle(.white)
                    Text(attributes.name).font(.caption).foregroundStyle(Palette.muted).lineLimit(1)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 2) {
                    Text(state.priceText ?? Format.price(state.price)).font(.title3.monospaced().bold()).foregroundStyle(.white)
                    Text(Format.percent(state.change)).font(.caption.monospaced()).foregroundStyle(Palette.change(state.change))
                }
            }
            HStack(alignment: .top, spacing: 8) {
                Verdict(buy: state.buy, signal: state.signal)
                Text(state.note).font(.caption2).foregroundStyle(Palette.muted).lineLimit(2)
                Spacer(minLength: 0)
            }
            Text("Altim · mis à jour \(state.updated, style: .relative)").font(.caption2).foregroundStyle(Palette.muted)
        }
        .padding(14)
    }
}
