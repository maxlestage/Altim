import Foundation

// Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT"):
// the last decision seen per asset (verdict, level, rating, unmet setup steps, buy conditions) is kept on the iPhone and
// compared with each new one (Radar refresh, asset page). The server is stateless: the diff is done here.
// Each snapshot also keeps the decision's measurements (family scores, composite score, nearest support and resistance,
// relative volume, RSI, news tone) so that a change of verdict or rating is explained ("Momentum −18 pts (+40 → +22)",
// "Volume en baisse (1,4× → 0,7× la moyenne)", …): `explainChange`.
// Port of web/src/webapp/config-changes.ts version 2 (same keys, same texts, same limits); version 1 data saved by
// an older version of the app is migrated on reading (its snapshots have no measurements, its transitions no
// explanation).

/// Measurements of a decision kept to explain a later change (the server's `snapshot`, else read from the decision).
public struct SignalMetrics: Codable, Sendable, Equatable {
    public struct Family: Codable, Sendable, Equatable {
        public var key: String
        public var label: String
        public var score: Double?
        public init(key: String, label: String, score: Double?) {
            self.key = key
            self.label = label
            self.score = score
        }
    }

    public struct Level: Codable, Sendable, Equatable {
        public var price: Double
        public var touches: Int
        public init(price: Double, touches: Int) {
            self.price = price
            self.touches = touches
        }
    }

    public struct News: Codable, Sendable, Equatable {
        public var title: String
        public var tone: String
        public init(title: String, tone: String) {
            self.title = title
            self.tone = tone
        }
    }

    public var price: Double?
    public var composite: Double?
    public var families: [Family]
    /// Last daily volume ÷ 20-day average.
    public var relVolume: Double?
    public var rsi: Double?
    public var support: Level?
    public var resistance: Level?
    public var newsScore: Double?
    public var topNews: News?

    public init(price: Double?, composite: Double?, families: [Family], relVolume: Double?, rsi: Double?, support: Level?, resistance: Level?,
                newsScore: Double?, topNews: News?) {
        self.price = price
        self.composite = composite
        self.families = families
        self.relVolume = relVolume
        self.rsi = rsi
        self.support = support
        self.resistance = resistance
        self.newsScore = newsScore
        self.topNews = topNews
    }
}

/// What a decision says, kept to compare with the next one.
public struct ConfigSnapshot: Codable, Sendable, Equatable {
    public var verdict: Decision.Verdict
    public var level: Decision.Level
    public var label: String
    public var levelLabel: String
    /// Setup steps not met yet ("Cassure de la résistance : pas encore").
    public var missing: [String]
    /// Conditions that would change the decision (the decision's `toBuy`).
    public var triggers: [String]
    /// ms.
    public var at: Double
    /// Added in version 2 (absent from migrated entries).
    public var rating: Decision.Rating?
    public var ratingLabel: String?
    public var metrics: SignalMetrics?

    var isValid: Bool { verdict != .unknown && level != .unknown && at.isFinite && rating != .unknown }
}

extension ConfigSnapshot {
    /// Damaged measurements alone are dropped (the snapshot stays).
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        verdict = try c.decode(Decision.Verdict.self, forKey: .verdict)
        level = try c.decode(Decision.Level.self, forKey: .level)
        label = try c.decode(String.self, forKey: .label)
        levelLabel = try c.decode(String.self, forKey: .levelLabel)
        missing = try c.decode([String].self, forKey: .missing)
        triggers = try c.decode([String].self, forKey: .triggers)
        at = try c.decode(Double.self, forKey: .at)
        rating = try c.decodeIfPresent(Decision.Rating.self, forKey: .rating)
        ratingLabel = try c.decodeIfPresent(String.self, forKey: .ratingLabel)
        metrics = (try? c.decodeIfPresent(SignalMetrics.self, forKey: .metrics)) ?? nil
    }
}

/// Verdict, level and rating of one side of a transition.
public struct ConfigSide: Codable, Sendable, Equatable {
    public var verdict: Decision.Verdict
    public var level: Decision.Level
    public var label: String
    public var levelLabel: String
    public var rating: Decision.Rating?
    public var ratingLabel: String?

    var isValid: Bool { verdict != .unknown && level != .unknown && rating != .unknown }
}

public struct ConfigTransition: Codable, Sendable, Equatable, Identifiable {
    public var symbol: String
    public var kind: Kind
    public var name: String
    /// Personal mode (with the user's average cost and weights) or market data only: compared separately.
    public var personal: Bool
    public var at: Double
    /// When the previous configuration was seen.
    public var since: Double
    public var from: ConfigSide
    public var to: ConfigSide
    public var missing: [String]
    public var triggers: [String]
    /// What changed in the measurements ("Momentum −18 pts (+40 → +22)"); empty when unknown (older entries).
    public var changes: [String]

    public var id: String { "\(kind.rawValue):\(symbol):\(personal):\(at)" }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }

    /// "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the level, then the rating, when only it changed).
    public var title: String { "🚨 \(symbol) — changement de configuration : \(change)" }

    /// "ATTENDRE → ZONE D'ACHAT": the verdicts, else the levels, else the ratings (the end of `title`).
    public var change: String {
        let (a, b): (String, String)
        if from.verdict != to.verdict {
            (a, b) = (from.label, to.label)
        } else if from.level != to.level || from.ratingLabel == nil || to.ratingLabel == nil || from.ratingLabel == "" || to.ratingLabel == "" {
            (a, b) = (from.levelLabel, to.levelLabel)
        } else {
            (a, b) = ("note \(from.ratingLabel ?? "")", "note \(to.ratingLabel ?? "")")
        }
        return "\(a) → \(b)"
    }

    /// Colour meaning: towards a buy, towards a sale, or neither.
    public var tone: Tone {
        switch to.verdict {
        case .buy, .buyZone: return .good
        case .sell, .trim: return .bad
        default: return .neutral
        }
    }

    var isValid: Bool { from.isValid && to.isValid && at.isFinite && since.isFinite }
}

extension ConfigTransition {
    /// Version 1 transitions have no `changes`: empty.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        symbol = try c.decode(String.self, forKey: .symbol)
        kind = try c.decode(Kind.self, forKey: .kind)
        name = try c.decode(String.self, forKey: .name)
        personal = try c.decode(Bool.self, forKey: .personal)
        at = try c.decode(Double.self, forKey: .at)
        since = try c.decode(Double.self, forKey: .since)
        from = try c.decode(ConfigSide.self, forKey: .from)
        to = try c.decode(ConfigSide.self, forKey: .to)
        missing = try c.decode([String].self, forKey: .missing)
        triggers = try c.decode([String].self, forKey: .triggers)
        changes = try c.decodeIfPresent([String].self, forKey: .changes) ?? []
    }
}

public struct ConfigState: Codable, Sendable, Equatable {
    public var version: Int = ConfigChanges.stateVersion
    public var last: [String: ConfigSnapshot] = [:]
    /// Newest first, at most `ConfigChanges.maxTransitions`.
    public var transitions: [ConfigTransition] = []

    public init() {}

    private enum CodingKeys: String, CodingKey { case version, last, transitions }

    /// Version 2, or version 1 migrated; a damaged entry is dropped, never trusted; another version starts empty.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        guard let v = try? c.decode(Int.self, forKey: .version), v == 1 || v == ConfigChanges.stateVersion else { return }
        let rawLast = (try? c.decode([String: Lenient<ConfigSnapshot>].self, forKey: .last)) ?? [:]
        last = rawLast.compactMapValues { $0.value.flatMap { $0.isValid ? $0 : nil } }
        let rawTransitions = (try? c.decode([Lenient<ConfigTransition>].self, forKey: .transitions)) ?? []
        transitions = Array(rawTransitions.compactMap { $0.value.flatMap { $0.isValid ? $0 : nil } }.prefix(ConfigChanges.maxTransitions))
    }
}

/// Decodes a value, or nil instead of failing the whole list.
struct Lenient<T: Decodable>: Decodable {
    let value: T?
    init(from decoder: Decoder) throws { value = try? T(from: decoder) }
}

public enum ConfigChanges {
    public static let maxTransitions = 50
    /// Version of the stored state (1: without measurements nor explanations, migrated on read).
    public static let stateVersion = 2

    private static let stepState: [Decision.StepState: String] = [.no: "pas encore", .unknown: "non vérifiable"]

    static func usd(_ v: Double) -> String { Money.moneyFmt(v, sep: " ") { JSFormat.fr($0, max: $0 >= 1 ? 2 : 6) } }

    private static func level(_ price: Double?, _ touches: Int?) -> SignalMetrics.Level? {
        guard let price, price.isFinite, let touches else { return nil }
        return SignalMetrics.Level(price: price, touches: touches)
    }

    /// The server's compact snapshot when given, else the same numbers read from the decision (older servers).
    public static func metrics(of d: Decision) -> SignalMetrics {
        let fam = d.families.map { SignalMetrics.Family(key: $0.key, label: $0.label, score: $0.score) }
        if let s = d.snapshot {
            let labels = Dictionary(d.families.map { ($0.key, $0.label) }, uniquingKeysWith: { a, _ in a })
            return SignalMetrics(
                price: s.price, composite: s.composite,
                families: s.families.map { f in SignalMetrics.Family(key: f.key, label: f.label.isEmpty ? labels[f.key] ?? f.key : f.label, score: f.score) },
                relVolume: s.relativeVolume, rsi: s.rsi,
                support: level(s.nearestSupport?.price, s.nearestSupport?.touches), resistance: level(s.nearestResistance?.price, s.nearestResistance?.touches),
                newsScore: s.newsScore, topNews: s.topNews.map { SignalMetrics.News(title: $0.title, tone: $0.tone) }
            )
        }
        return SignalMetrics(
            price: d.price, composite: d.score?.value, families: fam, relVolume: d.liquidity?.relativeVolume, rsi: nil,
            support: level(d.structure?.nearestSupport?.price, d.structure?.nearestSupport?.touches),
            resistance: level(d.structure?.nearestResistance?.price, d.structure?.nearestResistance?.touches),
            newsScore: fam.first { $0.key == "news" }?.score ?? nil, topNews: nil
        )
    }

    /// Thresholds under which a change is not worth telling.
    public enum Limits {
        public static let composite = 5.0, family = 10.0, volume = 0.3, rsi = 10.0, news = 15.0, level = 0.005
    }

    private static func x1(_ v: Double) -> String { "\(DecisionText.num(v, digits: 1))×" }
    private static func sameLevel(_ a: Double, _ b: Double) -> Bool { abs(a - b) / max(abs(a), 1e-9) <= Limits.level }
    private static func score(_ v: Double) -> String { DecisionText.signedScore(v) }

    /// What changed between two snapshots, most telling first: composite score, families (largest moves first, 4 at
    /// most), relative volume, RSI, support / resistance (broken, crossed, confirmed or replaced) and news. Pure.
    public static func explainChange(_ prev: SignalMetrics, _ next: SignalMetrics) -> [String] {
        var out: [String] = []
        if let a = prev.composite, let b = next.composite, abs(b - a) >= Limits.composite {
            out.append("Score composite \(score(a)) → \(score(b))")
        }
        let before = Dictionary(prev.families.map { ($0.key, $0) }, uniquingKeysWith: { a, _ in a })
        var moves: [(d: Double, text: String)] = []
        for f in next.families {
            guard let p = before[f.key] else { continue }
            if let ps = p.score, let fs = f.score {
                let d = fs - ps
                if abs(d) >= Limits.family { moves.append((abs(d), "\(f.label) \(score(d)) pts (\(score(ps)) → \(score(fs)))")) }
            } else if p.score != nil && f.score == nil {
                moves.append((0, "\(f.label) : plus mesuré(e) (données indisponibles)"))
            } else if p.score == nil, let fs = f.score {
                moves.append((0, "\(f.label) : de nouveau mesuré(e) (\(score(fs)))"))
            }
        }
        // Stable sort, largest move first (the web's Array.sort is stable).
        out += moves.enumerated().sorted { $0.element.d != $1.element.d ? $0.element.d > $1.element.d : $0.offset < $1.offset }.prefix(4).map(\.element.text)
        if let a = prev.relVolume, let b = next.relVolume, abs(b - a) >= Limits.volume {
            out.append("Volume en \(b > a ? "hausse" : "baisse") (\(x1(a)) → \(x1(b)) la moyenne)")
        }
        if let a = prev.rsi, let b = next.rsi, abs(b - a) >= Limits.rsi {
            out.append("RSI \(DecisionText.num(a, digits: 0)) → \(DecisionText.num(b, digits: 0))")
        }
        let usd = DecisionText.usd
        if let r0 = prev.resistance, let p = next.price, p > r0.price, prev.price.map({ $0 <= r0.price }) ?? true {
            out.append("Résistance \(usd(r0.price)) franchie (prix \(usd(p)))")
        } else if let r0 = prev.resistance, let r1 = next.resistance, sameLevel(r0.price, r1.price), r1.touches > r0.touches {
            out.append("Résistance \(usd(r1.price)) confirmée (touchée \(r1.touches) fois)")
        } else if let r0 = prev.resistance, let r1 = next.resistance, !sameLevel(r0.price, r1.price) {
            out.append("Résistance la plus proche : \(usd(r0.price)) → \(usd(r1.price))")
        }
        if let s0 = prev.support, let p = next.price, p < s0.price, prev.price.map({ $0 >= s0.price }) ?? true {
            out.append("Support \(usd(s0.price)) cassé (prix \(usd(p)))")
        } else if let s0 = prev.support, let s1 = next.support, sameLevel(s0.price, s1.price), s1.touches > s0.touches {
            out.append("Support \(usd(s1.price)) confirmé (touché \(s1.touches) fois)")
        } else if let s0 = prev.support, let s1 = next.support, !sameLevel(s0.price, s1.price) {
            out.append("Support le plus proche : \(usd(s0.price)) → \(usd(s1.price))")
        }
        if let n = next.topNews, n.tone != "neutral", n.title != prev.topNews?.title {
            out.append("Actualité \(n.tone == "negative" ? "négative" : "positive") : « \(n.title) »")
        } else if let a = prev.newsScore, let b = next.newsScore, abs(b - a) >= Limits.news {
            out.append("Ton des actualités \(score(a)) → \(score(b))")
        }
        return out
    }

    /// What the decision says is missing and what would change it, with its measurements.
    public static func snapshot(of d: Decision, at: Double) -> ConfigSnapshot {
        ConfigSnapshot(
            verdict: d.verdict, level: d.level, label: d.label, levelLabel: d.levelLabel,
            missing: d.setup.steps.filter { $0.state != .ok }.map { s in
                "\(s.label) : \(stepState[s.state] ?? s.state.rawValue)\(s.detail.isEmpty ? "" : " (\(s.detail))")"
            },
            triggers: d.toBuy.map { c in
                if let level = c.level, !c.text.contains("$") && !c.text.contains("€") { return "\(c.text) (\(usd(level)))" }
                return c.text
            },
            at: at,
            rating: d.rating,
            ratingLabel: d.rating.map { r in d.ratingLabel.flatMap { $0.isEmpty ? nil : $0 } ?? r.rawValue },
            metrics: metrics(of: d)
        )
    }

    public static func key(kind: Kind, symbol: String, personal: Bool) -> String { "\(kind.rawValue):\(symbol):\(personal ? "p" : "i")" }

    /// A transition when the verdict, the level or the rating (when both snapshots have one) changed; nil for the first
    /// sighting or the same configuration. `changes` explains it when both snapshots kept their measurements.
    public static func diff(_ prev: ConfigSnapshot?, _ next: ConfigSnapshot, symbol: String, kind: Kind, name: String, personal: Bool) -> ConfigTransition? {
        guard let prev else { return nil }
        let ratingChanged = prev.rating != nil && next.rating != nil && prev.rating != next.rating
        guard prev.verdict != next.verdict || prev.level != next.level || ratingChanged else { return nil }
        let side = { (s: ConfigSnapshot) in
            ConfigSide(verdict: s.verdict, level: s.level, label: s.label, levelLabel: s.levelLabel, rating: s.rating, ratingLabel: s.rating == nil ? nil : s.ratingLabel)
        }
        let changes = prev.metrics.flatMap { p in next.metrics.map { explainChange(p, $0) } } ?? []
        return ConfigTransition(symbol: symbol, kind: kind, name: name, personal: personal, at: next.at, since: prev.at,
                                from: side(prev), to: side(next), missing: next.missing, triggers: next.triggers, changes: changes)
    }

    /// New state after seeing a decision: baseline replaced, transition prepended when the configuration changed.
    /// A decision whose verdict, level or rating this version does not know is ignored (never stored).
    public static func apply(_ state: ConfigState, _ d: Decision, personal: Bool, now: Double) -> (state: ConfigState, transition: ConfigTransition?) {
        let next = snapshot(of: d, at: now)
        guard next.isValid else { return (state, nil) }
        let k = key(kind: d.kind, symbol: d.symbol, personal: personal)
        let t = diff(state.last[k], next, symbol: d.symbol, kind: d.kind, name: d.name.isEmpty ? d.symbol : d.name, personal: personal)
        var s = state
        s.version = stateVersion
        s.last[k] = next
        if let t { s.transitions = Array(([t] + s.transitions).prefix(maxTransitions)) }
        return (s, t)
    }

    /// Stored state from its JSON; damaged data gives an empty state.
    public static func parse(_ data: Data?) -> ConfigState {
        guard let data, let s = try? JSONDecoder().decode(ConfigState.self, from: data) else { return ConfigState() }
        return s
    }

    /// The latest transition of this asset if it led to the configuration shown now (same verdict and level), for
    /// "Pourquoi le signal a changé depuis …"; nil otherwise.
    public static func latestChange(_ transitions: [ConfigTransition], _ d: Decision) -> ConfigTransition? {
        let personal = d.isPersonal
        guard let t = transitions.first(where: { $0.symbol == d.symbol && $0.kind == d.kind && $0.personal == personal }) else { return nil }
        return t.to.verdict == d.verdict && t.to.level == d.level ? t : nil
    }
}
