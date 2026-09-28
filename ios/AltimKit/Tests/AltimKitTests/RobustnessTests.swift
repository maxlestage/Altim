import Foundation
import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimKit

/// Fake network: each request pops the next scripted answer (status + body) or network failure.
final class StubProtocol: URLProtocol {
    enum Step { case status(Int, String), offline }
    nonisolated(unsafe) static var script: [Step] = []
    nonisolated(unsafe) static var calls = 0
    nonisolated(unsafe) static var urls: [URL] = []

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.calls += 1
        if let url = request.url { Self.urls.append(url) }
        let step = Self.script.isEmpty ? .offline : Self.script.removeFirst()
        switch step {
        case let .status(code, body):
            let response = HTTPURLResponse(url: request.url!, statusCode: code, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "application/json"])!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Data(body.utf8))
            client?.urlProtocolDidFinishLoading(self)
        case .offline:
            client?.urlProtocol(self, didFailWithError: URLError(.notConnectedToInternet))
        }
    }
    override func stopLoading() {}
}

final class RobustnessTests: XCTestCase {
    let macroJSON = #"{"score":0,"level":"calm","factors":[],"themes":[],"values":{}}"#
    var dir: URL!

    override func setUp() {
        AltimClient.retryDelays = [0.01, 0.01]
        StubProtocol.script = []
        StubProtocol.calls = 0
        StubProtocol.urls = []
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("altim-cache-\(UUID().uuidString)")
    }

    override func tearDown() { try? FileManager.default.removeItem(at: dir) }

    func client(cache: ResponseCache? = nil, status: (@Sendable (Date?) -> Void)? = nil) -> AltimClient {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [StubProtocol.self]
        return AltimClient(baseURL: URL(string: "https://x.herokuapp.com")!, credentials: nil, session: AltimClient.makeSession(configuration: config), cache: cache, onStatus: status)
    }

    func testTemporaryServerErrorsAreRetried() async throws {
        StubProtocol.script = [.status(503, "{}"), .offline, .status(200, macroJSON)]
        let m = try await client().macro()
        XCTAssertEqual(m.level, "calm")
        XCTAssertEqual(StubProtocol.calls, 3)
    }

    func testGivesUpAfterThreeAttempts() async {
        StubProtocol.script = [.offline, .offline, .offline, .status(200, macroJSON)]
        do {
            _ = try await client().macro()
            XCTFail("should fail")
        } catch let e as AltimError {
            guard case .network = e else { return XCTFail("\(e)") }
        } catch { XCTFail("\(error)") }
        XCTAssertEqual(StubProtocol.calls, 3)
    }

    func testOfflineServesLastGoodAnswerWithItsDate() async throws {
        let cache = FileResponseCache(directory: dir)
        let seen = Box()
        let c = client(cache: cache) { seen.value = $0 }
        StubProtocol.script = [.status(200, macroJSON)]
        _ = try await c.macro()
        XCTAssertNil(seen.value, "online: no stale date")
        // Network gone: the cached answer comes back, with its date for the banner.
        StubProtocol.script = [.offline, .offline, .offline]
        let m = try await c.macro()
        XCTAssertEqual(m.level, "calm")
        XCTAssertNotNil(seen.value)
        XCTAssertLessThan(abs(seen.value!.timeIntervalSinceNow), 60)
        // Server down (500) after retries: same.
        StubProtocol.script = [.status(500, #"{"error":"panne"}"#)]
        _ = try await c.macro()
        // A search is never served from the cache.
        StubProtocol.script = [.offline, .offline, .offline]
        do {
            _ = try await c.search("btc")
            XCTFail("search is not cached")
        } catch {}
    }

    func testCachePruneKeepsTheMostRecent() throws {
        let cache = FileResponseCache(directory: dir, limit: 5)
        for i in 0..<12 { cache.save("/api/radar?i=\(i)", Data("\(i)".utf8)) }
        cache.prune()
        let files = try FileManager.default.contentsOfDirectory(atPath: dir.path)
        XCTAssertEqual(files.count, 5)
        XCTAssertNotEqual(FileResponseCache.fileName("/api/radar?symbols=BTC:crypto"), FileResponseCache.fileName("/api/radar?symbols=ETH:crypto"))
    }
}

final class Box: @unchecked Sendable {
    var value: Date?
}
