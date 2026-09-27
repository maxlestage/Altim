import SwiftUI
import AltimKit

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var confirmLogout = false

    var body: some View {
        @Bindable var model = model
        Form {
            Section("Serveur") {
                KeyValue(key: "Adresse", value: model.serverURL?.host ?? "—")
                KeyValue(key: "Identifiant", value: model.user ?? "accès ouvert")
                Button("Se déconnecter", role: .destructive) { confirmLogout = true }
            }
            Section {
                Toggle("Verrouiller avec Face ID", isOn: $model.faceIDLock)
            } header: {
                Text("Sécurité")
            } footer: {
                Text("Face ID (ou le code de l'iPhone) est demandé à l'ouverture et après 2 minutes en arrière-plan. Le mot de passe et la session sont chiffrés dans le trousseau de cet iPhone, jamais sauvegardés dans iCloud.")
            }
            Section("Données") {
                KeyValue(key: "Sources de prix", value: "40 (23 crypto, 17 actions)")
                KeyValue(key: "Prix en direct", value: "7 bourses crypto, actions toutes les 5 s")
                Text("Chaque prix est la médiane des sources qui s'accordent ; une source qui s'écarte est écartée. Les calculs (signaux, zones de Fibonacci, garde-fou, sélection) sont faits sur votre serveur, comme sur le site.")
                    .font(.footnote).foregroundStyle(Theme.textSecondary)
            }
            Section("À savoir") {
                Text("Altim est un conseiller : il ne passe aucun ordre et n'a accès à aucun de vos comptes. Les signaux sont des probabilités mesurées sur l'historique, jamais des certitudes ; investir comporte un risque de perte en capital.")
                    .font(.footnote).foregroundStyle(Theme.textSecondary)
                KeyValue(key: "Version", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "—")
            }
        }
        .altimScreen()
        .navigationTitle("Réglages")
        .confirmationDialog("Se déconnecter ?", isPresented: $confirmLogout, titleVisibility: .visible) {
            Button("Se déconnecter", role: .destructive) { Task { await model.logout() } }
        } message: {
            Text("Le mot de passe enregistré sur cet iPhone sera effacé. Vos avoirs et votre radar restent.")
        }
    }
}
