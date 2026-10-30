import SwiftUI
import AltimKit

/// « Sélection Altim »: every asset of a fixed list with its verdict; those in their up trend are held, each for a fixed
/// share of the money, the rest in cash; the rule replayed on the same assets against holding them all
/// (AltimKit Rotation.sleeves).
struct RotationView: View {
    @Environment(AppModel.self) private var model
    @State private var report: Loadable<RotationReport> = .idle

    var body: some View {
        @Bindable var model = model
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                FittingPicker(title: "Marché", selection: $model.selectionMarket) {
                    Text("Actions").tag(Kind.stock)
                    Text("Cryptos").tag(Kind.crypto)
                }
                switch report {
                case .idle, .loading:
                    Card(title: "Sélection Altim") {
                        ProgressView("Analyse de \(Rotation.universe[model.selectionMarket]?.count ?? 0) actifs…")
                            .frame(maxWidth: .infinity, minHeight: 100)
                    }
                case let .failed(m):
                    ErrorView(message: m) { Task { await load() } }
                case let .loaded(r):
                    content(r)
                }
                Text("Altim ne passe aucun ordre : une règle testée sur le passé, jamais une certitude.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
            .padding(16)
        }
        .altimScreen()
        .navigationTitle("Sélection")
        .navigationDestination(for: Asset.self) { AssetDetailView(asset: $0) }
        .refreshable { await load() }
        .task(id: model.selectionMarket) { await load() }
        .onDisappear { model.selectionAssets = [] }
    }

    @ViewBuilder private func content(_ r: RotationReport) -> some View {
        Card(title: "À détenir maintenant · \(r.selected.count) sur \(r.scanned)", glow: Theme.buy) {
            Text("Les \(r.kind == .crypto ? "cryptos" : "actions") en tendance de fond haussière avec un momentum positif (\(r.kind == .crypto ? "\(r.lookback) jours" : "3 mois")), chacune pour 1/\(r.top) du budget ; le reste (\(Int((Double(r.top - r.selected.count) / Double(max(1, r.top)) * 100).rounded())) %) en liquidités. Vérifié chaque jour.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if r.selected.isEmpty {
                Notice(text: "Aucun actif de ce marché n'est en tendance haussière : la règle reste entièrement en liquidités.", tone: .warn)
            }
            ForEach(r.selected) { p in NavigationLink(value: p.asset) { row(p) }.buttonStyle(.plain) }
        }
        if let t = r.test { testCard(t, r) }
        Card(title: "Les autres, du plus fort au plus faible") {
            ForEach(r.picks.filter { !$0.selected }) { p in NavigationLink(value: p.asset) { row(p) }.buttonStyle(.plain) }
        }
        Card(title: "Comment ça marche") {
            ForEach(r.notes, id: \.self) { n in
                Text("• \(n)").font(.caption).foregroundStyle(.white.opacity(0.85)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func row(_ p: RotationReport.Pick) -> some View {
        let tick = model.live.price(p.asset)
        return VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text("\(p.rank)").font(Theme.mono(12)).foregroundStyle(Theme.textSecondary).frame(width: 22, alignment: .leading)
                VStack(alignment: .leading, spacing: 2) {
                    Text(p.symbol).font(Theme.mono(15, weight: .bold)).foregroundStyle(.white)
                    Text(p.name).font(.caption).foregroundStyle(Theme.textSecondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(Format.price(tick?.price ?? p.price)).font(Theme.mono(13)).lineLimit(1).minimumScaleFactor(0.6)
                    if let m = p.momentum {
                        Text(Format.percent(m * 100, digits: 1)).font(Theme.mono(12)).foregroundStyle(Theme.color(forChange: m))
                    }
                }
                if p.selected { Image(systemName: "checkmark.seal.fill").foregroundStyle(Theme.buy).accessibilityLabel("Sélectionné") }
            }
            Text(p.reason).font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    private func testCard(_ t: RotationReport.Test, _ r: RotationReport) -> some View {
        let better = t.ret > t.basket
        return Card(title: "Rejoué sur le passé", glow: better ? Theme.buy : Theme.warning) {
            Text("Du \(Format.date(t.from)) au \(Format.date(t.to)), sur ces actifs : la règle (une part fixe par actif, investie seulement en tendance) contre tout garder. 0,1 % de frais par ordre.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 10) {
                tile("Parts Altim", t.ret, t.drawdown, Theme.magenta)
                tile("Tout garder", t.basket, t.basketDrawdown, .white)
            }
            TimeMachineChart(points: t.curve).frame(height: 160)
            KeyValue(key: "1re moitié", value: "\(TimeMachineCard.signed(t.firstHalf)) contre \(TimeMachineCard.signed(t.firstHalfBasket))",
                     tone: t.firstHalf > t.firstHalfBasket ? .good : .warn)
            KeyValue(key: "2de moitié", value: "\(TimeMachineCard.signed(t.secondHalf)) contre \(TimeMachineCard.signed(t.secondHalfBasket))",
                     tone: t.secondHalf > t.secondHalfBasket ? .good : .warn)
        }
    }

    private func tile(_ title: String, _ ret: Double, _ dd: Double, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption.weight(.semibold)).foregroundStyle(color)
            Text(TimeMachineCard.signed(ret)).font(Theme.mono(20, weight: .bold)).foregroundStyle(Theme.color(forChange: ret))
                .lineLimit(1).minimumScaleFactor(0.6)
            Text("pire creux \(TimeMachineCard.signed(dd))").font(.caption2).foregroundStyle(Theme.textSecondary)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.05)))
        .accessibilityElement(children: .combine)
    }

    private func load() async {
        guard let client = model.client else { return }
        report = .loading
        do {
            let r = try await client.rotation(model.selectionMarket)
            report = .loaded(r)
            model.selectionAssets = r.selected.map(\.asset)
        } catch {
            report = .failed(error.localizedDescription)
        }
    }
}
