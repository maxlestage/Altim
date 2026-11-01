import SwiftUI
import AltimKit

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var denied = false
    /// Risk accepted per position for the risk bot's size (read by AltimKit: BotEngine.riskBudget).
    @AppStorage(BotEngine.riskBudgetKey) private var riskBudget = 0.05

    var body: some View {
        @Bindable var model = model
        Form {
            Section {
                FittingPicker(title: "Devise d'affichage", selection: $model.currency) {
                    Text("Euro (€)").tag(Currency.eur)
                    Text("Dollar ($)").tag(Currency.usd)
                }
                FxNote()
                if model.currency == .eur, model.fx == nil, let e = model.fxError {
                    Text(e).font(.caption).foregroundStyle(Theme.warning)
                }
            } header: {
                Text("Devise d'affichage")
            } footer: {
                Text("Les cours viennent en dollars des sources (bourses américaines, plateformes crypto en USD ou USDT) ; Altim les convertit au taux EUR/USD du moment (Yahoo Finance, sinon taux de référence de la BCE). Les montants que vous saisissez (prix de revient, budget, liquidités) le sont dans cette devise.")
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
                Toggle("Seulement les achats conseillés (repli ou zone)", isOn: $model.alertsStrongOnly)
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
                Text("L'iPhone vérifie lui-même votre radar et vos avoirs : achetable seulement en tendance de fond haussière avec un momentum positif, sans source en désaccord ni choc de marché ; « conseillé » sur un repli dans cette tendance ou dans une zone d'achat Fibonacci. Une notification seulement quand un actif devient achetable ou que la raison change ; l'Apple Watch les reçoit aussi. iOS décide du rythme en arrière-plan (au mieux toutes les 15 minutes), et la vérification a lieu aussi à chaque ouverture. Conseil indicatif : Altim ne passe aucun ordre.")
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
                Toggle("Verrouiller avec Face ID", isOn: $model.faceIDLock)
            } header: {
                Text("Sécurité")
            } footer: {
                Text("Face ID (ou le code de l'iPhone) est demandé à l'ouverture et après 2 minutes en arrière-plan. Vos avoirs et réglages restent sur cet iPhone, dans un stockage protégé.")
            }
            Section {
                Picker("Risque accepté par position", selection: $riskBudget) {
                    Text("5 %").tag(0.05)
                    Text("10 %").tag(0.10)
                }
            } header: {
                Text("Taille des positions")
            } footer: {
                Text("Le bot Risque prévoit la pire baisse des 20 jours suivants ; Altim conseille d'engager la part de la somme prévue qui ne ferait perdre que ce pourcentage. Mesuré sur la règle avec le veto du bot : en crypto, 5 % a donné +16 % de rendement pour une pire baisse de −15 %, 10 % a donné +33 à +35 % pour −30 %, une position pleine +47 % pour −48 % ; sur les actions, presque pareil dans les deux cas. Des résultats passés, pas une promesse.")
            }
            Section {
                Text("Le verdict sur l'iPhone suit une règle choisie sur des résultats mesurés : investi seulement quand la tendance de fond est haussière (prix au-dessus de la moyenne 200 jours) et le momentum positif. Elle a été comparée à « tout garder » sur 26 actifs, avec chaque historique coupé en deux moitiés, puis sur une seconde liste d'actifs. Elle protège surtout dans les marchés baissiers. Un filtre de marché s'y ajoute : un actif n'est acheté ou gardé que si son marché (le Bitcoin pour les cryptos, le S&P 500 pour les actions) est lui aussi en tendance haussière ; au test, rendement égal ou meilleur et pire baisse plus faible dans les 8 cas mesurés. Deux bots entraînés sur l'iPhone (arbres de décision appris sur l'historique des actifs de chaque marché, mis à jour toutes les 12 heures) l'accompagnent : le bot Tendance peut bloquer un achat (testé sans jamais regarder le futur, ce veto a amélioré la règle sur les deux listes d'actifs, surtout en crypto) ; le bot Risque prévoit la pire baisse des 20 jours suivants, pour régler la taille d'une position. La machine à remonter le temps de chaque actif rejoue la règle et son filtre de marché, sans les bots.")
                    .font(.footnote).foregroundStyle(Theme.textSecondary)
                NavigationLink("Lexique : signal, zone d'achat, stop, flat tax…") { GlossaryView() }
            } header: {
                Text("Comment Altim décide")
            }
            Section {
                KeyValue(key: "Calculs", value: "sur cet iPhone, sans serveur")
                KeyValue(key: "Sources crypto", value: "8 bourses")
                KeyValue(key: "Sources actions", value: "Yahoo, Nasdaq, Robinhood")
                KeyValue(key: "Prix en direct", value: "crypto 5 s, actions 15 s en séance")
            } header: {
                Text("Données")
            } footer: {
                Text("Altim lit lui-même les cours publics des bourses crypto (Coinbase, Kraken, OKX, KuCoin, Gate.io, Bitstamp, Bitfinex, Binance), les données boursières (Yahoo Finance, Nasdaq, Robinhood), l'actualité (flux RSS publics), le Fear & Greed et le financement OKX. Chaque prix est la médiane des sources qui s'accordent ; une source qui s'écarte est écartée. Tout est calculé sur cet iPhone : aucun compte, aucun serveur Altim, vos avoirs ne quittent pas l'iPhone.")
            }
            Section("À savoir") {
                Text("Altim est un conseiller : il ne passe aucun ordre et n'a accès à aucun de vos comptes. Les signaux sont des probabilités mesurées sur l'historique, jamais des certitudes ; investir comporte un risque de perte en capital.")
                    .font(.footnote).foregroundStyle(Theme.textSecondary)
                KeyValue(key: "Version", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "—")
            }
        }
        .altimScreen()
        .navigationTitle("Réglages")
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
