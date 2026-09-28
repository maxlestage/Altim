import Foundation

// Automatic trading journal: every simulated purchase (paper) and every real purchase or sale recorded in « Mes avoirs »
// is written down with WHY it was taken (the decision shown at that moment), the price, the stop, the targets, the
// signal used and the market conditions. Everything stays on the iPhone (like the holdings).
//
// Later, when the journal is opened, each entry is reviewed on the daily candles AFTER the entry (never the entry day
// itself: its low may be earlier than the purchase, same rule as the simulation), at 3, 10 and 30 calendar days: max
// favourable / adverse excursion, whether the stop or a target was reached first (a candle reaching both counts as the
// stop: the worst case), result against the plan in R (multiples of the risk taken: entry − stop), facts on what worked
// or not, and whether the entry was coherent with the data available at that moment. The profile groups the results by
// rating at entry, plan respected or not, and market regime, in R and drawdown.
// Port of web/src/engine/journal.ts (same rules, same texts). Pure, deterministic functions.
// (Named "trade journal": `JournalEntry` is the log of the alerts, PriceAlerts.swift.)

public enum TradeJournalSource: String, Codable, Sendable { case paper, real }
public enum TradeSide: String, Codable, Sendable { case buy, sell }

/// The decision shown when the entry was written (a snapshot: the full decision is not kept).
public struct JournalDecision: Codable, Sendable, Equatable {
    public struct Setup: Codable, Sendable, Equatable {
        public struct Step: Codable, Sendable, Equatable {
            public var label: String
            /// "ok" | "no" | "unknown"
            public var state: String
        }
        public var name: String
        public var met: Int
        public var total: Int
        public var steps: [Step]
    }

    public struct Plan: Codable, Sendable, Equatable {
        public var zoneFrom: Double
        public var zoneTo: Double
        public var entry: Double
        public var stop: Double
        public var target1: Double
        public var target2: Double?
        public var riskReward: Double
        public var minRiskReward: Double
        public var acceptable: Bool
    }

    public var asOf: Double
    public var verdict: String
    public var label: String
    public var rating: String?
    public var ratingLabel: String?
    public var levelLabel: String
    public var headline: String
    public var confidence: Double
    /// Composite score /100 (nil: not computed or older server).
    public var score: Double?
    /// First three pros / cons.
    public var pros: [String]
    public var cons: [String]
    public var degraded: Bool
    public var degradedHeadline: String?
    public var setup: Setup
    /// Labels of the buy vetoes active at that moment.
    public var vetoes: [String]
    public var plan: Plan?
    public var horizon: String?
}

/// Market conditions at the entry; nil = not known (not loaded, source failed).
public struct JournalMarket: Codable, Sendable, Equatable {
    /// "riskOn" | "neutral" | "riskOff"
    public var regime: String?
    public var regimeLabel: String?
    /// Macro stress /100 and its level (calm / tense / high), from /api/macro.
    public var macroScore: Double?
    public var macroLevel: String?
    /// Daily ATR (14) in % of the price, on the daily candles closed before the entry.
    public var atrPct: Double?
    /// Volume of the last closed day ÷ average of the 20 before (or the decision's liquidity figure).
    public var relativeVolume: Double?
    /// Events of the next 7 days in the decision (economy, central banks, company).
    public var events: Int?

    public init(regime: String? = nil, regimeLabel: String? = nil, macroScore: Double? = nil, macroLevel: String? = nil, atrPct: Double? = nil,
                relativeVolume: Double? = nil, events: Int? = nil) {
        self.regime = regime
        self.regimeLabel = regimeLabel
        self.macroScore = macroScore
        self.macroLevel = macroLevel
        self.atrPct = atrPct
        self.relativeVolume = relativeVolume
        self.events = events
    }

    /// Fields of `patch` that are known replace these.
    public func merged(_ patch: JournalMarket) -> JournalMarket {
        JournalMarket(regime: patch.regime ?? regime, regimeLabel: patch.regimeLabel ?? regimeLabel, macroScore: patch.macroScore ?? macroScore,
                      macroLevel: patch.macroLevel ?? macroLevel, atrPct: patch.atrPct ?? atrPct, relativeVolume: patch.relativeVolume ?? relativeVolume,
                      events: patch.events ?? events)
    }
}

public struct TradeJournalEntry: Codable, Sendable, Identifiable, Equatable {
    public var id: String
    public var createdAt: Double
    public var source: TradeJournalSource
    public var side: TradeSide
    public var symbol: String
    public var kind: Kind
    public var name: String
    /// Price of the purchase or sale (paper: fill price with slippage; real: price entered or live price).
    public var price: Double
    public var quantity: Double?
    public var amount: Double?
    /// The user's own stop and target(s) (paper order, holding's stop); the decision's plan is kept apart.
    public var stop: Double?
    public var targets: [Double]
    /// "Signal utilisé" in words.
    public var signal: String
    /// "Pourquoi je suis entré" (optional).
    public var note: String
    /// Paper position or holding line this entry is about.
    public var refId: String?
    public var decision: JournalDecision?
    public var market: JournalMarket

    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }
    public var key: String { "\(kind.rawValue):\(symbol)" }
}

public struct TradeJournalState: Codable, Sendable, Equatable {
    public var version: Int = 1
    public var entries: [TradeJournalEntry] = []
    public init(entries: [TradeJournalEntry] = []) { self.entries = entries }
}

public enum TradeJournal {
    public static let day = 86_400_000.0
    public static let reviewDays = [3, 10, 30]
    /// Under this many entries a group's figures are shown with "échantillon trop faible".
    public static let minSample = 5
    /// Beyond this age the decision is not "the one of the moment" anymore.
    public static let staleDecisionMs = 24 * 3_600_000.0
    public static let maxEntries = 500
    /// A decision seen on the iPhone for this asset is used for an entry when it is younger than this.
    public static let decisionMaxAge = 6 * 3_600_000.0
    private static let buying = ["buy", "buyZone"]

    /// JavaScript Math.round(v × 10^d) / 10^d.
    static func round(_ v: Double, _ d: Int = 4) -> Double {
        let p = pow(10, Double(d))
        return (v * p + 0.5).rounded(.down) / p
    }
    static func fr(_ v: Double, _ d: Int = 1) -> String { JSFormat.fr(v, max: d) }
    public static func signed(_ v: Double, _ d: Int = 1) -> String { "\(v > 0 ? "+" : v < 0 ? "−" : "")\(fr(abs(v), d))" }
    static func plural(_ n: Int, _ w: String) -> String { "\(n) \(w)\(n > 1 ? "s" : "")" }
    static func rText(_ r: Double) -> String { "\(signed(r)) R" }

    // MARK: Writing an entry

    /// Snapshot of the decision (only what the journal shows and checks).
    public static func snapshot(_ d: Decision) -> JournalDecision {
        let p = d.plan
        return JournalDecision(
            asOf: d.asOf, verdict: d.verdict.rawValue, label: d.label, rating: d.rating?.rawValue, ratingLabel: d.ratingLabel,
            levelLabel: d.levelLabel, headline: d.headline, confidence: d.confidence, score: d.score?.value,
            pros: Array(d.pros.prefix(3)), cons: Array(d.cons.prefix(3)), degraded: d.degraded?.active == true,
            degradedHeadline: d.degraded?.active == true ? d.degraded?.headline : nil,
            setup: .init(name: d.setup.name, met: d.setup.met, total: d.setup.total, steps: d.setup.steps.map { .init(label: $0.label, state: $0.state.rawValue) }),
            vetoes: d.vetoes.filter(\.active).map(\.label),
            plan: p.map { .init(zoneFrom: $0.zoneFrom, zoneTo: $0.zoneTo, entry: $0.entry, stop: $0.stop, target1: $0.target1, target2: $0.target2,
                                riskReward: $0.riskReward, minRiskReward: $0.minRiskReward, acceptable: $0.acceptable) },
            horizon: d.horizon?.label ?? p?.horizon
        )
    }

    /// "Signal utilisé": the decision, its level and the setup's progress.
    public static func signalText(_ d: JournalDecision?) -> String {
        guard let d else { return "Aucune décision chargée pour cet actif à ce moment-là" }
        var parts = ["\(d.ratingLabel ?? d.label) (\(d.levelLabel))", "configuration « \(d.setup.name) » \(d.setup.met)/\(d.setup.total)"]
        if let h = d.horizon { parts.append("horizon \(h)") }
        return parts.joined(separator: " · ")
    }

    /// Market conditions the decision already carries (regime, relative volume, events).
    public static func market(from d: Decision?) -> JournalMarket {
        guard let d else { return JournalMarket() }
        return JournalMarket(regime: d.marketRegime?.kind.rawValue, regimeLabel: d.marketRegime?.label, relativeVolume: d.liquidity?.relativeVolume,
                             events: d.events?.count)
    }

    /// ATR % and relative volume on the daily candles CLOSED before `at` (no look-ahead: the entry day's candle is
    /// excluded). nil when the history is too short.
    public static func market(from candles: [Candle], at: Double) -> (atrPct: Double?, relativeVolume: Double?) {
        let before = candles.filter { $0.time + day <= at && $0.close > 0 }.sorted { $0.time < $1.time }
        let last = before.last
        let a = before.count >= 15 ? RiskEngine.atr(before).last ?? nil : nil
        let prev = before.dropLast().suffix(20).compactMap(\.volume).filter { $0.isFinite && $0 > 0 }
        let avg = prev.count >= 20 ? prev.reduce(0, +) / Double(prev.count) : 0
        let atrPct = a.flatMap { a in last.map { round(a / $0.close * 100, 3) } }
        let rel: Double? = (last.flatMap { l in avg > 0 && (l.volume ?? 0) > 0 ? round(l.volume! / avg, 3) : nil })
        return (atrPct, rel)
    }

    public struct NewEntry: Sendable {
        public var id: String
        public var now: Double
        public var source: TradeJournalSource
        public var side: TradeSide
        public var symbol: String
        public var kind: Kind
        public var name: String
        public var price: Double
        public var quantity: Double?
        public var amount: Double?
        public var stop: Double?
        public var targets: [Double?]
        public var note: String
        public var refId: String?
        public var decision: Decision?

        public init(id: String, now: Double, source: TradeJournalSource, side: TradeSide, symbol: String, kind: Kind, name: String, price: Double,
                    quantity: Double? = nil, amount: Double? = nil, stop: Double? = nil, targets: [Double?] = [], note: String = "",
                    refId: String? = nil, decision: Decision? = nil) {
            self.id = id
            self.now = now
            self.source = source
            self.side = side
            self.symbol = symbol
            self.kind = kind
            self.name = name
            self.price = price
            self.quantity = quantity
            self.amount = amount
            self.stop = stop
            self.targets = targets
            self.note = note
            self.refId = refId
            self.decision = decision
        }
    }

    static func cleanNote(_ s: String) -> String { String(s.trimmingCharacters(in: .whitespacesAndNewlines).prefix(1000)) }

    /// A new entry from what the screen knows at that moment (the decision already loaded, if any).
    public static func create(_ n: NewEntry) -> TradeJournalEntry {
        let decision = n.decision.map(snapshot)
        func ok(_ v: Double?) -> Double? { v.flatMap { $0.isFinite && $0 > 0 ? $0 : nil } }
        return TradeJournalEntry(
            id: n.id, createdAt: n.now, source: n.source, side: n.side, symbol: n.symbol, kind: n.kind, name: n.name, price: n.price,
            quantity: ok(n.quantity), amount: ok(n.amount),
            stop: n.side == .buy ? ok(n.stop).flatMap { $0 < n.price ? $0 : nil } : nil,
            targets: n.side == .buy ? n.targets.compactMap(ok).filter { $0 > n.price }.sorted() : [],
            signal: signalText(decision), note: cleanNote(n.note), refId: n.refId, decision: decision, market: market(from: n.decision)
        )
    }

    public struct HoldingChange: Sendable, Equatable {
        public var side: TradeSide
        public var quantity: Double
        public var price: Double
        /// Price deduced from the new average cost.
        public var implied: Bool
        public init(side: TradeSide, quantity: Double, price: Double, implied: Bool) {
            self.side = side
            self.quantity = quantity
            self.price = price
            self.implied = implied
        }
    }

    /// A holding line edited in « Mes avoirs » → the purchase or sale it records (nil: no quantity change, or no usable
    /// price). Purchase price: the one implied by the new average cost ((q2 × PRU2 − q1 × PRU1) ÷ (q2 − q1)), else the
    /// live price; a sale is priced at the live price (the average cost does not change on a sale).
    public static func holdingChange(before: (quantity: Double, averagePrice: Double?), after: (quantity: Double, averagePrice: Double?),
                                     livePrice: Double?) -> HoldingChange? {
        let dq = after.quantity - before.quantity
        guard abs(dq) > 1e-12 else { return nil }
        let live = livePrice.flatMap { $0.isFinite && $0 > 0 ? $0 : nil }
        if dq < 0 { return live.map { HoldingChange(side: .sell, quantity: -dq, price: $0, implied: false) } }
        if let a1 = after.averagePrice, let a0 = before.averagePrice, a1 != a0 {
            let implied = (after.quantity * a1 - before.quantity * a0) / dq
            if implied.isFinite && implied > 0 { return HoldingChange(side: .buy, quantity: dq, price: implied, implied: true) }
        }
        return live.map { HoldingChange(side: .buy, quantity: dq, price: $0, implied: false) }
    }

    public static func add(_ s: TradeJournalState, _ e: TradeJournalEntry) -> TradeJournalState {
        let entries = (s.entries.filter { $0.id != e.id } + [e]).enumerated()
            .sorted { $0.element.createdAt != $1.element.createdAt ? $0.element.createdAt < $1.element.createdAt : $0.offset < $1.offset }
            .map(\.element)
        return TradeJournalState(entries: Array(entries.suffix(maxEntries)))
    }

    /// Completes an entry (market data arrived after it was written, the user's note).
    public static func patch(_ s: TradeJournalState, id: String, note: String? = nil, market: JournalMarket? = nil, decision: JournalDecision? = nil) -> TradeJournalState {
        TradeJournalState(entries: s.entries.map { e in
            guard e.id == id else { return e }
            var x = e
            if let note { x.note = cleanNote(note) }
            if let market { x.market = e.market.merged(market) }
            if let decision {
                x.decision = decision
                x.signal = signalText(decision)
            }
            return x
        })
    }

    public static func remove(_ s: TradeJournalState, id: String) -> TradeJournalState {
        TradeJournalState(entries: s.entries.filter { $0.id != id })
    }

    public static func isValid(_ s: TradeJournalState) -> Bool {
        s.version == 1 && s.entries.allSatisfy { $0.createdAt.isFinite && $0.price.isFinite && $0.price > 0 }
    }

    /// Saved journal: empty, valid, or unreadable (ignored with a message, never a crash).
    public static func parseSaved(_ data: Data?) -> (state: TradeJournalState, error: String?) {
        guard let data, !data.isEmpty else { return (TradeJournalState(), nil) }
        if let s = try? JSONDecoder().decode(TradeJournalState.self, from: data), isValid(s) { return (s, nil) }
        return (TradeJournalState(), "Le journal enregistré sur cet iPhone est illisible : il a été ignoré.")
    }

    // MARK: Review

    public enum LevelSource: String, Sendable { case user, plan, none }

    public struct Levels: Sendable, Equatable {
        public var stop: Double?
        public var stopSource: LevelSource
        public var targets: [Double]
        public var targetsSource: LevelSource
    }

    /// Stop and targets the review uses: the user's own, else the plan of the decision shown at the entry.
    public static func levels(_ e: TradeJournalEntry) -> Levels {
        let p = e.decision?.plan
        let stop = e.stop ?? p.flatMap { $0.stop < e.price ? $0.stop : nil }
        let planTargets = p.map { [$0.target1, $0.target2].compactMap { $0 }.filter { $0 > e.price } } ?? []
        let targets = e.targets.isEmpty ? planTargets : e.targets
        return Levels(stop: stop, stopSource: e.stop != nil ? .user : stop != nil ? .plan : .none,
                      targets: targets, targetsSource: !e.targets.isEmpty ? .user : !targets.isEmpty ? .plan : .none)
    }

    public enum FirstHit: String, Sendable { case stop, target1, none }

    public struct HorizonReview: Sendable {
        public enum Status: String, Sendable { case pending, noData, ready }
        public var days: Int
        /// pending: not reached yet; noData: no candle after the entry in the history; ready: computed.
        public var status: Status
        public var availableAt: Double
        public var candles: Int = 0
        public var lastClose: Double?
        /// Price change since the entry (a sale: since the sale), %.
        public var returnPct: Double?
        /// Max favourable / adverse excursion since the entry, %.
        public var mfePct: Double?
        public var maePct: Double?
        public var mfeR: Double?
        public var maeR: Double?
        /// First level reached and after how many days (calendar days since the entry).
        public var first: FirstHit = .none
        public var firstDays: Int?
        /// Target 2 reached before the stop within the window.
        public var target2 = false
        /// Result following the plan (exit at the first level reached, else the last close), in R; nil without stop.
        public var resultR: Double?
    }

    public struct Check: Sendable, Identifiable {
        public var code: String
        public var label: String
        public var ok: Bool?
        public var detail: String
        public var id: String { code }
    }

    public struct ClosedInfo: Sendable {
        public var at: Double
        public var price: Double
        public var reason: String
        public init(at: Double, price: Double, reason: String) {
            self.at = at
            self.price = price
            self.reason = reason
        }
    }

    public struct EntryReview: Sendable, Identifiable {
        public var entry: TradeJournalEntry
        public var levels: Levels
        /// Risk per unit (entry − stop), nil without stop.
        public var risk: Double?
        /// Planned reward at target 1, in R.
        public var planR: Double?
        public var horizons: [HorizonReview]
        /// Most advanced ready horizon (the facts are built on it).
        public var latest: HorizonReview?
        public var worked: [String]
        public var failed: [String]
        public var coherence: [Check]
        /// true: every verifiable check passes; false: at least one fails; nil: nothing verifiable.
        public var coherent: Bool?
        /// Plan respected at the entry (same checks).
        public var planRespected: Bool?
        public var closed: ClosedInfo?
        public var realizedR: Double?
        public var id: String { entry.id }
    }

    /// Checks of the entry against the data available at that moment (a sale is checked against the decision only).
    public static func coherenceChecks(_ e: TradeJournalEntry, levels lv: Levels? = nil) -> [Check] {
        let levels = lv ?? self.levels(e)
        guard let d = e.decision else {
            return [Check(code: "decision", label: "Décision disponible", ok: nil, detail: "Aucune décision chargée pour cet actif à ce moment-là : cohérence non vérifiable.")]
        }
        var out: [Check] = []
        let age = e.createdAt - d.asOf
        out.append(age > staleDecisionMs
            ? Check(code: "fresh", label: "Décision récente", ok: false,
                    detail: "La décision affichée datait de \(plural(Int((age / 3_600_000 + 0.5).rounded(.down)), "heure")) : les données avaient pu changer.")
            : Check(code: "fresh", label: "Décision récente", ok: true, detail: "Décision calculée moins de 24 h avant."))
        if e.side == .sell {
            let selling = ["trim", "sell"].contains(d.verdict)
            out.append(Check(code: "verdict", label: "Vente conforme à la décision", ok: selling,
                             detail: selling ? "Décision « \(d.label) » : la vente allait dans son sens." : "Décision « \(d.label) » : la vente allait contre elle."))
            return out
        }
        let isBuying = buying.contains(d.verdict)
        out.append(Check(code: "verdict", label: "Achat conforme à la décision", ok: isBuying,
                         detail: isBuying ? "Décision « \(d.label) »." : "Achat alors que la décision était « \(d.label) »."))
        out.append(d.vetoes.isEmpty
            ? Check(code: "vetoes", label: "Aucune interdiction d'achat active", ok: true, detail: "Aucune interdiction d'achat active.")
            : Check(code: "vetoes", label: "Aucune interdiction d'achat active", ok: false,
                    detail: "Entrée malgré \(d.vetoes.count > 1 ? "des interdictions d'achat actives" : "une interdiction d'achat active") : \(d.vetoes.joined(separator: ", "))."))
        out.append(d.degraded
            ? Check(code: "degraded", label: "Signal non dégradé", ok: false, detail: d.degradedHeadline ?? "Le signal était dégradé à l'entrée.")
            : Check(code: "degraded", label: "Signal non dégradé", ok: true, detail: "Signal complet à l'entrée."))
        if let p = d.plan {
            let min = p.minRiskReward > 0 ? p.minRiskReward : 2
            let label = "Rapport gain / risque ≥ \(fr(min))"
            out.append(p.riskReward >= min
                ? Check(code: "rr", label: label, ok: true, detail: "Rapport du plan : \(fr(p.riskReward)).")
                : Check(code: "rr", label: label, ok: false, detail: "Rapport du plan : \(fr(p.riskReward)), sous le minimum de \(fr(min))."))
            let lo = Swift.min(p.zoneFrom, p.zoneTo), hi = Swift.max(p.zoneFrom, p.zoneTo)
            if e.price > hi {
                out.append(Check(code: "zone", label: "Entrée dans la zone d'achat", ok: false, detail: "Entrée hors zone d'achat : \(signed((e.price / hi - 1) * 100)) % au-dessus du haut de la zone."))
            } else if e.price < lo {
                out.append(Check(code: "zone", label: "Entrée dans la zone d'achat", ok: false, detail: "Entrée hors zone d'achat : \(fr((1 - e.price / lo) * 100)) % sous la zone (support peut-être cassé)."))
            } else {
                out.append(Check(code: "zone", label: "Entrée dans la zone d'achat", ok: true, detail: "Entrée dans la zone d'achat."))
            }
        } else {
            out.append(Check(code: "rr", label: "Rapport gain / risque ≥ 2", ok: nil, detail: "Pas de plan chiffré dans la décision."))
            out.append(Check(code: "zone", label: "Entrée dans la zone d'achat", ok: nil, detail: "Pas de zone d'achat dans la décision."))
        }
        out.append(levels.stop == nil
            ? Check(code: "stop", label: "Stop défini", ok: false, detail: "Aucun stop : risque non borné, résultat en R non mesurable.")
            : Check(code: "stop", label: "Stop défini", ok: true, detail: levels.stopSource == .user ? "Stop fixé à l'entrée." : "Pas de stop saisi : celui du plan de la décision est utilisé."))
        if let r = e.market.regime {
            out.append(r == "riskOff"
                ? Check(code: "regime", label: "Pas contre le régime de marché", ok: false, detail: "Achat en régime \(e.market.regimeLabel ?? "risk-off") : à contre-courant du marché.")
                : Check(code: "regime", label: "Pas contre le régime de marché", ok: true, detail: "Régime \(e.market.regimeLabel ?? r)."))
        } else {
            out.append(Check(code: "regime", label: "Pas contre le régime de marché", ok: nil, detail: "Régime de marché inconnu à l'entrée."))
        }
        return out
    }

    private static func daysSince(_ t: Double, _ e: TradeJournalEntry) -> Int { max(1, Int(((t - e.createdAt) / day).rounded(.up))) }

    static func horizonReview(_ e: TradeJournalEntry, days: Int, after: [Candle], now: Double, levels: Levels, risk: Double?) -> HorizonReview {
        let availableAt = e.createdAt + Double(days) * day
        var h = HorizonReview(days: days, status: .pending, availableAt: availableAt)
        guard now >= availableAt else { return h }
        let w = after.filter { $0.time <= availableAt }
        guard let last = w.last else {
            h.status = .noData
            return h
        }
        // Excursions are measured from the entry: never below it for the favourable one, never above it for the adverse one.
        let hi = max(e.price, w.map(\.high).max() ?? e.price)
        let lo = min(e.price, w.map(\.low).min() ?? e.price)
        func r(_ p: Double) -> Double? { risk.flatMap { $0 != 0 ? round((p - e.price) / $0, 3) : nil } }
        var exit: Double?
        if e.side == .buy {
            let t1 = levels.targets.first
            let t2 = levels.targets.count > 1 ? levels.targets[1] : nil
            for c in w {
                if let stop = levels.stop, c.low <= stop {
                    if h.first == .none {
                        h.first = .stop
                        h.firstDays = daysSince(c.time, e)
                        exit = min(stop, c.open)
                    }
                    break
                }
                if let t1, c.high >= t1, h.first == .none {
                    h.first = .target1
                    h.firstDays = daysSince(c.time, e)
                    exit = t1
                }
                if let t2, c.high >= t2 { h.target2 = true }
            }
        }
        let mark = exit ?? last.close
        h.status = .ready
        h.candles = w.count
        h.lastClose = last.close
        h.returnPct = round((last.close / e.price - 1) * 100, 3)
        h.mfePct = round((hi / e.price - 1) * 100, 3)
        h.maePct = round((lo / e.price - 1) * 100, 3)
        h.mfeR = r(hi)
        h.maeR = r(lo)
        h.resultR = e.side == .buy ? r(mark) : nil
        return h
    }

    /// Review of one entry on its asset's daily candles (any order). `closed`: the position was actually closed (paper
    /// trade) — its realized result in R is added.
    public static func review(_ e: TradeJournalEntry, candles: [Candle], now: Double, closed: ClosedInfo? = nil) -> EntryReview {
        let lv = levels(e)
        let risk: Double? = lv.stop.flatMap { e.price > $0 ? e.price - $0 : nil }
        let planR: Double? = risk.flatMap { r in lv.targets.first.map { round(($0 - e.price) / r, 3) } }
        // Candles starting after the entry (the entry day's own candle may predate it). A history starting after the entry
        // would leave a hole at its start: nothing is computed then.
        let covers = candles.contains { $0.time <= e.createdAt }
        let after = covers ? candles.filter { $0.time > e.createdAt && $0.low > 0 && $0.high >= $0.low }.sorted { $0.time < $1.time } : []
        let horizons = reviewDays.map { horizonReview(e, days: $0, after: after, now: now, levels: lv, risk: risk) }
        let latest = horizons.last { $0.status == .ready }
        let coherence = coherenceChecks(e, levels: lv)
        let verifiable = coherence.filter { $0.ok != nil }
        let coherent: Bool? = verifiable.isEmpty ? nil : verifiable.allSatisfy { $0.ok == true }
        let realizedR: Double? = closed.flatMap { c in risk.map { round((c.price - e.price) / $0, 3) } }
        let (worked, failed) = facts(e, latest, coherence, lv, planR, closed, realizedR)
        return EntryReview(entry: e, levels: lv, risk: risk, planR: planR, horizons: horizons, latest: latest, worked: worked, failed: failed,
                           coherence: coherence, coherent: coherent, planRespected: e.side == .buy ? coherent : nil, closed: closed, realizedR: realizedR)
    }

    /// "Qu'est-ce qui a fonctionné ? / Qu'est-ce qui n'a pas fonctionné ?" — facts only, from the review.
    static func facts(_ e: TradeJournalEntry, _ h: HorizonReview?, _ checks: [Check], _ levels: Levels, _ planR: Double?, _ closed: ClosedInfo?,
                      _ realizedR: Double?) -> ([String], [String]) {
        var worked: [String] = []
        var failed: [String] = []
        func failing(_ code: String) -> Check? { checks.first { $0.code == code && $0.ok == false } }
        if e.side == .sell {
            if let h, let ret = h.returnPct {
                let txt = "\(h.days) jours après la vente, le cours est à \(signed(ret)) % du prix de vente (plus haut \(signed(h.mfePct ?? 0)) %, plus bas \(signed(h.maePct ?? 0)) %)."
                if ret <= 0 { worked.append(txt) } else { failed.append(txt) }
            }
            if let v = failing("verdict") { failed.append(v.detail) }
            return (worked, failed)
        }
        if let h {
            if h.first == .target1 {
                worked.append("L'objectif 1 a été atteint en \(plural(h.firstDays ?? 0, "jour"))\(h.resultR.map { " (\(rText($0)))" } ?? "")\(h.target2 ? ", puis l'objectif 2" : "").")
            } else if h.first == .stop {
                var why: [String] = []
                if failing("degraded") != nil { why.append("le signal était dégradé à l'entrée") }
                if failing("vetoes") != nil { why.append("des interdictions d'achat étaient actives") }
                if failing("zone") != nil { why.append("l'entrée était hors zone d'achat") }
                failed.append("Le stop a été touché en \(plural(h.firstDays ?? 0, "jour"))\(h.resultR.map { " (\(rText($0)))" } ?? "")\(why.isEmpty ? "" : " alors que \(why.joined(separator: " et que "))").")
            } else if let r = h.resultR {
                let txt = "Ni stop ni objectif en \(h.days) jours : \(rText(r)) au dernier cours."
                if r > 0 { worked.append(txt) } else { failed.append(txt) }
            } else if let ret = h.returnPct {
                let txt = "\(signed(ret)) % en \(h.days) jours (sans stop, résultat en R non mesurable)."
                if ret > 0 { worked.append(txt) } else { failed.append(txt) }
            }
            if h.first != .target1, let m = h.mfeR, m >= 1, (h.resultR ?? 0) < 0 {
                failed.append("Le cours est monté jusqu'à \(rText(m)) avant de repasser sous l'entrée : le gain latent n'a pas été conservé.")
            }
            if h.first != .stop, let m = h.maeR, m <= -0.8 { failed.append("Recul jusqu'à \(rText(m)) : le stop a failli être touché.") }
            if h.first != .stop, let m = h.maeR, m > -0.3, let r = h.resultR, r > 0 { worked.append("Recul limité à \(rText(m)) depuis l'entrée.") }
        }
        if let closed, let r = realizedR {
            let txt = "Position clôturée (\(closed.reason)) : \(rText(r)) réalisé\(planR.map { " pour \(rText($0)) prévu à l'objectif 1" } ?? "")."
            if r > 0 { worked.append(txt) } else { failed.append(txt) }
        }
        for code in ["zone", "rr", "regime", "verdict", "stop"] {
            if let c = failing(code) { failed.append(c.detail) }
        }
        if checks.contains(where: { $0.code == "zone" && $0.ok == true }) && checks.allSatisfy({ $0.ok != false }) {
            worked.append("Entrée dans la zone d'achat, plan respecté (aucune interdiction, signal complet, rapport gain / risque suffisant).")
        }
        if levels.stopSource == .plan { failed.append("Aucun stop saisi : la revue utilise le stop du plan de la décision.") }
        return (worked, failed)
    }

    // MARK: Profile

    public struct GroupStat: Sendable, Identifiable {
        public var key: String
        public var label: String
        /// Entries reviewed at this horizon.
        public var n: Int
        /// Entries with a stop (results in R).
        public var withStop: Int
        public var lowSample: Bool
        public var avgR: Double?
        public var medianR: Double?
        /// Share of results above 0 R (or above 0 % without stop), %.
        public var winRate: Double?
        /// Average and worst adverse excursion (drawdown during the trade), in R.
        public var avgMaeR: Double?
        public var worstMaeR: Double?
        public var avgReturnPct: Double?
        public var avgMaePct: Double?
        public var id: String { key }
    }

    public struct Profile: Sendable {
        public var horizon: Int
        /// Purchases reviewed at this horizon / all purchases.
        public var reviewed: Int
        public var purchases: Int
        public var pending: Int
        public var all: GroupStat?
        public var byRating: [GroupStat]
        public var byPlan: [GroupStat]
        public var byRegime: [GroupStat]
    }

    static func mean(_ v: [Double]) -> Double? { v.isEmpty ? nil : round(v.reduce(0, +) / Double(v.count), 3) }
    static func median(_ v: [Double]) -> Double? {
        guard !v.isEmpty else { return nil }
        let s = v.sorted()
        let m = s.count / 2
        return round(s.count % 2 == 1 ? s[m] : (s[m - 1] + s[m]) / 2, 3)
    }

    static func stat(_ key: String, _ label: String, _ rows: [HorizonReview]) -> GroupStat {
        let inR = rows.filter { $0.resultR != nil }
        let rs = inR.compactMap(\.resultR)
        let wins = rows.filter { ($0.resultR ?? $0.returnPct ?? 0) > 0 }.count
        let maes = inR.compactMap(\.maeR)
        return GroupStat(key: key, label: label, n: rows.count, withStop: inR.count, lowSample: rows.count < minSample,
                         avgR: mean(rs), medianR: median(rs), winRate: rows.isEmpty ? nil : round(Double(wins) / Double(rows.count) * 100, 2),
                         avgMaeR: mean(maes), worstMaeR: maes.min(), avgReturnPct: mean(rows.compactMap(\.returnPct)),
                         avgMaePct: mean(rows.compactMap(\.maePct)))
    }

    static let regimeLabels = ["riskOn": "Risk-on", "neutral": "Neutre", "riskOff": "Risk-off", "unknown": "Régime inconnu"]
    static let ratingOrder = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"]

    /// Results of the purchases at one horizon, grouped by rating at entry, plan respected or not, and market regime.
    /// Groups are in R and drawdown; ordered by a fixed scale, never by performance.
    public static func profile(_ reviews: [EntryReview], horizon: Int) -> Profile {
        let buys = reviews.filter { $0.entry.side == .buy }
        let rows: [(r: EntryReview, h: HorizonReview)] = buys.compactMap { r in
            guard let h = r.horizons.first(where: { $0.days == horizon }), h.status == .ready else { return nil }
            return (r, h)
        }
        func group(_ keyOf: (EntryReview) -> (key: String, label: String), _ order: (String) -> Int) -> [GroupStat] {
            var keys: [String] = []
            var m: [String: (label: String, hs: [HorizonReview])] = [:]
            for (r, h) in rows {
                let k = keyOf(r)
                if m[k.key] == nil {
                    keys.append(k.key)
                    m[k.key] = (k.label, [])
                }
                m[k.key]!.hs.append(h)
            }
            return keys.map { stat($0, m[$0]!.label, m[$0]!.hs) }
                .sorted { order($0.key) != order($1.key) ? order($0.key) < order($1.key) : $0.key < $1.key }
        }
        return Profile(
            horizon: horizon, reviewed: rows.count, purchases: buys.count,
            pending: buys.filter { $0.horizons.first { $0.days == horizon }?.status == .pending }.count,
            all: rows.isEmpty ? nil : stat("all", "Tous les achats", rows.map(\.h)),
            byRating: group({ r in
                guard let d = r.entry.decision else { return ("none", "Sans décision") }
                return (d.rating ?? d.verdict, d.ratingLabel ?? d.label)
            }, { k in ratingOrder.firstIndex(of: k) ?? (k == "none" ? 99 : 50) }),
            byPlan: group({ r in
                guard let p = r.planRespected else { return ("unknown", "Non vérifiable") }
                return p ? ("yes", "Plan respecté") : ("no", "Plan non respecté")
            }, { ["yes", "no", "unknown"].firstIndex(of: $0) ?? -1 }),
            byRegime: group({ r in
                let k = r.entry.market.regime ?? "unknown"
                return (k, regimeLabels[k] ?? k)
            }, { ["riskOn", "neutral", "riskOff", "unknown"].firstIndex(of: $0) ?? -1 })
        )
    }

    // MARK: Texts of the screen (web Journal.tsx)

    /// "+1,25 R" / "—".
    public static func rValue(_ v: Double?) -> String { v.map { "\(signed($0, 2)) R" } ?? "—" }
    /// "+4,2 %" / "—".
    public static func pctValue(_ v: Double?) -> String { v.map { "\(signed($0)) %" } ?? "—" }
    public static let macroLabels = ["calm": "calme", "tense": "tendu", "high": "très tendu"]

    /// "10 j : +4,2 % au dernier cours · plus haut +11 % (+2,2 R) · plus bas −2 % (−0,4 R) · objectif 1 atteint en 6 j ·
    /// résultat selon le plan +2 R" (pending and no-data horizons are written apart).
    public static func horizonText(_ h: HorizonReview, buy: Bool, pendingDate: (Double) -> String) -> String {
        switch h.status {
        case .pending: return "\(h.days) j : disponible le \(pendingDate(h.availableAt))."
        case .noData: return "\(h.days) j : pas de bougie journalière après l'entrée dans l'historique disponible."
        case .ready:
            var s = "\(h.days) j : \(pctValue(h.returnPct)) au dernier cours · plus haut \(pctValue(h.mfePct))\(h.mfeR.map { " (\(rValue($0)))" } ?? "")"
                + " · plus bas \(pctValue(h.maePct))\(h.maeR.map { " (\(rValue($0)))" } ?? "")"
            if buy {
                let first: String
                switch h.first {
                case .stop: first = "stop touché en \(h.firstDays ?? 0) j"
                case .target1: first = "objectif 1 atteint en \(h.firstDays ?? 0) j\(h.target2 ? ", puis objectif 2" : "")"
                case .none: first = "ni stop ni objectif"
                }
                s += " · \(first)\(h.resultR.map { " · résultat selon le plan \(rValue($0))" } ?? "")"
            }
            return s
        }
    }
}
