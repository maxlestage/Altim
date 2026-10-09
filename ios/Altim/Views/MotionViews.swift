import SwiftUI
import AltimKit

// The app's motion, in the spirit of the web site (frontend/door.js, app.css): the logo traced at launch and after
// the login, prices that roll and flash on a live tick, a verdict that cross-fades when it changes, the « EN DIRECT »
// dot's pulse, sparklines that draw themselves. Restrained (it is a tool): nothing hides or delays a figure, and with
// Reduce Motion everything is still (`accessibilityReduceMotion`). Timings and geometry: AltimKit's `Motion`.

/// One stroke of the Altim logo (`Motion.Logo`) in a square fitted to the rect.
struct LogoStroke: Shape {
    var points: [Motion.Point]

    func path(in rect: CGRect) -> Path {
        let side = min(rect.width, rect.height)
        let origin = CGPoint(x: rect.midX - side / 2, y: rect.midY - side / 2)
        var path = Path()
        for (i, p) in points.enumerated() {
            let pt = CGPoint(x: origin.x + p.x * side, y: origin.y + p.y * side)
            if i == 0 { path.move(to: pt) } else { path.addLine(to: pt) }
        }
        return path
    }
}

/// The Altim logo traced stroke by stroke (`trim(from:to:)`): the peak, then the rising line in cyan, then its dot;
/// over the app's background, gone after `Motion.LaunchTrace.duration`. Shown at launch and after the login; never
/// with Reduce Motion (the caller does not show it).
struct LaunchTraceView: View {
    var done: () -> Void
    @State private var peak: CGFloat = 0
    @State private var line: CGFloat = 0
    @State private var dot = false
    @State private var leaving = false

    var body: some View {
        let size: CGFloat = 112
        let width = size * Motion.Logo.stroke
        let trace = Motion.LaunchTrace.self
        ZStack {
            AppBackground()
            ZStack {
                LogoStroke(points: Motion.Logo.peak)
                    .trim(from: 0, to: peak)
                    .stroke(.white, style: StrokeStyle(lineWidth: width, lineCap: .round, lineJoin: .round))
                LogoStroke(points: Motion.Logo.line)
                    .trim(from: 0, to: line)
                    .stroke(Theme.cyan, style: StrokeStyle(lineWidth: width, lineCap: .round, lineJoin: .round))
                Circle()
                    .fill(Theme.cyan)
                    .frame(width: size * Motion.Logo.dotRadius * 2.2, height: size * Motion.Logo.dotRadius * 2.2)
                    .position(x: size * Motion.Logo.dot.x, y: size * Motion.Logo.dot.y)
                    .scaleEffect(dot ? 1 : 0.2)
                    .opacity(dot ? 1 : 0)
            }
            .frame(width: size, height: size)
            .neonGlow(Theme.cyan, radius: 10)
            .scaleEffect(leaving ? 1.12 : 1)
        }
        .opacity(leaving ? 0 : 1)
        .accessibilityHidden(true)
        .allowsHitTesting(false)
        .task {
            withAnimation(.easeInOut(duration: trace.peak.upperBound - trace.peak.lowerBound)) { peak = 1 }
            withAnimation(.easeInOut(duration: trace.line.upperBound - trace.line.lowerBound).delay(trace.line.lowerBound)) { line = 1 }
            withAnimation(.spring(duration: trace.dot.upperBound - trace.dot.lowerBound).delay(trace.dot.lowerBound)) { dot = true }
            try? await Task.sleep(for: .seconds(trace.duration))
            withAnimation(.easeIn(duration: 0.25)) { leaving = true }
            try? await Task.sleep(for: .seconds(0.25))
            done()
        }
    }
}

/// A live price: its digits roll (`.numericText`) and it flashes cyan when it rises, red when it falls, for
/// `Motion.flash` seconds. Monospaced digits: nothing around it moves. Reduce Motion: the new figure, still.
struct LiveTickStyle: ViewModifier {
    var value: Double?
    var color: Color = .white
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var flash: Motion.Tick?
    @State private var lit = false

    func body(content: Content) -> some View {
        let tint = flash == .down ? Theme.sell : Theme.cyan
        content
            .monospacedDigit()
            .foregroundStyle(lit ? tint : color)
            .background(RoundedRectangle(cornerRadius: 4, style: .continuous).fill(tint.opacity(lit ? 0.25 : 0)).padding(-2))
            .contentTransition(reduceMotion ? .identity : .numericText(value: value ?? 0))
            .animation(reduceMotion ? nil : .snappy(duration: 0.35), value: value)
            .onChange(of: value) { old, new in
                guard !reduceMotion, let t = Motion.Tick.between(old, new) else { return }
                flash = t
                lit = true
                // Lit in this update, faded in the next one (both in one update would never show the flash).
                Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(40))
                    withAnimation(.easeOut(duration: Motion.flash)) { lit = false }
                }
            }
    }
}

extension View {
    /// See `LiveTickStyle`.
    func liveTick(_ value: Double?, color: Color = .white) -> some View { modifier(LiveTickStyle(value: value, color: color)) }
}

/// The « EN DIRECT » dot: a ring leaves it on each beat while the stream is live. Reduce Motion: the dot alone.
struct PulseDot: View {
    var color: Color
    var active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var beat = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 7, height: 7)
            .overlay(
                Circle()
                    .stroke(color, lineWidth: 1)
                    .scaleEffect(beat ? 2.4 : 0.6)
                    .opacity(beat ? 0 : 0.9)
                    .opacity(active && !reduceMotion ? 1 : 0)
            )
            .neonGlow(color, radius: 4)
            .onAppear {
                guard !reduceMotion else { return }
                withAnimation(.easeOut(duration: Motion.livePulse).repeatForever(autoreverses: false)) { beat = true }
            }
    }
}

/// Draws its content from left to right once, the first time it appears (sparklines). Reduce Motion: shown at once.
struct DrawIn: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown: CGFloat = 0

    func body(content: Content) -> some View {
        content
            .mask(alignment: .leading) {
                GeometryReader { g in
                    Rectangle().frame(width: g.size.width * (reduceMotion ? 1 : shown), height: g.size.height)
                }
            }
            .onAppear {
                guard shown == 0 else { return }
                withAnimation(.easeOut(duration: Motion.sparkDraw)) { shown = 1 }
            }
    }
}
