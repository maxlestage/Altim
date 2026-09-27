import Foundation
import Observation
import AltimCore

/// Live prices shared by every screen: nothing frozen. Each screen watches its assets while it is visible
/// (`.task { await services.live.watch(assets) }`); leaving the screen cancels the task and closes the subscriptions.
@MainActor
@Observable
final class LivePrices {
    private(set) var ticks: [String: LiveTick] = [:]
    /// Direction of the last move of each asset (true = up), for the flash.
    private(set) var rising: [String: Bool] = [:]
    private(set) var lastTick: Date?
    /// Incremented on every tick (to recompute amounts that depend on the prices).
    private(set) var version = 0
    @ObservationIgnored private let hub: LivePriceHub

    init(hub: LivePriceHub) { self.hub = hub }

    func watch(_ assets: [Asset]) async {
        guard !assets.isEmpty else { return }
        let stream = await hub.stream(assets)
        for await t in stream {
            if let before = ticks[t.assetID]?.price, before != t.price { rising[t.assetID] = t.price > before }
            ticks[t.assetID] = t
            lastTick = Date()
            version &+= 1
        }
    }
}
