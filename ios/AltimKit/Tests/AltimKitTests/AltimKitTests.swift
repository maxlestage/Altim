import Foundation
import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimKit

/// Fixtures = real answers of the Altim server (September 2026), so decoding is checked on the true shapes.
final class DecodingTests: XCTestCase {
    func fixture(_ name: String, _ ext: String = "json") throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: ext, subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func testRadar() throws {
        let rows = try JSONDecoder().decode([RadarRow].self, from: fixture("radar"))
        XCTAssertEqual(rows.map(\.symbol), ["BTC", "AAPL"])
        XCTAssertEqual(rows[0].signal?.action, .buy)
        XCTAssertEqual(rows[0].reliability?.level, "high")
        XCTAssertGreaterThan(rows[0].sparkline?.count ?? 0, 10)
    }

    func testQuotesAndSearch() throws {
        let q = try JSONDecoder().decode([Quote].self, from: fixture("tickers"))
        XCTAssertFalse(q.isEmpty)
        XCTAssertGreaterThan(q[0].price, 0)
        let s = try JSONDecoder().decode([SearchItem].self, from: fixture("search"))
        XCTAssertEqual(s.first?.asset, Asset(symbol: "NVDA", kind: .stock, name: "NVIDIA Corporation"))
    }

    func testGuardZonesMacro() throws {
        let g = try JSONDecoder().decode(GuardReport.self, from: fixture("guard"))
        XCTAssertEqual(g.trendLabel, "Haussière")
        XCTAssertEqual(g.shock.factors.first?.statusText, "peu d'historique : compté à moitié")
        XCTAssertNotNil(g.macro)
        let z = try JSONDecoder().decode(ZonesReport.self, from: fixture("zones"))
        XCTAssertEqual(z.zones.map(\.horizon), ["short", "medium", "long"])
        XCTAssertEqual(z.zones[0].statusLabel, "Attendre le repli")
        XCTAssertNotNil(z.zones[0].zone)
        let m = try JSONDecoder().decode(MacroInfo.self, from: fixture("macro"))
        XCTAssertEqual(m.levelLabel, "Calme")
        XCTAssertNotNil(m.values["vix"])
    }

    func testSelection() throws {
        let r = try JSONDecoder().decode(SelectionReport.self, from: fixture("sel"))
        XCTAssertEqual(r.horizon, .mo1)
        XCTAssertEqual(r.market, .crypto)
        XCTAssertEqual(r.orderedCriteria.first, .signal)
        XCTAssertEqual(Set(r.orderedCriteria).count, 5)
        XCTAssertNotNil(r.buy.first?.plan)
        XCTAssertEqual(r.validation?.edge, "clear")
        let amounts = r.allocate(budget: 10000)
        XCTAssertEqual(amounts.count, r.buy.filter { $0.plan != nil }.count)
        XCTAssertLessThanOrEqual(amounts.values.reduce(0, +), 10000.01)
        XCTAssertTrue(amounts.values.allSatisfy { $0 <= 2000.0001 })
    }

    func testCandles() throws {
        let s = try JSONDecoder().decode(Snapshot.self, from: fixture("candles"))
        XCTAssertEqual(s.candles.count, 30)
        XCTAssertGreaterThan(s.agreeing, 10)
    }

    func testHorizonsMatchServer() {
        XCTAssertEqual(Horizon.allCases.map(\.rawValue), ["30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m"])
    }
}

final class SSETests: XCTestCase {
    func testRealStreamCutAnywhere() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "live", withExtension: "txt", subdirectory: "Fixtures"))
        let raw = try Data(contentsOf: url)
        var whole = SSEParser()
        let all = whole.ticks(raw)
        XCTAssertGreaterThanOrEqual(all.count, 3)
        XCTAssertEqual(whole.retryMs, 3000)
        XCTAssertEqual(all.first?.key, "crypto:BTC")
        XCTAssertTrue(all.contains { $0.market == "closed" })
        // Same stream delivered byte by byte: same ticks.
        var split = SSEParser()
        var got: [LiveTick] = []
        for b in raw { got += split.ticks(Data([b])) }
        XCTAssertEqual(got.map(\.price), all.map(\.price))
    }

    func testCommentsCRLFAndMultiline() {
        var p = SSEParser()
        let events = p.feed(Data(": ok\r\n\r\ndata: a\r\ndata: b\r\n\r\ndata:c\n\n".utf8))
        XCTAssertEqual(events, ["a\nb", "c"])
    }

    func testUTF8SplitInsideCharacter() {
        var p = SSEParser()
        let bytes = Array("data: é€\n\n".utf8)
        var out: [String] = []
        for b in bytes { out += p.feed(Data([b])) }
        XCTAssertEqual(out, ["é€"])
    }
}

final class FormatAndClientTests: XCTestCase {
    func testFrenchFormats() {
        XCTAssertEqual(Format.price(84518.16), "84\u{202F}518,16 $")
        XCTAssertEqual(Format.price(0.3432), "0,3432 $")
        XCTAssertEqual(Format.price(0.00001234), "0,00001234 $")
        XCTAssertEqual(Format.price(nil), "—")
        XCTAssertEqual(Format.percent(-0.4), "−0,40 %")
        XCTAssertEqual(Format.percent(1.25), "+1,25 %")
        XCTAssertEqual(Format.money(1500), "1\u{202F}500 $")
    }

    func testNormalizeServer() {
        XCTAssertEqual(AltimClient.normalize("mon-app.herokuapp.com/")?.absoluteString, "https://mon-app.herokuapp.com")
        XCTAssertEqual(AltimClient.normalize(" https://x.herokuapp.com ")?.absoluteString, "https://x.herokuapp.com")
        XCTAssertEqual(AltimClient.normalize("http://localhost:4410")?.absoluteString, "http://localhost:4410")
        XCTAssertNil(AltimClient.normalize("http://x.herokuapp.com"), "no plain http on the Internet: the password would travel in clear")
        XCTAssertNil(AltimClient.normalize("https://x.herokuapp.com/app"))
        XCTAssertNil(AltimClient.normalize(""))
    }

    func testRequestsAndForm() {
        let c = AltimClient(baseURL: URL(string: "https://x.herokuapp.com")!, credentials: nil)
        let r = c.request("/api/radar", query: ["symbols": AltimClient.list(Asset.defaults.prefix(2).map { $0 }), "interval": "4h"])
        XCTAssertEqual(r.url?.absoluteString, "https://x.herokuapp.com/api/radar?interval=4h&symbols=BTC:crypto,ETH:crypto")
        XCTAssertEqual(c.request("/api/search", query: ["q": "a+b"]).url?.query, "q=a%2Bb")
        XCTAssertEqual(AltimClient.form(["text": "a&b=c d+é", "user": "max"]), "text=a%26b%3Dc%20d%2B%C3%A9&user=max")
    }

    func testSessionCookieHeader() {
        XCTAssertEqual(AltimClient.sessionCookie(in: "altim_session=abc.def-1; Path=/; HttpOnly; SameSite=Strict; Max-Age=604800; Secure"), "abc.def-1")
        XCTAssertEqual(AltimClient.sessionCookie(in: "other=1; Path=/, altim_session=xyz; Path=/"), "xyz")
        XCTAssertNil(AltimClient.sessionCookie(in: "altim_session=; Path=/; Max-Age=0"))
        XCTAssertNil(AltimClient.sessionCookie(in: ""))
    }

    func testPortfolio() {
        let btc = Holding(asset: Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), quantity: 0.5, averagePrice: 60000)
        let aapl = Holding(asset: Asset(symbol: "AAPL", kind: .stock, name: "Apple"), quantity: 10, averagePrice: 200)
        let p = Portfolio(holdings: [aapl, btc], prices: ["crypto:BTC": 80000, "stock:AAPL": 300])
        XCTAssertEqual(p.total, 43000)
        XCTAssertEqual(p.cost, 32000)
        XCTAssertEqual(p.gain, 11000)
        XCTAssertEqual(p.lines.first?.holding.asset.symbol, "BTC")
        XCTAssertEqual(p.lines.first?.gainPercent ?? 0, 33.333, accuracy: 0.01)
        XCTAssertEqual(p.cryptoShare, 40000.0 / 43000 * 100, accuracy: 0.001)
        XCTAssertEqual(p.warnings.count, 2)
        let partial = Portfolio(holdings: [aapl, btc], prices: ["stock:AAPL": 300])
        XCTAssertEqual(partial.total, 3000)
        XCTAssertTrue(partial.warnings.contains { $0.contains("BTC") })
    }
}

/// Against a running server: ALTIM_SERVER=http://localhost:4410 ALTIM_USER=… ALTIM_PASSWORD=… swift test
final class LiveServerTests: XCTestCase {
    func testLoginAndAPI() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let s = env["ALTIM_SERVER"], let url = AltimClient.normalize(s) else { throw XCTSkip("ALTIM_SERVER absent") }
        let creds = env["ALTIM_USER"].map { Credentials(user: $0, password: env["ALTIM_PASSWORD"] ?? "") }
        let anonymous = AltimClient(baseURL: url, credentials: nil, session: URLSession(configuration: .ephemeral))
        let mode = try await anonymous.accessMode()
        if case .login = mode {
            let c = try XCTUnwrap(creds)
            do {
                _ = try await anonymous.macro()
                XCTFail("API open without login")
            } catch let e as AltimError { XCTAssertEqual(e, .unauthorized) }
            do {
                try await anonymous.login(Credentials(user: c.user, password: c.password + "x"))
                XCTFail("wrong password accepted")
            } catch let e as AltimError { XCTAssertEqual(e, .wrongCredentials) }
        }
        // Fresh cookie jar, logged in automatically on the first 401.
        let client = AltimClient(baseURL: url, credentials: creds, session: URLSession(configuration: .ephemeral))
        let rows = try await client.radar(Asset.defaults)
        XCTAssertNotNil(client.sessionCookie)
        // The saved cookie is enough for a new client (next launch), without credentials.
        let reopened = AltimClient(baseURL: url, credentials: nil, sessionCookie: client.sessionCookie)
        _ = try await reopened.macro()
        XCTAssertEqual(rows.count, Asset.defaults.count)
        let z = try await client.zones(Asset.defaults[0])
        XCTAssertEqual(z.zones.count, 3)
        _ = try await client.guardReport(Asset.defaults[5])
        _ = try await client.macro()
        let found = try await client.search("sol")
        XCTAssertFalse(found.isEmpty)
        // Live prices: several ticks within a few seconds, for the assets asked.
        var ticks: [LiveTick] = []
        for try await t in client.liveTicks([Asset.defaults[0], Asset.defaults[5]]) {
            ticks.append(t)
            if ticks.count >= 5 { break }
        }
        XCTAssertTrue(ticks.allSatisfy { ["crypto:BTC", "stock:AAPL"].contains($0.key) && $0.price > 0 })
        // Without session, the stream says so instead of hanging.
        do {
            for try await _ in AltimClient(baseURL: url, credentials: nil).liveTicks([Asset.defaults[0]]) {}
            if case .login = mode { XCTFail("live stream open without login") }
        } catch let e as AltimError { XCTAssertEqual(e, .unauthorized) }
        let snap = try await client.candles(Asset.defaults[0], interval: "1d")
        XCTAssertGreaterThan(snap.candles.count, 100)
    }
}
