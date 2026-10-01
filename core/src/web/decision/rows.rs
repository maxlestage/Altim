//! Label / value rows of the decision card (DecisionCard.tsx `Figures`, `StructureList`), built here so that the
//! view only lays them out. A missing value is None (the card writes "non disponible" rather than a blank); "—"
//! from a formatter counts as missing too.
use super::format::{count, hash_rate, num, ny_date, pct, signed_score, usd, usd_compact};
use crate::engine::decision_types::{
    ActionZone, ActionZones, CryptoFundamentals, DevActivity, Liquidity, RatioHistory, StablecoinFlows, StockFundamentals, Track,
};
use crate::engine::structure::{Bias, LevelKind, Structure};
use crate::web::money::MoneyDisplay;

/// (label, value, extra class of the value).
pub type Row = (String, Option<String>, &'static str);

fn row(k: &str, v: Option<String>) -> Row {
    (k.to_string(), v, "")
}

/// A formatted value, None when the formatter wrote "—".
pub fn shown(v: String) -> Option<String> {
    (v != "—").then_some(v)
}

/// "+2,8 Md$" / "−271 M$".
pub fn signed_usd(v: Option<f64>, m: &MoneyDisplay) -> String {
    match v {
        None => "—".into(),
        Some(v) => format!(
            "{}{}",
            if v < 0.0 {
                "−"
            } else if v > 0.0 {
                "+"
            } else {
                ""
            },
            usd_compact(Some(v.abs()), m)
        ),
    }
}

/// Two rows per ratio: today vs median and range, then where today stands in the window.
pub fn history_rows(name: &str, r: Option<&RatioHistory>) -> Vec<Row> {
    let Some(r) = r else { return Vec::new() };
    vec![
        row(
            &format!("{name} sur la période"),
            Some(format!(
                "{} aujourd'hui · médiane {} · de {} à {}",
                num(Some(r.current), 1),
                num(Some(r.median), 1),
                num(Some(r.min), 1),
                num(Some(r.max), 1)
            )),
        ),
        row(
            &format!("Centile du {name}"),
            Some(format!(
                "plus haut que {} % des {} jours ({} – {})",
                num(Some(r.percentile), 0),
                count(Some(r.days as f64)),
                ny_date(r.from as f64),
                ny_date(r.to as f64)
            )),
        ),
    ]
}

pub fn stable_rows(s: Option<&StablecoinFlows>, label: &str, m: &MoneyDisplay) -> Vec<Row> {
    let Some(s) = s else { return Vec::new() };
    vec![
        row(&format!("Stablecoins ({label})"), Some(format!("{} au {}", usd_compact(Some(s.total), m), ny_date(s.date as f64)))),
        row("… sur 7 jours", s.change7d.map(|c| format!("{} ({})", signed_usd(Some(c), m), pct(s.change7d_pct, 2, true)))),
        row("… sur 30 jours", s.change30d.map(|c| format!("{} ({})", signed_usd(Some(c), m), pct(s.change30d_pct, 2, true)))),
    ]
}

/// The company's figures (`StockFund`).
pub fn stock_rows(f: &StockFundamentals, m: &MoneyDisplay) -> Vec<Row> {
    let c = |v: Option<f64>| shown(usd_compact(v, m));
    vec![
        row(
            "Chiffre d'affaires",
            f.revenue.map(|r| {
                format!(
                    "{}{}",
                    usd_compact(Some(r), m),
                    f.revenue_growth.map(|g| format!(" ({} sur un an)", pct(Some(g), 1, true))).unwrap_or_default()
                )
            }),
        ),
        row("Résultat net", c(f.net_income)),
        row(
            "Bénéfice par action",
            f.eps.map(|e| format!("{}{}", usd(Some(e), m), f.eps_growth.map(|g| format!(" ({})", pct(Some(g), 1, true))).unwrap_or_default())),
        ),
        row("Marge brute", shown(pct(f.gross_margin, 1, false))),
        row("Marge opérationnelle", shown(pct(f.operating_margin, 1, false))),
        row("Marge nette", shown(pct(f.net_margin, 1, false))),
        row(
            "Flux de trésorerie disponible",
            f.free_cash_flow.map(|v| {
                format!("{}{}", usd_compact(Some(v), m), f.fcf_margin.map(|g| format!(" ({} du CA)", pct(Some(g), 1, false))).unwrap_or_default())
            }),
        ),
        row("Dette", c(f.debt)),
        row("Trésorerie", c(f.cash)),
        row("Dette nette", c(f.net_debt)),
        row("Rentabilité des capitaux (ROE)", shown(pct(f.roe, 0, false))),
        row("PER", shown(num(f.per, 1))),
        row("PEG", shown(num(f.peg, 2))),
        row("EV/EBITDA", shown(num(f.ev_ebitda, 1))),
        row("P/S (capitalisation ÷ ventes)", shown(num(f.ps, 1))),
        row("P/B (capitalisation ÷ fonds propres)", shown(num(f.pb, 1))),
        row(
            "ROIC (rentabilité du capital investi)",
            f.roic.map(|r| {
                format!(
                    "{} (impôt {}{})",
                    pct(Some(r), 1, false),
                    pct(f.roic_tax_rate, 1, false),
                    if f.roic_tax_statutory { " : taux légal américain, taux effectif non calculable" } else { ", taux effectif" }
                )
            }),
        ),
        row("Rendement du dividende", shown(pct(f.dividend_yield, 2, false))),
        row("Nombre d'actions sur un an", f.share_change.map(|s| format!("{}{}", pct(Some(s), 1, true), if s < 0.0 { " (rachats)" } else { "" }))),
    ]
}

/// The token's and network's figures (`CryptoFund`).
pub fn crypto_rows(f: &CryptoFundamentals, m: &MoneyDisplay) -> Vec<Row> {
    let c = |v: Option<f64>| shown(usd_compact(v, m));
    let mut rows = vec![
        row("Capitalisation", c(f.market_cap)),
        row("Valorisation totale diluée (FDV)", c(f.fdv)),
        row("Capitalisation ÷ FDV", shown(num(f.mc_fdv, 2))),
        row(
            "Offre en circulation",
            f.circulating_supply.map(|s| {
                format!("{}{}", count(Some(s)), f.circulating_pct.map(|p| format!(" ({} du maximum)", pct(Some(p), 1, false))).unwrap_or_default())
            }),
        ),
        row("Offre totale", f.total_supply.map(|s| count(Some(s)))),
        row("Offre maximale", f.max_supply.map(|s| count(Some(s)))),
        row("Valeur bloquée (TVL)", c(f.tvl)),
        row("Frais sur 30 jours", c(f.fees30d)),
        row("Dominance du bitcoin", shown(pct(f.btc_dominance, 1, false))),
        row("Taux de financement (funding)", f.funding_rate.map(|r| pct(Some(r * 100.0), 4, true))),
        row("Positions ouvertes (OI)", c(f.open_interest)),
    ];
    if let Some(t) = f.tx_per_day {
        rows.push(row("Transactions par jour", Some(count(Some(t)))));
    }
    if let Some(h) = f.hash_rate {
        rows.push(row("Taux de hachage", Some(hash_rate(Some(h)))));
    }
    rows
}

pub fn dev_rows(a: &DevActivity) -> Vec<Row> {
    vec![
        row("Commits sur 4 semaines", a.commits4w.map(|v| count(Some(v)))),
        row(
            "Lignes ajoutées / supprimées (4 semaines)",
            match (a.additions4w, a.deletions4w) {
                (Some(ad), Some(de)) => Some(format!("+{} / −{}", count(Some(ad)), count(Some(de)))),
                _ => None,
            },
        ),
        row("Pull requests intégrées (total)", a.pull_requests_merged.map(|v| count(Some(v)))),
        row("Contributeurs", a.contributors.map(|v| count(Some(v)))),
        row("Étoiles", a.stars.map(|v| count(Some(v)))),
    ]
}

pub fn liquidity_rows(l: &Liquidity, m: &MoneyDisplay) -> Vec<Row> {
    vec![
        row("Écart achat/vente", l.spread_pct.map(|s| pct(Some(s), 4, false))),
        row("Montant échangé par jour (moyenne 20 j)", shown(usd_compact(l.daily_value, m))),
        row("Volume du jour ÷ moyenne", l.relative_volume.map(|v| format!("{} ×", num(Some(v), 1)))),
    ]
}

fn up_down(v: Option<f64>) -> &'static str {
    match v {
        None => "",
        Some(v) if v >= 0.0 => "up",
        Some(_) => "down",
    }
}

/// "Historique du signal".
pub fn track_rows(t: &Track) -> Vec<Row> {
    let s = |v: Option<f64>, d: usize, sign: bool| shown(pct(v, d, sign));
    vec![
        ("Trades".into(), Some(t.trades.to_string()), ""),
        ("Réussite".into(), s(Some(t.win_rate), 0, false), ""),
        ("Gain moyen".into(), s(t.avg_win, 1, true), "up"),
        ("Perte moyenne".into(), s(t.avg_loss, 1, true), "down"),
        ("Profit factor".into(), shown(num(t.profit_factor, 2)), ""),
        ("Sharpe".into(), shown(num(t.sharpe, 2)), ""),
        ("Sortino".into(), shown(num(t.sortino, 2)), ""),
        ("Pire recul (drawdown)".into(), s(Some(t.max_drawdown), 1, true), "down"),
        ("Rendement du signal".into(), s(Some(t.total_return), 1, true), up_down(Some(t.total_return))),
        ("Simple détention".into(), s(Some(t.buy_and_hold), 1, true), up_down(Some(t.buy_and_hold))),
        ("Frais et glissement par ordre".into(), Some(format!("{} + {}", pct(Some(t.fees_pct), 2, false), pct(Some(t.slippage_pct), 2, false))), ""),
        ("Plus longue série perdante".into(), Some(format!("{} trade{}", t.losing_streak, if t.losing_streak > 1 { "s" } else { "" })), ""),
    ]
}

/// French flat tax (PFU) on the net gain, as in the sale tool: an assumption, not the user's own situation.
pub const FLAT_TAX: f64 = 30.0;

/// The signal's return after the flat tax (paid at the end on a net gain, losses offset).
pub fn after_tax(total_return: f64) -> f64 {
    if total_return > 0.0 { total_return * (1.0 - FLAT_TAX / 100.0) } else { total_return }
}

/// A line of the action ladder: a zone (the price inside it or not), or the price's own marker between zones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LadderItem<'a> {
    Zone { zone: &'a ActionZone, here: bool },
    Marker,
}

/// The action zones from the highest price down; a price outside every zone gets its own marker, above the first
/// zone lying entirely under it (`ActionLadder`).
pub fn ladder(z: &ActionZones) -> Vec<LadderItem<'_>> {
    let rows: Vec<&ActionZone> = z.zones.iter().rev().collect();
    let marker_at = if z.here.is_some() { None } else { Some(rows.iter().position(|r| r.to < z.price).unwrap_or(rows.len())) };
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        if marker_at == Some(i) {
            out.push(LadderItem::Marker);
        }
        out.push(LadderItem::Zone { zone: r, here: z.here.as_deref() == Some(r.kind.as_str()) });
    }
    if marker_at == Some(rows.len()) {
        out.push(LadderItem::Marker);
    }
    out
}

/// "◀ vous êtes ici : entre … (275 $)" of the marker.
pub fn marker_text(z: &ActionZones) -> String {
    let t = z.here_text.strip_prefix("Vous êtes ici : ").map(|t| format!("vous êtes ici : {t}")).unwrap_or_else(|| z.here_text.clone());
    format!("◀ {t}")
}

/// What a structure item shows under its reading.
#[derive(Debug, Clone, PartialEq)]
pub enum Extra {
    None,
    /// A muted line.
    Text(String),
    /// A small list (relative strength periods).
    List(Vec<String>),
    /// Supports and resistances: (kind, price in the mono font, "· 3 contacts · +3,2 %").
    Levels(Vec<(&'static str, String, String)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructureRow {
    pub name: String,
    pub bias: Option<Bias>,
    pub reading: String,
    pub extra: Extra,
}

fn pts(v: f64) -> String {
    format!(
        "{}{} pts",
        if v > 0.0 {
            "+"
        } else if v < 0.0 {
            "−"
        } else {
            ""
        },
        num(Some(v.abs()), 1)
    )
}

/// Every item of the structure, measured or not (a missing one says why rather than disappearing).
pub fn structure_rows(st: &Structure, m: &MoneyDisplay) -> Vec<StructureRow> {
    let na = "Non disponible : historique trop court";
    let u = |v: f64| usd(Some(v), m);
    let r = |name: &str, bias: Option<Bias>, reading: &str, extra: Extra| StructureRow { name: name.into(), bias, reading: reading.into(), extra };
    let mut rows = vec![
        match &st.ichimoku {
            Some(i) => r(
                "Ichimoku (9, 26, 52)",
                Some(i.bias),
                &i.reading,
                Extra::Text(format!(
                    "Tenkan {} · Kijun {} · nuage {} – {}",
                    u(i.tenkan),
                    u(i.kijun),
                    u(i.senkou_a.min(i.senkou_b)),
                    u(i.senkou_a.max(i.senkou_b))
                )),
            ),
            None => r("Ichimoku (9, 26, 52)", None, na, Extra::None),
        },
        match &st.supertrend {
            Some(s) => r("Supertrend (ATR 10 × 3)", Some(s.bias), &s.reading, Extra::None),
            None => r("Supertrend (ATR 10 × 3)", None, na, Extra::None),
        },
        match &st.donchian {
            Some(d) => r(
                "Canal de Donchian (20)",
                Some(d.bias),
                &d.reading,
                Extra::Text(format!("Haut {} · milieu {} · bas {}", u(d.upper), u(d.mid), u(d.lower))),
            ),
            None => r("Canal de Donchian (20)", None, na, Extra::None),
        },
        match &st.vwap {
            Some(v) => r(&format!("VWAP glissant ({} bougies)", v.bars), Some(v.bias), &v.reading, Extra::None),
            None => r("VWAP glissant", None, "Non disponible : volume absent ou historique trop court", Extra::None),
        },
        match &st.volume_profile {
            Some(v) => r("Profil de volume (approximation)", Some(v.bias), &v.reading, Extra::Text(v.note.clone())),
            None => r("Profil de volume (approximation)", None, "Non disponible : volume absent ou historique trop court", Extra::None),
        },
        match &st.pivots {
            Some(p) => r(
                "Points pivots (dernière séance)",
                None,
                &p.reading,
                Extra::Text(format!("S2 {} · S1 {} · P {} · R1 {} · R2 {}", u(p.s2), u(p.s1), u(p.pivot), u(p.r1), u(p.r2))),
            ),
            None => r("Points pivots", None, na, Extra::None),
        },
        r(
            "Supports et résistances",
            None,
            &st.levels_reading,
            if st.levels.is_empty() {
                Extra::None
            } else {
                Extra::Levels(
                    st.levels
                        .iter()
                        .take(8)
                        .map(|l| {
                            (
                                if l.kind == LevelKind::Support { "Support" } else { "Résistance" },
                                u(l.price),
                                format!("· {} contacts · {}", l.touches, pct(Some(l.distance_pct), 1, true)),
                            )
                        })
                        .collect(),
                )
            },
        ),
        match &st.breakout {
            Some(b) => r("Cassure", Some(b.bias), &b.reading, Extra::None),
            None => r("Cassure", None, na, Extra::None),
        },
        match &st.market_structure {
            Some(s) => r("Structure (sommets et creux)", Some(s.bias), &s.reading, Extra::None),
            None => r("Structure (sommets et creux)", None, "Non disponible : pas assez de sommets et de creux", Extra::None),
        },
    ];
    for x in &st.relative {
        rows.push(r(
            &format!("Force relative contre {}", x.benchmark),
            Some(x.bias),
            &x.reading,
            Extra::List(
                x.periods
                    .iter()
                    .map(|p| {
                        format!("{} : {} contre {} ({})", p.label, pct(Some(p.asset_pct), 1, true), pct(Some(p.benchmark_pct), 1, true), pts(p.diff))
                    })
                    .collect(),
            ),
        ));
    }
    if st.relative.is_empty() {
        rows.push(r("Force relative", None, st.relative_note.as_deref().unwrap_or("Non disponible"), Extra::None));
    }
    rows
}

/// "timeframe. Direction d'ensemble : +40/100." head of the structure section: (timeframe, score text).
pub fn structure_score(st: &Structure) -> Option<String> {
    st.score.map(signed_score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // decision.test.ts "rating, score, degraded signal and structure" (the structure section)
    #[test]
    fn structure_section() {
        let st: Structure = serde_json::from_value(json!({
            "timeframe": "Bougies journalières clôturées", "score": 40,
            "ichimoku": { "tenkan": 173, "kijun": 164.5, "senkouA": 142.75, "senkouB": 125.5, "futureA": 168.75, "futureB": 150, "position": "above", "tkCross": null, "tkCrossBars": null, "bias": "bullish", "reading": "Prix au-dessus du nuage Ichimoku, Tenkan au-dessus de la Kijun" },
            "supertrend": { "direction": "up", "level": 160, "bars": 6, "bias": "bullish", "reading": "Supertrend haussier depuis 6 bougies (niveau 160,00 $)" },
            "donchian": null,
            "vwap": { "value": 170, "bars": 20, "deviationPct": 2, "bias": "bullish", "reading": "Prix au-dessus du VWAP glissant sur 20 bougies" },
            "volumeProfile": { "poc": 150, "valueAreaHigh": 160, "valueAreaLow": 140, "bars": 120, "bins": 24, "bias": "bullish", "reading": "Prix au-dessus de la zone de valeur", "note": "Approximation à partir des bougies : …" },
            "pivots": { "pivot": 170, "r1": 175, "r2": 180, "s1": 165, "s2": 160, "from": 0, "reading": "Prix entre le pivot et R1" },
            "levels": [{ "price": 180, "touches": 3, "kind": "resistance", "distancePct": 3.2 }], "nearestSupport": null,
            "nearestResistance": { "price": 180, "touches": 3, "kind": "resistance", "distancePct": 3.2 }, "levelsReading": "Résistance la plus proche 180,00 $ (3 contacts, +3,2 %)",
            "breakout": { "kind": "fake", "side": "up", "level": 180, "volumeRatio": 1.1, "bias": "bearish", "reading": "Fausse cassure : la résistance 180,00 $ a été dépassée puis le prix a refermé en dessous" },
            "marketStructure": null,
            "relative": [{ "benchmark": "S&P 500 (SPY)", "symbol": "SPY", "periods": [{ "label": "1 mois", "days": 30, "assetPct": 5, "benchmarkPct": 2, "diff": 3 }], "correlation": 0.4, "bias": "neutral", "reading": "En ligne avec le S&P 500 (SPY)" }],
            "relativeNote": null,
        }))
        .unwrap();
        let rows = structure_rows(&st, &MoneyDisplay::usd());
        let extra = |e: &Extra| match e {
            Extra::None => String::new(),
            Extra::Text(t) => t.clone(),
            Extra::List(l) => l.join(" | "),
            Extra::Levels(l) => l.iter().map(|(k, p, r)| format!("{k} {p} {r}")).collect::<Vec<_>>().join(" | "),
        };
        let text: String = rows.iter().map(|r| format!("{} {} {}\n", r.name, r.reading, extra(&r.extra))).collect();
        for s in [
            "Ichimoku (9, 26, 52)",
            "Supertrend haussier depuis 6 bougies",
            "Canal de Donchian (20)",
            "Non disponible : historique trop court",
            "Profil de volume (approximation)",
            "Approximation à partir des bougies",
            "Fausse cassure",
            "Force relative contre S&P 500 (SPY)",
            "1 mois : +5\u{202f}% contre +2\u{202f}% (+3 pts)",
            "Résistance 180\u{202f}$ · 3 contacts · +3,2\u{202f}%",
        ] {
            assert!(text.contains(s), "{s} in {text}");
        }
        assert_eq!(structure_score(&st).as_deref(), Some("+40"));
    }

    // guidance.test.ts "the ladder reads from the highest price down, with the price marked"
    #[test]
    fn action_ladder() {
        let d = super::super::doc::tests::guidance();
        let z = d.full().unwrap().action_zones.clone().unwrap();
        let kinds = |items: &[LadderItem]| -> Vec<String> {
            items
                .iter()
                .map(|i| match i {
                    LadderItem::Zone { zone, here } => format!("{}{}", zone.kind, if *here { " here" } else { "" }),
                    LadderItem::Marker => "marker".into(),
                })
                .collect()
        };
        assert_eq!(kinds(&ladder(&z)), ["profit", "wait here", "buy", "invalidation", "exit"]);
        assert_eq!(usd(Some(z.price), &MoneyDisplay::usd()), "341,04\u{202f}$");
        // Price between two zones: a marker of its own, placed above the first zone under it.
        let between = ActionZones {
            price: 275.0,
            here: None,
            here_text: "Vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)".into(),
            ..z.clone()
        };
        let k = kinds(&ladder(&between));
        let at = |s: &str| k.iter().position(|x| x == s).unwrap();
        assert!(at("marker") > at("buy") && at("marker") < at("invalidation"), "{k:?}");
        assert_eq!(marker_text(&between), "◀ vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)");
    }

    #[test]
    fn figures() {
        let m = MoneyDisplay::usd();
        let a = super::super::doc::tests::aapl();
        let Some(crate::engine::decision_types::Fundamentals::Stock(f)) = &a.full().unwrap().fundamentals else { panic!() };
        let rows = stock_rows(f, &m);
        assert!(rows.iter().any(|r| r.0 == "PEG") && rows.iter().any(|r| r.0 == "EV/EBITDA"));
        let t = super::super::doc::tests::btc().full().unwrap().track.clone().unwrap();
        assert_eq!(track_rows(&t)[4].0, "Profit factor");
        // config-changes.test.ts "track details": older answers show nothing more; 20 % × (1 − 30 %) after tax.
        let old = super::super::doc::tests::btc();
        assert!(!old.track_details());
        let mut raw = old.raw.clone();
        raw["track"]["expectancy"] = serde_json::json!(1);
        raw["track"]["regimes"] = serde_json::json!([]);
        let new = super::super::doc::parse_decision(raw).unwrap();
        assert!(new.track_details() && !new.track_has("spreadPct"));
        assert_eq!(pct(Some(after_tax(20.0)), 1, true), "+14\u{202f}%");
        assert_eq!(after_tax(-5.0), -5.0);
        assert_eq!(signed_usd(Some(-271e6), &m), "−271\u{202f}M$");
    }
}
