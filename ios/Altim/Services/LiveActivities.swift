import ActivityKit
import Foundation
import AltimKit

/// Live Activity of one followed asset (lock screen and Dynamic Island). Updated by the live price stream while the
/// app runs (at most every 5 s, iOS limits the rate) and by each alert check in the background; after 30 minutes
/// without an update iOS shows it as out of date. Without a push server (Apple key), it cannot move while the app
/// stays suspended: this is said in the Réglages.
@MainActor
final class LiveActivities {
    static let shared = LiveActivities()

    private var lastPush = Date.distantPast
    private var lastState: PriceActivityAttributes.ContentState?

    var available: Bool { ActivityAuthorizationInfo().areActivitiesEnabled }

    /// Asset shown in the running activity, if any.
    var current: Asset? {
        Activity<PriceActivityAttributes>.activities.first.flatMap { a in
            Kind(rawValue: a.attributes.kind).map { Asset(symbol: a.attributes.symbol, kind: $0, name: a.attributes.name) }
        }
    }

    func isFollowing(_ a: Asset) -> Bool { current?.id == a.id }

    /// One activity at a time: following an asset replaces the previous one.
    func start(_ a: Asset, price: Double, change: Double?, alert: BuyAlert?) async {
        await stopAll()
        let state = Self.state(price: price, change: change, alert: alert, signal: nil)
        let attributes = PriceActivityAttributes(symbol: a.symbol, name: a.name, kind: a.kind.rawValue)
        _ = try? Activity.request(attributes: attributes, content: .init(state: state, staleDate: Date(timeIntervalSinceNow: 30 * 60)), pushType: nil)
        lastState = state
        lastPush = Date()
    }

    func stopAll() async {
        for activity in Activity<PriceActivityAttributes>.activities {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
        lastState = nil
    }

    /// Live price tick of the followed asset.
    func update(tick: LiveTick) {
        guard let activity = Activity<PriceActivityAttributes>.activities.first,
              activity.attributes.symbol == tick.symbol, activity.attributes.kind == tick.kind.rawValue,
              Date().timeIntervalSince(lastPush) >= 5 else { return }
        var state = lastState ?? activity.content.state
        state.price = tick.price
        state.change = tick.change ?? state.change
        state.updated = Date()
        push(activity, state)
    }

    /// Result of an alert check: verdict and price of the followed asset.
    func refresh(with alerts: [BuyAlert]) {
        guard let activity = Activity<PriceActivityAttributes>.activities.first,
              let alert = alerts.first(where: { $0.symbol == activity.attributes.symbol && $0.kind.rawValue == activity.attributes.kind }) else { return }
        let previous = lastState ?? activity.content.state
        let state = Self.state(price: alert.price ?? previous.price, change: previous.change, alert: alert, signal: previous.signal)
        push(activity, state)
    }

    private func push(_ activity: Activity<PriceActivityAttributes>, _ state: PriceActivityAttributes.ContentState) {
        var state = state
        state.priceText = Format.price(state.price)
        lastState = state
        lastPush = Date()
        Task { await activity.update(.init(state: state, staleDate: Date(timeIntervalSinceNow: 30 * 60))) }
    }

    private static func state(price: Double, change: Double?, alert: BuyAlert?, signal: String?) -> PriceActivityAttributes.ContentState {
        let note: String
        if let alert {
            note = alert.buy ? (alert.reasons.first ?? alert.body) : (alert.blockers.first ?? "Ni signal d'achat ni prix dans une zone d'achat.")
        } else {
            note = "Verdict d'achat à la prochaine vérification."
        }
        return .init(price: price, change: change, signal: signal, buy: alert?.buy ?? false, note: note, updated: Date(), priceText: Format.price(price))
    }
}
