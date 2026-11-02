import Foundation

/// "Puis-je acheter maintenant ?" on the iPhone, on the rule chosen by its measured results (TrendEngine): buyable
/// only in an up background trend with a positive momentum; the best moment (advised) is a short pull-back inside it
/// (a Fibonacci buy zone is shown as information). Blocked by unreliable data, a market shock or a broken zone.
enum LocalAlertRule {
    struct Input {
        var asset: Asset
        var price: Double?
        var verdict: TrendEngine.Verdict?
        var reliability: Reliability?
        var zones: [FibZone]
        /// The guard's shock level ("calm", "agitated", "shock").
        var shock: String? = nil
        /// The stock's next quarterly results, when known.
        var earnings: EarningsEvent? = nil
        /// The next rate decision of the Fed (ms), when known.
        var fedDecision: Double? = nil
        var now: Date = Date()
    }

    struct Output: Equatable {
        var buy: Bool, strong: Bool
        var reasons: [String], blockers: [String], cautions: [String]
        var key: String, title: String, body: String
    }

    static func evaluate(_ a: Input) -> Output {
        var reasons: [String] = [], blockers: [String] = [], cautions: [String] = [], parts: [String] = []
        let state = a.verdict?.state ?? .unknown
        // The trained bot's veto (BotEngine.apply) turns an up-trend verdict into ATTENDRE or VENTE: no purchase then.
        let vetoed = a.verdict.map { $0.action != .buy && $0.action != .strongBuy } ?? false
        let inTrend = (state == .trend || state == .pullback) && !vetoed
        if let v = a.verdict, inTrend {
            reasons.append(v.text)
            parts.append(state == .pullback ? "repli" : "tendance")
        }
        // Longest horizon first: a long-term zone matters more than a short one.
        let order = ["long": 0, "medium": 1, "short": 2]
        let hit = a.zones.sorted { order[$0.horizon, default: 3] < order[$1.horizon, default: 3] }
            .first { ($0.status == "inZone" || $0.status == "golden") && $0.zone != nil }
        if let hit, let zone = hit.zone, inTrend {
            let band = hit.status == "golden" ? hit.golden ?? zone : zone
            reasons.append("Prix dans la \(hit.status == "golden" ? "zone d'or" : "zone d'achat") \(hit.label.lowercased()) (\(Money.money(band.to)) – \(Money.money(band.from))).")
            parts.append("zone:\(hit.horizon)")
        }
        switch state {
        case .down: blockers.append("Tendance de fond baissière et momentum négatif : rester à l'écart.")
        case .mixed: blockers.append("Signaux contraires (tendance de fond et momentum ne s'accordent pas) : ne pas ouvrir de position.")
        case .unknown: blockers.append("Historique trop court pour juger la tendance de fond.")
        default:
            if vetoed, a.verdict?.stale == true {
                blockers.append("Données de cours en retard (dernière clôture trop ancienne) : pas d'achat avant leur mise à jour.")
            } else if vetoed, a.verdict?.market == false {
                blockers.append("Tendance haussière, mais \(a.asset.kind == .crypto ? "le Bitcoin (le marché crypto)" : "le S&P 500 (le marché actions)") n'est pas en tendance haussière : pas d'achat (filtre de marché).")
            } else if vetoed, let p = a.verdict?.bot {
                blockers.append("Tendance haussière, mais le bot entraîné prévoit \(p >= 0 ? "+" : "−")\(JSFormat.fr(abs(p) * 100, min: 1, max: 1)) % sur 20 jours : pas d'achat.")
            }
        }
        if a.reliability?.level == "low" { blockers.append("Sources de prix en désaccord : données peu fiables.") }
        if a.shock == "shock" { blockers.append("Garde-fou : risque de choc élevé, mieux vaut attendre.") }
        if let hit, inTrend, let price = a.price, let inv = hit.invalidation, price < inv {
            blockers.append("Le plus bas du mouvement est cassé : zone invalidée.")
        }
        if a.shock == "agitated" { cautions.append("Marché agité : taille réduite, stop plus large.") }
        if a.reliability?.level == "medium" { cautions.append("Fiabilité moyenne des données : taille réduite.") }
        if let f = a.fedDecision {
            // Calendar days in New York's terms are close enough with UTC days: the decision is at 18:00 UTC.
            let dayNow = (a.now.timeIntervalSince1970 * 1000 / 86_400_000).rounded(.down), dayFed = (f / 86_400_000).rounded(.down)
            let d = Int(dayFed - dayNow)
            if (0...3).contains(d) {
                let fmt = DateFormatter()
                fmt.locale = Locale(identifier: "fr_FR"); fmt.timeZone = TimeZone(identifier: "UTC"); fmt.dateFormat = "d MMMM"
                let when = d == 0 ? "aujourd'hui" : d == 1 ? "demain" : "dans \(d) jours"
                cautions.append("Décision de taux de la Fed \(when) (\(fmt.string(from: Date(timeIntervalSince1970: f / 1000))), annoncée à 14 h heure de New York) : au labo, ce jour-là et le lendemain ont bougé en moyenne 15 à 20 % de plus que les autres jours.")
            }
        }
        if let e = a.earnings, (0...7).contains(e.daysAway(now: a.now)) {
            let d = e.daysAway(now: a.now)
            cautions.append("Résultats trimestriels \(d == 0 ? "aujourd'hui" : d == 1 ? "demain" : "dans \(d) jours") (\(e.dayText)) : le cours peut bouger fortement, taille prudente.")
        }
        if state == .trend { cautions.append("Pas de repli en cours : entrer en plusieurs fois a réduit les pires cas au test (10 % pires à 40 jours −33 % contre −37 % en crypto) mais rapporté un peu moins en moyenne : un choix de prudence.") }
        // The risk bot's size, on its calibrated fall (testRiskModelSizing, current rule: crypto rising market at 5 % of accepted risk
        // +20 % / worst fall −12 % instead of +59 % / −33 % for a full position).
        if inTrend, let r = a.verdict?.botRisk, let size = BotEngine.size(forFall: r), size < 100 {
            cautions.append("Bot Risque : pire baisse prévue −\(JSFormat.fr(abs(r) * 100, min: 0, max: 0)) % sur 20 jours, engager environ \(size) % de la somme prévue (risque accepté : \(Int(BotEngine.riskBudget * 100)) %).")
        }
        let buy = inTrend && blockers.isEmpty
        // « Conseillé » only for a short pull-back in the trend: it gave a better entry over the next week in all 8 lab
        // cases (testPullbackEdge). Being in a Fibonacci zone did not, consistently (testFibonacciZoneEdge: better in
        // half the cases, worse in the others) — it stays a piece of information.
        let strong = buy && state == .pullback
        let market = a.asset.kind == .crypto ? "le Bitcoin" : "le S&P 500"
        let stop = a.verdict?.ema200.map { " Sortie si le prix repasse sous \(Money.money($0)) (moyenne 200 jours), si le momentum devient négatif ou si \(market) quitte sa tendance haussière." } ?? ""
        let price = a.price.map { " à \(Money.money($0))" } ?? ""
        let symbol = a.asset.symbol
        return Output(buy: buy, strong: strong, reasons: reasons, blockers: blockers, cautions: cautions,
                      key: buy ? parts.joined(separator: "+") : "",
                      title: buy ? "\(symbol) : achat \(strong ? "conseillé" : "possible")\(price)" : "\(symbol) : pas d'achat pour l'instant",
                      body: buy ? (reasons + cautions).joined(separator: " ") + stop : blockers.first ?? "Pas de signal d'achat.")
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
    /// The rule on each asset: daily verdict (trend + momentum), zones, guard, reliability, consensus price.
    func alerts(_ list: [Asset]) async -> Data {
        let prices = await quotes(list)
        let results = await earnings(list.filter { $0.kind == .stock }.map(\.symbol))
        let fed = FedCalendar.next(await fedDecisions(), now: nowMs())
        let items: [[String: Any]] = await withTaskGroup(of: (Int, [String: Any]).self) { g in
            for (i, a) in list.enumerated() {
                g.addTask {
                    let price = prices.first { $0.symbol == a.symbol && $0.kind == a.kind }?.price
                    do {
                        async let d1 = self.snapshot(a, .d1)
                        async let z = try? self.zones(a)
                        async let g = try? self.guardReport(a)
                        let s = try await d1
                        let input = LocalAlertRule.Input(asset: a, price: price ?? s.candles.last?.close, verdict: await self.botVerdict(a, s.candles),
                                                         reliability: s.reliability, zones: await z?.zones ?? [], shock: await g?.shock.level,
                                                         earnings: results.first { $0.symbol == a.symbol && a.kind == .stock }, fedDecision: fed)
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
