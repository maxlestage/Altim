import SwiftUI
import AltimKit

/// "Détection d'anomalies" on the asset page (GET /api/anomalies): unusual readings (volume, price/volume, z-score; open
/// interest, funding, long/short ratio and liquidations for cryptos), each with its threshold and what it may mean.
/// Same texts as the web (AnomaliesCard.tsx). Stacked rows: nothing scrolls sideways.
struct AnomaliesCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @State private var report: AnomalyReport?
    @State private var error: String?

    var body: some View {
        Card(title: "Détection d'anomalies") {
            if let error { Notice(text: error, tone: .warn) }
            if report == nil && error == nil {
                ProgressView().frame(maxWidth: .infinity, minHeight: 60).accessibilityLabel("Chargement des anomalies")
            }
            if let r = report { content(r) }
        }
        .task(id: asset.id) { await load() }
    }

    @ViewBuilder private func content(_ r: AnomalyReport) -> some View {
        if r.anomalies.isEmpty {
            Text("Rien d'inhabituel sur les mesures ci-dessous\(r.session.map { " (séance du \(Self.day($0)))" } ?? "").")
                .font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
        } else {
            ForEach(r.anomalies) { a in
                InsightRow(icon: "exclamationmark.triangle.fill", tone: a.severity == .high ? .bad : .warn, title: "⚠️ \(a.title)", detail: a.meaning) {
                    Text(a.measured).font(.caption.monospacedDigit()).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                    Text("Source : \(a.source)").font(.caption2).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        ForEach(r.errors, id: \.self) { Notice(text: $0, tone: .warn) }
        if !r.normal.isEmpty {
            DisclosureGroup("Mesures dans la normale · \(r.normal.count)") {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(r.normal) { a in
                        (Text(a.title).foregroundStyle(.white) + Text(" \(a.measured)").foregroundStyle(Theme.textSecondary))
                            .font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(.top, 4)
            }
            .font(.footnote)
            .tint(.white)
        }
        if let d = r.derivatives { derivatives(d) }
        Text("Une anomalie est un écart mesuré, pas une prévision : son sens reste à confirmer.\(r.source.isEmpty ? "" : " \(r.source).")")
            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
    }

    @ViewBuilder private func derivatives(_ d: Derivatives) -> some View {
        Text("Dérivés et gros mouvements").font(.subheadline.weight(.semibold)).foregroundStyle(.white).padding(.top, 4)
        ForEach(d.errors, id: \.self) { Notice(text: $0, tone: .warn) }
        if let l = d.liquidations {
            VStack(alignment: .leading, spacing: 4) {
                DecisionRow(key: "Liquidations \(l.complete ? "24 h" : "\(Self.fr(l.hours)) h (lecture partielle)")", value: Opportunities.compactUsd(l.longUsd + l.shortUsd))
                DecisionRow(key: "Acheteurs liquidés (\(l.longCount))",
                            value: Opportunities.compactUsd(l.longUsd) + (l.longShare.map { " · \(Int(($0 + 0.5).rounded(.down))) %" } ?? ""), tone: .bad)
                DecisionRow(key: "Vendeurs liquidés (\(l.shortCount))", value: Opportunities.compactUsd(l.shortUsd), tone: .good)
                if let big = l.largest {
                    DecisionRow(key: "Plus grosse",
                                value: "\(Opportunities.compactUsd(big.usd)) · \(big.long ? "acheteur" : "vendeur") à \(Money.moneyFmt(big.price, sep: " ") { Self.fr($0, 2) }) · \(Self.time(big.time))")
                }
                Text(l.scope).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            .padding(10)
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.04)))
        }
        if let oi = d.openInterest {
            DecisionRow(key: "Open interest", value: Opportunities.compactUsd(oi.usd)
                        + (oi.change24h.map { " · \(Opportunities.signed($0)) 24 h" } ?? "") + (oi.change7d.map { " · \(Opportunities.signed($0)) 7 j" } ?? ""))
        }
        if let f = d.funding {
            DecisionRow(key: "Funding (dernier règlement\(f.periodHours.map { ", toutes les \(Self.fr($0, 0)) h" } ?? ""))",
                        value: "\(Opportunities.signed(f.rate, 4)) · habituel \(Opportunities.signed(f.p5, 4)) à \(Opportunities.signed(f.p95, 4))")
        }
        if let ls = d.longShort {
            DecisionRow(key: "Ratio comptes acheteurs / vendeurs", value: "\(Self.fr(ls.ratio, 2)) · habituel \(Self.fr(ls.p5, 2)) à \(Self.fr(ls.p95, 2))")
        }
        Text("Source : \(d.source) ; plages habituelles = 5 à 95 % des valeurs récentes (funding : ≈ 100 derniers règlements ; ratio : 30 j).")
            .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        ForEach(d.notCovered, id: \.label) { n in
            (Text("Non couvert — \(n.label)").bold() + Text(" : \(n.reason).")).font(.caption).foregroundStyle(.white.opacity(0.85))
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private static func fr(_ v: Double, _ d: Int = 1) -> String { Format.plain(v, digits: d) }

    /// "28/09/2026" (the web's toLocaleDateString).
    private static func day(_ ms: Double) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "dd/MM/yyyy"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }

    private static func time(_ ms: Double) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.dateFormat = "d MMM 'à' HH:mm"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }

    private func load() async {
        guard let client = model.client else { return }
        do {
            report = try await client.anomalies(asset)
            error = nil
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            if report == nil { self.error = error.localizedDescription }
        }
    }
}
