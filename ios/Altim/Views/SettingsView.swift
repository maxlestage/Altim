import SwiftUI
import AltimKit

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var confirmLogout = false
    @State private var denied = false

    var body: some View {
        @Bindable var model = model
        Form {
            Section("Serveur") {
                KeyValue(key: "Adresse", value: model.serverURL?.host ?? "—")
                KeyValue(key: "Identifiant", value: model.user ?? "accès ouvert")
                Button("Se déconnecter", role: .destructive) { confirmLogout = true }
            }
            Section {
                Toggle("Me prévenir quand je peux acheter", isOn: Binding(
                    get: { model.alertsEnabled },
                    set: { on in
                        if on {
                            Task { denied = !(await BuyNotifications.enable(model)) }
                        } else {
                            model.alertsEnabled = false
                            BuyNotifications.schedule(enabled: model.needsChecks)
                        }
                    }
                ))
                Toggle("Seulement les achats conseillés (signal + zone)", isOn: $model.alertsStrongOnly)
                    .disabled(!model.alertsEnabled)
                if denied {
                    Text("Notifications refusées : autorisez-les dans Réglages de l'iPhone → Notifications → Altim.").foregroundStyle(Theme.warning)
                }
                if let checked = model.lastAlertCheck {
                    Text("Dernière vérification : \(Format.date(checked.timeIntervalSince1970 * 1000, time: true)) · \(model.lastAlerts.filter(\.buy).count) actif(s) achetable(s).")
                        .font(.footnote).foregroundStyle(Theme.textSecondary)
                }
            } header: {
                Text("Notifications d'achat")
            } footer: {
                Text("Votre serveur vérifie le radar et vos avoirs : achetable si le signal 4 h dit ACHAT ou si le prix est dans une zone d'achat Fibonacci, sauf sources en désaccord, risque de choc ou zone cassée. Une notification seulement quand un actif devient achetable ou que la raison change ; l'Apple Watch les reçoit quand l'iPhone est verrouillé. iOS décide du rythme en arrière-plan (au mieux toutes les 15 minutes), et la vérification a lieu aussi à chaque ouverture. Conseil indicatif : Altim ne passe aucun ordre.")
            }
            Section {
                Toggle("Suivi en direct (écran verrouillé et Dynamic Island)", isOn: $model.liveActivityEnabled)
                if let a = model.activityAsset {
                    Button("Arrêter le suivi de \(a.symbol)", role: .destructive) {
                        Task {
                            await LiveActivities.shared.stopAll()
                            model.activityAsset = nil
                        }
                    }
                }
            } header: {
                Text("Live Activity")
            } footer: {
                Text("Sur la fiche d'un actif, le bouton « Suivre » affiche son prix et le verdict d'achat sur l'écran verrouillé et dans la Dynamic Island. Le prix bouge en direct tant qu'Altim est ouvert ; en arrière-plan, il est rafraîchi à chaque vérification des alertes (iOS le signale comme ancien après 30 minutes sans mise à jour).")
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
