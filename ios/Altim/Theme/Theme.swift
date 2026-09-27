import SwiftUI
import AltimKit

/// Identité visuelle "néon / holographique" d'Altim.
enum Theme {
    static let background = Color(red: 0.02, green: 0.03, blue: 0.07)
    static let surface = Color(red: 0.06, green: 0.08, blue: 0.15)
    static let cyan = Color(red: 0.0, green: 0.94, blue: 1.0)
    static let magenta = Color(red: 1.0, green: 0.17, blue: 0.84)
    static let violet = Color(red: 0.49, green: 0.3, blue: 1.0)
    static let buy = Color(red: 0.22, green: 1.0, blue: 0.53)
    static let sell = Color(red: 1.0, green: 0.23, blue: 0.36)
    static let warning = Color(red: 1.0, green: 0.78, blue: 0.2)
    static let textSecondary = Color.white.opacity(0.6)

    // Allocation: categorical palette validated (dark mode, all pairs, colorblind readers).
    static let allocCrypto = Color(red: 0x39 / 255, green: 0x87 / 255, blue: 0xE5 / 255)
    static let allocStock = Color(red: 0xD9 / 255, green: 0x59 / 255, blue: 0x26 / 255)
    static let allocCash = Color(red: 0x19 / 255, green: 0x9E / 255, blue: 0x70 / 255)

    static let accentGradient = LinearGradient(colors: [cyan, violet, magenta], startPoint: .leading, endPoint: .trailing)

    static func color(_ tone: Tone) -> Color {
        switch tone {
        case .good: return buy
        case .warn: return warning
        case .bad: return sell
        case .neutral: return cyan
        }
    }

    static func color(forChange change: Double?) -> Color { (change ?? 0) >= 0 ? buy : sell }

    static func mono(_ size: CGFloat, weight: Font.Weight = .semibold) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func display(_ size: CGFloat) -> Font {
        .system(size: size, weight: .heavy, design: .rounded)
    }
}

// MARK: - Modificateurs

struct GlassCard: ViewModifier {
    var glow: Color = Theme.cyan

    func body(content: Content) -> some View {
        content
            .padding(16)
            .background(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(.ultraThinMaterial)
                    .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Theme.surface.opacity(0.55)))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .strokeBorder(LinearGradient(colors: [glow.opacity(0.7), glow.opacity(0.05), Theme.magenta.opacity(0.35)],
                                                 startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 1)
            )
            .shadow(color: glow.opacity(0.18), radius: 18)
    }
}

struct NeonGlow: ViewModifier {
    var color: Color
    var radius: CGFloat = 8

    func body(content: Content) -> some View {
        content
            .shadow(color: color.opacity(0.9), radius: radius / 3)
            .shadow(color: color.opacity(0.5), radius: radius)
    }
}

extension View {
    func glassCard(glow: Color = Theme.cyan) -> some View { modifier(GlassCard(glow: glow)) }
    func neonGlow(_ color: Color, radius: CGFloat = 8) -> some View { modifier(NeonGlow(color: color, radius: radius)) }
}

struct NeonButtonStyle: ButtonStyle {
    var color: Color = Theme.cyan
    var filled = true

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 16, weight: .bold, design: .rounded))
            .tracking(1.5)
            .foregroundStyle(filled ? Color.black : color)
            .frame(maxWidth: .infinity, minHeight: 52)
            .background(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(filled ? AnyShapeStyle(color) : AnyShapeStyle(color.opacity(0.08)))
            )
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(color, lineWidth: filled ? 0 : 1.2))
            .neonGlow(color, radius: configuration.isPressed ? 4 : 12)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(.spring(response: 0.25), value: configuration.isPressed)
    }
}

// MARK: - Fond

/// Fond sombre avec halos (statique : pas d'animation qui use la batterie).
struct AppBackground: View {
    var body: some View {
        ZStack {
            Theme.background
            RadialGradient(colors: [Theme.violet.opacity(0.35), .clear], center: .top, startRadius: 10, endRadius: 420)
            RadialGradient(colors: [Theme.magenta.opacity(0.14), .clear], center: .bottomTrailing, startRadius: 10, endRadius: 380)
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
    }
}
