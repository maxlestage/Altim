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

    func send(_ alerts: [BuyAlert], checked: Date?) {
        guard isPaired, let session, let data = try? JSONEncoder().encode(alerts) else { return }
        try? session.updateApplicationContext(["alerts": data, "checked": checked ?? Date()])
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
