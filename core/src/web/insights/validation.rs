//! « Validation du modèle » (`web/src/webapp/model-validation.ts`): the pure helpers of the screen (order of the
//! assets, French texts) over the server's own contract (`engine::validation::ValidationReport`). Percentages are in
//! %, times in ms. The server computes everything; nothing here recomputes a statistic.
use std::cmp::Ordering;

use crate::engine::backtest::Regime;
use crate::engine::validation::{AssetClass, AssetResult, GroupStat, Pooled, RegimeGroup, Verdict};
use crate::js::fr;

pub use crate::web::bot::{NNBSP, plain};
use crate::web::sorting::Sorting;

pub const VALIDATION_URL: &str = "/api/validation";

pub const CLASS_ORDER: [AssetClass; 4] = [AssetClass::Stock, AssetClass::Btc, AssetClass::Eth, AssetClass::Altcoin];

/// `CLASS_SHORT`.
pub fn class_short(c: AssetClass) -> &'static str {
    match c {
        AssetClass::Stock => "Actions",
        AssetClass::Btc => "Bitcoin",
        AssetClass::Eth => "Ethereum",
        AssetClass::Altcoin => "Altcoins",
    }
}

/// Short label of a group id ("all" or a class) for the regime chips: "Tous", "Actions"…
pub fn group_short(id: &str) -> &'static str {
    CLASS_ORDER.iter().find(|c| c.id() == id).map(|c| class_short(*c)).unwrap_or("Tous")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortKey {
    Class,
    Name,
    Gap,
}

impl SortKey {
    pub fn id(self) -> &'static str {
        match self {
            SortKey::Class => "class",
            SortKey::Name => "name",
            SortKey::Gap => "gap",
        }
    }
}

pub const SORTS: [(SortKey, &str); 3] = [(SortKey::Class, "Par classe"), (SortKey::Name, "Par nom"), (SortKey::Gap, "Écart avec la détention")];

/// Rank of a character under `localeCompare(…, "fr")` for ticker symbols: punctuation ("-" before "."), then digits,
/// then letters whatever their case (ICU's root order for these characters).
fn collation_key(s: &str) -> Vec<(u8, char)> {
    s.chars()
        .map(|c| match c {
            '-' => (0, c),
            '.' => (1, c),
            c if c.is_ascii_digit() => (3, c),
            c if c.is_alphabetic() => (4, c.to_lowercase().next().unwrap_or(c)),
            c => (2, c),
        })
        .collect()
}

fn locale_compare(a: &str, b: &str) -> Ordering {
    collation_key(a).cmp(&collation_key(b)).then_with(|| a.cmp(b))
}

fn class_rank(c: Option<AssetClass>) -> i32 {
    c.and_then(|c| CLASS_ORDER.iter().position(|x| *x == c)).map(|i| i as i32).unwrap_or(-1)
}

/// Rows in the chosen order: by class (the basket's own order, the default), by name, or by the gap signal −
/// buy-and-hold (largest first). Never changes the report (the sort is stable, like `Array.prototype.sort`).
pub fn sort_assets(assets: &[AssetResult], key: SortKey) -> Vec<&AssetResult> {
    let mut rows: Vec<&AssetResult> = assets.iter().collect();
    match key {
        SortKey::Name => rows.sort_by_dyn(|a, b| locale_compare(&a.symbol, &b.symbol)),
        SortKey::Gap => {
            rows.sort_by_dyn(|a, b| (b.total_return - b.buy_and_hold).partial_cmp(&(a.total_return - a.buy_and_hold)).unwrap_or(Ordering::Equal))
        }
        SortKey::Class => rows.sort_by_key_dyn(|a| class_rank(a.class)),
    }
    rows
}

fn frm(v: f64, max: usize) -> String {
    fr(v, 0, max)
}

/// "+12,3 %", "−4 %", "—".
pub fn signed_pct(v: Option<f64>, digits: usize) -> String {
    match v.filter(|v| v.is_finite()) {
        None => "—".into(),
        Some(v) => format!(
            "{}{}{NNBSP}%",
            if v < 0.0 {
                "−"
            } else if v > 0.0 {
                "+"
            } else {
                ""
            },
            frm(v.abs(), digits)
        ),
    }
}

/// "12,4 ans", "1 an", "—".
pub fn years(v: Option<f64>) -> String {
    match v.filter(|v| v.is_finite()) {
        None => "—".into(),
        Some(v) => format!("{}{NNBSP}an{}", frm(v, 1), if v >= 2.0 { "s" } else { "" }),
    }
}

/// Tone of a verdict for the chips: good only for a measured positive gain, warning for a measured loss.
pub fn verdict_tone(v: Verdict) -> &'static str {
    match v {
        Verdict::Edge => "good",
        Verdict::Negative => "warning",
        _ => "info",
    }
}

/// "6 sur 34 (18 %)".
pub fn beat_text(beat: usize, n: usize, share: Option<f64>) -> String {
    if n == 0 {
        return "aucun actif comparable".into();
    }
    match share {
        None => format!("{beat} sur {n}"),
        Some(s) => format!("{beat} sur {n} ({}{NNBSP}%)", frm(s, 0)),
    }
}

/// What the regime days say, in one sentence (nothing when no asset spent 20 days in it).
pub fn regime_days_text(g: &RegimeGroup) -> String {
    if g.assets == 0 {
        return "Aucun actif n'a passé 20 jours dans ce régime : pas de comparaison avec la détention.".into();
    }
    format!(
        "Pendant ces jours, le signal a fait mieux que la détention sur {} actifs (médianes : signal {}, détention {}).",
        beat_text(g.beat_hold, g.assets, g.beat_share),
        signed_pct(g.median_signal, 1),
        signed_pct(g.median_hold, 1)
    )
}

/// Trades line of a pooled group: "650 trades · réussite 38 % · espérance +0,2 % · t = 2,2".
pub fn pooled_text(p: &Pooled) -> String {
    if p.trades == 0 {
        return "Aucun trade.".into();
    }
    let mut parts = vec![
        format!("{} trade{}", p.trades, if p.trades > 1 { "s" } else { "" }),
        format!("réussite {}{NNBSP}%", frm(p.win_rate.unwrap_or(0.0), 0)),
        format!("espérance {}", signed_pct(p.expectancy, 2)),
    ];
    if p.profit_factor.is_some() {
        parts.push(format!("facteur de profit {}", plain(p.profit_factor, 2)));
    }
    if p.t_stat.is_some() {
        parts.push(format!("t = {}", plain(p.t_stat, 1)));
    }
    parts.join(" · ")
}

pub const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

/// "sept. 2026" (UTC), "?" when unknown.
pub fn month_year(ms: Option<i64>) -> String {
    use chrono::Datelike;
    match ms.and_then(chrono::DateTime::from_timestamp_millis) {
        Some(d) => format!("{} {}", MONTHS[d.month0() as usize], d.year()),
        None => "?".into(),
    }
}

/// "30/09/2026": `new Date(ms).toLocaleDateString("fr-FR", { timeZone: "UTC" })`, "?" when unknown.
pub fn utc_day(ms: Option<i64>) -> String {
    match ms.and_then(chrono::DateTime::from_timestamp_millis) {
        Some(d) => d.format("%d/%m/%Y").to_string(),
        None => "?".into(),
    }
}

/// Regimes of a group, the "unknown" one (history too short to classify) last.
pub fn regimes_of(g: &GroupStat) -> Vec<&RegimeGroup> {
    let mut v: Vec<&RegimeGroup> = g.regimes.iter().collect();
    v.sort_by_key_dyn(|r| r.regime == Regime::Unknown);
    v
}

/// "Actifs testés" tile's history span: "3,1 à 12,4 ans" when the spans differ by 0.2 year or more, else the median.
pub fn history_tile(years_median: Option<f64>, min: Option<f64>, max: Option<f64>) -> String {
    match (min, max) {
        (Some(lo), Some(hi)) if hi - lo >= 0.2 => format!("{} à {}", plain(Some(lo), 1), years(Some(hi))),
        _ => years(years_median),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::validation::ValidationReport;

    fn report() -> ValidationReport {
        serde_json::from_str(include_str!("../../../../backend/tests/samples/validation.json")).unwrap()
    }

    #[test]
    fn contract_sample_adds_up() {
        let r = report();
        assert_eq!(VALIDATION_URL, "/api/validation");
        assert_eq!(r.assets.len() + r.failures.len(), r.basket.len());
        assert_eq!(r.overall.pooled.trades, r.assets.iter().map(|a| a.trades).sum::<usize>());
        assert_eq!(r.classes.iter().map(|g| g.assets).sum::<usize>(), r.overall.assets);
        assert_eq!(r.overall.regimes.iter().map(|g| g.pooled.trades).sum::<usize>(), r.overall.pooled.trades);
        assert_eq!(r.overall.beat_hold, r.assets.iter().filter(|a| a.beat_hold).count());
        let ids: Vec<&str> = r.classes.iter().map(|g| g.id.as_str()).collect();
        let expected: Vec<&str> = CLASS_ORDER.iter().map(|c| c.id()).filter(|c| ids.contains(c)).collect();
        assert_eq!(ids, expected);
        assert_eq!(r.out_of_sample.status, "notVerifiable");
        assert_eq!(r.parameters.tax_rate_pct, 30.0);
        assert!(r.headline.starts_with(&format!("Sur {} actifs", r.overall.assets)));
    }

    #[test]
    fn basket_order_by_default_never_ranked() {
        let r = report();
        let by_class: Vec<&str> = sort_assets(&r.assets, SortKey::Class).iter().map(|a| a.symbol.as_str()).collect();
        assert_eq!(by_class, r.assets.iter().map(|a| a.symbol.as_str()).collect::<Vec<_>>());
        let by_gap: Vec<f64> = sort_assets(&r.assets, SortKey::Gap).iter().map(|a| a.total_return - a.buy_and_hold).collect();
        let mut sorted = by_gap.clone();
        sorted.sort_by_dyn(|a, b| b.partial_cmp(a).unwrap());
        assert_eq!(by_gap, sorted);
        let by_name: Vec<&str> = sort_assets(&r.assets, SortKey::Name).iter().map(|a| a.symbol.as_str()).collect();
        let mut names = by_name.clone();
        names.sort_by_dyn(|a, b| locale_compare(a, b));
        assert_eq!(by_name, names);
        assert_eq!(r.assets[0].symbol, r.basket[0].symbol);
        // localeCompare("fr") of tickers: punctuation, digits, letters.
        let mut t = vec!["BRK.B", "BRK-B", "B2", "BA", "ba"];
        t.sort_by_dyn(|a, b| locale_compare(a, b));
        assert_eq!(t, ["B2", "BA", "ba", "BRK-B", "BRK.B"]);
    }

    fn regime() -> RegimeGroup {
        RegimeGroup {
            regime: Regime::Bear,
            label: "Marché baissier".into(),
            pooled: Pooled {
                trades: 7,
                win_rate: Some(42.9),
                profit_factor: Some(0.8),
                expectancy: Some(-0.31),
                avg_r: Some(-0.1),
                t_stat: Some(-0.4),
            },
            days: 3922,
            assets: 20,
            beat_hold: 7,
            beat_share: Some(35.0),
            median_signal: Some(-1.5),
            median_hold: Some(-9.25),
            low_sample: true,
            verdict: Verdict::Insufficient,
            verdict_label: "Échantillon trop faible".into(),
        }
    }

    #[test]
    fn numbers_in_french_with_signs() {
        let g = regime();
        assert_eq!(signed_pct(Some(12.345), 1), "+12,3\u{202f}%");
        assert_eq!(signed_pct(Some(-4.0), 1), "−4\u{202f}%");
        assert_eq!(signed_pct(None, 1), "—");
        assert_eq!(beat_text(6, 34, Some(17.6)), "6 sur 34 (18\u{202f}%)");
        assert_eq!(beat_text(0, 0, None), "aucun actif comparable");
        assert_eq!(pooled_text(&g.pooled), "7 trades · réussite 43\u{202f}% · espérance −0,31\u{202f}% · facteur de profit 0,8 · t = −0,4");
        assert_eq!(pooled_text(&Pooled { trades: 0, ..g.pooled }), "Aucun trade.");
        assert!(regime_days_text(&g).contains("7 sur 20 (35\u{202f}%) actifs"));
        assert!(regime_days_text(&RegimeGroup { assets: 0, ..g }).contains("Aucun actif"));
        assert_eq!(verdict_tone(Verdict::Edge), "good");
        assert_eq!(verdict_tone(Verdict::Negative), "warning");
        assert_eq!(verdict_tone(Verdict::Unproven), "info");
        assert_eq!(years(Some(1.0)), "1\u{202f}an");
        assert_eq!(years(Some(12.44)), "12,4\u{202f}ans");
        assert_eq!(month_year(Some(1_790_722_229_334)), "sept. 2026");
        assert_eq!(month_year(None), "?");
        assert_eq!(utc_day(Some(1_790_722_229_334)), "29/09/2026");
        assert_eq!(history_tile(Some(5.0), Some(3.1), Some(12.44)), "3,1 à 12,4\u{202f}ans");
        assert_eq!(history_tile(Some(5.0), Some(5.0), Some(5.1)), "5\u{202f}ans");
        assert_eq!(group_short("all"), "Tous");
        assert_eq!(group_short("eth"), "Ethereum");
    }

    #[test]
    fn unknown_regime_last() {
        let r = report();
        let order: Vec<Regime> = regimes_of(&r.overall).iter().map(|g| g.regime).collect();
        if order.contains(&Regime::Unknown) {
            assert_eq!(order.last(), Some(&Regime::Unknown));
        }
        assert_eq!(order[..4], [Regime::Bull, Regime::Bear, Regime::Range, Regime::Crisis]);
    }
}
