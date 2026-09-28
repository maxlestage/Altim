import Foundation

// « Pourquoi ça bouge ? »: GET /api/why (backend/src/engine/why.rs, camelCase) and the optional POST /api/ask, offered
// only when the server says `askEnabled`. Observations that happen together, never presented as causes. Same labels as
// web/src/webapp/WhyCard.tsx.

public struct WhyReport: Codable, Sendable {
    public enum Direction: String, Codable, Sendable {
        case up, down, neutral
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .neutral }
        public var icon: String { self == .up ? "↗" : self == .down ? "↘" : "→" }
        public var word: String { self == .up ? "haussier" : self == .down ? "baissier" : "neutre" }
    }

    public enum Magnitude: String, Codable, Sendable {
        case low, medium, high
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .low }
        public var word: String { self == .high ? "fort" : self == .medium ? "moyen" : "faible" }
    }

    public enum Certainty: String, Codable, Sendable {
        case observed, possibleCorrelation, unverifiable
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unverifiable }
        public var tone: Tone {
            switch self {
            case .observed: return .good
            case .possibleCorrelation: return .neutral
            case .unverifiable: return .warn
            }
        }
    }

    public struct Factor: Codable, Sendable, Identifiable {
        public var key: String
        public var label: String
        public var direction: Direction
        public var magnitude: Magnitude
        public var detail: String
        public var source: String
        public var certainty: Certainty
        public var certaintyLabel: String
        public var brief: String?
        public var id: String { key }

        /// "Sens haussier, ampleur forte · source : Binance".
        public var meta: String { "Sens \(direction.word), ampleur \(magnitude.word) · source : \(source)" }
    }

    public struct Change: Codable, Sendable {
        public var pct: Double
        public var price: Double
        public var previousClose: Double
        public var previousCloseTime: Double
        public var atrPct: Double?
    }

    public struct Volume: Codable, Sendable {
        public var ratio: Double
        public var day: Double
        public var volume: Double
        public var average: Double
    }

    public struct NotCovered: Codable, Sendable, Identifiable {
        public var key: String
        public var label: String
        public var reason: String
        public var id: String { key }
    }

    public struct Source: Codable, Sendable {
        public var name: String
        public var ok: Bool
        public var detail: String
    }

    public var symbol: String
    public var kind: Kind
    public var name: String
    public var asOf: Double
    public var change: Change?
    public var volume: Volume?
    public var factors: [Factor]
    public var notCovered: [NotCovered]
    public var summary: String
    public var disclaimer: String
    /// The optional question box is offered (a key is configured on the server).
    public var askEnabled: Bool?
    public var sources: [Source]?

    /// "Non couvert : actualités (aucune source) ; …." or nil.
    public var notCoveredText: String? {
        guard !notCovered.isEmpty else { return nil }
        return "Non couvert : " + notCovered.map { n -> String in
            var reason = n.reason
            if reason.hasSuffix(".") { reason.removeLast() }
            return "\(n.label.lowercased()) (\(reason))"
        }.joined(separator: " ; ") + "."
    }
}

/// Answer of POST /api/ask.
public struct AskAnswer: Decodable, Sendable {
    public var answer: String
    public var model: String
    public var question: String
    public var disclaimer: String
    /// The data sent to the model held the last decision computed by the server.
    public var usedDecision: Bool

    private enum Keys: String, CodingKey { case answer, model, question, data, disclaimer }
    private enum DataKeys: String, CodingKey { case derniereDecision }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        answer = try c.decode(String.self, forKey: .answer)
        model = (try? c.decode(String.self, forKey: .model)) ?? ""
        question = (try? c.decode(String.self, forKey: .question)) ?? ""
        disclaimer = (try? c.decode(String.self, forKey: .disclaimer)) ?? ""
        let data = try? c.nestedContainer(keyedBy: DataKeys.self, forKey: .data)
        usedDecision = data.map { $0.contains(.derniereDecision) && (try? $0.decodeNil(forKey: .derniereDecision)) == false } ?? false
    }

    /// "Données utilisées : les observations ci-dessus et la dernière décision calculée."
    public var dataText: String { "Données utilisées : les observations ci-dessus\(usedDecision ? " et la dernière décision calculée" : "")." }
}

public enum Why {
    public static let maxQuestion = 500
    public static let askNote = "Réponse d'un modèle d'IA (Claude, Anthropic) à partir des seules données ci-dessus, envoyées à Anthropic avec votre question (jamais vos avoirs). Elle peut se tromper ; ce n'est pas un conseil. Quelques questions par minute au plus."

    /// A question can be sent (3 characters at least, 500 at most once trimmed).
    public static func canAsk(_ q: String) -> Bool {
        let t = q.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.count >= 3 && t.count <= maxQuestion
    }

    static func askBody(_ asset: Asset, question: String) throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "symbol": asset.symbol, "kind": asset.kind.rawValue, "question": question.trimmingCharacters(in: .whitespacesAndNewlines),
        ], options: [.sortedKeys])
    }
}

extension AltimClient {
    /// « Pourquoi ça bouge ? » of one asset (sources already cached by the server).
    public func why(_ asset: Asset) async throws -> WhyReport {
        try await getWhy(["symbol": asset.symbol, "kind": asset.kind.rawValue])
    }

    /// Optional question on the same data (503 with a French message when the server has no key).
    public func ask(_ asset: Asset, question: String) async throws -> AskAnswer {
        try await postJSON("/api/ask", Why.askBody(asset, question: question))
    }
}
