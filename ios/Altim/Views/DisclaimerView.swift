import SwiftUI
import AltimKit

/// Mandatory warning at first launch.
struct DisclaimerView: View {
    @Environment(AppModel.self) private var model
    @State private var understood = false

    var body: some View {
        ZStack {
            AppBackground()
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
                        point("exclamationmark.triangle", "Investir en crypto-actifs et en actions comporte un risque de perte totale du capital investi.")
                        point("lightbulb", "Altim est un conseiller : il ne passe aucun ordre et ne demande aucun accès à vos comptes. Vous décidez, chez votre courtier habituel.")
                        point("lock.shield", "Vos avoirs restent sur cet iPhone, jamais envoyés au serveur. Les analyses viennent de votre serveur Altim privé (40 sources de prix).")
                    }
                    .glassCard(glow: Theme.warning)

                    Toggle(isOn: $understood) {
                        Text("J'ai compris que je reste seul responsable de mes décisions d'investissement.")
                            .font(.subheadline)
                    }
                    .toggleStyle(.switch)
                    .tint(Theme.cyan)

                    Button("ENTRER") { model.acceptedDisclaimer = true }
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
