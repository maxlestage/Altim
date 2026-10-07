import XCTest
@testable import AltimKit

/// Messages of the real-time socket `/api/ws` (protocol v1) and the Watch's own readings.
final class LiveSocketTests: XCTestCase {
    private func decode(_ s: String) -> LiveMessage? { LiveMessage.decode(Data(s.utf8)) }

    func testDecodeServerMessages() {
        XCTAssertEqual(decode(#"{"type":"hello","v":1,"maxAssets":20,"pingEvery":20}"#), .hello(version: 1))
        guard case .tick(let t)? = decode(#"{"type":"tick","symbol":"BTC","kind":"crypto","price":60000.5,"change":1.2,"agreeing":3,"total":4,"sources":["OKX"],"time":1700000000000}"#) else {
            return XCTFail("tick")
        }
        XCTAssertEqual(t.key, "crypto:BTC")
        XCTAssertEqual(t.price, 60000.5)
        let alerts = #"{"type":"alerts","checkedAt":1700000000000,"items":[{"symbol":"AAPL","kind":"stock","name":"Apple","price":212.4,"buy":true,"strong":false,"key":"zone:medium","title":"AAPL : achat possible","body":"…"},{"symbol":"PEPE","kind":"crypto","name":"Pepe","error":"indisponible"}]}"#
        guard case .alerts(let items, let at)? = decode(alerts) else { return XCTFail("alerts") }
        XCTAssertEqual(items.map(\.id), ["stock:AAPL", "crypto:PEPE"])
        XCTAssertEqual(items.map(\.buy), [true, false])
        XCTAssertEqual(at, 1_700_000_000_000)
        guard case .verdict(let v)? = decode(#"{"type":"verdict","symbol":"ETH","kind":"crypto","verdict":"wait","label":"ATTENDRE","rating":"hold","ratingLabel":"ATTENDRE","chipNote":"zone plus bas","asOf":5}"#) else {
            return XCTFail("verdict")
        }
        XCTAssertEqual(v.key, "crypto:ETH")
        XCTAssertEqual(v.chipNote, "zone plus bas")
        XCTAssertEqual(decode(#"{"type":"fx","rate":0.91,"asOf":7,"usdPerEur":1.0989,"source":"BCE"}"#), .fx(rate: 0.91, asOf: 7))
        XCTAssertNil(decode(#"{"type":"fx","rate":0}"#))
        XCTAssertEqual(decode(#"{"type":"checked","checkedAt":9}"#), .checked(9))
        XCTAssertEqual(decode(#"{"type":"pong","time":1}"#), .pong)
        XCTAssertEqual(decode(#"{"type":"error","code":"too_many_assets","message":"20 actifs au plus"}"#), .error(code: "too_many_assets", message: "20 actifs au plus"))
        XCTAssertEqual(decode(#"{"type":"newer"}"#), .other)
        XCTAssertNil(decode("[1]"))
        XCTAssertNil(decode("nope"))
    }

    func testSubscribeBackoffSilence() throws {
        let s = LiveMessage.subscribe([Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), Asset(symbol: "AAPL", kind: .stock, name: "Apple")], usd: false)
        let obj = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(s.utf8)) as? [String: Any])
        XCTAssertEqual(obj["type"] as? String, "subscribe")
        XCTAssertEqual(obj["currency"] as? String, "EUR")
        XCTAssertEqual(obj["interval"] as? String, "4h")
        XCTAssertEqual((obj["assets"] as? [[String: String]])?.first, ["kind": "crypto", "symbol": "BTC"])
        let many = (0..<25).map { Asset(symbol: "C\($0)", kind: .crypto, name: "C\($0)") }
        let big = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(LiveMessage.subscribe(many, usd: true).utf8)) as? [String: Any])
        XCTAssertEqual((big["assets"] as? [Any])?.count, 20)
        XCTAssertEqual(big["currency"] as? String, "USD")
        XCTAssertEqual((0..<8).map(LiveMessage.backoff), [1, 2, 4, 8, 16, 30, 30, 30])
        let now = Date()
        XCTAssertTrue(LiveMessage.isLive(lastMessage: now.addingTimeInterval(-10), now: now))
        XCTAssertFalse(LiveMessage.isLive(lastMessage: now.addingTimeInterval(-50), now: now))
        XCTAssertFalse(LiveMessage.isLive(lastMessage: nil, now: now))
    }

    func testSocketRequest() {
        let c = AltimClient(baseURL: URL(string: "https://altim.example")!, credentials: nil, sessionCookie: "abc.def")
        let r = c.liveSocketRequest()
        XCTAssertEqual(r.url?.absoluteString, "wss://altim.example/api/ws")
        XCTAssertEqual(r.value(forHTTPHeaderField: "Cookie"), "altim_session=abc.def")
        XCTAssertNil(r.value(forHTTPHeaderField: "Origin"))
        let dev = AltimClient(baseURL: URL(string: "http://localhost:3000")!, credentials: nil)
        XCTAssertEqual(dev.liveSocketRequest().url?.absoluteString, "ws://localhost:3000/api/ws")
    }

    func testWatchLink() throws {
        let link = WatchLink(server: "https://altim.example", session: "abc.def")
        let back = try JSONDecoder().decode(WatchLink.self, from: JSONEncoder().encode(link))
        XCTAssertEqual(back, link)
        XCTAssertNotNil(link.client())
        XCTAssertNil(WatchLink(server: "https://altim.example", session: "").client())
        let now = Date()
        XCTAssertEqual(WatchLink.checkedText(now.addingTimeInterval(-12), now: now), "Vérifié il y a 12 s")
        XCTAssertEqual(WatchLink.checkedText(now.addingTimeInterval(-185), now: now), "Vérifié il y a 3 min")
        XCTAssertEqual(WatchLink.checkedText(now.addingTimeInterval(-2 * 86_400 - 22 * 3600), now: now), "Vérifié il y a 2 j")
        XCTAssertEqual(WatchLink.checkedText(nil, now: now), "Jamais vérifié")
        XCTAssertEqual(WatchLink.message(for: AltimError.unauthorized), WatchLink.notLinked)
        XCTAssertTrue(WatchLink.message(for: AltimError.network("x")).hasPrefix("Réseau indisponible"))
        let a = BuyAlert(symbol: "AAPL", kind: .stock, name: "Apple", price: 290, buy: false, strong: false, key: "", title: "", body: "")
        let b = BuyAlert(symbol: "BNB", kind: .crypto, name: "BNB", price: 690, buy: true, strong: false, key: "zone", title: "", body: "")
        let merged = WatchLink.merge([a, b], quotes: ["stock:AAPL": 296.43, "crypto:BNB": .nan])
        XCTAssertEqual(merged.map(\.symbol), ["BNB", "AAPL"])
        XCTAssertEqual(merged.map(\.price), [690, 296.43])
    }
}
