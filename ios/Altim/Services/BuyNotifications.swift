import BackgroundTasks
import Foundation
import UserNotifications
import AltimKit

/// Notifications "you can buy" on the iPhone (and on the Apple Watch, which shows the iPhone's notifications when
/// the iPhone is locked). The check runs when iOS grants a background refresh (at best every 15 minutes, iOS decides
/// according to usage and battery) and each time the app comes to the foreground.
@MainActor
enum BuyNotifications {
    static let taskID = "com.maxlestage.altim.alerts"
    private static var lastForegroundCheck: Date?

    /// To call before the end of the launch (iOS requirement for background tasks).
    static func register(model: @escaping @MainActor () -> AppModel) {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: taskID, using: nil) { task in
            guard let refresh = task as? BGAppRefreshTask else { return task.setTaskCompleted(success: false) }
            let work = Task { @MainActor in
                let ok = await run(model())
                schedule(enabled: model().needsChecks)
                refresh.setTaskCompleted(success: ok)
            }
            refresh.expirationHandler = { work.cancel() }
        }
    }

    static func schedule(enabled: Bool) {
        BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: taskID)
        guard enabled else { return }
        let request = BGAppRefreshTaskRequest(identifier: taskID)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(request)
    }

    /// Permission for a price alert (the buy alerts may stay off).
    static func authorize() async -> Bool {
        (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
    }

    /// Asks the permission (first time only) and turns the checks on.
    static func enable(_ model: AppModel) async -> Bool {
        let center = UNUserNotificationCenter.current()
        let granted = (try? await center.requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        model.alertsEnabled = granted
        schedule(enabled: model.needsChecks)
        if granted { await run(model) }
        return granted
    }

    /// App in the foreground: a check at most every 15 minutes.
    static func foregroundCheck(_ model: AppModel) async {
        guard model.needsChecks else { return }
        if let last = lastForegroundCheck, Date().timeIntervalSince(last) < 15 * 60 { return }
        lastForegroundCheck = Date()
        await run(model)
        schedule(enabled: model.needsChecks)
    }

    /// One check: new alerts notified, the Watch and the Live Activity refreshed. False on failure (network…).
    @discardableResult
    static func run(_ model: AppModel) async -> Bool {
        guard model.needsChecks || WatchBridge.shared.isPaired else { return true }
        do {
            guard let result = try await model.checkAlerts() else { return false }
            WatchBridge.shared.send(model.lastAlerts, checked: model.lastAlertCheck)
            LiveActivities.shared.refresh(with: model.lastAlerts)
            if model.alertsEnabled { await post(result.buy) }
            await postNews(result.news)
            for (t, price) in result.targets {
                let title = t.move != nil
                    ? "\(t.asset.symbol) a bougé de \(Format.percent((price / t.price - 1) * 100, digits: 1))"
                    : "\(t.asset.symbol) \(t.above ? "au-dessus de" : "en dessous de") \(Format.price(t.price))"
                await add(id: "altim.target.\(t.id)", title: title,
                          body: "Prix actuel \(Format.price(price)) : votre alerte de prix est atteinte. Réarmez-la dans l'onglet Alertes si besoin.", asset: t.asset.id)
            }
            return true
        } catch {
            return false
        }
    }

    /// Up to 3 alerts: one notification each; beyond, a single summary (the first check can find many at once).
    static func post(_ alerts: [BuyAlert]) async {
        let center = UNUserNotificationCenter.current()
        guard await center.notificationSettings().authorizationStatus == .authorized else { return }
        if alerts.count > 3 {
            let strong = alerts.filter(\.strong).map(\.symbol)
            let others = alerts.filter { !$0.strong }.map(\.symbol)
            var body = strong.isEmpty ? "" : "Achat conseillé : \(strong.joined(separator: ", ")). "
            if !others.isEmpty { body += "Achat possible : \(others.joined(separator: ", ")). " }
            await add(id: "altim.summary", title: "\(alerts.count) actifs achetables", body: body + "Ouvrez Altim pour le détail de chacun.", asset: nil)
        } else {
            for a in alerts { await add(id: "altim.\(a.id)", title: a.title, body: a.body, asset: a.id) }
        }
    }

    /// Up to 2 stories: one notification each; beyond, a single summary. A tap opens the Actu tab.
    static func postNews(_ items: [NewsItem]) async {
        guard !items.isEmpty, await UNUserNotificationCenter.current().notificationSettings().authorizationStatus == .authorized else { return }
        let list: [(String, String, String)] = items.count <= 2
            ? items.map { n in
                ("altim.news.\(n.id)", n.alert ? "Alerte actualité" : "Actualité : \(n.assets.map { $0.components(separatedBy: ":").last ?? $0 }.joined(separator: ", "))",
                 "\(n.title) (\(n.source)\(n.alsoIn.isEmpty ? "" : " +\(n.alsoIn.count)"))")
            }
            : [("altim.news.summary", "\(items.count) actualités importantes", items.prefix(3).map(\.title).joined(separator: " · "))]
        for (id, title, body) in list {
            let content = UNMutableNotificationContent()
            content.title = title
            content.body = body
            content.sound = .default
            content.threadIdentifier = "actualites"
            content.userInfo = ["news": true]
            try? await UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
        }
    }

    private static func add(id: String, title: String, body: String, asset: String?) async {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body + " Conseil indicatif : Altim ne passe aucun ordre."
        content.sound = .default
        content.threadIdentifier = "achats"
        content.interruptionLevel = .timeSensitive
        if let asset { content.userInfo = ["asset": asset] }
        try? await UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }
}

/// Tapped notification → opens the asset; notifications also shown while the app is open.
final class NotificationDelegate: NSObject, UNUserNotificationCenterDelegate {
    var open: (@MainActor (String) -> Void)?
    var openNews: (@MainActor () -> Void)?

    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        if response.notification.request.content.userInfo["news"] as? Bool == true {
            await MainActor.run { openNews?() }
            return
        }
        guard let id = response.notification.request.content.userInfo["asset"] as? String else { return }
        await MainActor.run { open?(id) }
    }
}
