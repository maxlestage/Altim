import Foundation

// The Radar's verdict is the full decision (the same as the asset's Décision card), never the 4 h technical signal,
// which is only one of its inputs (web: DecisionBadge / RATING_RANK / ratingTone in DecisionCard.tsx, technicalText in
// ui.tsx, sort and opportunities in Radar.tsx). The decisions seen on this iPhone are kept as small digests.

extension Action {
    /// The technical signal as a direction, not an order (web `technicalText`).
    public var technicalText: String {
        switch self {
        case .strongBuy: return "nettement haussier"
        case .buy: return "haussier"
        case .sell: return "baissier"
        case .strongSell: return "nettement baissier"
        case .hold: return "neutre"
        }
    }

    /// Colour of the direction: rising, falling or neutral.
    public var directionTone: Tone { self == .buy || self == .strongBuy ? .good : self == .sell || self == .strongSell ? .bad : .neutral }
}

/// What the Radar keeps of a decision seen on this iPhone (its page or the Radar's 15-minute re-reading).
public struct DecisionDigest: Codable, Sendable, Equatable {
    public var verdict: Decision.Verdict
    public var rating: Decision.Rating?
    /// The decision's own label ("ATTENDRE", "ZONE D'ACHAT"…), shown when it has no rating.
    public var label: String
    public var level: Decision.Level
    /// 0–100.
    public var confidence: Double
    public var personal: Bool
    /// ms: when the decision was computed (never later than when it was received).
    public var at: Double

    public init(verdict: Decision.Verdict, rating: Decision.Rating?, label: String, level: Decision.Level, confidence: Double, personal: Bool, at: Double) {
        self.verdict = verdict
        self.rating = rating
        self.label = label
        self.level = level
        self.confidence = confidence
        self.personal = personal
        self.at = at
    }

    /// `now` in ms; an answer served from the offline cache keeps its own (older) time.
    public init(_ d: Decision, personal: Bool, now: Double) {
        let at = d.asOf.isFinite && d.asOf > 0 ? min(now, d.asOf) : now
        self.init(verdict: d.verdict, rating: d.rating, label: d.label, level: d.level, confidence: d.confidence, personal: personal, at: at)
    }

    /// The rating when known, else nil (an unknown server value counts as none).
    var knownRating: Decision.Rating? { rating == .unknown ? nil : rating }

    /// Badge text: the rating's label (web RATING_UI), else the decision's label.
    public var badgeLabel: String { knownRating?.label ?? label }
    public var tone: Tone { DecisionDigests.ratingTone(rating, verdict: verdict) }
    /// ACHETER or ZONE D'ACHAT.
    public var isBuy: Bool { verdict == .buy || verdict == .buyZone }
}

public enum DecisionDigests {
    /// Older than this, the Radar shows "Décision…" rather than a stale verdict.
    public static let maxAge: Double = 12 * 3_600_000
    /// Entries kept at most (the newest).
    public static let maxEntries = 200
    /// Shown while no recent decision is known.
    public static let pendingLabel = "Décision…"

    public static func key(kind: Kind, symbol: String) -> String { "\(kind.rawValue):\(symbol)" }

    /// Adds a decision; an informational one does not replace a personal one still recent (seen on the asset's page).
    public static func record(_ all: [String: DecisionDigest], _ d: Decision, personal: Bool, now: Double) -> [String: DecisionDigest] {
        let k = key(kind: d.kind, symbol: d.symbol)
        let next = DecisionDigest(d, personal: personal, now: now)
        if !personal, let old = all[k], old.personal, now - old.at <= maxAge { return all }
        if let old = all[k], old.at > next.at { return all }
        var out = all
        out[k] = next
        if out.count > maxEntries {
            out = Dictionary(uniqueKeysWithValues: out.sorted { $0.value.at > $1.value.at }.prefix(maxEntries).map { ($0.key, $0.value) })
        }
        return out
    }

    /// The decision of this asset when less than 12 h old.
    public static func fresh(_ all: [String: DecisionDigest], _ asset: Asset, now: Double) -> DecisionDigest? {
        guard let d = all[key(kind: asset.kind, symbol: asset.symbol)], now - d.at <= maxAge else { return nil }
        return d
    }

    /// Rank of a rating for sorting (buy side first); none or unknown last (web RATING_RANK).
    public static func ratingRank(_ r: Decision.Rating?) -> Int {
        switch r {
        case .strongBuy: return 0
        case .buy: return 1
        case .hold: return 2
        case .reduce: return 3
        case .sell: return 4
        case .strongSell: return 5
        case .unknown, nil: return 9
        }
    }

    /// Colour of a rating on the Radar; the verdict's when there is no rating (web `ratingTone`).
    public static func ratingTone(_ r: Decision.Rating?, verdict: Decision.Verdict) -> Tone {
        let r = r == .unknown ? nil : r
        if r == .strongBuy || r == .buy || (r == nil && (verdict == .buy || verdict == .buyZone)) { return .good }
        if r == .sell || r == .strongSell || r == .reduce || (r == nil && (verdict == .sell || verdict == .trim)) { return .bad }
        return .neutral
    }

    /// "Décision" sort of the Radar: rating (buy side first), then confidence; the user's order breaks ties.
    public static func sortByDecision(_ assets: [Asset], _ all: [String: DecisionDigest], now: Double) -> [Asset] {
        let keyed = assets.enumerated().map { i, a -> (Int, Int, Double, Asset) in
            let d = fresh(all, a, now: now)
            return (ratingRank(d?.rating), i, d?.confidence ?? 0, a)
        }
        return keyed.sorted { x, y in
            if x.0 != y.0 { return x.0 < y.0 }
            if x.2 != y.2 { return x.2 > y.2 }
            return x.1 < y.1
        }.map(\.3)
    }

    /// "Variation" sort: the largest move first, either way; unknown last, the user's order breaks ties.
    public static func sortByChange(_ assets: [Asset], change: (Asset) -> Double?) -> [Asset] {
        let keyed: [(i: Int, a: Asset, c: Double)] = assets.enumerated().map { i, a in
            let c: Double = change(a).map { Swift.abs($0) } ?? -1
            return (i: i, a: a, c: c)
        }
        let sorted = keyed.sorted { x, y in x.c != y.c ? x.c > y.c : x.i < y.i }
        return sorted.map { $0.a }
    }

    /// "Opportunités détectées": watched assets whose recent full decision is ACHETER or ZONE D'ACHAT (not the 4 h
    /// technical signal alone), the most confident first, 3 at most.
    public static func opportunities(_ assets: [Asset], _ all: [String: DecisionDigest], now: Double, limit: Int = 3) -> [DecisionOpportunity] {
        let found: [(i: Int, o: DecisionOpportunity)] = assets.enumerated().compactMap { i, a in
            guard let d = fresh(all, a, now: now), d.isBuy else { return nil }
            return (i: i, o: DecisionOpportunity(asset: a, decision: d))
        }
        let sorted = found.sorted { x, y in
            x.o.decision.confidence != y.o.decision.confidence ? x.o.decision.confidence > y.o.decision.confidence : x.i < y.i
        }
        return sorted.prefix(limit).map { $0.o }
    }
}

/// A watched asset whose recent full decision is ACHETER or ZONE D'ACHAT.
public struct DecisionOpportunity: Sendable, Identifiable {
    public var asset: Asset
    public var decision: DecisionDigest
    public var id: String { asset.id }
}
