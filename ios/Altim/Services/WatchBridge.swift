import Foundation
import WatchConnectivity
import AltimKit

/// Link with the Apple Watch app. The iPhone sends it the latest alerts (every check, and every change pushed by the
/// live socket while the app is open) and the server's address with the session (`WatchLink`, never the password):
/// the Watch then reads the alerts and the prices itself over HTTPS while its app is on screen. watchOS does not allow
/// a WebSocket for an ordinary app (TN3135), hence this rather than a live socket on the Watch.
@MainActor
final class WatchBridge: NSObject {
    static let shared = WatchBridge()
    /// Runs one alert check and returns (asked by an older Watch app).
    var refresh: (@MainActor () async -> Void)?
    /// The server and session of the iPhone, nil when logged out.
    var link: (@MainActor () -> WatchLink?)?

    private var session: WCSession? { WCSession.isSupported() ? WCSession.default : nil }

    var isPaired: Bool { session?.activationState == .activated && session?.isPaired == true && session?.isWatchAppInstalled == true }

    func activate() {
        guard let session else { return }
        session.delegate = self
        session.activate()
    }

    /// The application context replaces the previous one: alerts, display currency and link always travel together.
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

    /// The session changed (login, renewal, logout): the Watch gets the new one, or none.
    func sendLink() { push() }

    private func push() {
        guard isPaired, let session else { return }
        // Empty data = logged out: the Watch forgets its copy.
        context["link"] = link?().flatMap { try? JSONEncoder().encode($0) } ?? Data()
        try? session.updateApplicationContext(context)
    }
}

extension WatchBridge: WCSessionDelegate {
    nonisolated func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        guard activationState == .activated else { return }
        Task { @MainActor in self.push() }
    }
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
