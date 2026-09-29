import Foundation
import XCTest
@testable import AltimKit

final class ChangeNoticesTests: XCTestCase {
    func btc() throws -> Decision {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "decision-btc", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(Decision.self, from: Data(contentsOf: url))
    }

    func transition(_ d: Decision, symbol: String? = nil, at: Double = 2000) throws -> ConfigTransition {
        var first = d
        if let symbol { first.symbol = symbol }
        var next = first
        next.verdict = .buyZone
        next.label = "ZONE D'ACHAT"
        let s = ConfigChanges.apply(ConfigState(), first, personal: false, now: at - 1000).state
        return try XCTUnwrap(ConfigChanges.apply(s, next, personal: false, now: at).transition)
    }

    // MARK: Configuration changes

    func testOneChangeNotifiedOnceWithTheCardWording() throws {
        let d = try btc()
        let t = try transition(d)
        let r = ChangeNotices.configNotice([t], state: ChangeNoticeState())
        let n = try XCTUnwrap(r.notice)
        XCTAssertEqual(n.title, "🚨 BTC — changement de configuration : \(d.label) → ZONE D'ACHAT")
        XCTAssertEqual(n.asset, "crypto:BTC")
        let lines = n.body.components(separatedBy: "\n")
        XCTAssertEqual(lines.first { $0.hasPrefix("Conditions manquantes") }, "Conditions manquantes : \(t.missing.joined(separator: " ; ")).")
        XCTAssertEqual(lines.first { $0.hasPrefix("Ce qui changerait") }, "Ce qui changerait la décision : \(t.triggers.joined(separator: " ; ")).")
        XCTAssertEqual(lines.last, ChangeNotices.disclaimer)
        XCTAssertEqual(r.state.configNotified, [t.id])
        // Same transition again (another check, or found twice): no second notification.
        XCTAssertNil(ChangeNotices.configNotice([t], state: r.state).notice)
        XCTAssertNil(ChangeNotices.configNotice([], state: r.state).notice)
    }

    func testWhyItChangedAndFallback() throws {
        var t = try transition(try btc())
        t.changes = ["Momentum −18 pts (+40 → +22)"]
        t.missing = []
        t.triggers = []
        XCTAssertEqual(ChangeNotices.configText([t]).body, "Pourquoi le signal a changé : Momentum −18 pts (+40 → +22).\n\(ChangeNotices.disclaimer)")
        t.changes = []
        XCTAssertEqual(ChangeNotices.configText([t]).body, "Niveau \(t.from.levelLabel) → \(t.to.levelLabel).\n\(ChangeNotices.disclaimer)")
    }

    func testSeveralChangesGrouped() throws {
        let d = try btc()
        var list: [ConfigTransition] = []
        for (i, s) in ["BTC", "ETH", "SOL", "ADA", "XRP", "DOT", "LINK"].enumerated() { list.append(try transition(d, symbol: s, at: 2000 + Double(i))) }
        let r = ChangeNotices.configNotice(list, state: ChangeNoticeState())
        let n = try XCTUnwrap(r.notice)
        XCTAssertEqual(n.title, "🚨 Changements de configuration · 7")
        XCTAssertNil(n.asset)
        let lines = n.body.components(separatedBy: "\n")
        // Newest first, 5 listed, the others counted.
        XCTAssertTrue(lines[0].hasPrefix("LINK : \(d.label) → ZONE D'ACHAT (conditions manquantes : "))
        XCTAssertEqual(lines[5], "+ 2 autre(s) : détail sur le Radar.")
        XCTAssertEqual(r.state.configNotified.count, 7)
        // One already notified is left out; only the new one is told.
        let more = try transition(d, symbol: "AVAX", at: 3000)
        let again = ChangeNotices.configNotice(list + [more], state: r.state)
        XCTAssertEqual(again.notice?.title, more.title)
        XCTAssertEqual(again.notice?.asset, "crypto:AVAX")
    }

    func testRememberedIdsAreBounded() throws {
        var s = ChangeNoticeState()
        s.configNotified = (0..<ChangeNotices.maxRemembered).map { "old\($0)" }
        let t = try transition(try btc())
        let r = ChangeNotices.configNotice([t], state: s)
        XCTAssertEqual(r.state.configNotified.count, ChangeNotices.maxRemembered)
        XCTAssertEqual(r.state.configNotified.last, t.id)
        XCTAssertEqual(r.state.configNotified.first, "old1")
    }

    // MARK: Dangerous positions

    func danger(_ id: String, _ symbol: String, _ codes: [DangerCode]) -> Danger {
        Danger(id: id, symbol: symbol, kind: .crypto, name: symbol, reasons: codes.map { .init(code: $0, text: "Raison \($0.rawValue).") })
    }

    func testNewDangerNotifiedOnce() throws {
        let d = danger("L1", "BTC", [.stopBroken])
        let r = ChangeNotices.dangerNotice([d], state: ChangeNoticeState(), now: 1000)
        let n = try XCTUnwrap(r.notice)
        XCTAssertEqual(n.title, "⚠ Position devenue dangereuse : BTC")
        XCTAssertEqual(n.body, "BTC : stop cassé.\n\(ChangeNotices.dangerDetail)\n\(ChangeNotices.dangerAdvice)")
        XCTAssertEqual(n.asset, "crypto:BTC")
        XCTAssertEqual(r.state.dangerActive, ["L1:stop_broken"])
        // Still dangerous at the next checks: nothing more.
        let again = ChangeNotices.dangerNotice([d], state: r.state, now: 2000)
        XCTAssertNil(again.notice)
        // Gone, then back within 24 hours: not repeated; after 24 hours: notified again.
        let gone = ChangeNotices.dangerNotice([], state: again.state, now: 3000).state
        XCTAssertEqual(gone.dangerActive, [])
        XCTAssertNil(ChangeNotices.dangerNotice([d], state: gone, now: 4000).notice)
        let later = ChangeNotices.dangerNotice([], state: ChangeNotices.dangerNotice([d], state: gone, now: 4000).state, now: 5000).state
        XCTAssertNotNil(ChangeNotices.dangerNotice([d], state: later, now: 1000 + ChangeNotices.dangerCooldown).notice)
    }

    func testNewReasonOfALineAndGrouping() throws {
        let first = ChangeNotices.dangerNotice([danger("L1", "BTC", [.nearStop])], state: ChangeNoticeState(), now: 0).state
        // Near the stop → stop broken: a new state, told with all the reasons of the line; ETH enters too.
        let r = ChangeNotices.dangerNotice([danger("L1", "BTC", [.stopBroken, .lossOverRisk]), danger("L2", "ETH", [.lossOverRisk])], state: first, now: 10)
        let n = try XCTUnwrap(r.notice)
        XCTAssertEqual(n.title, "⚠ Positions devenues dangereuses : BTC, ETH")
        XCTAssertEqual(n.body, "BTC : stop cassé ; perte au-delà de votre risque par idée.\nETH : perte au-delà de votre risque par idée.\n\(ChangeNotices.dangerDetail)\n\(ChangeNotices.dangerAdvice)")
        XCTAssertFalse(n.body.contains("$"), "no amount in a notification")
        XCTAssertNil(n.asset)
        // A line still dangerous for an old reason only is not listed.
        let s = ChangeNotices.dangerNotice([danger("L1", "BTC", [.stopBroken]), danger("L2", "ETH", [.lossOverRisk]), danger("L3", "SOL", [.nearStop])],
                                           state: r.state, now: 20)
        XCTAssertEqual(s.notice?.title, "⚠ Position devenue dangereuse : SOL")
    }

    func testBaselineAndUnknownNearStop() throws {
        let d = danger("L1", "BTC", [.nearStop])
        // Never measured here, but already shown on Mes avoirs: not notified.
        let seen = ChangeNotices.dangerNotice([d], state: ChangeNoticeState(), now: 0, baseline: ["L1:near_stop"])
        XCTAssertNil(seen.notice)
        // Candles unreadable: the near-stop state is kept, so coming back readable does not notify it again.
        let unknown = ChangeNotices.dangerNotice([], state: seen.state, now: 10, nearStopUnknown: ["L1"])
        XCTAssertEqual(unknown.state.dangerActive, ["L1:near_stop"])
        XCTAssertNil(ChangeNotices.dangerNotice([d], state: unknown.state, now: 20 + ChangeNotices.dangerCooldown).notice)
        XCTAssertEqual(ChangeNotices.dangerKeys([danger("A", "BTC", [.stopBroken, .lossOverRisk])]), ["A:stop_broken", "A:loss_over_risk"])
    }

    func testStateStorage() throws {
        var s = ChangeNoticeState()
        s.configNotified = ["a"]
        s.dangerActive = ["L1:near_stop"]
        s.dangerNotified = ["L1:near_stop": 5]
        XCTAssertEqual(try JSONDecoder().decode(ChangeNoticeState.self, from: JSONEncoder().encode(s)), s)
        let damaged = try JSONDecoder().decode(ChangeNoticeState.self, from: Data(#"{"configNotified":3,"dangerActive":["x"],"dangerNotified":"no"}"#.utf8))
        XCTAssertEqual(damaged.configNotified, [])
        XCTAssertEqual(damaged.dangerActive, ["x"])
        XCTAssertEqual(damaged.dangerNotified, [:])
        XCTAssertNil(try JSONDecoder().decode(ChangeNoticeState.self, from: Data("{}".utf8)).dangerActive)
    }
}
