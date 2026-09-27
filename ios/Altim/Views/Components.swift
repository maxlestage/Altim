import SwiftUI
import Charts
import AltimCore

/// Pastille d'action (ACHAT / VENTE / ATTENDRE).
struct ActionBadge: View {
    let action: SignalAction
    var compact = false

    var body: some View {
        let color = Theme.color(for: action)
        Text(action.label)
            .font(.system(size: compact ? 11 : 13, weight: .heavy, design: .rounded))
            .tracking(1.2)
            .padding(.horizontal, compact ? 8 : 12)
            .padding(.vertical, compact ? 4 : 6)
            .foregroundStyle(color)
            .background(Capsule().fill(color.opacity(0.14)))
            .overlay(Capsule().strokeBorder(color.opacity(0.8), lineWidth: 1))
            .neonGlow(color, radius: 6)
    }
}

/// Jauge semi-circulaire du score (-100 → +100).
struct ScoreGauge: View {
    let score: Double
    let confidence: Double

    var body: some View {
        let normalized = (score + 100) / 200
        ZStack {
            Circle()
                .trim(from: 0.5, to: 1)
                .stroke(Color.white.opacity(0.08), style: StrokeStyle(lineWidth: 14, lineCap: .round))
            Circle()
                .trim(from: 0.5, to: 1)
                .stroke(AngularGradient(colors: [Theme.sell, Theme.warning, Theme.cyan, Theme.buy],
                                        center: .center, startAngle: .degrees(180), endAngle: .degrees(360)),
                        style: StrokeStyle(lineWidth: 14, lineCap: .round))
                .opacity(0.9)
            // Aiguille
            Capsule()
                .fill(Color.white)
                .frame(width: 4, height: 64)
                .offset(y: -32)
                .rotationEffect(.degrees(-90 + normalized * 180))
                .neonGlow(.white, radius: 6)
                .animation(.spring(response: 0.8, dampingFraction: 0.6), value: score)
            VStack(spacing: 2) {
                Text(String(format: "%+.0f", score))
                    .font(Theme.mono(30, weight: .bold))
                Text("confiance \(Int(confidence.rounded())) %")
                    .font(.caption2)
                    .foregroundStyle(Theme.textSecondary)
            }
            .offset(y: 22)
        }
        .frame(width: 170, height: 170)
        .frame(height: 120, alignment: .top)
    }
}

/// Mini-graphique de tendance.
struct Sparkline: View {
    let values: [Double]

    var body: some View {
        let up = (values.last ?? 0) >= (values.first ?? 0)
        let color = up ? Theme.buy : Theme.sell
        Chart(Array(values.enumerated()), id: \.offset) { item in
            LineMark(x: .value("i", item.offset), y: .value("p", item.element))
                .interpolationMethod(.catmullRom)
                .foregroundStyle(color)
                .lineStyle(StrokeStyle(lineWidth: 1.6))
        }
        .chartXAxis(.hidden)
        .chartYAxis(.hidden)
        .chartYScale(domain: (values.min() ?? 0)...(max(values.max() ?? 1, (values.min() ?? 0) + 1e-9)))
        .neonGlow(color, radius: 4)
    }
}

struct SectionTitle: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: 12, weight: .bold, design: .monospaced))
            .tracking(2)
            .foregroundStyle(Theme.cyan.opacity(0.9))
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
