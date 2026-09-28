import Foundation

// Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT"):
// the last decision seen per asset (verdict, level, unmet setup steps, buy conditions) is kept on the iPhone and
// compared with each new one (Radar refresh, asset page). The server is stateless: the diff is done here.
// Port of web/src/webapp/config-changes.ts (same keys, same texts, same limits).

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

    var isValid: Bool { verdict != .unknown && level != .unknown && at.isFinite }
}

/// Verdict and level of one side of a transition.
public struct ConfigSide: Codable, Sendable, Equatable {
    public var verdict: Decision.Verdict
    public var level: Decision.Level
    public var label: String
    public var levelLabel: String

    var isValid: Bool { verdict != .unknown && level != .unknown }
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

    public var id: String { "\(kind.rawValue):\(symbol):\(personal):\(at)" }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }

    /// "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the levels when only the level changed).
    public var title: String {
        let (a, b) = from.verdict == to.verdict ? (from.levelLabel, to.levelLabel) : (from.label, to.label)
        return "🚨 \(symbol) — changement de configuration : \(a) → \(b)"
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

public struct ConfigState: Codable, Sendable, Equatable {
    public var version: Int = 1
    public var last: [String: ConfigSnapshot] = [:]
    /// Newest first, at most `ConfigChanges.maxTransitions`.
    public var transitions: [ConfigTransition] = []

    public init() {}

    private enum CodingKeys: String, CodingKey { case version, last, transitions }

    /// A damaged or older entry is dropped, never trusted; another version starts empty.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        guard (try? c.decode(Int.self, forKey: .version)) == 1 else { return }
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

    private static let stepState: [Decision.StepState: String] = [.no: "pas encore", .unknown: "non vérifiable"]

    static func usd(_ v: Double) -> String { "\(JSFormat.fr(v, max: v >= 1 ? 2 : 6)) $" }

    /// What the decision says is missing and what would change it.
    public static func snapshot(of d: Decision, at: Double) -> ConfigSnapshot {
        ConfigSnapshot(
            verdict: d.verdict, level: d.level, label: d.label, levelLabel: d.levelLabel,
            missing: d.setup.steps.filter { $0.state != .ok }.map { s in
                "\(s.label) : \(stepState[s.state] ?? s.state.rawValue)\(s.detail.isEmpty ? "" : " (\(s.detail))")"
            },
            triggers: d.toBuy.map { c in
                if let level = c.level, !c.text.contains("$") { return "\(c.text) (\(usd(level)))" }
                return c.text
            },
            at: at
        )
    }

    public static func key(kind: Kind, symbol: String, personal: Bool) -> String { "\(kind.rawValue):\(symbol):\(personal ? "p" : "i")" }

    /// A transition when the verdict or the level changed; nil for the first sighting or the same configuration.
    public static func diff(_ prev: ConfigSnapshot?, _ next: ConfigSnapshot, symbol: String, kind: Kind, name: String, personal: Bool) -> ConfigTransition? {
        guard let prev, prev.verdict != next.verdict || prev.level != next.level else { return nil }
        let side = { (s: ConfigSnapshot) in ConfigSide(verdict: s.verdict, level: s.level, label: s.label, levelLabel: s.levelLabel) }
        return ConfigTransition(symbol: symbol, kind: kind, name: name, personal: personal, at: next.at, since: prev.at,
                                from: side(prev), to: side(next), missing: next.missing, triggers: next.triggers)
    }

    /// New state after seeing a decision: baseline replaced, transition prepended when the configuration changed.
    /// A decision whose verdict or level this version does not know is ignored (never stored).
    public static func apply(_ state: ConfigState, _ d: Decision, personal: Bool, now: Double) -> (state: ConfigState, transition: ConfigTransition?) {
        let next = snapshot(of: d, at: now)
        guard next.isValid else { return (state, nil) }
        let k = key(kind: d.kind, symbol: d.symbol, personal: personal)
        let t = diff(state.last[k], next, symbol: d.symbol, kind: d.kind, name: d.name.isEmpty ? d.symbol : d.name, personal: personal)
        var s = state
        s.last[k] = next
        if let t { s.transitions = Array(([t] + s.transitions).prefix(maxTransitions)) }
        return (s, t)
    }

    /// Stored state from its JSON; damaged data gives an empty state.
    public static func parse(_ data: Data?) -> ConfigState {
        guard let data, let s = try? JSONDecoder().decode(ConfigState.self, from: data) else { return ConfigState() }
        return s
    }
}
