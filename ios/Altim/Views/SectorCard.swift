import SwiftUI
import AltimKit

/// "Exposition sectorielle" of Mes avoirs (GET /api/sectors): stocks by sector (Nasdaq / SEC), cryptos as their own
/// block, stocks without a known sector as "Secteur inconnu". Same content as the web (SectorCard.tsx): one thin
/// single-colour bar per block, values in text colours, warnings, source line. Stacked rows: nothing scrolls sideways.
/// The iPhone has no cash field: no "Liquidités" block, percentages over the lines (said in the source line).
struct SectorCard: View {
    @Environment(AppModel.self) private var model
    let lines: [Sectors.Line]
    @State private var state: Loaded?

    private struct Loaded {
        var key: String
        var items: [String: SectorItem]?
        var error: String?
    }

    private static let bar = Color(red: 0x39 / 255, green: 0x87 / 255, blue: 0xE5 / 255)

    private var stocks: [String] { Sectors.stockSymbols(lines) }
    private var key: String { stocks.joined(separator: ",") }

    var body: some View {
        let key = self.key
        Card(title: "Exposition sectorielle") {
            if let state, state.key == key {
                content(Sectors.exposure(lines, cash: 0, sectors: state.items,
                                         failure: state.error.map { "classement indisponible (\($0))" } ?? "classement sectoriel indisponible"))
            } else {
                Text("Lecture des secteurs de vos actions…").font(.footnote).foregroundStyle(Theme.textSecondary)
            }
        }
        .task(id: key) { await load(key) }
    }

    @ViewBuilder private func content(_ e: SectorExposure) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(e.blocks) { b in row(b, e) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Poids de chaque secteur dans le patrimoine")
        // The effective number of sectors is shown as a figure below.
        ForEach(e.insights.filter { $0.code != .sectorEffective }, id: \.code) { i in
            InsightRow(icon: i.level == .warning ? "exclamationmark.triangle.fill" : "info.circle",
                       tone: i.level == .warning ? .warn : .neutral, title: Sectors.text(i), detail: nil)
        }
        if let eff = e.effectiveSectors {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("Secteurs effectifs (actions classées)").font(.footnote).foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 4)
                Text(Format.plain(eff, digits: 1)).font(.footnote.weight(.semibold).monospacedDigit()).foregroundStyle(.white)
            }
        }
        if !e.unknown.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                ForEach(e.unknown, id: \.symbol) { u in
                    (Text(u.symbol).bold() + Text(" : \(u.reason)")).font(.caption).foregroundStyle(.white.opacity(0.85))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        Text(Sectors.footnote(e)).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    private func row(_ b: ExposureBlock, _ e: SectorExposure) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(b.label).font(.subheadline).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 4)
                Text(Sectors.pc(b.weight)).font(.subheadline.weight(.semibold).monospacedDigit()).foregroundStyle(.white).layoutPriority(1)
            }
            Rectangle().fill(Color.white.opacity(0.07)).frame(height: 6)
                .overlay(alignment: .leading) {
                    GeometryReader { g in
                        UnevenRoundedRectangle(bottomTrailingRadius: 3, topTrailingRadius: 3).fill(Self.bar)
                            .frame(width: max(2, g.size.width * min(100, max(0, b.weight)) / 100))
                    }
                }
                .clipShape(UnevenRoundedRectangle(bottomTrailingRadius: 3, topTrailingRadius: 3))
                .accessibilityHidden(true)
            if !b.symbols.isEmpty || b.stockWeight != nil {
                Text(b.symbols.joined(separator: ", ")
                     + (b.stockWeight.flatMap { w -> String? in e.stockValue < e.total ? " · \(Sectors.pc(w)) des actions" : nil } ?? ""))
                    .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func load(_ key: String) async {
        let stocks = self.stocks
        guard !stocks.isEmpty else { return state = Loaded(key: key, items: [:], error: nil) }
        guard let client = model.client else { return }
        do {
            let r = try await client.sectors(stocks)
            guard !Task.isCancelled else { return }
            state = Loaded(key: key, items: r.bySymbol, error: nil)
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            guard !Task.isCancelled else { return }
            state = Loaded(key: key, items: nil, error: error.localizedDescription)
        }
    }
}
