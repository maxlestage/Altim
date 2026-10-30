import Foundation

/// Macro-economic and geopolitical context on the iPhone (core/src/engine/macro_ctx.rs, backend/src/macro_data.rs):
/// market stress from the VIX, the S&P 500, oil, gold, the dollar and US 10-year yields (Yahoo Finance daily closes),
/// plus the escalation headlines of the world news read by the iPhone. Shared by every asset.
enum MacroEngine {
    static let symbols: [(key: String, symbol: String)] = [("vix", "^VIX"), ("spx", "^GSPC"), ("oil", "CL=F"), ("gold", "GC=F"),
                                                          ("dollar", "DX-Y.NYB"), ("rates", "^TNX")]
    static let tense = 25.0, high = 50.0, window = 250

    struct Point { var time: Double; var close: Double }

    static func one(_ v: Double) -> String { JSFormat.fr(v, min: 0, max: 1) }
    static func signed(_ v: Double) -> String { "\(v >= 0 ? "+" : "−")\(one(abs(v))) %" }

    static func day(_ t: Double) -> Int { Int((t / 86_400_000).rounded(.down)) }

    /// Each series on the dates of the VIX (or the S&P 500), last known value.
    static func align(_ series: [String: [Point]]) -> (times: [Double], values: [String: [Double?]]) {
        let reference = (series["vix"]?.isEmpty == false ? series["vix"] : series["spx"]) ?? []
        let times = reference.map(\.time)
        var values: [String: [Double?]] = [:]
        for (k, _) in symbols {
            let byDay = Dictionary((series[k] ?? []).map { (day($0.time), $0.close) }, uniquingKeysWith: { _, b in b })
            var last: Double?
            values[k] = times.map { t in
                if let v = byDay[day(t)] { last = v }
                return last
            }
        }
        return (times, values)
    }

    /// z-score of the 5-day change at i against the previous `window` 5-day changes (no look-ahead), and the change.
    static func z5(_ v: [Double?], _ i: Int) -> (z: Double, change: Double)? {
        func ch(_ j: Int) -> Double? {
            guard j >= 5, j < v.count, let a = v[j], let b = v[j - 5], b > 0 else { return nil }
            return (a / b - 1) * 100
        }
        guard let now = ch(i) else { return nil }
        var past: [Double] = []
        var j = max(5, i - window)
        while j < i - 5 {
            if let c = ch(j) { past.append(c) }
            j += 1
        }
        guard past.count >= 60 else { return nil }
        let m = past.reduce(0, +) / Double(past.count)
        let sd = (past.reduce(0) { $0 + ($1 - m) * ($1 - m) } / Double(past.count)).squareRoot()
        return sd > 0 ? ((now - m) / sd, now) : nil
    }

    static func stress(_ values: [String: [Double?]], _ i: Int) -> (score: Double, factors: [MacroInfo.Factor]) {
        var f: [MacroInfo.Factor] = []
        let vix = values["vix"] ?? []
        if i < vix.count, let v = vix[i] {
            if v >= 30 { f.append(.init(code: "vixHigh", points: 25, text: "Peur généralisée : VIX à \(one(v)) (au-delà de 30).")) }
            else if v >= 25 { f.append(.init(code: "vixHigh", points: 15, text: "Nervosité élevée : VIX à \(one(v)).")) }
        }
        if let (z, c) = z5(vix, i), z >= 2 { f.append(.init(code: "vixJump", points: 15, text: "Le VIX a bondi de \(signed(c)) en 5 séances, inhabituel.")) }
        let spx = values["spx"] ?? []
        if i < spx.count, let now = spx[i] {
            let peak = spx[max(0, i - 20)...i].compactMap { $0 }.max() ?? 0
            let dd = peak > 0 ? (1 - now / peak) * 100 : 0
            if dd >= 7 { f.append(.init(code: "spxDrawdown", points: 20, text: "Le S&P 500 a perdu \(one(dd)) % depuis son plus haut du mois.")) }
            else if dd >= 4 { f.append(.init(code: "spxDrawdown", points: 10, text: "Le S&P 500 recule de \(one(dd)) % depuis son plus haut du mois.")) }
        }
        let unusual: [(String, Bool, Double, (Double) -> String)] = [
            ("oil", false, 15, { "Pétrole \(signed($0)) en 5 séances : choc d'offre possible (tensions géopolitiques)." }),
            ("gold", false, 10, { "Or \(signed($0)) en 5 séances : les investisseurs cherchent un refuge." }),
            ("dollar", false, 10, { "Dollar \(signed($0)) en 5 séances : fuite vers la sécurité." }),
            ("rates", true, 10, { "Taux américains à 10 ans \(signed($0)) en 5 séances : banques centrales / inflation sous tension." }),
        ]
        for (k, both, pts, text) in unusual {
            if let (z, c) = z5(values[k] ?? [], i), both ? abs(z) >= 2 : z >= 2 { f.append(.init(code: k, points: pts, text: text(c))) }
        }
        return (min(100, f.reduce(0) { $0 + $1.points }), f)
    }

    static func ascii(_ p: String) -> NSRegularExpression { NewsEngine.regex(p, [.caseInsensitive]) }

    static let themeDefs: [(String, String, NSRegularExpression)] = [
        ("geopolitics", "Géopolitique / guerre", ascii(#"\b(wars?|invasion|invades?|invaded|missiles?|air ?strikes?|drone strikes?|military|troops|nuclear|sanctions?|ceasefire|hostages?|coup|blockade)\b"#)),
        ("monetary", "Banques centrales / inflation", ascii(#"\b(fed|federal reserve|fomc|powell|ecb|rate (hikes?|cuts?)|interest rates?|inflation|cpi|treasury yields?)\b"#)),
        ("trade", "Commerce / droits de douane", ascii(#"\b(tariffs?|trade war|export (ban|controls?)|embargo)\b"#)),
        ("stress", "Crise financière", ascii(#"\b(recession|default(s|ed)?|bank (runs?|collapse|failures?)|financial crisis|market crash|sell-?off|bankruptcy|contagion)\b"#)),
    ]

    static let escalations: [(NSRegularExpression, String)] = [
        (ascii(#"\bdeclar(es|ed|ing) war\b"#), "déclaration de guerre"),
        (ascii(#"\b(invades?|invaded|invasion of)\b"#), "invasion"),
        (ascii(#"\bnuclear (strike|attack|threat|test)\b"#), "menace nucléaire"),
        (ascii(#"\b(missile|air) ?strikes? on\b"#), "frappes militaires"),
        (ascii(#"\b(closes?|closed|blockade of) (the )?strait\b"#), "blocage d'un détroit (pétrole)"),
        (ascii(#"\bmartial law|state of emergency\b"#), "état d'urgence"),
        (ascii(#"\bbank runs?|bank collapse\b"#), "panique bancaire"),
        (ascii(#"\bcircuit breaker|trading halted\b"#), "cotations suspendues"),
        (ascii(#"\bdefaults? on (its )?debt\b"#), "défaut de paiement d'un État"),
    ]
    /// A question ("Is a bank run coming?") is speculation, not an event.
    static let question = NewsEngine.regex(#"\?\s*(-\s*[^-]+)?$"#)

    static func headlines(_ items: [(title: String, time: Double)], now: Double) -> (themes: [MacroInfo.Theme], factors: [MacroInfo.Factor]) {
        let recent = items.filter { $0.time <= now && $0.time >= now - 86_400_000 }
        let themes = themeDefs.map { def -> MacroInfo.Theme in
            let hits = recent.filter { NewsEngine.matches(def.2, $0.title) }
            return .init(theme: def.0, label: def.1, count: hits.count, examples: hits.prefix(3).map(\.title))
        }.filter { $0.count > 0 }.enumerated().sorted { ($0.element.count, -$0.offset) > ($1.element.count, -$1.offset) }.map(\.element)
        let last12 = recent.filter { $0.time >= now - 12 * 3_600_000 && !NewsEngine.matches(question, $0.title) }
        let found = escalations.filter { e in last12.contains { NewsEngine.matches(e.0, $0.title) } }.map(\.1)
        let factors: [MacroInfo.Factor] = found.isEmpty ? [] : [.init(code: "escalation", points: min(30, 10 * Double(found.count)),
                                                                      text: "Actualité : \(found.joined(separator: ", ")) (dernières 12 h, non vérifiable sur l'historique).")]
        return (themes, factors)
    }

    static func report(_ series: [String: [Point]], news: [(title: String, time: Double)], now: Double) -> MacroInfo? {
        let a = align(series)
        guard !a.times.isEmpty else { return nil }
        let i = a.times.count - 1
        let market = stress(a.values, i)
        let h = headlines(news, now: now)
        let score = min(100, market.score + h.factors.reduce(0) { $0 + $1.points })
        var values: [String: MacroInfo.Value] = [:]
        if i >= 5 {
            for (k, _) in symbols {
                if let now = a.values[k]?[i], let prev = a.values[k]?[i - 5], prev > 0 { values[k] = .init(value: now, change5d: (now / prev - 1) * 100) }
            }
        }
        return MacroInfo(score: score, level: score >= high ? "high" : score >= tense ? "tense" : "calm", marketScore: market.score,
                         factors: market.factors + h.factors, themes: h.themes, values: values, asOf: a.times[i], evidence: nil, regime: nil)
    }
}

extension LocalBackend {
    /// `/api/macro`: the six series (Yahoo Finance, 2 years of daily closes) and the world headlines of the news engine.
    func macroReport() async throws -> MacroInfo {
        if let c = macroCache, nowMs() - c.at < 600_000 { return c.info }
        let fetcher = self.fetcher
        async let news = news([])
        let series: [String: [MacroEngine.Point]] = await withTaskGroup(of: (String, [MacroEngine.Point]).self) { g in
            for (k, sym) in MacroEngine.symbols {
                g.addTask {
                    guard let d = try? await fetcher.json("https://query1.finance.yahoo.com/v8/finance/chart/\(Sources.enc(sym))?interval=1d&range=2y"),
                          let bars = try? Parse.yahoo(d) else { return (k, []) }
                    return (k, bars.map { .init(time: $0.time, close: $0.close) })
                }
            }
            var out: [String: [MacroEngine.Point]] = [:]
            for await (k, v) in g where !v.isEmpty { out[k] = v }
            return out
        }
        guard series["vix"] != nil || series["spx"] != nil else { throw MarketError("données macro indisponibles") }
        let world = (await news).items.filter { $0.category == "monde" || $0.category == "marches" }.map { (title: $0.title, time: $0.time) }
        guard let info = MacroEngine.report(series, news: world, now: nowMs()) else { throw MarketError("données macro indisponibles") }
        macroCache = (nowMs(), info)
        return info
    }
}
