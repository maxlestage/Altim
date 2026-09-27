import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Abstraction réseau (injectable pour les tests).
public protocol HTTPTransport: Sendable {
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse)
}

public struct URLSessionTransport: HTTPTransport {
    public init() {}

    public func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        var request = request
        request.timeoutInterval = 15
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw APIError.invalidResponse }
        return (data, http)
    }
}

public enum APIError: Error, LocalizedError, Equatable {
    case invalidResponse
    case http(status: Int, message: String)
    case decoding(String)
    case unavailableInRegion
    case missingCredentials

    public var errorDescription: String? {
        switch self {
        case .invalidResponse: return "Réponse réseau invalide."
        case let .http(status, message): return "Erreur \(status) : \(message)"
        case let .decoding(what): return "Données illisibles (\(what))."
        case .unavailableInRegion: return "Service indisponible depuis votre région."
        case .missingCredentials: return "Clés API manquantes : ajoutez-les dans Réglages."
        }
    }
}

enum JSON {
    static func object(_ data: Data) throws -> [String: Any] {
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw APIError.decoding("objet attendu")
        }
        return obj
    }

    static func array(_ data: Data) throws -> [Any] {
        guard let arr = try JSONSerialization.jsonObject(with: data) as? [Any] else {
            throw APIError.decoding("tableau attendu")
        }
        return arr
    }

    static func double(_ value: Any?) -> Double? {
        switch value {
        case let d as Double: return d
        case let i as Int: return Double(i)
        case let n as NSNumber: return n.doubleValue
        case let s as String: return Double(s)
        default: return nil
        }
    }

    static func decimal(_ value: Any?) -> Decimal? {
        switch value {
        case let s as String: return Decimal(string: s, locale: Locale(identifier: "en_US_POSIX"))
        case let d as Double: return Decimal(string: String(d), locale: Locale(identifier: "en_US_POSIX"))
        case let i as Int: return Decimal(i)
        case let n as NSNumber: return n.decimalValue
        default: return nil
        }
    }
}

extension Decimal {
    /// Arrondi vers le bas au multiple de `step` (règles de quantité des bourses).
    func floored(toStep step: Decimal) -> Decimal {
        guard step > 0 else { return self }
        var ratio = self / step
        var rounded = Decimal()
        NSDecimalRound(&rounded, &ratio, 0, .down)
        return rounded * step
    }

    /// Arrondi au multiple de `step` le plus proche.
    func rounded(toStep step: Decimal) -> Decimal {
        guard step > 0 else { return self }
        var ratio = self / step
        var rounded = Decimal()
        NSDecimalRound(&rounded, &ratio, 0, .plain)
        return rounded * step
    }

    /// Représentation décimale simple, sans notation scientifique, acceptée par les API.
    var plainString: String {
        NSDecimalNumber(decimal: self).description(withLocale: Locale(identifier: "en_US_POSIX"))
    }

    var doubleValue: Double { NSDecimalNumber(decimal: self).doubleValue }
}

public extension Double {
    var decimal: Decimal { Decimal(string: String(self), locale: Locale(identifier: "en_US_POSIX")) ?? Decimal(self) }
}
