import ActivityKit
import AppIntents
import SwiftUI
import WidgetKit
import AltimKit

@main
struct AltimWidgetsBundle: WidgetBundle {
    var body: some Widget {
        VerdictWidget()
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

// MARK: - Home screen and lock screen widget

/// What the widget shows: up to 4 assets (symbols or names, found like the app's search) and the currency.
struct VerdictWidgetIntent: WidgetConfigurationIntent {
    static var title: LocalizedStringResource = "Verdict Altim"
    static var description = IntentDescription("Le prix et le verdict Altim de vos actifs, calculés par le widget lui-même à partir des cours publics.")

    @Parameter(title: "Actif 1", default: "BTC") var first: String
    @Parameter(title: "Actif 2", default: "ETH") var second: String?
    @Parameter(title: "Actif 3", default: "AAPL") var third: String?
    @Parameter(title: "Actif 4", default: "NVDA") var fourth: String?
    @Parameter(title: "Montants en dollars", default: false) var dollars: Bool

    var symbols: [String] {
        [first, second, third, fourth].compactMap { $0?.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }
}

struct VerdictEntry: TimelineEntry {
    struct Row: Identifiable {
        var symbol: String
        var name: String
        var price: Double?
        var change: Double?
        var action: Action?
        var id: String { symbol }
    }

    var date: Date
    var rows: [Row]
    var message: String?

    static let sample = VerdictEntry(date: .now, rows: [
        .init(symbol: "BTC", name: "Bitcoin", price: 77_000, change: 1.2, action: .buy),
        .init(symbol: "ETH", name: "Ethereum", price: 2_230, change: -0.8, action: .hold),
        .init(symbol: "AAPL", name: "Apple", price: 348, change: 0.4, action: .buy),
        .init(symbol: "NVDA", name: "NVIDIA", price: 205, change: -2.1, action: .sell),
    ], message: nil)
}

/// Reads everything itself (no server, no data shared with the app): the assets found by the search, their daily
/// verdict and consensus price, the EUR/USD rate. Again every 30 minutes (WidgetKit decides the exact moment).
struct VerdictProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> VerdictEntry { .sample }

    func snapshot(for configuration: VerdictWidgetIntent, in context: Context) async -> VerdictEntry {
        context.isPreview ? .sample : await entry(configuration)
    }

    func timeline(for configuration: VerdictWidgetIntent, in context: Context) async -> Timeline<VerdictEntry> {
        let e = await entry(configuration)
        return Timeline(entries: [e], policy: .after(Date().addingTimeInterval(e.message == nil ? 30 * 60 : 10 * 60)))
    }

    private func entry(_ c: VerdictWidgetIntent) async -> VerdictEntry {
        let client = AltimClient.standalone(local: LocalBackend(bot: false))
        if c.dollars {
            Money.setDisplay(.usd, nil)
        } else {
            Money.setDisplay(.eur, (try? await client.fx())?.rate)
        }
        var assets: [Asset] = []
        for text in c.symbols.prefix(4) {
            guard let found = try? await client.search(text, limit: 5) else { continue }
            if let item = found.first(where: { $0.symbol == text.uppercased() }) ?? found.first, !assets.contains(item.asset) {
                assets.append(item.asset)
            }
        }
        guard !assets.isEmpty else {
            return VerdictEntry(date: .now, rows: [], message: "Actifs introuvables : modifiez le widget (appui long).")
        }
        guard let rows = try? await client.radar(assets, interval: Timeframe.d1.rawValue) else {
            return VerdictEntry(date: .now, rows: assets.map { .init(symbol: $0.symbol, name: $0.name) }, message: "Réseau indisponible : nouvel essai bientôt.")
        }
        return VerdictEntry(date: .now, rows: assets.map { a in
            let r = rows.first { $0.symbol == a.symbol && $0.kind == a.kind }
            return .init(symbol: a.symbol, name: a.name, price: r?.price, change: r?.change, action: r?.signal?.action)
        }, message: nil)
    }
}

struct VerdictWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "altim.verdict", intent: VerdictWidgetIntent.self, provider: VerdictProvider()) { entry in
            VerdictWidgetView(entry: entry)
                .containerBackground(Palette.background, for: .widget)
        }
        .configurationDisplayName("Verdict Altim")
        .description("Prix et verdict de la règle Altim pour vos actifs, calculés par le widget à partir des cours publics (le veto du bot entraîné est dans l'app). Altim ne passe aucun ordre.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge, .accessoryRectangular])
    }
}

private struct VerdictWidgetView: View {
    @Environment(\.widgetFamily) private var family
    var entry: VerdictEntry

    var body: some View {
        switch family {
        case .systemSmall: small
        case .accessoryRectangular: lockScreen
        default: list
        }
    }

    private var small: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let r = entry.rows.first {
                Text(r.symbol).font(.headline.monospaced()).foregroundStyle(.white)
                Text(r.name).font(.caption2).foregroundStyle(Palette.muted).lineLimit(1)
                Spacer(minLength: 2)
                Text(r.price.map { Format.price($0) } ?? "—").font(.title3.monospaced().bold()).foregroundStyle(.white)
                    .minimumScaleFactor(0.5).lineLimit(1)
                Text(Format.percent(r.change)).font(.caption.monospaced()).foregroundStyle(Palette.change(r.change))
                VerdictChip(action: r.action)
            } else {
                Text("Altim").font(.headline).foregroundStyle(Palette.cyan)
            }
            if let m = entry.message { Text(m).font(.caption2).foregroundStyle(Palette.muted).lineLimit(2) }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: family == .systemLarge ? 12 : 6) {
            HStack {
                Text("Verdict Altim").font(.caption.bold()).foregroundStyle(Palette.cyan)
                Spacer()
                Text(entry.date, style: .time).font(.caption2).foregroundStyle(Palette.muted)
            }
            ForEach(entry.rows) { r in
                HStack(spacing: 8) {
                    VStack(alignment: .leading, spacing: 0) {
                        Text(r.symbol).font(.subheadline.monospaced().bold()).foregroundStyle(.white)
                        if family == .systemLarge { Text(r.name).font(.caption2).foregroundStyle(Palette.muted).lineLimit(1) }
                    }
                    Spacer(minLength: 4)
                    VStack(alignment: .trailing, spacing: 0) {
                        Text(r.price.map { Format.price($0) } ?? "—").font(.caption.monospaced()).foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.6)
                        Text(Format.percent(r.change)).font(.caption2.monospaced()).foregroundStyle(Palette.change(r.change))
                    }
                    VerdictChip(action: r.action).frame(minWidth: 62, alignment: .trailing)
                }
            }
            if let m = entry.message { Text(m).font(.caption2).foregroundStyle(Palette.muted).lineLimit(2) }
            Spacer(minLength: 0)
        }
    }

    private var lockScreen: some View {
        VStack(alignment: .leading, spacing: 1) {
            ForEach(entry.rows.prefix(3)) { r in
                HStack(spacing: 4) {
                    Text(r.symbol).font(.caption2.monospaced().bold())
                    Spacer(minLength: 2)
                    Text(Format.percent(r.change)).font(.caption2.monospaced())
                    Text(r.action.map(VerdictChip.short) ?? "—").font(.caption2.bold())
                }
            }
        }
    }
}

private struct VerdictChip: View {
    var action: Action?

    static func short(_ a: Action) -> String {
        switch a {
        case .strongBuy: return "ACHAT+"
        case .buy: return "ACHAT"
        case .hold: return "ATTENDRE"
        case .sell, .strongSell: return "SORTIR"
        }
    }

    private var color: Color {
        switch action {
        case .strongBuy?, .buy?: return Palette.buy
        case .sell?, .strongSell?: return Palette.sell
        default: return Palette.cyan
        }
    }

    var body: some View {
        Text(action.map(Self.short) ?? "…")
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .foregroundStyle(color)
            .background(Capsule().fill(color.opacity(0.15)))
            .lineLimit(1)
            .fixedSize()
    }
}
