import BackgroundTasks
import Foundation
import UserNotifications
import AltimKit

/// Notifications "you can buy" on the iPhone (and on the Apple Watch, which shows the iPhone's notifications when
/// the iPhone is locked). The check runs when iOS grants a background refresh (at best every 15 minutes, iOS decides
/// according to usage and battery) and each time the app comes to the foreground. The same check also tells the
/// configuration changes of the radar and the positions that became dangerous (Réglages → Radar et avoirs).
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
        // The texts of the notifications (server's and the device's) use the current EUR/USD rate.
        await model.refreshFxIfOld()
        do {
            guard let result = try await model.checkAlerts() else { return false }
            WatchBridge.shared.send(model.lastAlerts, checked: model.lastAlertCheck)
            LiveActivities.shared.refresh(with: model.lastAlerts)
            if model.alertsEnabled { await post(result.buy) }
            await postNews(result.news)
            for (t, price) in result.targets {
                // The threshold in its own currency; the move measured on the price in that currency.
                let title = t.move != nil
                    ? "\(t.asset.symbol) a bougé de \(Format.percent((t.inCurrency(price) / t.price - 1) * 100, digits: 1))"
                    : "\(t.asset.symbol) \(t.above ? "au-dessus de" : "en dessous de") \(Money.threshold(t.price, Money.stored(t.currency)))"
                await add(id: "altim.target.\(t.id)", title: title,
                          body: "Prix actuel \(Format.price(price)) : votre alerte de prix est atteinte. Réarmez-la dans l'onglet Alertes si besoin.", asset: t.asset.id)
            }
        } catch {
            return false
        }
        await runChanges(model)
        return true
    }

    /// After a check that reached the server: the positions that newly became dangerous, then the configuration
    /// changes of the radar (decisions re-read within about 20 seconds, the rest at the next check). One grouped
    /// notification each at most; never the same change or danger twice (ChangeNotices).
    private static func runChanges(_ model: AppModel) async {
        let wantsDangers = model.dangerAlertsEnabled && !model.holdings.isEmpty
        let wantsConfig = model.configAlertsEnabled && !model.watchlist.isEmpty
        guard wantsDangers || wantsConfig, model.offlineSince == nil,
              await UNUserNotificationCenter.current().notificationSettings().authorizationStatus == .authorized else { return }
        let deadline = Date(timeIntervalSinceNow: 20)
        if wantsDangers, let m = await model.measureDangers() {
            let r = ChangeNotices.dangerNotice(m.dangers, state: model.changeNotices, now: Date().timeIntervalSince1970 * 1000,
                                               baseline: model.dangers.map { ChangeNotices.dangerKeys($0.items) } ?? [],
                                               nearStopUnknown: m.nearStopUnknown)
            model.changeNotices = r.state
            if let n = r.notice { await post(n, thread: "dangers", urgent: true) }
        }
        if wantsConfig, !Task.isCancelled {
            let found = await model.checkConfigChanges(until: deadline)
            let r = ChangeNotices.configNotice(found, state: model.changeNotices)
            model.changeNotices = r.state
            if let n = r.notice { await post(n, thread: "configurations", urgent: false) }
        }
    }

    private static let privateCategory = "altim.private"
    private static var categoryRegistered = false

    /// The category of the portfolio notices: when previews are hidden, "Altim : ouvrez l'app pour le détail".
    private static func registerPrivateCategory() {
        guard !categoryRegistered else { return }
        categoryRegistered = true
        let center = UNUserNotificationCenter.current()
        let id = privateCategory
        let category = UNNotificationCategory(
            identifier: id,
            actions: [],
            intentIdentifiers: [],
            hiddenPreviewsBodyPlaceholder: "Ouvrez Altim pour le détail",
            options: []
        )
        center.getNotificationCategories { existing in
            center.setNotificationCategories(existing.filter { $0.identifier != id }.union([category]))
        }
    }

    /// A notification whose texts (disclaimer included) come from AltimKit; a tap opens its asset when it has one.
    private static func post(_ n: LocalNotice, thread: String, urgent: Bool) async {
        registerPrivateCategory()
        let content = UNMutableNotificationContent()
        content.title = n.title
        content.body = n.body
        content.sound = .default
        content.threadIdentifier = thread
        // Previews hidden (lock screen, watch): a neutral line instead of the body.
        content.categoryIdentifier = privateCategory
        // No time-sensitive entitlement in the project: iOS would downgrade it anyway.
        content.interruptionLevel = .active
        if let asset = n.asset { content.userInfo = ["asset": asset] }
        try? await UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: n.id, content: content, trigger: nil))
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
