import Foundation
import XCTest
@testable import AltimKit

/// Strategy lab: long daily histories of a basket of real assets, saved once (`ALTIM_DUMP=<dir>`), then every rule
/// is measured offline on them (`ALTIM_LAB=<dir>`). Skipped by default.
final class StrategyLabTests: XCTestCase {
    static let basket: [(String, Kind)] = [
        ("BTC", .crypto), ("ETH", .crypto), ("SOL", .crypto), ("XRP", .crypto), ("BNB", .crypto), ("ADA", .crypto), ("DOGE", .crypto),
        ("LINK", .crypto), ("AVAX", .crypto), ("LTC", .crypto), ("DOT", .crypto), ("ATOM", .crypto),
        ("AAPL", .stock), ("MSFT", .stock), ("NVDA", .stock), ("AMZN", .stock), ("GOOGL", .stock), ("META", .stock), ("TSLA", .stock),
        ("JPM", .stock), ("KO", .stock), ("XOM", .stock), ("SPY", .stock), ("QQQ", .stock), ("INTC", .stock), ("PFE", .stock),
    ]

    func testDumpDataset() async throws {
        guard let dir = ProcessInfo.processInfo.environment["ALTIM_DUMP"] else { throw XCTSkip("ALTIM_DUMP absent") }
        let backend = LocalBackend()
        for (s, k) in Self.basket {
            let a = Asset(symbol: s, kind: k, name: s)
            do {
                let bars = try await backend.longDaily(a)
                let rows = bars.map { [$0.time, $0.open, $0.high, $0.low, $0.close, $0.volume] }
                try JSONSerialization.data(withJSONObject: rows).write(to: URL(fileURLWithPath: "\(dir)/\(k.rawValue)-\(s).json"))
                print("\(s): \(bars.count) jours")
            } catch { print("\(s): \(error)") }
        }
    }

    static func load(_ dir: String) -> [(Asset, [Bar])] {
        basket.compactMap { s, k in
            guard let d = try? Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(k.rawValue)-\(s).json")),
                  let rows = try? JSONSerialization.jsonObject(with: d) as? [[Double]] else { return nil }
            return (Asset(symbol: s, kind: k, name: s), rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
    }

    func testCurrentSignal() throws {
        guard let dir = ProcessInfo.processInfo.environment["ALTIM_LAB"] else { throw XCTSkip("ALTIM_LAB absent") }
        var rows: [String] = []
        for (a, bars) in Self.load(dir) {
            let r = Backtest.run(bars, asset: a)
            rows.append("\(a.symbol)\t\(r.days)j\tsignal \(Int(r.strategyReturn))% dd \(Int(r.strategyDrawdown))%\tgarder \(Int(r.holdReturn))% dd \(Int(r.holdDrawdown))%\ttrades \(r.trades.count) gain \(Int(r.winRate ?? 0))% expo \(Int(r.exposure))%")
        }
        print(rows.joined(separator: "\n"))
    }
}

extension StrategyLabTests {
    static func v(_ x: Double?) -> Double { x ?? .nan }
    /// 60 days for a market open every day (cryptos), 90 for stocks (≈ 63 sessions… measured in candles: 60 / 62).
    static func lb(_ s: LabSeries) -> Int { s.c.count > 950 ? 60 : 62 }
    /// Exit threshold of the trend bot measured until the market filter came (testMarketAndVeto dropped it).
    static let oldExitBelow = -0.03

    static var rules: [LabRule] {
        [
            LabRule(name: "toujours investi", enter: { _, _ in true }, exit: { _, _ in false }),
            LabRule(name: "signal actuel", enter: { s, i in SignalEngine.analyze(Array(s.c[max(0, i - 299)...i]), now: s.c[i].time)?.action.isBuy ?? false },
                    exit: { s, i in SignalEngine.analyze(Array(s.c[max(0, i - 299)...i]), now: s.c[i].time)?.action.isSell ?? false }),
            LabRule(name: "prix>EMA200", enter: { s, i in s.close[i] > v(s.ema200[i]) }, exit: { s, i in s.close[i] < v(s.ema200[i]) }),
            LabRule(name: "prix>EMA100", enter: { s, i in s.close[i] > v(s.ema100[i]) }, exit: { s, i in s.close[i] < v(s.ema100[i]) }),
            LabRule(name: "EMA50>EMA200", enter: { s, i in v(s.ema50[i]) > v(s.ema200[i]) }, exit: { s, i in v(s.ema50[i]) < v(s.ema200[i]) }),
            LabRule(name: "EMA20>EMA50", enter: { s, i in v(s.ema20[i]) > v(s.ema50[i]) }, exit: { s, i in v(s.ema20[i]) < v(s.ema50[i]) }),
            LabRule(name: "EMA20>50 & >200", enter: { s, i in v(s.ema20[i]) > v(s.ema50[i]) && s.close[i] > v(s.ema200[i]) },
                    exit: { s, i in v(s.ema20[i]) < v(s.ema50[i]) }),
            LabRule(name: "Donchian 55/20", enter: { s, i in i > 56 && s.close[i] > s.highest(i - 1, 55) }, exit: { s, i in s.close[i] < s.lowest(i - 1, 20) }),
            LabRule(name: "Donchian 20/10", enter: { s, i in i > 21 && s.close[i] > s.highest(i - 1, 20) }, exit: { s, i in s.close[i] < s.lowest(i - 1, 10) }),
            LabRule(name: "momentum 90j>0", enter: { s, i in (s.ret(i, 90) ?? -1) > 0 }, exit: { s, i in (s.ret(i, 90) ?? 0) < 0 }),
            LabRule(name: "EMA100 + trail3", enter: { s, i in s.close[i] > v(s.ema100[i]) && v(s.ema20[i]) > v(s.ema50[i]) },
                    exit: { s, i in s.close[i] < v(s.ema100[i]) }, trail: 3),
            LabRule(name: "EMA50 + trail4", enter: { s, i in s.close[i] > v(s.ema50[i]) && v(s.ema50[i]) > v(s.ema200[i]) },
                    exit: { s, i in s.close[i] < v(s.ema50[i]) * 0.97 }, trail: 4),
            LabRule(name: "repli RSI2<10 tendance", enter: { s, i in s.close[i] > v(s.ema200[i]) && v(s.rsi2[i]) < 10 },
                    exit: { s, i in v(s.rsi2[i]) > 70 || s.close[i] < v(s.ema200[i]) }),
            LabRule(name: "H1 repli→tendance", enter: { s, i in s.close[i] > v(s.ema200[i]) && v(s.rsi2[i]) < 15 },
                    exit: { s, i in s.close[i] < v(s.ema200[i]) * 0.97 }),
            LabRule(name: "H2 EMA200 ±3%", enter: { s, i in s.close[i] > v(s.ema200[i]) * 1.03 }, exit: { s, i in s.close[i] < v(s.ema200[i]) * 0.97 }),
            LabRule(name: "H3 EMA100 ±3%", enter: { s, i in s.close[i] > v(s.ema100[i]) * 1.03 }, exit: { s, i in s.close[i] < v(s.ema100[i]) * 0.97 }),
            LabRule(name: "H4 repli→EMA100", enter: { s, i in s.close[i] > v(s.ema200[i]) && v(s.rsi2[i]) < 15 },
                    exit: { s, i in s.close[i] < v(s.ema100[i]) * 0.97 }),
            LabRule(name: "H5 repli→trail5", enter: { s, i in s.close[i] > v(s.ema200[i]) && v(s.rsi2[i]) < 15 },
                    exit: { s, i in s.close[i] < v(s.ema200[i]) }, trail: 5),
            LabRule(name: "H6 tend.|repli", enter: { s, i in s.close[i] > v(s.ema200[i]) && (v(s.rsi2[i]) < 15 || v(s.ema20[i]) > v(s.ema50[i]) && v(s.ema50[i]) > v(s.ema200[i])) },
                    exit: { s, i in s.close[i] < v(s.ema200[i]) * 0.97 }),
            LabRule(name: "H7 50>200 sortie 100", enter: { s, i in v(s.ema50[i]) > v(s.ema200[i]) && s.close[i] > v(s.ema50[i]) },
                    exit: { s, i in s.close[i] < v(s.ema100[i]) * 0.97 }),
            LabRule(name: "M1 >EMA200 & mom", enter: { s, i in s.close[i] > v(s.ema200[i]) && (s.ret(i, Self.lb(s)) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < v(s.ema200[i]) || (s.ret(i, Self.lb(s)) ?? 0) < 0 }),
            LabRule(name: "M2 >EMA200 & mom ±", enter: { s, i in s.close[i] > v(s.ema200[i]) && (s.ret(i, Self.lb(s)) ?? -1) > 0.05 },
                    exit: { s, i in s.close[i] < v(s.ema200[i]) * 0.97 || (s.ret(i, Self.lb(s)) ?? 0) < -0.05 }),
            LabRule(name: "M3 mom & EMA20>50", enter: { s, i in s.close[i] > v(s.ema200[i]) && (s.ret(i, Self.lb(s)) ?? -1) > 0 && v(s.ema20[i]) > v(s.ema50[i]) },
                    exit: { s, i in s.close[i] < v(s.ema200[i]) || v(s.ema20[i]) < v(s.ema50[i]) && (s.ret(i, Self.lb(s)) ?? 0) < 0 }),
            LabRule(name: "tendance+repli EMA20", enter: { s, i in v(s.ema50[i]) > v(s.ema200[i]) && s.c[i].low <= v(s.ema20[i]) && s.close[i] > v(s.ema50[i]) },
                    exit: { s, i in s.close[i] < v(s.ema50[i]) * 0.97 }, trail: 4),
        ]
    }

    func testRules() throws {
        guard let dir = ProcessInfo.processInfo.environment["ALTIM_LAB"] else { throw XCTSkip("ALTIM_LAB absent") }
        let data = Self.load(dir).filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        let names = ProcessInfo.processInfo.environment["ALTIM_RULES"].map { Set($0.split(separator: ",").map(String.init)) }
        let chosen = Self.rules.filter { names?.contains($0.name) ?? ($0.name != "signal actuel") }
        print(Lab.report(data, chosen))
        print(Lab.full(data, chosen))
    }
}

extension StrategyLabTests {
    func testPortfolios() throws {
        guard let dir = ProcessInfo.processInfo.environment["ALTIM_LAB"] else { throw XCTSkip("ALTIM_LAB absent") }
        let all = Self.load(dir).filter { $0.1.count > 600 }
        for kind in [Kind.crypto, .stock] {
            let data = all.filter { $0.0.kind == kind }.map { LabSeries($0.1) }
            let times = data.flatMap { $0.c.map(\.time) }.sorted()
            let start = times[0] + 200 * 86_400_000 * (kind == .crypto ? 1 : 1.45), end = times.last! + 1
            let mid = (start + end) / 2
            print("== \(kind.rawValue)")
            for (top, lb, every, trend) in [(3, 45, 7, true), (3, 60, 7, true), (3, 75, 7, true), (3, 90, 7, true), (3, 120, 7, true), (2, 60, 7, true), (4, 60, 7, true), (3, 60, 14, true), (3, 90, 14, true), (3, 60, 21, true), (3, 120, 21, true), (4, 90, 7, true), (3, 60, 7, false)] {
                var line = "top\(top) \(lb)j /\(every)j \(trend ? "tendance" : "brut    ")"
                for (a, b) in [(start, mid), (mid, end), (start, end)] {
                    let r = PortfolioLab.run(data, top: top, lookback: lb, every: every, trend: trend, from: a, to: b)
                    line += " | \(Int(r.ret))% (panier \(Int(r.bench))%) creux \(Int(r.dd))/\(Int(r.benchDD)) expo \(Int(r.expo))"
                }
                print(line)
            }
        }
    }
}

extension StrategyLabTests {
    /// The app's engines on the lab's data: per-asset backtest and the rotation of each market.
    func testAppEngines() throws {
        guard let dir = ProcessInfo.processInfo.environment["ALTIM_LAB"] else { throw XCTSkip("ALTIM_LAB absent") }
        let data = Self.load(dir)
        var beatRisk = 0, n = 0
        for (a, bars) in data where bars.count > 600 {
            let r = TrendEngine.backtest(bars, asset: a)
            let old = Backtest.run(bars, asset: a)
            n += 1
            if (1 + r.strategyReturn / 100) / (0.05 - r.strategyDrawdown / 100) > (1 + r.holdReturn / 100) / (0.05 - r.holdDrawdown / 100) { beatRisk += 1 }
            print("\(a.symbol)\tnouveau \(Int(r.strategyReturn))% (creux \(Int(r.strategyDrawdown))%)\tancien \(Int(old.strategyReturn))% (\(Int(old.strategyDrawdown))%)\tgarder \(Int(r.holdReturn))% (\(Int(r.holdDrawdown))%)\ttrades \(r.trades.count) expo \(Int(r.exposure))%")
        }
        print("rendement/risque meilleur que garder : \(beatRisk)/\(n)")
        for kind in [Kind.crypto, .stock] {
            let t = Rotation.test(data.filter { $0.0.kind == kind && $0.1.count > 600 }, kind: kind)!
            print("rotation \(kind.rawValue): \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))/\(Int(t.basketDrawdown)) — moitiés \(Int(t.firstHalf))/\(Int(t.firstHalfBasket)) et \(Int(t.secondHalf))/\(Int(t.secondHalfBasket))")
        }
    }
}

extension StrategyLabTests {
    /// The app's universe saved (`ALTIM_UDUMP`), then the rotation variants measured on it (`ALTIM_ULAB`).
    func testUniverseDumpAndVariants() async throws {
        let env = ProcessInfo.processInfo.environment
        if let dir = env["ALTIM_UDUMP"] {
            let backend = LocalBackend()
            for a in (Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? []) {
                guard let bars = try? await backend.longDaily(a) else { print("\(a.symbol) absent"); continue }
                try JSONSerialization.data(withJSONObject: bars.map { [$0.time, $0.open, $0.high, $0.low, $0.close, $0.volume] })
                    .write(to: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json"))
            }
        }
        guard let dir = env["ALTIM_ULAB"] else { throw XCTSkip("ALTIM_ULAB absent") }
        for kind in [Kind.crypto, .stock] {
            let data: [(Asset, [Bar])] = (Rotation.universe[kind] ?? []).compactMap { a in
                guard let d = try? Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(kind.rawValue)-\(a.symbol).json")),
                      let rows = try? JSONSerialization.jsonObject(with: d) as? [[Double]] else { return nil }
                return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
            }
            let long = data.filter { $0.1.count > 600 }
            for top in [3, 100] {
                let t = Rotation.test(long, kind: kind, top: top)!
                print("  sans historiques courts \(kind.rawValue) top \(top): \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))/\(Int(t.basketDrawdown)) du \(Format.date(t.from))")
            }
            for top in [3, 5, 100] {
                let t = Rotation.test(data, kind: kind, top: top)!
                print("  période du \(Format.date(t.from)) au \(Format.date(t.to))")
                print("\(kind.rawValue) top \(top == 100 ? "tous" : "\(top)"): \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))/\(Int(t.basketDrawdown)) moitiés \(Int(t.firstHalf))/\(Int(t.firstHalfBasket)) \(Int(t.secondHalf))/\(Int(t.secondHalfBasket))")
            }
        }
        // The same on the lab basket.
        if let lab = env["ALTIM_LAB"] {
            let data = Self.load(lab).filter { $0.1.count > 600 }
            for kind in [Kind.crypto, .stock] {
                for top in [3, 5, 100] {
                    let t = Rotation.test(data.filter { $0.0.kind == kind }, kind: kind, top: top)!
                    print("labo \(kind.rawValue) top \(top == 100 ? "tous" : "\(top)"): \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))/\(Int(t.basketDrawdown)) moitiés \(Int(t.firstHalf))/\(Int(t.firstHalfBasket)) \(Int(t.secondHalf))/\(Int(t.secondHalfBasket))")
                }
            }
        }
    }
}

extension StrategyLabTests {
    func testUniverseAudit() throws {
        guard let dir = ProcessInfo.processInfo.environment["ALTIM_ULAB"] else { throw XCTSkip("ALTIM_ULAB absent") }
        for a in (Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? []) {
            guard let d = try? Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")),
                  let rows = try? JSONSerialization.jsonObject(with: d) as? [[Double]] else { print("\(a.symbol): absent"); continue }
            let bars = rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) }
            let c = Indicators.sanitize(bars)
            let jumps = c.indices.dropFirst().map { abs(log(c[$0].close / c[$0 - 1].close)) }
            let worst = jumps.enumerated().max { $0.element < $1.element }
            let r = TrendEngine.backtest(bars, asset: a)
            let f = Format.date(c.first?.time ?? 0), l = Format.date(c.last?.time ?? 0)
            print("\(a.symbol)\t\(c.count)j \(f)→\(l)\tsaut max \(Int((exp(worst?.element ?? 0) - 1) * 100))% le \(Format.date(c[(worst?.offset ?? 0) + 1].time))\trègle \(Int(r.strategyReturn))% (\(Int(r.strategyDrawdown))) garder \(Int(r.holdReturn))% (\(Int(r.holdDrawdown)))")
        }
    }
}

extension StrategyLabTests {
    func testSameAssetsBothDumps() throws {
        let env = ProcessInfo.processInfo.environment
        guard let u = env["ALTIM_ULAB"], let l = env["ALTIM_LAB"] else { throw XCTSkip("dossiers absents") }
        let syms = ["BTC", "ETH", "SOL", "XRP", "ADA", "DOGE", "LINK", "AVAX", "LTC", "DOT", "ATOM"]
        func read(_ dir: String, _ s: String) -> [Bar] {
            let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/crypto-\(s).json")))) as? [[Double]] ?? []
            return rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) }
        }
        for dir in [l, u] {
            let data = syms.map { (Asset(symbol: $0, kind: .crypto, name: $0), read(dir, $0)) }
            let t = Rotation.test(data, kind: .crypto, top: 100)!
            print("\(dir == l ? "labo" : "univ"): \(Int(t.ret))% (panier \(Int(t.basket))%) du \(Format.date(t.from)) — BTC dernier \(data[0].1.last?.close ?? 0) n=\(data[0].1.count) premier \(Format.date(data[0].1.first?.time ?? 0))")
        }
        for extra in ["XLM", "BCH"] {
            var data = syms.map { (Asset(symbol: $0, kind: .crypto, name: $0), read(u, $0)) }
            data.append((Asset(symbol: extra, kind: .crypto, name: extra), read(u, extra)))
            let t = Rotation.test(data, kind: .crypto, top: 100)!
            print("+\(extra): \(Int(t.ret))% (panier \(Int(t.basket))%)")
        }
    }
}

extension StrategyLabTests {
    func testReviewFrequency() throws {
        let env = ProcessInfo.processInfo.environment
        guard let u = env["ALTIM_ULAB"], let l = env["ALTIM_LAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let sets: [(String, Kind, [(Asset, [Bar])])] = [
            ("univ", .crypto, (Rotation.universe[.crypto] ?? []).compactMap { read(u, $0) }),
            ("univ", .stock, (Rotation.universe[.stock] ?? []).compactMap { read(u, $0) }),
            ("labo", .crypto, Self.load(l).filter { $0.0.kind == .crypto && $0.1.count > 600 }),
            ("labo", .stock, Self.load(l).filter { $0.0.kind == .stock && $0.1.count > 600 }),
        ]
        for (name, kind, data) in sets {
            if let t = Rotation.sleeves(data, kind: kind) {
                print("\(name) \(kind.rawValue) PARTS FIXES: \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))/\(Int(t.basketDrawdown)) moitiés \(Int(t.firstHalf))/\(Int(t.firstHalfBasket)) \(Int(t.secondHalf))/\(Int(t.secondHalfBasket)) du \(Format.date(t.from))")
            }
            for every in [1, 7] where env["ALTIM_ALL"] != nil {
                for top in [3, 5, 100] {
                    let t = Rotation.test(data, kind: kind, top: top, every: every)!
                    print("\(name) \(kind.rawValue) revue \(every)j top \(top == 100 ? "tous" : "\(top)"): \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))/\(Int(t.basketDrawdown)) moitiés \(Int(t.firstHalf))/\(Int(t.firstHalfBasket)) \(Int(t.secondHalf))/\(Int(t.secondHalfBasket))")
                }
            }
        }
    }
}

extension StrategyLabTests {
    /// Variants of the per-asset verdict on both lists (lab and the app's universe), cut in two halves.
    func testVerdictVariants() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        let n = { (s: LabSeries) in Self.lb(s) }
        let rules = [
            LabRule(name: "actuel", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s)) ?? 0) < 0 }),
            LabRule(name: "tampon 2%", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) * 1.02 && (s.ret(i, n(s)) ?? -1) > 0.02 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) * 0.98 || (s.ret(i, n(s)) ?? 0) < -0.02 }),
            LabRule(name: "sortie tendance seule", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) }),
            LabRule(name: "sortie 2 conditions", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? 0) < 0 }),
            LabRule(name: "confirmation 3 j", enter: { s, i in i > 3 && (0..<3).allSatisfy { k in s.close[i - k] > Self.v(s.ema200[i - k]) && (s.ret(i - k, n(s)) ?? -1) > 0 } },
                    exit: { s, i in i > 3 && (0..<3).allSatisfy { k in s.close[i - k] < Self.v(s.ema200[i - k]) || (s.ret(i - k, n(s)) ?? 0) < 0 } }),
            LabRule(name: "momentum court", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s) / 2) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s) / 2) ?? 0) < 0 }),
            LabRule(name: "momentum long", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s) * 2) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s) * 2) ?? 0) < 0 }),
            LabRule(name: "EMA 100", enter: { s, i in s.close[i] > Self.v(s.ema100[i]) && (s.ret(i, n(s)) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema100[i]) || (s.ret(i, n(s)) ?? 0) < 0 }),
            LabRule(name: "stop suiveur 3 ATR", enter: { s, i in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 },
                    exit: { s, i in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s)) ?? 0) < 0 }, trail: 3),
        ]
        print("== labo"); print(Lab.report(lab, rules)); print(Lab.full(lab, rules))
        print("== univers"); print(Lab.report(uni, rules)); print(Lab.full(uni, rules))
    }
}

extension StrategyLabTests {
    /// Does « ACHAT FORT » (a short pull-back, RSI 2 < 10, inside the up trend) lead to better results than a plain
    /// « ACHAT » day? Average return 5 and 20 days later, bought at the next open, per kind, half and list.
    func testPullbackEdge() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            for kind in [Kind.crypto, .stock] {
                for half in 0..<2 {
                    var plain: [[Double]] = [[], []], pull: [[Double]] = [[], []]
                    for (a, bars) in list where a.kind == kind {
                        let s = LabSeries(bars), n = Self.lb(s), mid = s.c.count / 2
                        let range = half == 0 ? 200..<mid : mid..<(s.c.count - 21)
                        for i in range where s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n) ?? -1) > 0 {
                            let entry = s.c[i + 1].open
                            let r = [s.close[i + 5] / entry - 1, s.close[i + 20] / entry - 1]
                            for k in 0..<2 { if (s.rsi2[i] ?? 50) < 10 { pull[k].append(r[k]) } else { plain[k].append(r[k]) } }
                        }
                    }
                    func avg(_ x: [Double]) -> String { x.isEmpty ? "–" : String(format: "%+.2f%%", 100 * x.reduce(0, +) / Double(x.count)) }
                    func win(_ x: [Double]) -> String { x.isEmpty ? "–" : String(format: "%.0f%%", 100 * Double(x.filter { $0 > 0 }.count) / Double(x.count)) }
                    print("\(name) \(kind.rawValue) M\(half + 1) | achat n=\(plain[0].count) 5j \(avg(plain[0])) 20j \(avg(plain[1])) (gagnant \(win(plain[1]))) | achat fort n=\(pull[0].count) 5j \(avg(pull[0])) 20j \(avg(pull[1])) (gagnant \(win(pull[1])))")
                }
            }
        }
    }
}

extension StrategyLabTests {
    /// The bot trained on the device (BotEngine, walk-forward ridge on 12 measures, pooled per market) against the
    /// current rule, on both lists and both halves.
    func testTrainedBot() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        func key(_ s: LabSeries) -> String { "\(s.c.count)-\(s.close[0])-\(s.close[s.close.count - 1])" }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            var preds: [String: [Double?]] = [:]
            for kind in [Kind.crypto, .stock] {
                let group = list.filter { $0.0.kind == kind }
                let p = env["ALTIM_TREES"] != nil ? BotEngine.walkForwardTrees(group.map(\.1)) : BotEngine.walkForward(group.map(\.1))
                for (k, (_, bars)) in group.enumerated() { preds[key(LabSeries(bars))] = p[k] }
            }
            let data = list.map { ($0.0, LabSeries($0.1)) }
            func p(_ s: LabSeries, _ i: Int) -> Double? { preds[key(s)]?[i] ?? nil }
            let n = { (s: LabSeries) in Self.lb(s) }
            let ruleIn = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
            let ruleOut = { (s: LabSeries, i: Int) in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s)) ?? 0) < 0 }
            let rules = [
                LabRule(name: "actuel", enter: ruleIn, exit: ruleOut),
                LabRule(name: "bot seul", enter: { s, i in (p(s, i) ?? -1) > 0 }, exit: { s, i in (p(s, i) ?? 0) < 0 }),
                LabRule(name: "bot seul (marge 1%)", enter: { s, i in (p(s, i) ?? -1) > 0.01 }, exit: { s, i in (p(s, i) ?? 0) < -0.01 }),
                LabRule(name: "règle + bot", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? -1) > 0 }, exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < 0 }),
                LabRule(name: "règle, veto bot", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > -0.01 }, exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < -0.03 }),
                LabRule(name: "veto 0 / -2%", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > 0 }, exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < -0.02 }),
                LabRule(name: "veto -2 / -4%", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > -0.02 }, exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < -0.04 }),
                LabRule(name: "veto -1 / -5%", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > -0.01 }, exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < -0.05 }),
                LabRule(name: "veto entrée seule -1%", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > -0.01 }, exit: ruleOut),
                LabRule(name: "veto sortie seule -3%", enter: ruleIn, exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < -0.03 }),
            ]
            print("== \(name)"); print(Lab.report(data, rules)); print(Lab.full(data, rules))
        }
    }
}

extension StrategyLabTests {
    /// Several trained bots, each with its own job, measured out of sample (walk-forward) on both lists and halves:
    /// how far apart the outcomes of their 20 % highest and 20 % lowest predictions were, then as a filter of the rule.
    func testBots() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        func key(_ s: LabSeries) -> String { "\(s.c.count)-\(s.close[0])-\(s.close[s.close.count - 1])" }
        let bots: [(String, BotEngine.Target)] = [("Tendance 20 j", .gain(days: 20)), ("Court terme 5 j", .gain(days: 5)), ("Risque 20 j", .fall(days: 20))]
        for (name, list) in [("labo", lab), ("univers", uni)] {
            var preds: [String: [String: [Double?]]] = [:]
            for (bot, target) in bots {
                for kind in [Kind.crypto, .stock] {
                    let group = list.filter { $0.0.kind == kind }
                    let p = BotEngine.walkForwardTrees(group.map(\.1), target: target)
                    for (k, (_, bars)) in group.enumerated() { preds[bot, default: [:]][key(LabSeries(bars))] = p[k] }
                }
            }
            let data = list.map { ($0.0, LabSeries($0.1)) }
            print("== \(name) : écart entre les 20 % de prévisions les plus hautes et les plus basses (réalisé)")
            for (bot, target) in bots {
                var line = bot.padding(toLength: 16, withPad: " ", startingAt: 0)
                for kind in [Kind.crypto, .stock] {
                    for half in 0..<2 {
                        var pairs: [(Double, Double)] = []
                        for (a, s) in data where a.kind == kind {
                            guard let p = preds[bot]?[key(s)] else { continue }
                            let bs = BotEngine.Series(s.c), mid = s.c.count / 2
                            for i in (half == 0 ? 200..<mid : mid..<s.c.count) {
                                if let x = p[i], let y = BotEngine.label(bs, i, target) { pairs.append((x, y)) }
                            }
                        }
                        pairs.sort { $0.0 < $1.0 }
                        let q = pairs.count / 5
                        guard q > 10 else { line += " | –"; continue }
                        func mean(_ v: ArraySlice<(Double, Double)>) -> Double { v.map(\.1).reduce(0, +) / Double(v.count) }
                        let lo = mean(pairs[..<q]), hi = mean(pairs[(pairs.count - q)...])
                        line += String(format: " | %@%d haut %+.1f%% bas %+.1f%%", kind == .crypto ? "C" : "A", half + 1, hi * 100, lo * 100)
                    }
                }
                print(line)
            }
            func p(_ bot: String, _ s: LabSeries, _ i: Int) -> Double? { preds[bot]?[key(s)]?[i] ?? nil }
            let n = { (s: LabSeries) in Self.lb(s) }
            let ruleIn = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
            let ruleOut = { (s: LabSeries, i: Int) in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s)) ?? 0) < 0 }
            let veto = { (s: LabSeries, i: Int) in (p("Tendance 20 j", s, i) ?? 0) > BotEngine.entryBelow }
            let vetoOut = { (s: LabSeries, i: Int) in (p("Tendance 20 j", s, i) ?? 0) < Self.oldExitBelow }
            let rules = [
                LabRule(name: "actuel", enter: ruleIn, exit: ruleOut),
                LabRule(name: "veto tendance", enter: { s, i in ruleIn(s, i) && veto(s, i) }, exit: { s, i in ruleOut(s, i) || vetoOut(s, i) }),
                LabRule(name: "+ court terme > 0", enter: { s, i in ruleIn(s, i) && veto(s, i) && (p("Court terme 5 j", s, i) ?? 1) > 0 },
                        exit: { s, i in ruleOut(s, i) || vetoOut(s, i) }),
                LabRule(name: "+ risque > −15 %", enter: { s, i in ruleIn(s, i) && veto(s, i) && (p("Risque 20 j", s, i) ?? 0) > -0.15 },
                        exit: { s, i in ruleOut(s, i) || vetoOut(s, i) }),
                LabRule(name: "+ sortie risque < −25 %", enter: { s, i in ruleIn(s, i) && veto(s, i) },
                        exit: { s, i in ruleOut(s, i) || vetoOut(s, i) || (p("Risque 20 j", s, i) ?? 0) < -0.25 }),
            ]
            print(Lab.report(data, rules)); print(Lab.full(data, rules))
        }
    }
}

extension StrategyLabTests {
    /// Placebo of the trend bot's veto: the same veto fired on random days, as often as the bot fires it. If chance did
    /// as well, the bot brings no information and the veto must go.
    func testVetoPlacebo() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        func key(_ s: LabSeries) -> String { "\(s.c.count)-\(s.close[0])-\(s.close[s.close.count - 1])" }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            var preds: [String: [Double?]] = [:]
            for kind in [Kind.crypto, .stock] {
                let group = list.filter { $0.0.kind == kind }
                let p = BotEngine.walkForwardTrees(group.map(\.1))
                for (k, (_, bars)) in group.enumerated() { preds[key(LabSeries(bars))] = p[k] }
            }
            let data = list.map { ($0.0, LabSeries($0.1)) }
            func p(_ s: LabSeries, _ i: Int) -> Double? { preds[key(s)]?[i] ?? nil }
            // How often the bot blocks an entry / forces an exit on in-trend days.
            var days = 0, noEntry = 0, exits = 0
            for (_, s) in data {
                for i in 200..<s.c.count where s.close[i] > Self.v(s.ema200[i]) {
                    guard let x = p(s, i) else { continue }
                    days += 1
                    if x <= BotEngine.entryBelow { noEntry += 1 }
                    if x < Self.oldExitBelow { exits += 1 }
                }
            }
            let fEntry = Double(noEntry) / Double(max(days, 1)), fExit = Double(exits) / Double(max(days, 1))
            print("== \(name) : veto d'entrée \(Int(fEntry * 100)) % des jours en tendance, sortie \(Int(fExit * 100)) %")
            let n = { (s: LabSeries) in Self.lb(s) }
            let ruleIn = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
            let ruleOut = { (s: LabSeries, i: Int) in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s)) ?? 0) < 0 }
            func noise(_ s: LabSeries, _ i: Int, _ seed: Int) -> Double {
                var h = UInt64(bitPattern: Int64(s.c[i].time)) &+ UInt64(seed) &* 0x9E3779B97F4A7C15 &+ UInt64(s.c.count)
                h ^= h >> 33; h &*= 0xff51afd7ed558ccd; h ^= h >> 33; h &*= 0xc4ceb9fe1a85ec53; h ^= h >> 33
                return Double(h % 1_000_000) / 1_000_000
            }
            var rules = [
                LabRule(name: "actuel", enter: ruleIn, exit: ruleOut),
                LabRule(name: "veto du bot", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > BotEngine.entryBelow },
                        exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < Self.oldExitBelow }),
            ]
            for seed in 1...5 {
                rules.append(LabRule(name: "veto au hasard \(seed)", enter: { s, i in ruleIn(s, i) && noise(s, i, seed) >= fEntry },
                                     exit: { s, i in ruleOut(s, i) || noise(s, i, seed + 100) < fExit }))
            }
            print(Lab.report(data, rules)); print(Lab.full(data, rules))
        }
    }
}

extension StrategyLabTests {
    /// Time to train the app's two bots on the app's universe (what the iPhone does every 12 hours).
    func testBotTrainingTime() throws {
        guard let u = ProcessInfo.processInfo.environment["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        for kind in [Kind.crypto, .stock] {
            let histories = (Rotation.universe[kind] ?? []).compactMap { a -> [Bar]? in
                guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(u)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
                return rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) }
            }
            for target in [BotEngine.Target.gain(days: 20), .fall(days: 20)] {
                let t0 = Date()
                let m = BotEngine.Model.train(histories, target: target)
                print("\(kind.rawValue) \(target) : \(histories.count) historiques, \(m?.examples ?? 0) exemples, \(String(format: "%.2f", Date().timeIntervalSince(t0))) s")
            }
        }
    }
}

extension StrategyLabTests {
    /// Position size from the risk bot (the app's advice: size so that the predicted worst fall costs ≈ 5 % of the
    /// planned amount) against a full position and against a size from plain volatility, on the rule + veto.
    /// Daily close-to-close, weight decided at the close and applied the next day, 0,1 % fee on each change.
    func testRiskSizing() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            var trend: [Int: [Double?]] = [:], risk: [Int: [Double?]] = [:]
            for kind in [Kind.crypto, .stock] {
                let idx = list.indices.filter { list[$0].0.kind == kind }
                let t = BotEngine.walkForwardTrees(idx.map { list[$0].1 })
                let r = BotEngine.walkForwardTrees(idx.map { list[$0].1 }, target: .fall(days: 20))
                for (k, i) in idx.enumerated() { trend[i] = t[k]; risk[i] = r[k] }
            }
            // Per sizing: per kind and half, median return and median worst fall, and the share of assets where the
            // return per unit of worst fall beat the full position.
            let sizings = ["pleine", "bot risque 5 %", "volatilité", "bot risque 10 %"]
            print("== \(name)")
            for kind in [Kind.crypto, .stock] {
                for half in 0..<2 {
                    var rets: [[Double]] = [[], [], [], []], dds: [[Double]] = [[], [], [], []], better = [0, 0, 0, 0], n = 0
                    for (i, (a, bars)) in list.enumerated() where a.kind == kind {
                        let s = LabSeries(bars), lb = Self.lb(s), len = s.c.count - 200
                        let from = 200 + half * len / 2, to = 200 + (half + 1) * len / 2
                        var res: [(Double, Double)] = []
                        for z in 0..<4 {
                            var eq = 1.0, peak = 1.0, dd = 0.0, w = 0.0, inPos = false
                            for d in from..<(to - 1) {
                                let up = s.close[d] > Self.v(s.ema200[d]) && (s.ret(d, lb) ?? -1) > 0
                                let p = trend[i]?[d] ?? nil
                                if !inPos, up, (p ?? 0) > BotEngine.entryBelow { inPos = true }
                                else if inPos, !up || (p ?? 0) < Self.oldExitBelow { inPos = false }
                                var target = 0.0
                                if inPos {
                                    switch z {
                                    case 1, 3: target = min(1, (z == 1 ? 0.05 : 0.10) / max(abs(BotEngine.calibratedFall(risk[i]?[d] ?? -0.05, kind: a.kind) ?? -0.05), 0.01))
                                    case 2:
                                        var sq = 0.0
                                        for k in (d - 19)...d { let r = s.close[k] / s.close[k - 1] - 1; sq += r * r }
                                        // Typical 20-day fall ≈ 1.5 × the 20-day volatility.
                                        target = min(1, 0.05 / max(1.5 * sqrt(sq / 20) * sqrt(20), 0.01))
                                    default: target = 1
                                    }
                                }
                                eq *= 1 - abs(target - w) * Lab.fee
                                w = target
                                eq *= 1 + w * (s.close[d + 1] / s.close[d] - 1)
                                peak = max(peak, eq); dd = min(dd, eq / peak - 1)
                            }
                            res.append(((eq - 1) * 100, dd * 100))
                        }
                        n += 1
                        for z in 0..<4 {
                            rets[z].append(res[z].0); dds[z].append(res[z].1)
                            let c = (1 + res[z].0 / 100) / (-res[z].1 / 100 + 0.05), c0 = (1 + res[0].0 / 100) / (-res[0].1 / 100 + 0.05)
                            if c > c0 { better[z] += 1 }
                        }
                    }
                    let parts = (0..<4).map { z in "\(sizings[z]) ret \(Int(Lab.median(rets[z])))% creux \(Int(Lab.median(dds[z])))% mieux/risque \(better[z])/\(n)" }
                    print("\(kind == .crypto ? "C" : "A")\(half + 1) | " + parts.joined(separator: " | "))
                }
            }
        }
    }
}

extension StrategyLabTests {
    /// The app trains its bots on its universe and applies them to any asset of the same market. Does the veto still
    /// help on assets the bot never learned from? Lab assets absent from the universe, bot trained on the universe only.
    func testVetoOutsideUniverse() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 400 }
        let inUniverse = Set(uni.map { $0.0.id })
        let outside = Self.load(l).filter { $0.1.count > 600 && !inUniverse.contains($0.0.id) }
        print("== hors univers : \(outside.map(\.0.symbol).joined(separator: ", "))")
        var preds: [String: [Double?]] = [:]
        func key(_ s: LabSeries) -> String { "\(s.c.count)-\(s.close[0])-\(s.close[s.close.count - 1])" }
        for kind in [Kind.crypto, .stock] {
            let targets = outside.filter { $0.0.kind == kind }
            guard !targets.isEmpty else { continue }
            let p = BotEngine.walkForwardTrees(train: uni.filter { $0.0.kind == kind }.map(\.1), predict: targets.map(\.1))
            for (k, (_, bars)) in targets.enumerated() { preds[key(LabSeries(bars))] = p[k] }
        }
        let data = outside.map { ($0.0, LabSeries($0.1)) }
        func p(_ s: LabSeries, _ i: Int) -> Double? { preds[key(s)]?[i] ?? nil }
        let n = { (s: LabSeries) in Self.lb(s) }
        let ruleIn = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
        let ruleOut = { (s: LabSeries, i: Int) in s.close[i] < Self.v(s.ema200[i]) || (s.ret(i, n(s)) ?? 0) < 0 }
        let rules = [
            LabRule(name: "actuel", enter: ruleIn, exit: ruleOut),
            LabRule(name: "veto (bot de l'univers)", enter: { s, i in ruleIn(s, i) && (p(s, i) ?? 0) > BotEngine.entryBelow },
                    exit: { s, i in ruleOut(s, i) || (p(s, i) ?? 0) < Self.oldExitBelow }),
        ]
        print(Lab.report(data, rules)); print(Lab.full(data, rules))

        // Leave-one-out on the universe: each asset predicted by a bot that never saw it.
        var loo: [String: [Double?]] = [:]
        for kind in [Kind.crypto, .stock] {
            let group = uni.filter { $0.0.kind == kind }
            for (k, (_, bars)) in group.enumerated() {
                let others = group.enumerated().filter { $0.offset != k }.map(\.element.1)
                loo[key(LabSeries(bars))] = BotEngine.walkForwardTrees(train: others, predict: [bars])[0]
            }
        }
        let uData = uni.filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        func q(_ s: LabSeries, _ i: Int) -> Double? { loo[key(s)]?[i] ?? nil }
        let looRules = [
            LabRule(name: "actuel", enter: ruleIn, exit: ruleOut),
            LabRule(name: "veto (actif jamais vu)", enter: { s, i in ruleIn(s, i) && (q(s, i) ?? 0) > BotEngine.entryBelow },
                    exit: { s, i in ruleOut(s, i) || (q(s, i) ?? 0) < Self.oldExitBelow }),
        ]
        print("== univers, chaque actif exclu de l'entraînement")
        print(Lab.report(uData, looRules)); print(Lab.full(uData, looRules))
    }
}

extension StrategyLabTests {
    /// Calibration of the risk bot: does a predicted worst fall of −10 % mean about −10 % in reality? Out of sample
    /// (walk-forward), by fifth of predictions, both lists. The app sizes positions on this number.
    func testRiskCalibration() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            for kind in [Kind.crypto, .stock] {
                let group = list.filter { $0.0.kind == kind }
                let preds = BotEngine.walkForwardTrees(group.map(\.1), target: .fall(days: 20))
                var pairs: [(Double, Double)] = []
                for (k, (_, bars)) in group.enumerated() {
                    let s = BotEngine.Series(bars)
                    for i in 0..<s.c.count { if let p = preds[k][i], let y = BotEngine.label(s, i, .fall(days: 20)) { pairs.append((p, y)) } }
                }
                pairs.sort { $0.0 < $1.0 }
                let q = pairs.count / 5
                var line = "\(name) \(kind.rawValue) n=\(pairs.count)"
                for f in 0..<5 {
                    let part = pairs[(f * q)..<(f == 4 ? pairs.count : (f + 1) * q)]
                    let pm = part.map(\.0).reduce(0, +) / Double(part.count), ym = part.map(\.1).reduce(0, +) / Double(part.count)
                    let worse = Double(part.filter { $0.1 < $0.0 }.count) / Double(part.count)
                    line += String(format: " | prévu %.1f%% réel %.1f%% (pire que prévu %.0f%%)", pm * 100, ym * 100, worse * 100)
                }
                print(line)
            }
        }
    }
}

extension StrategyLabTests {
    /// Market filter: an asset is bought only when its whole market is in an up trend too (Bitcoin for crypto, SPY for
    /// stocks, close above their EMA 200 on the same day). Lab and universe lists, both halves.
    func testMarketFilter() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        guard let btc = lab.first(where: { $0.0.symbol == "BTC" })?.1, let spy = lab.first(where: { $0.0.symbol == "SPY" })?.1 else { throw XCTSkip("BTC ou SPY absent") }
        func days(_ s: LabSeries, _ up: (LabSeries, Int) -> Bool) -> [Double: Bool] {
            var d: [Double: Bool] = [:]
            for i in s.c.indices { d[(s.c[i].time / 86_400_000).rounded(.down)] = up(s, i) }
            return d
        }
        let n = { (s: LabSeries) in Self.lb(s) }
        let trendUp = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) }
        let fullUp = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
        let mkTrend = [true: days(btc, trendUp), false: days(spy, trendUp)]
        let mkFull = [true: days(btc, fullUp), false: days(spy, fullUp)]
        func market(_ m: [Bool: [Double: Bool]], _ s: LabSeries, _ i: Int) -> Bool {
            let crypto = s.c.count > 950, day = (s.c[i].time / 86_400_000).rounded(.down)
            return m[crypto]?[day] ?? m[crypto]?[day - 1] ?? m[crypto]?[day - 2] ?? m[crypto]?[day - 3] ?? true
        }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            var hit = 0, all = 0
            for (_, s) in list { for i in 200..<s.c.count { all += 1; if mkTrend[s.c.count > 950]?[(s.c[i].time / 86_400_000).rounded(.down)] != nil { hit += 1 } } }
            print("jours du marché trouvés (\(name)) : \(hit) / \(all)")
        }
        let enter = { (s: LabSeries, i: Int) in fullUp(s, i) }
        let exit = { (s: LabSeries, i: Int) in !fullUp(s, i) }
        let rules = [
            LabRule(name: "actuel", enter: enter, exit: exit),
            LabRule(name: "marché > EMA200 entrée", enter: { s, i in enter(s, i) && market(mkTrend, s, i) }, exit: exit),
            LabRule(name: "marché > EMA200 e+s", enter: { s, i in enter(s, i) && market(mkTrend, s, i) }, exit: { s, i in exit(s, i) || !market(mkTrend, s, i) }),
            LabRule(name: "marché complet entrée", enter: { s, i in enter(s, i) && market(mkFull, s, i) }, exit: exit),
            LabRule(name: "marché complet e+s", enter: { s, i in enter(s, i) && market(mkFull, s, i) }, exit: { s, i in exit(s, i) || !market(mkFull, s, i) }),
        ]
        print("== labo"); print(Lab.report(lab, rules)); print(Lab.full(lab, rules))
        print("== univers"); print(Lab.report(uni, rules)); print(Lab.full(uni, rules))
    }
}

extension StrategyLabTests {
    /// The market filter together with the trend bot's veto (walk-forward trees): does each still add to the other?
    func testMarketAndVeto() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }
        guard let btc = lab.first(where: { $0.0.symbol == "BTC" }).map({ LabSeries($0.1) }),
              let spy = lab.first(where: { $0.0.symbol == "SPY" }).map({ LabSeries($0.1) }) else { throw XCTSkip("BTC ou SPY absent") }
        let n = { (s: LabSeries) in Self.lb(s) }
        let ruleIn = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
        let ruleOut = { (s: LabSeries, i: Int) in !ruleIn(s, i) }
        func days(_ s: LabSeries) -> [Double: Bool] {
            var d: [Double: Bool] = [:]
            for i in s.c.indices { d[(s.c[i].time / 86_400_000).rounded(.down)] = ruleIn(s, i) }
            return d
        }
        let mk = [true: days(btc), false: days(spy)]
        func market(_ s: LabSeries, _ i: Int) -> Bool {
            let m = mk[s.c.count > 950]!, day = (s.c[i].time / 86_400_000).rounded(.down)
            return m[day] ?? m[day - 1] ?? m[day - 2] ?? m[day - 3] ?? true
        }
        func key(_ s: LabSeries) -> String { "\(s.c.count)-\(s.close[0])-\(s.close[s.close.count - 1])" }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            var preds: [String: [Double?]] = [:]
            for kind in [Kind.crypto, .stock] {
                let group = list.filter { $0.0.kind == kind }
                let p = BotEngine.walkForwardTrees(group.map(\.1))
                for (k, (_, bars)) in group.enumerated() { preds[key(LabSeries(bars))] = p[k] }
            }
            let data = list.map { ($0.0, LabSeries($0.1)) }
            func p(_ s: LabSeries, _ i: Int) -> Double? { preds[key(s)]?[i] ?? nil }
            let vetoIn = { (s: LabSeries, i: Int) in (p(s, i) ?? 0) > BotEngine.entryBelow }
            let vetoOut = { (s: LabSeries, i: Int) in (p(s, i) ?? 0) < Self.oldExitBelow }
            let rules = [
                LabRule(name: "règle seule", enter: ruleIn, exit: ruleOut),
                LabRule(name: "veto bot", enter: { s, i in ruleIn(s, i) && vetoIn(s, i) }, exit: { s, i in ruleOut(s, i) || vetoOut(s, i) }),
                LabRule(name: "filtre marché", enter: { s, i in ruleIn(s, i) && market(s, i) }, exit: { s, i in ruleOut(s, i) || !market(s, i) }),
                LabRule(name: "filtre + veto", enter: { s, i in ruleIn(s, i) && market(s, i) && vetoIn(s, i) },
                        exit: { s, i in ruleOut(s, i) || !market(s, i) || vetoOut(s, i) }),
                LabRule(name: "filtre + veto entrée", enter: { s, i in ruleIn(s, i) && market(s, i) && vetoIn(s, i) },
                        exit: { s, i in ruleOut(s, i) || !market(s, i) }),
            ]
            print("== \(name)"); print(Lab.report(data, rules)); print(Lab.full(data, rules))
        }
    }
}

extension StrategyLabTests {
    /// The Selection's fixed shares (Rotation.sleeves) with and without the market filter, on both lists.
    func testSleevesMarketFilter() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l)
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }
        guard let btc = lab.first(where: { $0.0.symbol == "BTC" })?.1, let spy = lab.first(where: { $0.0.symbol == "SPY" })?.1 else { throw XCTSkip("BTC ou SPY absent") }
        for (name, list) in [("labo", lab), ("univers", uni)] {
            for kind in [Kind.crypto, .stock] {
                let d = list.filter { $0.0.kind == kind }
                let mk = kind == .crypto ? btc : spy
                for (label, t) in [("règle seule", Rotation.sleeves(d, kind: kind)), ("filtre marché", Rotation.sleeves(d, kind: kind, market: mk))] {
                    guard let t else { continue }
                    print("\(name) \(kind.rawValue) \(label.padding(toLength: 13, withPad: " ", startingAt: 0)) : total \(Int(t.ret))% (panier \(Int(t.basket))%) creux \(Int(t.drawdown))% (\(Int(t.basketDrawdown))%) | 1re moitié \(Int(t.firstHalf))% | 2de moitié \(Int(t.secondHalf))%")
                }
            }
        }
    }
}

extension StrategyLabTests {
    /// Is the market filter's gain an accident of its settings? The market's own trend read with neighbouring
    /// settings (EMA 100 / 200, momentum 30 / 60 / 120 days, or 3 / 6 months for SPY), entry and exit, both lists.
    func testMarketFilterSensitivity() throws {
        let env = ProcessInfo.processInfo.environment
        guard let l = env["ALTIM_LAB"], let u = env["ALTIM_ULAB"] else { throw XCTSkip("dossiers absents") }
        func read(_ dir: String, _ a: Asset) -> (Asset, [Bar])? {
            guard let rows = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: "\(dir)/\(a.kind.rawValue)-\(a.symbol).json")))) as? [[Double]] else { return nil }
            return (a, rows.map { Bar(time: $0[0], open: $0[1], high: $0[2], low: $0[3], close: $0[4], volume: $0[5]) })
        }
        let lab = Self.load(l).filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        let uni = ((Rotation.universe[.crypto] ?? []) + (Rotation.universe[.stock] ?? [])).compactMap { read(u, $0) }.filter { $0.1.count > 600 }.map { ($0.0, LabSeries($0.1)) }
        guard let btc = lab.first(where: { $0.0.symbol == "BTC" })?.1, let spy = lab.first(where: { $0.0.symbol == "SPY" })?.1 else { throw XCTSkip("BTC ou SPY absent") }
        let n = { (s: LabSeries) in Self.lb(s) }
        let ruleIn = { (s: LabSeries, i: Int) in s.close[i] > Self.v(s.ema200[i]) && (s.ret(i, n(s)) ?? -1) > 0 }
        func days(_ s: LabSeries, ema100: Bool, mom: Double) -> [Double: Bool] {
            var d: [Double: Bool] = [:]
            let k = Int((Double(n(s)) * mom).rounded())
            for i in s.c.indices {
                let e = ema100 ? s.ema100[i] : s.ema200[i]
                d[(s.c[i].time / 86_400_000).rounded(.down)] = s.close[i] > Self.v(e) && (s.ret(i, k) ?? -1) > 0
            }
            return d
        }
        var rules = [LabRule(name: "règle seule", enter: ruleIn, exit: { s, i in !ruleIn(s, i) })]
        for (ema100, mom, name) in [(false, 1.0, "EMA200 · mom 1×"), (false, 0.5, "EMA200 · mom ½×"), (false, 2.0, "EMA200 · mom 2×"),
                                    (true, 1.0, "EMA100 · mom 1×"), (true, 0.5, "EMA100 · mom ½×"), (true, 2.0, "EMA100 · mom 2×")] {
            let mk = [true: days(btc, ema100: ema100, mom: mom), false: days(spy, ema100: ema100, mom: mom)]
            let ok = { (s: LabSeries, i: Int) in mk[s.c.count > 950]![(s.c[i].time / 86_400_000).rounded(.down)] ?? true }
            rules.append(LabRule(name: "marché " + name, enter: { s, i in ruleIn(s, i) && ok(s, i) }, exit: { s, i in !ruleIn(s, i) || !ok(s, i) }))
        }
        print("== labo"); print(Lab.report(lab, rules)); print(Lab.full(lab, rules))
        print("== univers"); print(Lab.report(uni, rules)); print(Lab.full(uni, rules))
    }
}
