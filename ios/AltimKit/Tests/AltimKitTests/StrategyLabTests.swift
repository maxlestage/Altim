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
