import Foundation
import XCTest
@testable import AltimKit

/// Rating, composite score, degraded signal, market regime, horizon, technical structure, target 3, the next 7 days'
/// events and the score weights: a real /api/decision answer (AAPL, events not verified), the hand-completed samples,
/// and the texts the web writes for them (web/src/webapp/decision.ts, DecisionCard.tsx).
final class DecisionSynthesisTests: XCTestCase {
    func decision(_ name: String) throws -> Decision {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(Decision.self, from: Data(contentsOf: url))
    }

    let n = "\u{202F}"

    func testRealAnswer() throws {
        let d = try decision("decision-aapl-real")
        XCTAssertEqual(d.rating, .hold)
        XCTAssertEqual(d.ratingLabel, "ATTENDRE")
        XCTAssertTrue(d.knowsEvents)
        XCTAssertNil(d.events, "calendar not verified")
        XCTAssertEqual(DecisionText.eventsBadge(d.events), "non vérifié")
        XCTAssertEqual(DecisionText.eventsNote(d.events, kind: .stock), "Calendrier indisponible ou incomplet : les annonces à venir n'ont pas pu être vérifiées.")
        // The new "announcement" veto is one veto among others (no fixed list).
        XCTAssertTrue(d.vetoes.contains { $0.code == "announcement" })
        XCTAssertEqual(d.vetoes.count, 16)
        let s = try XCTUnwrap(d.score)
        XCTAssertEqual(s.value, 24)
        XCTAssertEqual(s.factors.map(\.key), ["tech", "mom", "fund", "sent", "news", "macro"])
        XCTAssertNil(s.factors[3].value)
        XCTAssertEqual(DecisionText.signedScore(try XCTUnwrap(s.value)), "+24")
        XCTAssertEqual(s.factors.map { $0.value == nil ? "non mesuré" : "poids \(DecisionText.num($0.applied, digits: 1)) %" },
                       ["poids 35,6 %", "poids 20 %", "poids 22,2 %", "non mesuré", "poids 11,1 %", "poids 11,1 %"])
        XCTAssertEqual(d.degraded?.active, false)
        XCTAssertEqual(d.marketRegime?.kind, .neutral)
        XCTAssertEqual(d.marketRegime?.kind.label, "Neutre")
        XCTAssertEqual(d.horizon?.label, "Moyen terme")
        let p = try XCTUnwrap(d.plan)
        XCTAssertEqual(try XCTUnwrap(p.target3), 399.54416, accuracy: 1e-9)
        XCTAssertEqual(p.reward3Pct, 36.8)
        XCTAssertEqual(DecisionText.usd(p.target3), "399,54\(n)$")
        XCTAssertEqual(DecisionText.target3Source(try XCTUnwrap(p.target3Source)),
                       "Objectif 3 : projection : objectif 2 + (objectif 2 − objectif 1), aucun niveau touché avant.")
    }

    func testStructureRows() throws {
        let st = try XCTUnwrap(try decision("decision-aapl-real").structure)
        XCTAssertEqual(DecisionText.structureHeader(st), "Bougies journalières clôturées. Direction d'ensemble : +70/100.")
        let rows = DecisionText.structureRows(st)
        XCTAssertEqual(rows.map(\.name), [
            "Ichimoku (9, 26, 52)", "Supertrend (ATR 10 × 3)", "Canal de Donchian (20)", "VWAP glissant (20 bougies)", "Profil de volume (approximation)",
            "Points pivots (dernière séance)", "Supports et résistances", "Cassure", "Structure (sommets et creux)",
            "Force relative contre S&P 500 (SPY)", "Force relative contre Nasdaq-100 (QQQ)",
        ])
        // Same figures as the web (usd, pct, points).
        XCTAssertEqual(rows[0].extra, "Tenkan 336,85\(n)$ · Kijun 326,17\(n)$ · nuage 309,16\(n)$ – 316,10\(n)$")
        XCTAssertEqual(rows[0].bias, .bullish)
        XCTAssertEqual(rows[2].extra, "Haut 345,34\(n)$ · milieu 327,37\(n)$ · bas 309,40\(n)$")
        XCTAssertEqual(rows[5].extra, "S2 331,95\(n)$ · S1 336,51\(n)$ · P 339,09\(n)$ · R1 343,65\(n)$ · R2 346,23\(n)$")
        XCTAssertEqual(rows[6].list.count, 8)
        XCTAssertEqual(rows[6].list[0], "Support 335,61\(n)$ · 2 contacts · −0,8\(n)%")
        XCTAssertEqual(rows[9].list[0], "1 mois : +8,8\(n)% contre +0,7\(n)% (+8,1 pts)")
        XCTAssertTrue(rows[7].reading.hasPrefix("Fausse cassure baissière"))

        // Nothing measured: every item says why instead of disappearing.
        let empty = try XCTUnwrap(try decision("decision-btc-full").structure)
        let e = DecisionText.structureRows(empty)
        XCTAssertEqual(e.count, 10)
        XCTAssertEqual(e[0].reading, "Non disponible : historique trop court")
        XCTAssertEqual(e[3].name, "VWAP glissant")
        XCTAssertEqual(e[3].reading, "Non disponible : volume absent ou historique trop court")
        XCTAssertEqual(e[8].reading, "Non disponible : pas assez de sommets et de creux")
        XCTAssertEqual(e[9].name, "Force relative")
        XCTAssertEqual(e[9].reading, "Force relative non calculée : c'est la référence elle-même")
        XCTAssertEqual(DecisionText.structureHeader(empty), "Bougies journalières clôturées.")
    }

    func testCompletedSamples() throws {
        let a = try decision("decision-aapl-full")
        XCTAssertEqual(a.events?.count, 4)
        XCTAssertEqual(DecisionText.eventsBadge(a.events), "4 événements")
        XCTAssertNil(DecisionText.eventsNote(a.events, kind: .stock))
        let b = try decision("decision-btc-full")
        XCTAssertEqual(b.events?.count, 0)
        XCTAssertEqual(DecisionText.eventsBadge(b.events), "rien de majeur")
        XCTAssertEqual(DecisionText.eventsNote(b.events, kind: .crypto), "Aucune annonce majeure (banques centrales, inflation, emploi, PIB) dans les 7 jours.")
        XCTAssertEqual(DecisionText.eventsNote([], kind: .stock), "Aucune annonce majeure (banques centrales, inflation, emploi, PIB), ni résultats, dividende ou split dans les 7 jours.")
        XCTAssertEqual(DecisionText.eventsBadge(Array((a.events ?? []).prefix(1))), "1 événement")
        XCTAssertEqual(b.degraded?.active, true)
        XCTAssertEqual(b.degraded?.reasons.count, 1)
        XCTAssertEqual(b.marketRegime?.kind, .riskOff)
        XCTAssertNil(b.score?.value)
        XCTAssertNil(b.horizon)
        // Older answers: no rating, so the events section is not shown (nil there means "older server").
        let old = try decision("decision-btc")
        XCTAssertNil(old.rating)
        XCTAssertFalse(old.knowsEvents)
        XCTAssertNil(old.plan?.target3)
        XCTAssertNil(old.structure)
    }

    /// Fresher real answers: AAPL with its 7 events and the announcement veto active, BTC with a degraded signal.
    func testRealAnswersWithEventsAndDegradedSignal() throws {
        let a = try decision("decision-aapl-events")
        let events = try XCTUnwrap(a.events)
        XCTAssertEqual(events.count, 7)
        XCTAssertTrue(events.allSatisfy { $0.kind == .macro && $0.link != nil })
        XCTAssertEqual(events[0].title, "Croissance (PIB)")
        XCTAssertEqual(DecisionText.eventsBadge(events), "7 événements")
        XCTAssertEqual(Agenda.groupByDay(events).map(\.day), ["2026-09-30", "2026-10-02"])
        let announcement = try XCTUnwrap(a.vetoes.first { $0.code == "announcement" })
        XCTAssertTrue(announcement.active)
        XCTAssertTrue(announcement.detail.hasPrefix("Croissance (PIB)"))
        XCTAssertEqual(a.sortedVetoes.first { $0.active }?.code != nil, true)

        let b = try decision("decision-btc-degraded")
        let dg = try XCTUnwrap(b.degraded)
        XCTAssertTrue(dg.active)
        XCTAssertTrue(dg.headline.hasPrefix("⚠️ Signal dégradé"))
        XCTAssertEqual(dg.reasons.count, 1)
        XCTAssertEqual(b.events?.count, 7)
        let rows = DecisionText.structureRows(try XCTUnwrap(b.structure))
        XCTAssertEqual(rows.last?.name, "Force relative")
        XCTAssertEqual(rows.last?.reading, "BTC est lui-même l'indice de référence : pas de force relative calculée")
        XCTAssertNotNil(b.plan?.target3)
    }

    func testRatingsAndTexts() throws {
        let r = try JSONDecoder().decode([Decision.Rating].self, from: Data(#"["strongBuy","buy","hold","reduce","sell","strongSell","moon"]"#.utf8))
        XCTAssertEqual(r.map(\.label), ["ACHAT FORT", "ACHAT", "ATTENDRE", "ALLÉGER", "VENDRE", "VENTE FORTE", "ATTENDRE"])
        XCTAssertEqual(r.map(\.emoji), ["🟢", "🟢", "⚪", "🟠", "🔴", "🔴", "⚪"])
        XCTAssertEqual(r[6], .unknown)
        XCTAssertEqual(DecisionText.signedScore(34.4), "+34")
        XCTAssertEqual(DecisionText.signedScore(-12), "−12")
        XCTAssertEqual(DecisionText.signedScore(-12.5), "−12")
        XCTAssertEqual(DecisionText.signedScore(0.3), "+0")
        XCTAssertEqual(DecisionText.signedScore(0), "0")
        XCTAssertEqual(DecisionText.usd(288), "288\(n)$")
        XCTAssertEqual(DecisionText.usd(0.5), "0,5000\(n)$")
        XCTAssertEqual(DecisionText.points(-3.25), "−3,3 pts")
        let regime = try JSONDecoder().decode(Decision.MarketRegime.self, from: Data(#"{"kind":"riskOn","label":"Risk-on","benchmark":null,"reasons":[]}"#.utf8))
        XCTAssertEqual(regime.kind.label, "Risk-on (appétit pour le risque)")
        XCTAssertEqual(regime.kind.emoji, "🟢")
        let macro = #"{"score":30,"level":"tense","factors":[],"themes":[],"values":{},"regime":{"kind":"riskOff","label":"Risk-off","benchmark":"S&P 500","reasons":["VIX 32"]}}"#
        XCTAssertEqual(try JSONDecoder().decode(MacroInfo.self, from: Data(macro.utf8)).regime?.reasons, ["VIX 32"])
    }

    /// Same cases as web/test/decision.test.ts ("composite score weights").
    func testScoreWeights() throws {
        XCTAssertNil(ScoreWeights.defaults.parameter)
        var w = ScoreWeights.defaults
        w.tech = 50
        w.macro = 0
        XCTAssertEqual(w.parameter, "tech:50,mom:18,fund:20,sent:10,news:10,macro:0")
        let q = AltimClient.decisionQuery(Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), cost: nil, weights: [], scoreWeights: w)
        XCTAssertEqual(q, ["symbol": "BTC", "kind": "crypto", "w": "tech:50,mom:18,fund:20,sent:10,news:10,macro:0"])
        XCTAssertNil(AltimClient.decisionQuery(Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), cost: nil, weights: [], scoreWeights: .defaults)["w"])
        let url = AltimClient(baseURL: URL(string: "https://altim.example")!, credentials: nil).request("/api/decision", query: q).url
        XCTAssertEqual(url?.absoluteString, "https://altim.example/api/decision?kind=crypto&symbol=BTC&w=tech:50,mom:18,fund:20,sent:10,news:10,macro:0")
        // Stored values are cleaned; all at 0 falls back on the defaults.
        let cleaned = try JSONDecoder().decode(ScoreWeights.self, from: Data(#"{"tech":140,"mom":-3,"fund":12.6,"sent":"x"}"#.utf8))
        XCTAssertEqual(cleaned, ScoreWeights(tech: 100, mom: 0, fund: 13, sent: 10, news: 10, macro: 10))
        let zero = try JSONDecoder().decode(ScoreWeights.self, from: Data(#"{"tech":0,"mom":0,"fund":0,"sent":0,"news":0,"macro":0}"#.utf8))
        XCTAssertEqual(zero, .defaults)
        XCTAssertEqual(ScoreWeights(tech: 0, mom: 0, fund: 0, sent: 0, news: 0, macro: 0).sanitized, .defaults)
        XCTAssertEqual(ScoreWeights.defaults.total, 100)
        XCTAssertEqual(ScoreWeights.factors.map(\.label), ["Technique", "Momentum", "Fondamentaux", "Sentiment", "Actualités", "Macro"])
    }
}
