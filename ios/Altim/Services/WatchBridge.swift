import Foundation
import WatchConnectivity
import AltimKit

/// Link with the Apple Watch app: the iPhone sends it the latest alerts (the Watch never holds the password nor the
/// session: it only shows what the iPhone computed). The Watch can ask for a fresh check.
@MainActor
final class WatchBridge: NSObject {
    static let shared = WatchBridge()
    /// Runs one alert check and returns (asked by the Watch).
    var refresh: (@MainActor () async -> Void)?

    private var session: WCSession? { WCSession.isSupported() ? WCSession.default : nil }

    var isPaired: Bool { session?.activationState == .activated && session?.isPaired == true && session?.isWatchAppInstalled == true }

    func activate() {
        guard let session else { return }
        session.delegate = self
        session.activate()
    }

    /// The application context replaces the previous one: the alerts and the display currency always travel together.
    private var context: [String: Any] = [:]

    func send(_ alerts: [BuyAlert], checked: Date?) {
        guard let data = try? JSONEncoder().encode(alerts) else { return }
        context["alerts"] = data
        context["checked"] = checked ?? Date()
        push()
    }

    /// Display currency and EUR/USD rate: the Watch formats the prices like the iPhone.
    func sendDisplay(currency: Currency, fx: FxRate?) {
        context["currency"] = currency.rawValue
        context["fx"] = fx.flatMap { try? JSONEncoder().encode($0) } ?? Data()
        push()
    }

    private func push() {
        guard isPaired, let session, context["alerts"] != nil else { return }
        try? session.updateApplicationContext(context)
    }
}

extension WatchBridge: WCSessionDelegate {
    nonisolated func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {}
    nonisolated func sessionDidBecomeInactive(_ session: WCSession) {}
    nonisolated func sessionDidDeactivate(_ session: WCSession) { session.activate() }

    nonisolated func session(_ session: WCSession, didReceiveMessage message: [String: Any], replyHandler: @escaping ([String: Any]) -> Void) {
        guard message["refresh"] as? Bool == true else { return replyHandler([:]) }
        Task { @MainActor in
            await self.refresh?()
            replyHandler(["ok": true])
        }
    }
}
