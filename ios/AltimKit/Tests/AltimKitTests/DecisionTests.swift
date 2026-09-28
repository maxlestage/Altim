import Foundation
import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimKit

/// Example answers of GET /api/decision (backend/tests/samples, the contract's own samples).
final class DecisionTests: XCTestCase {
    func fixture(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func testInformationalBitcoin() throws {
        let d = try JSONDecoder().decode(Decision.self, from: fixture("decision-btc"))
        XCTAssertEqual(d.symbol, "BTC")
        XCTAssertEqual(d.kind, .crypto)
        XCTAssertFalse(d.isPersonal)
        XCTAssertEqual(d.verdict, .wait)
        XCTAssertEqual(d.label, "ATTENDRE")
        XCTAssertEqual(d.level, .waiting)
        XCTAssertEqual(d.level.emoji, "⚪")
        XCTAssertFalse(d.families.isEmpty)
        XCTAssertTrue(d.families.contains { $0.status == .unavailable && $0.score == nil })
        XCTAssertEqual(d.setup.steps.count, d.setup.total)
        XCTAssertFalse(d.whyWait.isEmpty)
        XCTAssertNil(d.position)
        XCTAssertNil(d.exposure)
        let plan = try XCTUnwrap(d.plan)
        XCTAssertFalse(plan.acceptable)
        XCTAssertEqual(plan.minRiskReward, 2)
        guard case let .crypto(f)? = d.fundamentals else { return XCTFail("crypto fundamentals expected") }
        XCTAssertNil(f.tvl)
        XCTAssertEqual(try XCTUnwrap(f.mcFdv), 0.948, accuracy: 1e-9)
        XCTAssertFalse(f.unlocks.isEmpty)
        XCTAssertNotNil(d.track?.profitFactor)
        XCTAssertTrue(d.disclaimer.contains("Mode informationnel"))
        // Active vetoes first, the unverifiable last.
        let sorted = d.sortedVetoes
        XCTAssertEqual(sorted.count, d.vetoes.count)
        if let firstInactive = sorted.firstIndex(where: { !$0.active }) {
            XCTAssertTrue(sorted[firstInactive...].allSatisfy { !$0.active })
        }
        XCTAssertFalse(try XCTUnwrap(sorted.last).verifiable)
    }

    func testPersonalApple() throws {
        let d = try JSONDecoder().decode(Decision.self, from: fixture("decision-aapl"))
        XCTAssertEqual(d.kind, .stock)
        XCTAssertTrue(d.isPersonal)
        XCTAssertEqual(d.verdict, .trim)
        XCTAssertEqual(d.label, "ALLÉGER")
        XCTAssertEqual(d.level, .moderate)
        XCTAssertTrue(d.blocked)
        XCTAssertTrue(d.sortedVetoes[0].active)
        let p = try XCTUnwrap(d.position)
        XCTAssertEqual(p.cost, 275)
        XCTAssertEqual(p.exits.count, 4)
        XCTAssertEqual(p.exits[0].kind, .profit)
        XCTAssertTrue(p.exits[0].now)
        XCTAssertNil(p.exits[3].price)
        XCTAssertEqual(p.exits[3].kind, .macro)
        let e = try XCTUnwrap(d.exposure)
        XCTAssertEqual(e.factor, "S&P 500")
        XCTAssertNotNil(e.warning)
        guard case let .stock(f)? = d.fundamentals else { return XCTFail("stock fundamentals expected") }
        XCTAssertEqual(try XCTUnwrap(f.nextEarnings).estimated, true)
        XCTAssertEqual(f.surprises.count, 1)
        XCTAssertEqual(try XCTUnwrap(f.revisions).changePct, -0.2, accuracy: 1e-9)
        XCTAssertEqual(try XCTUnwrap(f.per), 46.6, accuracy: 1e-9)
        XCTAssertEqual(d.whyNot.uncertainty, .medium)
        XCTAssertEqual(d.scenarios.map(\.kind), [.bull, .neutral, .bear])
    }

    func testUnknownValuesDoNotBreakDecoding() throws {
        var json = try XCTUnwrap(JSONSerialization.jsonObject(with: fixture("decision-btc")) as? [String: Any])
        json["verdict"] = "strongBuyLater"
        json["level"] = "purple"
        json["fundamentals"] = ["kind": "bond", "yield": 4.1]
        var families = try XCTUnwrap(json["families"] as? [[String: Any]])
        families[0]["status"] = "mixed"
        json["families"] = families
        var why = try XCTUnwrap(json["whyNot"] as? [String: Any])
        why["uncertainty"] = "extreme"
        json["whyNot"] = why
        json["newField"] = [1, 2, 3]
        let d = try JSONDecoder().decode(Decision.self, from: JSONSerialization.data(withJSONObject: json))
        XCTAssertEqual(d.verdict, .unknown)
        XCTAssertEqual(d.level, .unknown)
        XCTAssertEqual(d.families[0].status, .unknown)
        XCTAssertEqual(d.whyNot.uncertainty, .unknown)
        guard case .unknown? = d.fundamentals else { return XCTFail("unknown fundamentals expected") }
        // The server's own label is kept for display.
        XCTAssertEqual(d.label, "ATTENDRE")
    }

    func testRoundTrip() throws {
        let d = try JSONDecoder().decode(Decision.self, from: fixture("decision-aapl"))
        let again = try JSONDecoder().decode(Decision.self, from: JSONEncoder().encode(d))
        guard case let .stock(f)? = again.fundamentals else { return XCTFail("stock fundamentals expected") }
        XCTAssertEqual(f.eps, 7.31)
        XCTAssertEqual(again.verdict, .trim)
    }

    // MARK: Personal inputs

    let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
    let aapl = Asset(symbol: "AAPL", kind: .stock, name: "Apple")
    let eth = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")

    func testWeightsArePercentagesOfThePortfolio() {
        let holdings = [
            Holding(asset: btc, quantity: 0.1, averagePrice: 60_000),   // 8 000
            Holding(asset: aapl, quantity: 10, averagePrice: 200),      // 3 000
            Holding(asset: btc, quantity: 0.05, averagePrice: 90_000),  // 4 000
            Holding(asset: eth, quantity: 2, averagePrice: nil),        // no price: left out
        ]
        let w = DecisionInputs.weights(holdings, prices: [btc.id: 80_000, aapl.id: 300])
        XCTAssertEqual(w.map(\.asset.symbol), ["BTC", "AAPL"])
        XCTAssertEqual(w[0].weight, 80)
        XCTAssertEqual(w[1].weight, 20)
        XCTAssertEqual(DecisionInputs.weightsParameter(w), "BTC:crypto:80,AAPL:stock:20")
        XCTAssertEqual(DecisionInputs.weightsParameter([DecisionWeight(asset: btc, weight: 35.2)]), "BTC:crypto:35.2")
        XCTAssertTrue(DecisionInputs.weights([], prices: [:]).isEmpty)
        XCTAssertTrue(DecisionInputs.weights(holdings, prices: [:]).isEmpty)
    }

    func testWeightsKeepTheTwentyLargest() {
        let holdings = (1...25).map { Holding(asset: Asset(symbol: "S\($0)", kind: .stock, name: "S\($0)"), quantity: Double($0), averagePrice: nil) }
        let prices = Dictionary(uniqueKeysWithValues: holdings.map { ($0.asset.id, 1.0) })
        let w = DecisionInputs.weights(holdings, prices: prices)
        XCTAssertEqual(w.count, 20)
        XCTAssertEqual(w.first?.asset.symbol, "S25")
        XCTAssertEqual(w.last?.asset.symbol, "S6")
        XCTAssertTrue(zip(w, w.dropFirst()).allSatisfy { $0.weight >= $1.weight })
    }

    func testAverageCost() {
        let holdings = [Holding(asset: btc, quantity: 0.1, averagePrice: 60_000), Holding(asset: btc, quantity: 0.05, averagePrice: 90_000), Holding(asset: aapl, quantity: 1, averagePrice: nil)]
        XCTAssertEqual(try XCTUnwrap(DecisionInputs.cost(of: btc, in: holdings)), 70_000, accuracy: 1e-6)
        XCTAssertNil(DecisionInputs.cost(of: aapl, in: holdings))
        XCTAssertNil(DecisionInputs.cost(of: eth, in: holdings))
    }

    func testQueryNeverCarriesQuantities() {
        XCTAssertEqual(AltimClient.decisionQuery(btc, cost: nil, weights: []), ["symbol": "BTC", "kind": "crypto"])
        let q = AltimClient.decisionQuery(aapl, cost: 275.5, weights: [DecisionWeight(asset: btc, weight: 35.2), DecisionWeight(asset: aapl, weight: 10)])
        XCTAssertEqual(q["cost"], "275.5")
        XCTAssertEqual(q["weights"], "BTC:crypto:35.2,AAPL:stock:10")
        XCTAssertEqual(AltimClient.decisionQuery(btc, cost: 0.00001234, weights: [])["cost"], "0.00001234")
        XCTAssertNil(AltimClient.decisionQuery(btc, cost: 0, weights: [])["cost"])
    }

    func testLargeAmounts() {
        XCTAssertEqual(Format.large(421e9), "421 Md$")
        XCTAssertEqual(Format.large(1654e9), "1\u{202F}654 Md$")
        XCTAssertEqual(Format.large(3.16e9), "3,2 Md$")
        XCTAssertEqual(Format.large(38e6), "38 M$")
        XCTAssertEqual(Format.large(19_930_000, unit: ""), "19,9 M")
        XCTAssertEqual(Format.large(819_900, unit: ""), "819\u{202F}900")
        XCTAssertEqual(Format.large(nil), "—")
    }

    // MARK: Client

    func testClientSendsParametersAndServesOfflineCopy() async throws {
        AltimClient.retryDelays = [0.01, 0.01]
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("altim-decision-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [StubProtocol.self]
        let c = AltimClient(baseURL: URL(string: "https://x.herokuapp.com")!, credentials: nil, session: AltimClient.makeSession(configuration: config),
                            cache: FileResponseCache(directory: dir))
        let body = String(decoding: try fixture("decision-aapl"), as: UTF8.self)
        StubProtocol.urls = []
        StubProtocol.script = [.status(200, body)]
        let d = try await c.decision(asset: aapl, cost: 275, weights: [DecisionWeight(asset: aapl, weight: 62)])
        XCTAssertEqual(d.verdict, .trim)
        let url = try XCTUnwrap(StubProtocol.urls.last)
        XCTAssertEqual(url.path, "/api/decision")
        XCTAssertEqual(url.query, "cost=275&kind=stock&symbol=AAPL&weights=AAPL:stock:62")
        // Offline: the last answer for the same request comes back.
        StubProtocol.script = [.offline, .offline, .offline]
        let cached = try await c.decision(asset: aapl, cost: 275, weights: [DecisionWeight(asset: aapl, weight: 62)])
        XCTAssertEqual(cached.label, "ALLÉGER")
    }
}
