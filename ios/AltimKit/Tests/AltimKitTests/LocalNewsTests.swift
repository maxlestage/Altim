import Foundation
import XCTest
@testable import AltimKit

final class LocalNewsTests: XCTestCase {
    let now = 1_793_400_000_000.0 // 30 Oct 2026 ≈ 22:40 UTC

    func testRssAtomAndGoogleNews() {
        let rss = """
        <?xml version="1.0"?><rss><channel>
        <item><title><![CDATA[Bitcoin surges to record high &amp; ETFs see inflows]]></title><link>https://example.com/a</link>
        <pubDate>Fri, 30 Oct 2026 20:00:00 GMT</pubDate><description>&lt;p&gt;Big &lt;b&gt;day&lt;/b&gt;&lt;/p&gt;</description></item>
        <item><title>Lien piégé</title><link>javascript:alert(1)</link><pubDate>Fri, 30 Oct 2026 20:00:00 GMT</pubDate></item>
        <item><title>La Fed maintient ses taux directeurs - Le Monde</title><link>https://news.google.com/x</link>
        <pubDate>Fri, 30 Oct 2026 19:00:00 +0000</pubDate><source url="https://lemonde.fr">Le Monde</source></item>
        </channel></rss>
        """
        let items = NewsEngine.parse(rss, fallbackSource: "Google News (world)")
        XCTAssertEqual(items.map(\.title), ["Bitcoin surges to record high & ETFs see inflows", "La Fed maintient ses taux directeurs"])
        XCTAssertEqual(items[0].summary, "Big day")
        XCTAssertEqual(items[0].time, 1_793_390_400_000)
        XCTAssertEqual(items[1].source, "Le Monde")
        let atom = #"<feed><entry><title>Nvidia earnings beat estimates</title><link rel="alternate" href="https://example.com/n"/><updated>2026-10-30T18:30:00Z</updated></entry></feed>"#
        let a = NewsEngine.parse(atom, fallbackSource: "Feed")
        XCTAssertEqual(a.first?.link, "https://example.com/n")
        XCTAssertEqual(a.first?.source, "Feed")
        // Google's own " - Source" suffix when the item has no <source>.
        let g = NewsEngine.parse("<item><title>Wall Street ends higher as tech stocks rally - Reuters</title><link>https://g.co/1</link><pubDate>Fri, 30 Oct 2026 21:00:00 GMT</pubDate></item>", fallbackSource: "Google News (markets)")
        XCTAssertEqual(g.first?.source, "Reuters")
        XCTAssertEqual(g.first?.title, "Wall Street ends higher as tech stocks rally")
    }

    func testMergeClassifyAndMatchAssets() {
        let f1 = NewsEngine.Feed(name: "CoinDesk", url: "", category: "crypto")
        let f2 = NewsEngine.Feed(name: "Decrypt", url: "", category: "crypto")
        let f3 = NewsEngine.Feed(name: "CNBC", url: "", category: "marches")
        let raw1 = [NewsEngine.Raw(title: "Bitcoin price surges past record high as ETF inflows grow", link: "https://a/1", time: now - 3_600_000, source: "CoinDesk")]
        let raw2 = [NewsEngine.Raw(title: "Bitcoin price surges past record high as ETF inflows jump", link: "https://b/1", time: now - 1_800_000, source: "Decrypt")]
        let raw3 = [NewsEngine.Raw(title: "Russia declares war, markets plunge", link: "https://c/1", time: now - 600_000, source: "CNBC"),
                    NewsEngine.Raw(title: "Could a bank run hit regional lenders?", link: "https://c/2", time: now - 700_000, source: "CNBC"),
                    NewsEngine.Raw(title: "Old story", link: "https://c/3", time: now - 72 * 3_600_000, source: "CNBC")]
        let watch = [NewsEngine.WatchAsset(id: "crypto:BTC", symbol: "BTC", name: "Bitcoin")]
        let items = NewsEngine.aggregate([(f1, raw1), (f2, raw2), (f3, raw3)], assets: watch, now: now)
        XCTAssertEqual(items.count, 3)
        let btc = items.first { $0.assets == ["crypto:BTC"] }!
        XCTAssertEqual(btc.category, "actifs")
        XCTAssertEqual(btc.source, "CoinDesk")
        XCTAssertEqual(btc.alsoIn, ["Decrypt"])
        XCTAssertEqual(btc.tone, "positive")
        XCTAssertEqual(btc.lang, "en")
        let war = items.first { $0.link == "https://c/1" }!
        XCTAssertTrue(war.alert)
        XCTAssertTrue(war.themes.contains("geopolitics"))
        XCTAssertEqual(war.tone, "negative")
        XCTAssertFalse(items.first { $0.link == "https://c/2" }!.alert)
        XCTAssertEqual(NewsEngine.top(items, 5).map(\.id), [btc.id])
        let d = NewsEngine.digest(items, now: now)
        XCTAssertEqual(d.total, 3)
        XCTAssertEqual(NewsEngine.hash("https://a/1"), NewsEngine.hash("https://a/1"))
    }

    func testNewsRouteListsDeadFeeds() async throws {
        let backend = LocalBackend(fetcher: FakeFetcher(routes: [("https://cointelegraph.com", "<rss><item><title>Ethereum upgrade goes live on mainnet today</title><link>https://ct/1</link><pubDate>\(Self.rfc(Date()))</pubDate></item></rss>")]))
        let r = await backend.answer(path: "/api/news", query: ["symbols": "ETH:crypto"])!
        let report = try JSONDecoder().decode(NewsReport.self, from: r.0)
        XCTAssertEqual(report.items.map(\.title), ["Ethereum upgrade goes live on mainnet today"])
        XCTAssertEqual(report.items.first?.category, "actifs")
        XCTAssertEqual(report.sources.first { $0.name == "Cointelegraph" }?.ok, true)
        XCTAssertEqual(report.sources.first { $0.name == "CoinDesk" }?.ok, false)
    }

    static func rfc(_ d: Date) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(identifier: "UTC")
        f.dateFormat = "EEE, dd MMM yyyy HH:mm:ss 'GMT'"
        return f.string(from: d)
    }

    func testLiveNews() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        let r = await NewsEngine.report([Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), Asset(symbol: "AAPL", kind: .stock, name: "Apple")], fetcher: URLSessionFetcher())
        print("news: \(r.items.count) articles, sources ok \(r.sources.filter(\.ok).count)/\(r.sources.count): " + r.sources.map { "\($0.name)=\($0.ok ? "\($0.count)" : $0.error ?? "?")" }.joined(separator: " "))
        for i in r.topItems { print("À la une: \(i.title) [\(i.source) + \(i.alsoIn.count)]") }
        print("thèmes:", r.digest.themes.map { "\($0.label) \($0.count)" }, "ton:", r.digest.tone)
        for i in r.items.filter({ $0.category == "actifs" }).prefix(4) { print("actif \(i.assets): \(i.title)") }
    }
}
