import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimCore

enum Fixtures {
    /// Série synthétique : tendance + oscillation + bruit déterministe.
    static func candles(count: Int, start: Double = 100, drift: Double, amplitude: Double = 2, seed: UInt64 = 42) -> [Candle] {
        var rng = seed
        func noise() -> Double {
            rng = rng &* 6364136223846793005 &+ 1442695040888963407
            return Double(rng >> 33) / Double(1 << 31) - 0.5
        }
        var candles: [Candle] = []
        var price = start
        for i in 0..<count {
            let open = price
            price = max(1, price * (1 + drift) + amplitude * sin(Double(i) / 6) * 0.3 + noise() * amplitude * 0.5)
            let close = price
            let high = max(open, close) * (1 + abs(noise()) * 0.01)
            let low = min(open, close) * (1 - abs(noise()) * 0.01)
            let volume = 1000 + (close > open ? 400 : 0) + noise() * 200
            candles.append(Candle(time: Date(timeIntervalSince1970: 1_700_000_000 + Double(i) * 3600),
                                  open: open, high: high, low: low, close: close, volume: volume))
        }
        return candles
    }
}

/// Transport HTTP simulé : réponses par chemin, enregistrement des requêtes.
final class MockTransport: HTTPTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var _requests: [URLRequest] = []
    var routes: [String: (Int, String)] = [:]

    var requests: [URLRequest] { lock.lock(); defer { lock.unlock() }; return _requests }

    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        lock.lock()
        _requests.append(request)
        lock.unlock()
        let key = "\(request.httpMethod ?? "GET") \(request.url!.path)"
        let (status, body) = routes[key] ?? (404, #"{"code":-1,"msg":"no route \#(key)"}"#)
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!
        return (Data(body.utf8), response)
    }
}
