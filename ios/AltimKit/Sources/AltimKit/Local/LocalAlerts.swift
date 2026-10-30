import Foundation

/// "Puis-je acheter maintenant ?" on the iPhone (core/src/engine/alerts.rs `buy_alert`): an asset is buyable when a
/// measured reason says so (4 h signal ACHAT / ACHAT FORT, or the price in a Fibonacci buy zone) and nothing blocks it.
///
/// The server also waits for its full decision before announcing a buy; that decision is not computed on the iPhone
/// yet, so the standalone rule is stricter instead: the data must be highly reliable (3 independent sources at least),
/// and the alert says so. The market guard is computed on the iPhone: a shock blocks, an agitated market warns.
enum LocalAlertRule {
    struct Input {
        var asset: Asset
        var price: Double?
        var signal: LocalSignal?
        var reliability: Reliability?
        var zones: [FibZone]
        /// The guard's shock level ("calm", "agitated", "shock") and background trend ("up", "down", "range").
        var shock: String? = nil
        var trend: String? = nil
    }

    struct Output: Equatable {
        var buy: Bool, strong: Bool
        var reasons: [String], blockers: [String], cautions: [String]
        var key: String, title: String, body: String
    }

    static let noGuard = "Décision complète pas encore calculée sur l'iPhone : taille prudente."

    static func evaluate(_ a: Input) -> Output {
        var reasons: [String] = [], blockers: [String] = [], cautions: [String] = [], parts: [String] = []
        let signalBuy = a.signal?.action.isBuy ?? false
        if let s = a.signal, signalBuy {
            reasons.append("Signal \(s.action == .strongBuy ? "ACHAT FORT" : "ACHAT") en 4 h (confiance \(Int(s.confidence.rounded())) %).")
            parts.append("signal")
        }
        // Longest horizon first: a long-term zone matters more than a short one.
        let order = ["long": 0, "medium": 1, "short": 2]
        let hit = a.zones.sorted { order[$0.horizon, default: 3] < order[$1.horizon, default: 3] }
            .first { ($0.status == "inZone" || $0.status == "golden") && $0.zone != nil }
        if let hit, let zone = hit.zone {
            let band = hit.status == "golden" ? hit.golden ?? zone : zone
            reasons.append("Prix dans la \(hit.status == "golden" ? "zone d'or" : "zone d'achat") \(hit.label.lowercased()) (\(Money.money(band.to)) – \(Money.money(band.from))).")
            parts.append("zone:\(hit.horizon)")
        }
        switch a.reliability?.level {
        case "low", nil: blockers.append("Sources de prix en désaccord : données peu fiables.")
        case "medium": blockers.append("Fiabilité moyenne des données : sans serveur, Altim n'annonce un achat que sur des données très fiables.")
        default: break
        }
        if a.shock == "shock" { blockers.append("Garde-fou : risque de choc élevé, mieux vaut attendre.") }
        if let hit, let price = a.price, let inv = hit.invalidation, price < inv {
            blockers.append("Le plus bas du mouvement est cassé : zone invalidée.")
        }
        if a.trend == "down" || (a.trend == nil && (a.signal?.factors.first { $0.name == "UT supérieure" }?.score ?? 0) < 0) {
            cautions.append("Tendance de fond baissière : taille réduite.")
        }
        if a.shock == "agitated" { cautions.append("Marché agité : stop plus large.") }
        cautions.append(noGuard)
        let buy = !reasons.isEmpty && blockers.isEmpty
        let strong = buy && signalBuy && hit != nil
        let stop = hit?.invalidation.map { " Invalidé sous \(Money.money($0))." } ?? ""
        let price = a.price.map { " à \(Money.money($0))" } ?? ""
        let symbol = a.asset.symbol
        return Output(buy: buy, strong: strong, reasons: reasons, blockers: blockers, cautions: cautions,
                      key: buy ? parts.joined(separator: "+") : "",
                      title: buy ? "\(symbol) : achat \(strong ? "conseillé" : "possible")\(price)" : "\(symbol) : pas d'achat pour l'instant",
                      body: buy ? (reasons + cautions).joined(separator: " ") + stop : blockers.first ?? "Ni signal d'achat ni prix dans une zone d'achat.")
    }

    /// `/api/alerts` item (the server's JSON: an error item carries only the asset and `error`).
    static func json(_ a: Asset, _ o: Output?, price: Double?, asOf: Double, error: String? = nil) -> [String: Any] {
        var d: [String: Any] = ["symbol": a.symbol, "kind": a.kind.rawValue, "name": a.name]
        guard let o else {
            d["error"] = error ?? "données indisponibles"
            return d
        }
        if let price { d["price"] = price }
        d["asOf"] = asOf
        d["buy"] = o.buy
        d["strong"] = o.strong
        d["reasons"] = o.reasons
        d["blockers"] = o.blockers
        d["cautions"] = o.cautions
        d["key"] = o.key
        d["title"] = o.title
        d["body"] = o.body
        return d
    }
}

extension LocalBackend {
    /// The rule on each asset: 4 h signal (confirmed by the day), zones, reliability, consensus price.
    func alerts(_ list: [Asset]) async -> Data {
        let prices = await quotes(list)
        let items: [[String: Any]] = await withTaskGroup(of: (Int, [String: Any]).self) { g in
            for (i, a) in list.enumerated() {
                g.addTask {
                    let price = prices.first { $0.symbol == a.symbol && $0.kind == a.kind }?.price
                    do {
                        async let h4 = self.snapshot(a, .h4)
                        async let d1 = try? self.snapshot(a, .d1)
                        async let z = try? self.zones(a)
                        async let g = try? self.guardReport(a)
                        let s = try await h4
                        let guardReport = await g
                        let input = LocalAlertRule.Input(asset: a, price: price ?? s.candles.last?.close, signal: s.signal(higher: await d1?.candles),
                                                         reliability: s.reliability, zones: await z?.zones ?? [],
                                                         shock: guardReport?.shock.level, trend: guardReport?.regime.trend)
                        return (i, LocalAlertRule.json(a, LocalAlertRule.evaluate(input), price: input.price, asOf: nowMs()))
                    } catch {
                        return (i, LocalAlertRule.json(a, nil, price: nil, asOf: nowMs(), error: (error as? MarketError)?.message ?? error.localizedDescription))
                    }
                }
            }
            var out: [(Int, [String: Any])] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map(\.1)
        }
        return (try? JSONSerialization.data(withJSONObject: items)) ?? Data("[]".utf8)
    }
}
