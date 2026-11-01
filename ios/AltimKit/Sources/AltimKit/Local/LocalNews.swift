import Foundation

/// The news section read by the iPhone itself (core/src/engine/news.rs, backend/src/news.rs): public RSS / Atom feeds
/// of the world economy, the markets, crypto and the user's own assets, in French and English. The same story told by
/// several sources is merged (its coverage tells how important it is); each one is classified by theme and tone, and
/// the user's assets named in its title are found. Titles stay in their language: nothing is rewritten.
enum NewsEngine {
    struct Feed: Sendable {
        var name: String
        var url: String
        /// "monde", "marches", "crypto" or "actifs".
        var category: String
        /// For the searches of one asset: where a story that does not name it goes.
        var fallback: String?
    }

    struct Raw: Sendable, Equatable {
        var title: String
        var link: String
        var time: Double
        var source: String
        var summary: String?
    }

    struct WatchAsset: Sendable {
        var id: String, symbol: String, name: String
    }

    // MARK: Feeds

    static func google(_ q: String, fr: Bool, when: String) -> String {
        let query = Sources.enc("\(q) when:\(when)")
        return fr ? "https://news.google.com/rss/search?q=\(query)&hl=fr&gl=FR&ceid=FR:fr"
            : "https://news.google.com/rss/search?q=\(query)&hl=en-US&gl=US&ceid=US:en"
    }

    static let feeds: [Feed] = [
        Feed(name: "Le Monde Économie", url: "https://www.lemonde.fr/economie/rss_full.xml", category: "monde"),
        Feed(name: "BFM Économie", url: "https://www.bfmtv.com/rss/economie/", category: "monde"),
        Feed(name: "La Tribune", url: "https://www.latribune.fr/feed.xml", category: "monde"),
        Feed(name: "Google Actualités (économie)", url: google("(économie OR bourse OR BCE OR Fed OR inflation OR guerre OR sanctions OR droits de douane)", fr: true, when: "1d"), category: "monde"),
        Feed(name: "Google News (world)", url: google(#"(war OR sanctions OR "Federal Reserve" OR inflation OR tariffs OR recession OR ceasefire)"#, fr: false, when: "1d"), category: "monde"),
        Feed(name: "MarketWatch", url: "https://feeds.content.dowjones.io/public/rss/mw_topstories", category: "marches"),
        Feed(name: "CNBC", url: "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=100003114", category: "marches"),
        Feed(name: "Investing.com", url: "https://www.investing.com/rss/news_25.rss", category: "marches"),
        Feed(name: "Google News (markets)", url: google(#"("stock market" OR "Wall Street" OR Nasdaq OR "S&P 500")"#, fr: false, when: "1d"), category: "marches"),
        Feed(name: "CoinDesk", url: "https://www.coindesk.com/arc/outboundfeeds/rss/", category: "crypto"),
        Feed(name: "Cointelegraph", url: "https://cointelegraph.com/rss", category: "crypto"),
        Feed(name: "Decrypt", url: "https://decrypt.co/feed", category: "crypto"),
        Feed(name: "The Block", url: "https://www.theblock.co/rss.xml", category: "crypto"),
        Feed(name: "Cryptoast", url: "https://cryptoast.fr/feed/", category: "crypto"),
        Feed(name: "Journal du Token", url: "https://journaldutoken.com/feed/", category: "crypto"),
    ]

    static func companyName(_ name: String) -> String {
        let r = name.range(of: #",? (Inc|Corp|Corporation|Ltd|plc)\.?$"#, options: [.regularExpression, .caseInsensitive])
        return r.map { name.replacingCharacters(in: $0, with: "") } ?? name
    }

    /// Two searches per asset (English and French), plus Yahoo Finance's headlines for a stock.
    static func assetFeeds(_ assets: [Asset]) -> [Feed] {
        assets.prefix(20).flatMap { a -> [Feed] in
            let name = companyName(a.name)
            let symbol = a.symbol.replacingOccurrences(of: "-", with: ".")
            let crypto = a.kind == .crypto
            let fallback = crypto ? "crypto" : "marches"
            var list = [
                Feed(name: "Google News", url: google(crypto ? "\"\(name)\" crypto" : "\(symbol) stock \"\(name)\"", fr: false, when: "2d"), category: "actifs", fallback: fallback),
                Feed(name: "Google Actualités", url: google(crypto ? "\"\(name)\" crypto" : "\"\(name)\" OR \(symbol) action", fr: true, when: "2d"), category: "actifs", fallback: fallback),
            ]
            if !crypto {
                list.append(Feed(name: "Yahoo Finance", url: "https://feeds.finance.yahoo.com/rss/2.0/headline?s=\(Sources.enc(a.symbol))&region=US&lang=en-US",
                                 category: "actifs", fallback: fallback))
            }
            return list
        }
    }

    // MARK: Parsing

    static func regex(_ p: String, _ options: NSRegularExpression.Options = []) -> NSRegularExpression {
        // swiftlint:disable:next force_try
        try! NSRegularExpression(pattern: p, options: options)
    }

    static func replace(_ s: String, _ r: NSRegularExpression, _ template: String) -> String {
        r.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: template)
    }

    static let cdata = regex(#"<!\[CDATA\[(.*?)\]\]>"#, [.dotMatchesLineSeparators])
    static let tags = regex("<[^>]*>")
    static let spaces = regex(#"\s+"#)
    static let hexEntity = regex("&#x([0-9a-f]+);", [.caseInsensitive])
    static let decEntity = regex("&#([0-9]+);")
    static let item = regex(#"<item\b[^>]*>(.*?)</item>"#, [.dotMatchesLineSeparators])
    static let entry = regex(#"<entry\b[^>]*>(.*?)</entry>"#, [.dotMatchesLineSeparators])
    static let linkHref = regex(#"<link\b[^>]*href="([^"]+)""#)
    static let tagNames = ["title", "link", "pubDate", "dc:date", "updated", "published", "source", "description", "summary"]
    static let tagRegex: [String: NSRegularExpression] = Dictionary(uniqueKeysWithValues: tagNames.map {
        ($0, regex("<\(NSRegularExpression.escapedPattern(for: $0))\\b[^>]*>(.*?)</\(NSRegularExpression.escapedPattern(for: $0))>", [.dotMatchesLineSeparators]))
    })

    static func codePoints(_ s: String, _ r: NSRegularExpression, radix: Int) -> String {
        let ns = s as NSString
        var out = ""
        var last = 0
        for m in r.matches(in: s, range: NSRange(location: 0, length: ns.length)) {
            out += ns.substring(with: NSRange(location: last, length: m.range.location - last))
            let digits = ns.substring(with: m.range(at: 1))
            if let n = UInt32(digits, radix: radix), let u = Unicode.Scalar(n) { out.unicodeScalars.append(u) } else { out += ns.substring(with: m.range) }
            last = m.range.location + m.range.length
        }
        guard last > 0 else { return s }
        return out + ns.substring(from: last)
    }

    static func entities(_ t: String) -> String {
        codePoints(codePoints(t, hexEntity, radix: 16), decEntity, radix: 10)
            .replacingOccurrences(of: "&nbsp;", with: " ").replacingOccurrences(of: "&quot;", with: "\"")
            .replacingOccurrences(of: "&#39;", with: "'").replacingOccurrences(of: "&apos;", with: "'")
            .replacingOccurrences(of: "&lt;", with: "<").replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&amp;", with: "&")
    }

    /// XML text decoded: CDATA, entities (twice: some feeds encode their HTML), tags, spaces.
    static func decode(_ s: String) -> String {
        var t = replace(s, cdata, "$1")
        t = replace(entities(t), tags, " ")
        t = replace(entities(t), tags, " ")
        return replace(t, spaces, " ").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Only http(s) links are kept (a feed could carry javascript: or data: URLs).
    static func safeLink(_ s: String) -> String? {
        let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let u = URL(string: t), let scheme = u.scheme?.lowercased(), scheme == "https" || scheme == "http", u.host != nil else { return nil }
        return t
    }

    static let rfc822: [DateFormatter] = ["EEE, dd MMM yyyy HH:mm:ss Z", "EEE, dd MMM yyyy HH:mm:ss zzz", "EEE, d MMM yyyy HH:mm:ss Z",
                                          "EEE, d MMM yyyy HH:mm:ss zzz", "dd MMM yyyy HH:mm:ss Z", "EEE, dd MMM yyyy HH:mm Z",
                                          "EEE, dd MMM yyyy HH:mm:ss"].map { f in
        let d = DateFormatter()
        d.locale = Locale(identifier: "en_US_POSIX")
        d.timeZone = TimeZone(identifier: "UTC")
        d.dateFormat = f
        return d
    }

    /// RFC 822 (RSS) or ISO 8601 (Atom) date, in ms.
    static func date(_ s: String) -> Double? {
        let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return nil }
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let d = iso.date(from: t) { return d.timeIntervalSince1970 * 1000 }
        iso.formatOptions = [.withInternetDateTime]
        if let d = iso.date(from: t) { return d.timeIntervalSince1970 * 1000 }
        for f in rfc822 { if let d = f.date(from: t) { return d.timeIntervalSince1970 * 1000 } }
        return nil
    }

    /// RSS 2.0 and Atom. `fallbackSource` names the feed when items do not (Google News gives each item's source).
    static func parse(_ xml: String, fallbackSource: String) -> [Raw] {
        let ns = xml as NSString
        let all = NSRange(location: 0, length: ns.length)
        let blocks = (item.matches(in: xml, range: all) + entry.matches(in: xml, range: all)).map { ns.substring(with: $0.range(at: 1)) }
        return blocks.compactMap { b -> Raw? in
            let bns = b as NSString
            func tag(_ name: String) -> String? {
                guard let m = tagRegex[name]?.firstMatch(in: b, range: NSRange(location: 0, length: bns.length)) else { return nil }
                return bns.substring(with: m.range(at: 1))
            }
            var title = decode(tag("title") ?? "")
            let linkText = decode(tag("link") ?? "")
            let link: String?
            if linkText.isEmpty, let m = linkHref.firstMatch(in: b, range: NSRange(location: 0, length: bns.length)) {
                link = safeLink(bns.substring(with: m.range(at: 1)))
            } else {
                link = safeLink(linkText)
            }
            let time = date(decode(tag("pubDate") ?? tag("dc:date") ?? tag("updated") ?? tag("published") ?? ""))
            let sourceText = decode(tag("source") ?? "")
            var source = sourceText.isEmpty ? fallbackSource : sourceText
            // Google News appends " - Source" to the title.
            if title.hasSuffix(" - \(source)") {
                title = String(title.dropLast(source.count + 3))
            } else if source == fallbackSource, fallbackSource.hasPrefix("Google"), let cut = title.range(of: " - ", options: .backwards),
                      title[..<cut.lowerBound].count > 20 {
                source = String(title[cut.upperBound...])
                title = String(title[..<cut.lowerBound])
            }
            var summary = decode(tag("description") ?? tag("summary") ?? "")
            // Google News' description only repeats the title and the source.
            if summary.hasPrefix(String(title.prefix(30))) { summary = "" }
            if summary.count > 280 {
                var cut = String(summary.prefix(277))
                if let space = cut.lastIndex(where: { $0.isWhitespace }) { cut = String(cut[..<space]) }
                summary = cut + "…"
            }
            guard let link, let time, !title.isEmpty else { return nil }
            return Raw(title: title, link: link, time: time, source: source, summary: summary.isEmpty ? nil : summary)
        }
    }

    // MARK: Classification

    static func words(_ s: String) -> [String] {
        let folded = s.lowercased().folding(options: .diacriticInsensitive, locale: nil)
        let t = String(folded.map { ($0.isASCII && ($0.isLetter || $0.isNumber)) || $0 == " " ? $0 : " " })
        var out: [String] = []
        for w in t.split(separator: " ") where w.count > 2 && !out.contains(String(w)) { out.append(String(w)) }
        return out
    }

    /// Same story: at least 60 % of the words in common (Jaccard), told within 36 h.
    static func sameStory(_ a: Raw, _ b: Raw) -> Bool {
        guard abs(a.time - b.time) <= 36 * 3_600_000 else { return false }
        let wa = words(a.title), wb = words(b.title)
        guard wa.count >= 3, wb.count >= 3 else { return false }
        let common = wa.filter { wb.contains($0) }.count
        return Double(common) / Double(wa.count + wb.count - common) >= 0.6
    }

    static func wordRegex(_ alternatives: String) -> NSRegularExpression { regex("\\b(\(alternatives))\\b", [.caseInsensitive]) }

    static let themes: [(String, String, NSRegularExpression)] = [
        ("geopolitics", "Géopolitique / guerre", wordRegex("wars?|invasion|invades?|invaded|missiles?|air ?strikes?|drones?|military|troops|nuclear|sanctions?|ceasefire|hostages?|coup|blockade|guerre|frappes?|armée|militaires?|nucléaire|cessez-le-feu|otages?")),
        ("monetary", "Banques centrales / taux", wordRegex("fed|federal reserve|fomc|powell|ecb|bce|lagarde|rate (hikes?|cuts?)|interest rates?|inflation|cpi|treasury yields?|taux (directeurs?|d'intérêt)|banque centrale|baisse des taux|hausse des taux")),
        ("trade", "Commerce / droits de douane", wordRegex("tariffs?|trade war|export (ban|controls?)|embargo|droits de douane|guerre commerciale|taxes douanières")),
        ("stress", "Crise / krach", wordRegex("recession|default(s|ed)?|bank (runs?|collapse|failures?)|financial crisis|market crash|crash|sell-?off|bankruptcy|contagion|récession|krach|faillite|crise financière|effondrement|défaut de paiement")),
        ("regulation", "Régulation", wordRegex("sec|cftc|regulators?|regulation|lawsuit|etf approval|mica|amf|régulateur|régulation|réglementation|plainte|procès")),
        ("earnings", "Résultats d'entreprises", wordRegex("earnings|quarterly results|revenue|guidance|eps|profit warning|résultats (trimestriels|annuels|semestriels)|chiffre d'affaires|bénéfice")),
    ]
    static let hedged = wordRegex("could|might|may|would|risks?|fears? of|what if|pourrai(t|ent)|risquer?ai(t|ent)|risque de|et si|selon (cet|un|une)")
    static let escalation = wordRegex("declar(es|ed|ing) war|invades?|invaded|invasion of|nuclear (strike|attack|threat|test)|martial law|state of emergency|bank runs?|bank collapse|circuit breaker|trading halted|defaults? on (its )?debt|déclare la guerre|déclaration de guerre|loi martiale|état d'urgence|panique bancaire|cotations suspendues")
    static let negative = wordRegex("hack(ed)?|exploit|breach|stolen|lawsuit|sues|sued|fraud|bankrupt(cy)?|insolvency|liquidat(ed|ion)|delist(ed|ing)?|ban(ned)?|crackdown|crash(es)?|plunges?|tumbles?|sinks?|slumps?|slides?|sell-?off|downgraded?|misses|layoffs|recall|outage|warning|indictment|falls?|drops?|losses?|piratage|fraude|faillite|chute|plonge|recule|recul|dégringole|effondre|baisse|pertes?|licenciements?|panne|avertissement|sanctions?")
    static let positive = wordRegex("approv(al|ed|es)|inflows|record high|all-time high|surges?|soars?|rall(y|ies)|upgraded?|beats|raises guidance|buyback|partnership|adoption|breakthrough|jumps?|climbs?|gains?|rebounds?|hausse|bondit|grimpe|record|rebond|progresse|s'envole|partenariat|rachat|approbation")
    static let french = wordRegex("le|la|les|des|du|une|pour|dans|sur|avec|est|sont|pas|plus|qui|après|selon|français|bourse|marchés?")
    static let accented = regex("[éèàùç]", [.caseInsensitive])

    static func matches(_ r: NSRegularExpression, _ s: String) -> Bool { r.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) != nil }
    static func count(_ r: NSRegularExpression, _ s: String) -> Int { r.numberOfMatches(in: s, range: NSRange(s.startIndex..., in: s)) }

    /// Negative or positive when only one kind of word is found, else neutral.
    static func tone(_ title: String) -> String {
        let neg = matches(negative, title), pos = matches(positive, title)
        return neg && !pos ? "negative" : pos && !neg ? "positive" : "neutral"
    }

    /// Does the title name this asset? Its name as a whole word (4 letters at least), or its ticker (3+ letters).
    struct Matcher {
        var name: NSRegularExpression?
        var symbol: NSRegularExpression?
        init(_ a: WatchAsset) {
            let n = a.name.range(of: #",? (Inc|Corp|Corporation|Ltd|plc|SA|NV|Holdings?)\.?$"#, options: [.regularExpression, .caseInsensitive])
                .map { a.name.replacingCharacters(in: $0, with: "") }?.trimmingCharacters(in: .whitespaces) ?? a.name.trimmingCharacters(in: .whitespaces)
            name = n.count >= 4 ? NewsEngine.regex("(?:^|\\P{L})\(NSRegularExpression.escapedPattern(for: n))(?:\\P{L}|$)", [.caseInsensitive]) : nil
            symbol = a.symbol.count >= 3 ? NewsEngine.regex("(?:^|[^A-Za-z$])\\$?\(NSRegularExpression.escapedPattern(for: a.symbol))(?:[^A-Za-z]|$)") : nil
        }
        func matches(_ t: String) -> Bool {
            (name.map { NewsEngine.matches($0, t) } ?? false) || (symbol.map { NewsEngine.matches($0, t) } ?? false)
        }
    }

    /// FNV-1a over the UTF-16 code units, base 36 (the server's ids).
    static func hash(_ s: String) -> String {
        var h: UInt32 = 2_166_136_261
        for u in s.utf16 { h = (h ^ UInt32(u)) &* 16_777_619 }
        return String(h, radix: 36)
    }

    static let perCategory: [String: Int] = ["monde": 70, "marches": 60, "crypto": 60, "actifs": 80]

    /// Raw items of every feed → the news section: duplicates merged (the earliest kept, the others in `alsoIn`), most
    /// recent first, 48 h at most, a cap per category. A story goes under "actifs" only if it names one of the assets.
    static func aggregate(_ feeds: [(Feed, [Raw])], assets: [WatchAsset], now: Double, maxAge: Double = 48 * 3_600_000) -> [NewsItem] {
        var all: [(Raw, String)] = []
        for (f, items) in feeds {
            let category = f.category == "actifs" ? f.fallback ?? "marches" : f.category
            for i in items where i.time <= now + 600_000 && i.time >= now - maxAge { all.append((i, category)) }
        }
        all.sort { $0.0.time < $1.0.time }
        var kept: [(news: Raw, category: String, alsoIn: [String])] = []
        for (it, category) in all {
            if let k = kept.firstIndex(where: { $0.news.link == it.link || sameStory($0.news, it) }) {
                if kept[k].news.source != it.source && !kept[k].alsoIn.contains(it.source) { kept[k].alsoIn.append(it.source) }
                continue
            }
            kept.append((it, category, []))
        }
        let matchers = assets.map { ($0, Matcher($0)) }
        var items = kept.map { k -> NewsItem in
            let n = k.news
            let text = "\(n.title) \(n.summary ?? "")"
            let ids = matchers.filter { $0.1.matches(n.title) }.map(\.0.id)
            return NewsItem(id: hash(n.link), title: n.title, link: n.link, time: n.time, source: n.source, summary: n.summary,
                            lang: count(french, n.title) >= 2 || matches(accented, n.title) ? "fr" : "en",
                            category: ids.isEmpty ? k.category : "actifs",
                            themes: themes.filter { matches($0.2, text) }.map(\.0), tone: tone(n.title), assets: ids,
                            alsoIn: Array(k.alsoIn.prefix(8)), alert: matches(escalation, n.title) && !matches(hedged, n.title))
        }
        items.sort { $0.time > $1.time }
        var counts: [String: Int] = [:]
        return items.filter { i in
            counts[i.category, default: 0] += 1
            return counts[i.category]! <= perCategory[i.category, default: 60]
        }
    }

    /// "À la une": serious escalations first, then the stories told by the most sources (2 at least), recent first.
    static func top(_ items: [NewsItem], _ n: Int) -> [NewsItem] {
        Array(items.filter { !$0.alsoIn.isEmpty }.sorted {
            ($0.alert ? 1 : 0, $0.alsoIn.count, $0.time) > ($1.alert ? 1 : 0, $1.alsoIn.count, $1.time)
        }.prefix(n))
    }

    /// Counts per theme and tone over the last 24 h.
    static func digest(_ items: [NewsItem], now: Double) -> NewsReport.Digest {
        let day = items.filter { $0.time >= now - 86_400_000 }
        let t = themes.map { th in NewsReport.Digest.Theme(theme: th.0, label: th.1, count: day.filter { $0.themes.contains(th.0) }.count) }
            .filter { $0.count > 0 }.enumerated().sorted { ($0.element.count, -$0.offset) > ($1.element.count, -$1.offset) }.map(\.element)
        func n(_ tone: String) -> Int { day.filter { $0.tone == tone }.count }
        return .init(total: day.count, themes: t, tone: .init(negative: n("negative"), positive: n("positive"), neutral: n("neutral")))
    }

    /// Every feed read in parallel (a dead one is listed in `sources`), then aggregated.
    static func report(_ assets: [Asset], fetcher: MarketFetcher, now: Double = nowMs()) async -> NewsReport {
        let list = feeds + assetFeeds(assets)
        let results: [(Feed, [Raw], String?)] = await withTaskGroup(of: (Int, Feed, [Raw], String?).self) { g in
            for (i, f) in list.enumerated() {
                g.addTask {
                    do {
                        var text = try await fetcher.text(f.url)
                        if text.hasPrefix("\u{FEFF}") { text.removeFirst() }
                        guard text.utf8.count <= 5_000_000 else { return (i, f, [], "flux trop gros") }
                        let items = parse(text, fallbackSource: f.name)
                        return (i, f, items, items.isEmpty ? "flux vide" : nil)
                    } catch {
                        return (i, f, [], String(((error as? MarketError)?.message ?? error.localizedDescription).prefix(80)))
                    }
                }
            }
            var out: [(Int, Feed, [Raw], String?)] = []
            for await r in g { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map { ($0.1, $0.2, $0.3) }
        }
        var sources: [NewsReport.Source] = []
        for (f, items, error) in results {
            if let i = sources.firstIndex(where: { $0.name == f.name }) {
                let ok = sources[i].ok || error == nil
                sources[i] = .init(name: f.name, ok: ok, count: sources[i].count + items.count, error: ok ? nil : sources[i].error ?? error)
            } else {
                sources.append(.init(name: f.name, ok: error == nil, count: items.count, error: error))
            }
        }
        let watch = assets.map { WatchAsset(id: $0.id, symbol: $0.symbol, name: $0.name) }
        let items = aggregate(results.map { ($0.0, $0.1) }, assets: watch, now: now)
        return NewsReport(asOf: now, items: items, top: top(items, 5).map(\.id), digest: digest(items, now: now), sources: sources, summary: nil)
    }
}

/// News feeds read once and shared for `ttl`: the news tab, the guard of every asset and the macro context all read
/// the same general feeds; without sharing, checking the alerts of 10 assets downloaded them 10 times. Simultaneous
/// readers wait for the same download; a failed read is kept for 2 minutes only.
actor FeedCache {
    private var entries: [String: (at: Double, task: Task<Data, Error>, failed: Bool)] = [:]
    let ttl: Double
    /// A failed read (dead feed, source down) is remembered this long: the readers that follow do not try it again
    /// and again, yet it is retried soon.
    let failureTTL: Double
    init(ttl: Double = 300_000, failureTTL: Double = 120_000) { self.ttl = ttl; self.failureTTL = min(ttl, failureTTL) }

    func data(_ url: String, fetcher: MarketFetcher) async throws -> Data {
        let now = nowMs()
        entries = entries.filter { now - $0.value.at < ($0.value.failed ? failureTTL : ttl) }
        if let e = entries[url] { return try await e.task.value }
        let task = Task { try await fetcher.data(url, method: "GET", body: nil) }
        entries[url] = (now, task, false)
        do { return try await task.value } catch {
            if entries[url]?.task == task { entries[url] = (now, task, true) }
            throw error
        }
    }
}

/// A fetcher whose GET requests go through a `FeedCache`.
struct SharedFeedFetcher: MarketFetcher {
    let base: MarketFetcher
    let cache: FeedCache
    func data(_ url: String, method: String, body: Data?) async throws -> Data {
        guard method == "GET", body == nil else { return try await base.data(url, method: method, body: body) }
        return try await cache.data(url, fetcher: base)
    }
}
