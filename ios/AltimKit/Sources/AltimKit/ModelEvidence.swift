import Foundation

// « Preuve du modèle » inside each decision (`modelEvidence` of /api/decision, backend engine/model_evidence.rs): what
// the cross-asset validation (« Validation du modèle ») says about the asset's class and the regime it is in now.
// Same fields and texts as web/src/webapp/decision.ts and DecisionCard.tsx (`EvidenceLine`). Older answers have no
// `modelEvidence` (nil); inside the block only `available` and `text` are required (as the web's check), every other
// field falls back to the backend's default when missing.

/// Verdict of the cross-asset validation on a group of trades.
public enum ProofVerdict: String, Codable, Sendable {
    case insufficient, edge, negative, unproven, unknown
    public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
}

public struct ModelEvidence: Codable, Sendable, Equatable {
    /// False when no validation report is computed yet (the text says so).
    public var available: Bool
    /// "stock" | "btc" | "eth" | "altcoin"
    public var assetClass: String
    public var classLabel: String
    public var classVerdict: ProofVerdict?
    public var classVerdictLabel: String?
    /// Assets of the class tested; those where the signal beat buy-and-hold ("n/m" in `beatHold`).
    public var assets: Int
    public var beatHoldCount: Int
    public var beatHold: String?
    public var trades: Int
    public var tStat: Double?
    /// "bull" | "bear" | "range" | "crisis" | "unknown"
    public var regime: String
    public var regimeLabel: String
    public var regimeVerdict: ProofVerdict?
    public var regimeTrades: Int
    public var regimeTStat: Double?
    /// Class verdict "negative" or buy-and-hold better on more than two thirds of the class: confidence capped at 60.
    public var weak: Bool
    public var text: String
    /// Time of the validation report (ms); nil when unavailable.
    public var asOf: Double?
    public var link: String

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = try c.decode(Bool.self, forKey: .available)
        text = try c.decode(String.self, forKey: .text)
        assetClass = try c.decodeIfPresent(String.self, forKey: .assetClass) ?? "stock"
        classLabel = try c.decodeIfPresent(String.self, forKey: .classLabel) ?? ""
        classVerdict = try c.decodeIfPresent(ProofVerdict.self, forKey: .classVerdict)
        classVerdictLabel = try c.decodeIfPresent(String.self, forKey: .classVerdictLabel)
        assets = try c.decodeIfPresent(Int.self, forKey: .assets) ?? 0
        beatHoldCount = try c.decodeIfPresent(Int.self, forKey: .beatHoldCount) ?? 0
        beatHold = try c.decodeIfPresent(String.self, forKey: .beatHold)
        trades = try c.decodeIfPresent(Int.self, forKey: .trades) ?? 0
        tStat = try c.decodeIfPresent(Double.self, forKey: .tStat)
        regime = try c.decodeIfPresent(String.self, forKey: .regime) ?? "unknown"
        regimeLabel = try c.decodeIfPresent(String.self, forKey: .regimeLabel) ?? ""
        regimeVerdict = try c.decodeIfPresent(ProofVerdict.self, forKey: .regimeVerdict)
        regimeTrades = try c.decodeIfPresent(Int.self, forKey: .regimeTrades) ?? 0
        regimeTStat = try c.decodeIfPresent(Double.self, forKey: .regimeTStat)
        weak = try c.decodeIfPresent(Bool.self, forKey: .weak) ?? false
        asOf = try c.decodeIfPresent(Double.self, forKey: .asOf)
        link = try c.decodeIfPresent(String.self, forKey: .link) ?? ""
    }

    /// Colour of the block's edge, as the web's `dec-proof` classes: not computed, weak, no edge shown, edge.
    public enum Tone: String, Sendable { case na, weak, unproven, edge }

    public var tone: Tone {
        guard available else { return .na }
        if weak { return .weak }
        switch classVerdict {
        case .edge: return .edge
        case .negative: return .weak
        default: return .unproven
        }
    }

    public static let title = "Preuve du modèle"
    public static let linkText = "Voir la validation du modèle →"

    /// "bat la détention : 2/22", only when the validation is available.
    public var beatHoldChip: String? {
        guard available, let b = beatHold, !b.isEmpty else { return nil }
        return "bat la détention : \(b)"
    }

    /// " · calculée le 29/09 à 08:22" after the link; nil without a report.
    public var asOfText: String? { asOf.map { " · calculée le \(DecisionGuidance.shortDateTime($0))" } }
}
