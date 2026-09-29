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
                Text("Votre serveur vérifie le radar et vos avoirs : achetable seulement si la décision complète de l'actif dit ACHETER ou ZONE D'ACHAT (signal 4 h ou zone Fibonacci, sans source en désaccord, choc ni zone cassée). Une notification seulement quand un actif devient achetable ou que la raison change ; l'Apple Watch les reçoit quand l'iPhone est verrouillé. iOS décide du rythme en arrière-plan (au mieux toutes les 15 minutes), et la vérification a lieu aussi à chaque ouverture. Conseil indicatif : Altim ne passe aucun ordre.")
            }
            Section {
                Toggle("Me prévenir des actualités importantes", isOn: Binding(
                    get: { model.newsAlertsEnabled },
                    set: { on in
                        if on {
                            Task {
                                let granted = await BuyNotifications.authorize()
                                denied = !granted
                                model.newsAlertsEnabled = granted
                                BuyNotifications.schedule(enabled: model.needsChecks)
                            }
                        } else {
                            model.newsAlertsEnabled = false
                            BuyNotifications.schedule(enabled: model.needsChecks)
                        }
                    }
                ))
            } header: {
                Text("Alertes actualité")
            } footer: {
                Text("Une escalade grave (guerre déclarée, invasion, panique bancaire…) reprise par au moins 2 sources, ou un sujet sur un actif de votre radar ou de vos avoirs repris par au moins 3 sources, dans les 6 dernières heures. Un même sujet raconté par plusieurs médias ne prévient qu'une fois.")
            }
            Section {
                Toggle("Changements de configuration", isOn: Binding(
                    get: { model.configAlertsEnabled },
                    set: { on in
                        if on {
                            Task {
                                let granted = await BuyNotifications.authorize()
                                denied = !granted
                                model.configAlertsEnabled = granted
                                BuyNotifications.schedule(enabled: model.needsChecks)
                            }
                        } else {
                            model.configAlertsEnabled = false
                            BuyNotifications.schedule(enabled: model.needsChecks)
                        }
                    }
                ))
                Toggle("Positions dangereuses", isOn: Binding(
                    get: { model.dangerAlertsEnabled },
                    set: { on in
                        if on {
                            Task {
                                let granted = await BuyNotifications.authorize()
                                denied = !granted
                                // Measured again from the last state shown on Mes avoirs (the dangers of before are not kept).
                                if granted && !model.dangerAlertsEnabled { model.changeNotices.dangerActive = nil }
                                model.dangerAlertsEnabled = granted
                                BuyNotifications.schedule(enabled: model.needsChecks)
                            }
                        } else {
                            model.dangerAlertsEnabled = false
                            BuyNotifications.schedule(enabled: model.needsChecks)
                        }
                    }
                ))
            } header: {
                Text("Radar et avoirs")
            } footer: {
                Text("Pendant la vérification en arrière-plan (au mieux toutes les 15 minutes, iOS décide) et à chaque ouverture. Changements de configuration : les décisions des 20 premiers actifs du radar sont relues, 2 à la fois, celles déjà analysées depuis moins de 15 minutes attendent ; une seule notification regroupe les nouveaux changements (ATTENDRE → ZONE D'ACHAT…), avec les conditions manquantes. Positions dangereuses : cours de vos avoirs et bougies journalières des lignes qui ont un stop ; prévenu quand une ligne devient dangereuse (stop cassé, à moins d'une volatilité journalière du stop, perte au-delà de votre risque accepté par idée), pas à chaque vérification, et pas de nouveau avant 24 h si elle sort puis revient. Rien n'est vérifié hors ligne. Conseil indicatif : Altim ne passe aucun ordre.")
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
                ForEach(RiskSettings.fields, id: \.label) { f in
                    RiskStepper(field: f, settings: $model.risk)
                }
                Button("Valeurs recommandées") { model.risk = .defaults }
                    .disabled(model.risk == .defaults)
            } header: {
                Text("Prudence des conseils")
            } footer: {
                Text("Ces limites servent à Mes avoirs : la part de votre patrimoine qu'une ligne peut perdre si son stop est touché, la taille maximale d'une ligne, la perte max du jour et la part crypto max. Règle professionnelle : ne jamais risquer plus de 1 à 2 % de son patrimoine sur une seule idée.\n\nPerte max du jour : si votre patrimoine a déjà perdu ce pourcentage depuis la clôture de la veille, Mes avoirs vous conseille de ne plus ouvrir de position aujourd'hui. Part crypto max : au-delà, Mes avoirs signale une surexposition aux cryptos, qui peuvent perdre 50 % ou plus ensemble (60 % par défaut ; 10 à 30 % est plus courant pour un patrimoine prudent).")
            }
            Section {
                ForEach(ScoreWeights.factors, id: \.key) { f in
                    ScoreWeightSlider(factor: f, weights: $model.scoreWeights)
                }
                Text("Total : \(model.scoreWeights.total) (ramené à 100 %)." + (model.scoreWeights.total == 0 ? " Tous à 0 : les poids par défaut sont utilisés." : ""))
                    .font(.footnote).foregroundStyle(Theme.textSecondary)
                Button("Poids par défaut (32 / 18 / 20 / 10 / 10 / 10)") { model.scoreWeights = .defaults }
                    .disabled(model.scoreWeights == .defaults)
            } header: {
                Text("Score composite")
            } footer: {
                Text("Poids de chaque famille dans le score de −100 à +100 de la carte Décision. Seuls les facteurs mesurés comptent : leurs poids sont ramenés à 100 %. Le verdict, lui, ne change pas.")
            }
            Section {
                Toggle("Verrouiller avec Face ID", isOn: $model.faceIDLock)
            } header: {
                Text("Sécurité")
            } footer: {
                Text("Face ID (ou le code de l'iPhone) est demandé à l'ouverture et après 2 minutes en arrière-plan. Le mot de passe et la session sont chiffrés dans le trousseau de cet iPhone, jamais sauvegardés dans iCloud.")
            }
            Section {
                NavigationLink("Voir la validation") { ValidationView() }
            } header: {
                Text("Validation du modèle")
            } footer: {
                Text("Le signal testé sur 34 actions, cryptos et ETF choisis à l'avance, par classe d'actifs et par régime de marché, avec ses biais et limites.")
            }
            Section {
                NavigationLink("Voir le bot") { BotScreen() }
            } header: {
                Text("Bot Altim")
            } footer: {
                Text(ModelBot.settingsText)
            }
            Section("Comprendre") {
                NavigationLink("Lexique : signal, zone d'achat, stop, flat tax…") { GlossaryView() }
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

/// Weight of one factor of the composite score, 0 – 100 %.
private struct ScoreWeightSlider: View {
    let factor: ScoreWeights.Factor
    @Binding var weights: ScoreWeights

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(factor.label).font(.subheadline.weight(.semibold))
                    Text(factor.hint).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                Text("\(weights[keyPath: factor.path])\u{202F}%").font(Theme.mono(14))
            }
            Slider(value: Binding(
                get: { Double(weights[keyPath: factor.path]) },
                set: { weights[keyPath: factor.path] = Int(min(100, max(0, $0.rounded()))) }
            ), in: 0...100, step: 1)
            .accessibilityLabel("Poids \(factor.label)")
            .accessibilityValue("\(weights[keyPath: factor.path]) %")
        }
    }
}

/// "Perte max du jour   −  3 %  +": one risk limit, kept within its bounds.
private struct RiskStepper: View {
    let field: RiskSettings.Field
    @Binding var settings: RiskSettings

    private var value: Double { settings[keyPath: field.key] }
    private var text: String { "\(Format.plain(value, digits: 2))\(field.unit)" }

    var body: some View {
        HStack(spacing: 10) {
            Text(field.label).fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 8)
            Button {
                settings = settings.stepped(field, up: false)
            } label: {
                Image(systemName: "minus.circle")
            }
            .disabled(value <= field.min)
            .accessibilityLabel("Diminuer \(field.label)")
            Text(text).font(Theme.mono(14)).frame(minWidth: 56)
            Button {
                settings = settings.stepped(field, up: true)
            } label: {
                Image(systemName: "plus.circle")
            }
            .disabled(value >= field.max)
            .accessibilityLabel("Augmenter \(field.label)")
        }
        .buttonStyle(.borderless)
        .font(.subheadline)
    }
}
