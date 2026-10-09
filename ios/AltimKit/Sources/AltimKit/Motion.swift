import Foundation

/// The pure side of the app's motion (the web's frontend/door.js, motion.js and app.css micro-animations), shared by
/// the iPhone views and tested on Linux: the Altim logo's strokes, the launch trace's timing, the direction of a price
/// tick, the sparkline's draw-in. Every animation has a still state (reduced motion), and none hides or delays a figure.
public enum Motion {
    /// A point of a drawing in a unit box (0…1, y down).
    public struct Point: Equatable, Sendable {
        public let x: Double
        public let y: Double
        public init(_ x: Double, _ y: Double) {
            self.x = x
            self.y = y
        }
    }

    /// The Altim logo (web/public/logo.svg, 1024 box): the peak, the rising line in the accent colour, its dot.
    public enum Logo {
        public static let peak: [Point] = [pt(232, 780), pt(512, 214), pt(792, 780)]
        public static let line: [Point] = [pt(330, 600), pt(430, 520), pt(520, 575), pt(700, 420)]
        public static let dot = pt(700, 420)
        public static let dotRadius = 30.0 / 1024
        /// Stroke width of the logo, relative to its box.
        public static let stroke = 54.0 / 1024

        private static func pt(_ x: Double, _ y: Double) -> Point { Point(x / 1024, y / 1024) }
    }

    /// The launch trace: the peak drawn first, the line from 75 % of it, then the dot; then a short hold before the
    /// app shows. Seconds.
    public enum LaunchTrace {
        public static let peak = 0.0...0.55
        public static let line = 0.42...0.8
        public static let dot = 0.78...0.95
        /// When the trace is over and the app takes the screen.
        public static let duration = 1.05

        /// How much of a stroke is drawn at `t` seconds after launch (0 → 1, eased).
        public static func progress(_ span: ClosedRange<Double>, at t: Double) -> Double {
            ease(((t - span.lowerBound) / (span.upperBound - span.lowerBound)).clamped)
        }
    }

    /// Hermite smoothstep 0 → 1 (the web's `smoothstep(0, 1, x)`).
    public static func ease(_ x: Double) -> Double {
        let t = x.clamped
        return t * t * (3 - 2 * t)
    }

    /// Direction of a live tick: the price flashes cyan when it rises, red when it falls, nothing otherwise (first
    /// price, same price, a missing value).
    public enum Tick: Equatable, Sendable {
        case up, down

        public static func between(_ old: Double?, _ new: Double?) -> Tick? {
            guard let old, let new, old.isFinite, new.isFinite, old != new else { return nil }
            return new > old ? .up : .down
        }
    }

    /// How long a tick's flash lasts (seconds), like the web's `.live-price.flash-up` (0.9 s).
    public static let flash = 0.9
    /// How long a sparkline takes to draw itself the first time (seconds), like the web's `.spark` (0.9 s).
    public static let sparkDraw = 0.9
    /// One beat of the « EN DIRECT » dot (seconds), like the web's `.live-badge` (1.6 s).
    public static let livePulse = 1.6
}

extension Double {
    fileprivate var clamped: Double { Swift.min(Swift.max(self, 0), 1) }
}
