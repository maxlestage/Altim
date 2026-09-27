import SwiftUI

/// Avertissement obligatoire au premier lancement.
struct DisclaimerView: View {
    @Environment(AppSettings.self) private var settings
    @State private var understood = false

    var body: some View {
        ZStack {
            CyberGridBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    Text("ALTIM")
                        .font(Theme.display(56))
                        .foregroundStyle(Theme.accentGradient)
                        .neonGlow(Theme.cyan, radius: 14)
                        .padding(.top, 40)
                    Text("Avant de commencer")
                        .font(.title2.bold())

                    VStack(alignment: .leading, spacing: 14) {
                        point("waveform.path.ecg", "Les signaux sont des probabilités calculées sur l'historique, jamais des certitudes. Aucun outil ne garantit un gain.")
                        point("exclamationmark.triangle", "Le trading de crypto-actifs et d'actions comporte un risque de perte totale du capital investi.")
                        point("testtube.2", "L'application démarre en mode DÉMO (argent fictif). Testez longtemps avant de passer en réel.")
                        point("lock.shield", "Vos clés API restent chiffrées sur cet appareil. N'activez jamais la permission de retrait.")
                        point("person.fill.checkmark", "Chaque ordre doit être confirmé par Face ID / code. Altim n'exécute rien sans vous.")
                    }
                    .glassCard(glow: Theme.warning)

                    Toggle(isOn: $understood) {
                        Text("J'ai compris que je reste seul responsable de mes décisions d'investissement.")
                            .font(.subheadline)
                    }
                    .toggleStyle(.switch)
                    .tint(Theme.cyan)

                    Button("ENTRER") { settings.acceptedDisclaimer = true }
                        .buttonStyle(NeonButtonStyle())
                        .disabled(!understood)
                        .opacity(understood ? 1 : 0.4)
                }
                .padding(24)
            }
        }
    }

    private func point(_ icon: String, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon)
                .foregroundStyle(Theme.cyan)
                .frame(width: 24)
            Text(text).font(.subheadline).foregroundStyle(.white.opacity(0.85))
        }
    }
}
