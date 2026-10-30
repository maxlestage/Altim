import Foundation

/// Sector of each held stock, on the iPhone: a built-in table of the large US stocks and funds (their sector does not
/// change), then Yahoo Finance's search for the others (its `sector` field, or `ETF`). The same JSON as the server's
/// /api/sectors (classification "nasdaq" for a company sector, "etf" for a fund).
enum LocalSectors {
    static let funds: Set<String> = ["SPY", "QQQ", "VOO", "VTI", "IVV", "DIA", "IWM", "VT", "VEA", "VWO", "EFA", "AGG", "BND", "TLT", "GLD",
                                     "SLV", "XLK", "XLF", "XLE", "XLV", "XLI", "XLY", "XLP", "XLU", "XLB", "XLRE", "XLC", "ARKK", "SCHD",
                                     "VGT", "SMH", "SOXX", "IBIT", "FBTC", "ETHA", "VNQ", "HYG", "LQD", "EEM", "TQQQ", "SQQQ"]

    /// English (Nasdaq screener) sector of large US stocks.
    static let table: [String: String] = {
        var t: [String: String] = [:]
        let groups: [(String, [String])] = [
            ("Technology", ["AAPL", "MSFT", "NVDA", "AVGO", "ORCL", "CRM", "ADBE", "AMD", "INTC", "CSCO", "IBM", "QCOM", "TXN", "AMAT",
                            "MU", "NOW", "INTU", "PLTR", "SNOW", "SHOP", "UBER", "PANW", "CRWD", "ANET", "ADI", "LRCX", "KLAC", "MRVL",
                            "ARM", "TSM", "ASML", "SAP", "GOOGL", "GOOG", "META", "DELL", "HPQ", "SMCI", "NET", "DDOG", "MSTR", "COIN"]),
            ("Consumer Discretionary", ["AMZN", "TSLA", "HD", "MCD", "NKE", "SBUX", "LOW", "BKNG", "TJX", "ABNB", "GM", "F", "RIVN",
                                        "LULU", "CMG", "MAR", "NFLX", "DIS", "RBLX", "EBAY", "ETSY", "TGT", "BABA", "PDD", "JD"]),
            ("Consumer Staples", ["WMT", "COST", "PG", "KO", "PEP", "PM", "MO", "MDLZ", "CL", "KHC", "GIS", "KR", "EL", "STZ", "KDP"]),
            ("Finance", ["JPM", "BAC", "WFC", "C", "GS", "MS", "BLK", "SCHW", "AXP", "V", "MA", "PYPL", "SQ", "BRK.B", "BRK-B", "SPGI",
                         "CB", "PGR", "MMC", "ICE", "CME", "USB", "PNC", "COF", "HOOD", "SOFI", "AIG", "MET"]),
            ("Health Care", ["UNH", "JNJ", "LLY", "PFE", "MRK", "ABBV", "TMO", "ABT", "DHR", "BMY", "AMGN", "GILD", "CVS", "MDT", "ISRG",
                             "VRTX", "REGN", "ZTS", "SYK", "BSX", "CI", "ELV", "HCA", "MRNA", "NVO", "AZN"]),
            ("Energy", ["XOM", "CVX", "COP", "SLB", "EOG", "OXY", "PSX", "MPC", "VLO", "KMI", "WMB", "HAL", "BP", "SHEL", "TTE"]),
            ("Industrials", ["CAT", "BA", "GE", "HON", "UPS", "RTX", "LMT", "DE", "UNP", "MMM", "ADP", "GD", "NOC", "FDX", "CSX", "ETN",
                             "EMR", "WM", "ITW", "PH", "DAL", "UAL", "LUV"]),
            ("Telecommunications", ["T", "VZ", "TMUS", "CMCSA", "CHTR"]),
            ("Utilities", ["NEE", "DUK", "SO", "D", "AEP", "EXC", "SRE", "XEL", "CEG", "VST"]),
            ("Real Estate", ["PLD", "AMT", "EQIX", "CCI", "SPG", "O", "PSA", "WELL", "DLR"]),
            ("Basic Materials", ["LIN", "SHW", "APD", "FCX", "NEM", "ECL", "DOW", "DD", "NUE", "VALE", "RIO", "BHP"]),
        ]
        for (sector, symbols) in groups { for s in symbols { t[s] = sector } }
        return t
    }()

    /// Yahoo's sector names → the screener's.
    static let yahooToScreener = ["Technology": "Technology", "Communication Services": "Telecommunications", "Consumer Cyclical": "Consumer Discretionary",
                                  "Consumer Defensive": "Consumer Staples", "Financial Services": "Finance", "Healthcare": "Health Care",
                                  "Industrials": "Industrials", "Energy": "Energy", "Utilities": "Utilities", "Real Estate": "Real Estate",
                                  "Basic Materials": "Basic Materials"]

    static let french = ["Technology": "Technologie", "Finance": "Finance", "Health Care": "Santé", "Consumer Discretionary": "Consommation",
                         "Industrials": "Industrie", "Energy": "Énergie", "Telecommunications": "Télécoms", "Consumer Staples": "Biens de base",
                         "Basic Materials": "Matériaux", "Real Estate": "Immobilier", "Utilities": "Services publics"]

    static let etfLabel = "ETF / fonds indiciel (plusieurs secteurs)"

    static func item(_ symbol: String, sector: String?, etf: Bool, source: String) -> [String: Any] {
        var d: [String: Any] = ["symbol": symbol, "etf": etf]
        if etf {
            d["sector"] = etfLabel
            d["classification"] = "etf"
            d["source"] = source
        } else if let sector {
            let fr = french[sector] ?? sector
            d["sector"] = fr
            d["classification"] = "nasdaq"
            d["source"] = source
            d["nasdaq"] = ["sector": sector, "sectorFr": fr, "industry": ""]
        } else {
            d["reason"] = "Secteur non couvert : absent de la table d'Altim et de Yahoo Finance."
        }
        return d
    }

    static func report(_ symbols: [String], fetcher: MarketFetcher) async -> Data {
        var unique: [String] = []
        for s in symbols.map({ $0.trimmingCharacters(in: .whitespaces).uppercased() }) where !s.isEmpty && !unique.contains(s) { unique.append(s) }
        var yahooOK: Bool?
        var items: [[String: Any]] = []
        for s in unique.prefix(50) {
            if funds.contains(s) { items.append(item(s, sector: nil, etf: true, source: "Altim (liste des fonds connus)")); continue }
            if let sector = table[s] { items.append(item(s, sector: sector, etf: false, source: "Altim (table des grandes valeurs US)")); continue }
            guard let d = try? await fetcher.json("https://query1.finance.yahoo.com/v1/finance/search?q=\(Sources.enc(s))&quotesCount=3&newsCount=0") else {
                yahooOK = false
                items.append(item(s, sector: nil, etf: false, source: ""))
                continue
            }
            yahooOK = yahooOK ?? true
            let q = (arr(at(d, "quotes")) ?? []).compactMap { obj($0) }.first { ($0["symbol"] as? String)?.uppercased() == s }
            let etf = (q?["quoteType"] as? String) == "ETF"
            let sector = (q?["sector"] as? String).flatMap { yahooToScreener[$0] }
            items.append(item(s, sector: sector, etf: etf, source: "Yahoo Finance (secteur de la société)"))
        }
        var sources: [[String: Any]] = [["name": "Altim (table intégrée)", "ok": true]]
        if let ok = yahooOK { sources.append(["name": "Yahoo Finance", "ok": ok] as [String: Any]) }
        let body: [String: Any] = ["asOf": nowMs(), "items": items, "sources": sources]
        return (try? JSONSerialization.data(withJSONObject: body)) ?? Data("{}".utf8)
    }
}
