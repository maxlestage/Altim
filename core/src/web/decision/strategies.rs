//! Strategy comparator (/api/strategies, strategies.ts): the pure helpers of the card (colours, chart geometry,
//! direct labels, list view, formats). The report is the server's own `engine::strategies::StrategiesReport`.
use crate::engine::strategies::{StrategiesReport, StrategyId, StrategyResult};
use crate::js::fr;
use crate::types::Kind;
use crate::web::bot::encode_uri_component;

pub const NNBSP: &str = "\u{202f}";

pub fn strategies_url(symbol: &str, kind: Kind) -> String {
    format!("/api/strategies?symbol={}&kind={}", encode_uri_component(symbol), kind.as_str())
}

/// Fixed order of the chips and of the cards.
pub const ORDER: [StrategyId; 8] = [
    StrategyId::Trend,
    StrategyId::Momentum,
    StrategyId::Breakout,
    StrategyId::MeanReversion,
    StrategyId::Swing,
    StrategyId::Dca,
    StrategyId::BuyHold,
    StrategyId::Value,
];
pub const DEFAULT_SELECTION: [StrategyId; 5] =
    [StrategyId::Trend, StrategyId::Breakout, StrategyId::MeanReversion, StrategyId::Dca, StrategyId::BuyHold];
/// Buy and hold is the neutral, dashed reference.
pub const REFERENCE: StrategyId = StrategyId::BuyHold;

/// One colour per strategy, whatever is selected (colour follows the entity). Palette validated on the card surface
/// (#0c1021, dark); buy and hold is the neutral reference.
pub fn color(id: StrategyId) -> &'static str {
    match id {
        StrategyId::Trend => "#3987e5",
        StrategyId::Breakout => "#c98500",
        StrategyId::MeanReversion => "#d55181",
        StrategyId::Dca => "#008300",
        StrategyId::Momentum => "#9085e9",
        StrategyId::Swing => "#d95926",
        StrategyId::Value => "#199e70",
        StrategyId::BuyHold => "#9aa0b4",
    }
}

/// Short names of the direct labels.
pub fn short(id: StrategyId) -> &'static str {
    match id {
        StrategyId::Trend => "Tendance",
        StrategyId::Momentum => "Momentum",
        StrategyId::Breakout => "Cassure",
        StrategyId::MeanReversion => "Moyenne",
        StrategyId::Swing => "Swing",
        StrategyId::Dca => "DCA",
        StrategyId::BuyHold => "Conserver",
        StrategyId::Value => "PER",
    }
}

/// The id as sent by the server ("meanReversion").
pub fn key(id: StrategyId) -> &'static str {
    match id {
        StrategyId::Trend => "trend",
        StrategyId::Momentum => "momentum",
        StrategyId::Breakout => "breakout",
        StrategyId::MeanReversion => "meanReversion",
        StrategyId::Swing => "swing",
        StrategyId::Dca => "dca",
        StrategyId::BuyHold => "buyHold",
        StrategyId::Value => "value",
    }
}

fn fmt(v: f64, digits: usize) -> String {
    fr(v, digits, digits)
}

/// "+12,3 %" / "−4,0 %" / "—".
pub fn signed_pct(v: Option<f64>, digits: usize) -> String {
    let Some(v) = v else { return "—".into() };
    let s = if v > 0.0 {
        "+"
    } else if v < 0.0 {
        "−"
    } else {
        ""
    };
    format!("{s}{}{NNBSP}%", fmt(v.abs(), digits))
}

pub fn plain_pct(v: Option<f64>, digits: usize) -> String {
    match v {
        None => "—".into(),
        Some(v) => format!("{}{NNBSP}%", fmt(v, digits)),
    }
}

pub fn ratio(v: Option<f64>) -> String {
    match v {
        None => "—".into(),
        Some(v) => format!("{}{}", if v < 0.0 { "−" } else { "" }, fmt(v.abs(), 2)),
    }
}

/// "3 janv. 2024" (UTC).
pub fn short_date(t: i64) -> String {
    super::format::short_date_utc(t as f64)
}

/// Value of 100 invested, no decimal ("112").
pub fn value100(v: f64) -> String {
    fr(v, 0, 0)
}

/// Strategies to draw: selected, available, with a curve, in the fixed order.
pub fn drawable<'a>(report: &'a StrategiesReport, selected: &[StrategyId]) -> Vec<&'a StrategyResult> {
    ORDER
        .iter()
        .filter_map(|id| report.strategies.iter().find(|s| s.id == *id))
        .filter(|s| selected.contains(&s.id) && s.available && s.equity.len() > 1)
        .collect()
}

/// Chart box (viewBox units): width, height, left, right, top and bottom margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartBox {
    pub w: f64,
    pub h: f64,
    pub l: f64,
    pub r: f64,
    pub t: f64,
    pub b: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub lo: f64,
    pub hi: f64,
    pub n: usize,
    bx: ChartBox,
}

impl Geometry {
    pub fn x(&self, i: usize) -> f64 {
        self.bx.l + (i as f64 / (self.n - 1) as f64) * (self.bx.w - self.bx.l - self.bx.r)
    }
    pub fn y(&self, v: f64) -> f64 {
        self.bx.t + (1.0 - (v - self.lo) / (self.hi - self.lo)) * (self.bx.h - self.bx.t - self.bx.b)
    }
}

/// One axis (value of 100 invested), 100 always inside the range, a little air above and below.
pub fn geometry(series: &[&StrategyResult], bx: ChartBox) -> Option<Geometry> {
    let n = series.iter().map(|s| s.equity.len()).max().unwrap_or(0);
    if n < 2 {
        return None;
    }
    let values = series.iter().flat_map(|s| s.equity.iter().map(|p| p.1));
    let (mut lo, mut hi) = values.fold((100.0f64, 100.0f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let pad = match (hi - lo) * 0.06 {
        p if p == 0.0 || p.is_nan() => 5.0,
        p => p,
    };
    lo -= pad;
    hi += pad;
    Some(Geometry { lo, hi, n, bx })
}

/// Vertical positions of the end labels, pushed apart by `gap` and kept inside [top, bottom]; same order as `ys`.
pub fn place_labels(ys: &[f64], gap: f64, top: f64, bottom: f64) -> Vec<f64> {
    let mut order: Vec<(f64, usize)> = ys.iter().copied().zip(0..).collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<f64> = order.iter().map(|o| o.0.max(top).min(bottom)).collect();
    for k in 1..out.len() {
        out[k] = out[k].max(out[k - 1] + gap);
    }
    // Overflow at the bottom: shift the stack up.
    let over = out.last().map(|l| l - bottom).unwrap_or(0.0);
    if over > 0.0 {
        for v in out.iter_mut() {
            *v -= over;
        }
    }
    for k in (0..out.len().saturating_sub(1)).rev() {
        out[k] = out[k].min(out[k + 1] - gap);
    }
    let mut res = vec![0.0; ys.len()];
    for (k, o) in order.iter().enumerate() {
        res[o.1] = out[k];
    }
    res
}

/// Point index under a pointer at `px` (viewBox units).
pub fn index_at(px: f64, g: &Geometry, bx: ChartBox) -> usize {
    let i = crate::js::round((px - bx.l) / (bx.w - bx.l - bx.r) * (g.n - 1) as f64);
    i.clamp(0.0, (g.n - 1) as f64) as usize
}

/// List view: value of 100 invested at `steps` evenly spaced dates (first and last included).
pub fn checkpoints(s: &StrategyResult, steps: usize) -> Vec<(i64, f64)> {
    let n = s.equity.len();
    if n == 0 {
        return Vec::new();
    }
    let k_max = steps.min(n);
    let mut idx: Vec<usize> = Vec::new();
    for k in 0..k_max {
        let i = crate::js::round((k * (n - 1)) as f64 / (k_max.saturating_sub(1)).max(1) as f64) as usize;
        if !idx.contains(&i) {
            idx.push(i);
        }
    }
    idx.into_iter().map(|i| s.equity[i]).collect()
}

/// "échantillon trop faible (3 trades)" for the signal strategies with fewer than 10 trades.
pub fn low_sample_text(s: &StrategyResult) -> Option<String> {
    let t = s.metrics.as_ref().map(|m| m.trades).unwrap_or(0);
    s.low_sample.then(|| format!("échantillon trop faible ({t} trade{})", if t > 1 { "s" } else { "" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(name: &str) -> StrategiesReport {
        let path = format!("{}/../backend/tests/samples/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }
    const BOX: ChartBox = ChartBox { w: 320.0, h: 180.0, l: 34.0, r: 62.0, t: 10.0, b: 18.0 };

    // strategies.test.ts "contract samples (real /api/strategies answers)"
    #[test]
    fn samples() {
        for r in [report("strategies-aapl.json"), report("strategies-btc.json")] {
            assert_eq!(r.strategies.iter().map(|s| s.id).collect::<Vec<_>>(), ORDER);
            for s in r.strategies.iter().filter(|s| s.available) {
                assert!(s.equity.len() <= 200);
                assert_eq!((s.equity[0].0, s.equity[s.equity.len() - 1].0), (r.from, r.to));
                assert!(s.metrics.is_some() && s.rule.chars().count() > 20);
            }
            assert!(r.notes.iter().any(|n| n.contains("aucune optimisation")));
            for s in r.strategies.iter().filter(|s| s.metrics.is_some()) {
                if matches!(s.id, StrategyId::BuyHold | StrategyId::Dca) {
                    assert!(!s.low_sample);
                } else {
                    assert_eq!(s.low_sample, s.metrics.as_ref().unwrap().trades < 10);
                }
                assert_eq!(low_sample_text(s).is_some(), s.low_sample);
            }
        }
        let (aapl, btc) = (report("strategies-aapl.json"), report("strategies-btc.json"));
        assert!(aapl.strategies.iter().find(|s| s.id == StrategyId::Value).unwrap().available);
        let v = btc.strategies.iter().find(|s| s.id == StrategyId::Value).unwrap();
        assert!(!v.available && v.unavailable.as_deref().unwrap().contains("pas de bénéfices"));
    }

    // "chart helpers"
    #[test]
    fn chart_helpers() {
        let btc = report("strategies-btc.json");
        let d = drawable(&btc, &DEFAULT_SELECTION);
        assert_eq!(d.iter().map(|s| s.id).collect::<Vec<_>>(), DEFAULT_SELECTION);
        let colors: std::collections::HashSet<&str> = ORDER.iter().map(|id| color(*id)).collect();
        assert_eq!(colors.len(), ORDER.len());
        assert!(drawable(&btc, &[StrategyId::Value]).is_empty());
        let aapl = report("strategies-aapl.json");
        let d = drawable(&aapl, &DEFAULT_SELECTION);
        let g = geometry(&d, BOX).unwrap();
        assert!(g.lo < 100.0 && g.hi > 100.0);
        assert!((g.y(g.hi) - BOX.t).abs() < 1e-9);
        assert!((g.y(g.lo) - (BOX.h - BOX.b)).abs() < 1e-9);
        assert_eq!(index_at(g.x(37), &g, BOX), 37);
        assert_eq!(index_at(-50.0, &g, BOX), 0);
        assert_eq!(index_at(999.0, &g, BOX), g.n - 1);
    }

    #[test]
    fn labels_never_overlap() {
        let out = place_labels(&[50.0, 52.0, 51.0, 170.0, 171.0], 11.0, 14.0, 162.0);
        let mut sorted = out.clone();
        sorted.sort_by(f64::total_cmp);
        for k in 1..sorted.len() {
            assert!(sorted[k] - sorted[k - 1] >= 11.0 - 1e-9);
        }
        assert!(out.iter().copied().fold(f64::MIN, f64::max) <= 162.0);
        assert!(out.iter().copied().fold(f64::MAX, f64::min) >= 14.0 - 1e-9);
        assert!(out[0] < out[3]);
    }

    #[test]
    fn list_view_and_formats() {
        let aapl = report("strategies-aapl.json");
        let s = &aapl.strategies[0];
        let c = checkpoints(s, 5);
        assert_eq!(c.len(), 5);
        assert_eq!((c[0].0, c[4].0), (s.equity[0].0, s.equity[s.equity.len() - 1].0));
        assert_eq!(signed_pct(Some(12.345), 1), "+12,3\u{202f}%");
        assert_eq!(signed_pct(Some(-4.0), 1), "−4,0\u{202f}%");
        assert_eq!(signed_pct(None, 1), "—");
        assert_eq!(strategies_url("BRK.B", Kind::Stock), "/api/strategies?symbol=BRK.B&kind=stock");
        let dca = aapl.strategies.iter().find(|s| s.id == StrategyId::Dca).unwrap();
        assert!(dca.note.as_deref().unwrap_or("").contains("tout investir au départ"));
        let btc = report("strategies-btc.json");
        assert!(btc.strategies.iter().find(|s| s.id == StrategyId::Value).unwrap().unavailable.as_deref().unwrap().contains("Non applicable"));
    }
}
