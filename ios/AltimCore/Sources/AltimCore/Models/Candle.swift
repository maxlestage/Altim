import Foundation

/// Une bougie OHLCV.
public struct Candle: Codable, Hashable, Sendable {
    public let time: Date
    public let open: Double
    public let high: Double
    public let low: Double
    public let close: Double
    public let volume: Double
    /// `false` pour la bougie en cours de formation (ne doit pas servir aux signaux : elle "repeint").
    public let isClosed: Bool

    public init(time: Date, open: Double, high: Double, low: Double, close: Double, volume: Double, isClosed: Bool = true) {
        self.time = time
        self.open = open
        self.high = high
        self.low = low
        self.close = close
        self.volume = volume
        self.isClosed = isClosed
    }

    /// Une bougie est valide si ses prix sont finis, positifs et cohérents.
    public var isValid: Bool {
        let values = [open, high, low, close, volume]
        guard values.allSatisfy({ $0.isFinite }) else { return false }
        return low > 0 && high >= low && high >= max(open, close) && low <= min(open, close) && volume >= 0
    }
}

public extension Array where Element == Candle {
    var closes: [Double] { map(\.close) }
    var highs: [Double] { map(\.high) }
    var lows: [Double] { map(\.low) }
    var volumes: [Double] { map(\.volume) }

    /// Bougies clôturées et valides uniquement, triées chronologiquement, sans doublons.
    func sanitized() -> [Candle] {
        var seen = Set<Date>()
        return filter { $0.isClosed && $0.isValid }
            .sorted { $0.time < $1.time }
            .filter { seen.insert($0.time).inserted }
    }
}
