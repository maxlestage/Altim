//! Shared UI primitives (ui.tsx) and the French date formats of the browser. Same markup and CSS classes as the
//! React components, so app.css applies unchanged.
use altim_core::engine::backtest::BacktestTrade;
use altim_core::engine::reliability::{Reliability, ReliabilityLevel};
use altim_core::engine::signal::{Action, Candle, ema};
use altim_core::web::market::format_percent;
use wasm_bindgen::JsValue;
use yew::prelude::*;

/// "buy" / "sell" / "hold": colour class of an action.
pub fn action_kind(a: Action) -> &'static str {
    match a {
        Action::Buy | Action::StrongBuy => "buy",
        Action::Sell | Action::StrongSell => "sell",
        Action::Hold => "hold",
    }
}

/// The 4 h technical signal as a direction, not an order: the verdict is the full decision's (Radar, asset page).
pub fn technical_text(a: Action) -> &'static str {
    match a {
        Action::StrongBuy => "nettement haussier",
        Action::Buy => "haussier",
        Action::Sell => "baissier",
        Action::StrongSell => "nettement baissier",
        Action::Hold => "neutre",
    }
}

#[derive(Properties, PartialEq)]
pub struct ActionBadgeProps {
    pub action: Action,
    #[prop_or_default]
    pub big: bool,
}

#[component]
pub fn ActionBadge(p: &ActionBadgeProps) -> Html {
    html! { <span class={classes!("badge", action_kind(p.action), p.big.then_some("big"))}>{ p.action.label() }</span> }
}

#[derive(Properties, PartialEq)]
pub struct ReliabilityBadgeProps {
    pub rel: Reliability,
}

#[component]
pub fn ReliabilityBadge(p: &ReliabilityBadgeProps) -> Html {
    let (icon, cls) = match p.rel.level {
        ReliabilityLevel::High => ("✔", "high"),
        ReliabilityLevel::Medium => ("!", "medium"),
        ReliabilityLevel::Low => ("✕", "low"),
    };
    let label = p.rel.level.label();
    html! {
        <span class={classes!("rel-badge", cls)} title={format!("{label} — {}/100", altim_core::js::round(p.rel.score))}>
            { format!("{icon} {label}") }
        </span>
    }
}

#[derive(Properties, PartialEq)]
pub struct ChangeProps {
    pub value: Option<f64>,
}

/// "+1,24 %" in green or red, "—" when unknown.
#[component]
pub fn Change(p: &ChangeProps) -> Html {
    match p.value.filter(|v| v.is_finite()) {
        None => html! { <span class="muted">{ "—" }</span> },
        Some(v) => html! { <span class={if v >= 0.0 { "up" } else { "down" }}>{ format_percent(v) }</span> },
    }
}

#[derive(Properties, PartialEq)]
pub struct PriceProps {
    pub value: Option<f64>,
}

#[component]
pub fn Price(p: &PriceProps) -> Html {
    let _m = crate::money::use_money();
    html! { { p.value.filter(|v| v.is_finite()).map(crate::money::price).unwrap_or_else(|| "—".into()) } }
}

/// The gauge's gradient stops (ui.tsx, LiveSignal, PhoneMockup).
pub fn gauge_stops() -> Html {
    html! {
        <>
            <stop offset="0" stop-color="#ff3b5c" />
            <stop offset="0.45" stop-color="#ffc733" />
            <stop offset="0.7" stop-color="#00f0ff" />
            <stop offset="1" stop-color="#39ff88" />
        </>
    }
}

#[derive(Properties, PartialEq)]
pub struct GaugeProps {
    pub score: f64,
    #[prop_or(180.0)]
    pub size: f64,
    /// Id of the gradient (unique in the page).
    #[prop_or(AttrValue::Static("app-g"))]
    pub gradient: AttrValue,
    /// ui.tsx sets the width and an aria label; the site's gauge (LiveSignal) does not.
    #[prop_or(true)]
    pub app: bool,
}

/// Score gauge −100…+100 with its needle.
#[component]
pub fn Gauge(p: &GaugeProps) -> Html {
    let angle = -90.0 + ((p.score + 100.0) / 200.0) * 180.0;
    let score = altim_core::js::to_fixed(p.score, 0);
    let sign = if p.score >= 0.0 { "+" } else { "" };
    html! {
        <div class="gauge-wrap" style={p.app.then(|| format!("width: {}px;", p.size))}>
            <svg viewBox="0 0 200 115" class="gauge" role={p.app.then_some("img")} aria-label={p.app.then(|| format!("Score {score} sur 100"))}>
                <defs>
                    <linearGradient id={p.gradient.clone()} x1="0" x2="1">{ gauge_stops() }</linearGradient>
                </defs>
                <path d="M20 100 A80 80 0 0 1 180 100" stroke="rgba(255,255,255,.08)" stroke-width="14" fill="none" stroke-linecap="round" />
                <path d="M20 100 A80 80 0 0 1 180 100" stroke={format!("url(#{})", p.gradient)} stroke-width="14" fill="none" stroke-linecap="round" />
                <g style={format!("transform: rotate({angle}deg); transform-origin: 100px 100px; transition: transform 1s cubic-bezier(.3,1.6,.5,1);")}>
                    <line x1="100" y1="100" x2="100" y2="32" stroke="#fff" stroke-width="4" stroke-linecap="round" />
                </g>
                <circle cx="100" cy="100" r="7" fill="#fff" />
            </svg>
            <b class="gauge-score">{ format!("{sign}{score}") }</b>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct SparklineProps {
    pub values: Vec<f64>,
}

#[component]
pub fn Sparkline(p: &SparklineProps) -> Html {
    let v = &p.values;
    if v.len() < 2 {
        return html! { <svg class="spark" /> };
    }
    let min = v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = if max - min == 0.0 { 1.0 } else { max - min };
    let pts: Vec<String> = v
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let px = altim_core::js::to_fixed(i as f64 / (v.len() - 1) as f64 * 100.0, 2);
            let py = altim_core::js::to_fixed(30.0 - (x - min) / range * 28.0 - 1.0, 2);
            format!("{px},{py}")
        })
        .collect();
    let up = v[v.len() - 1] >= v[0];
    html! {
        <svg class="spark" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
            <polyline points={pts.join(" ")} fill="none" stroke={if up { "#39ff88" } else { "#ff3b5c" }} stroke-width="1.6" vector-effect="non-scaling-stroke" />
        </svg>
    }
}

/// SVG path "M x,y L x,y…" of a series on a W×H box (gaps skipped), as the charts of ui.tsx / LiveSignal.
pub fn chart_path(series: &[Option<f64>], x: impl Fn(usize) -> f64, y: impl Fn(f64) -> f64) -> String {
    let mut d = String::new();
    for (i, v) in series.iter().enumerate() {
        if let Some(v) = v {
            d.push_str(if d.is_empty() { "M" } else { "L" });
            d.push_str(&format!("{},{}", altim_core::js::to_fixed(x(i), 1), altim_core::js::to_fixed(y(*v), 1)));
        }
    }
    d
}

#[derive(Properties, PartialEq)]
pub struct PriceChartProps {
    pub candles: Vec<Candle>,
    #[prop_or_default]
    pub stop: Option<f64>,
    #[prop_or_default]
    pub target: Option<f64>,
    #[prop_or_default]
    pub trades: Vec<BacktestTrade>,
}

/// Price chart: close, EMA 20/50, stop/target lines and backtest entries.
#[component]
pub fn PriceChart(p: &PriceChartProps) -> Html {
    let closes: Vec<f64> = p.candles.iter().map(|c| c.close).collect();
    let (e20, e50) = (ema(&closes, 20), ema(&closes, 50));
    let start = closes.len().saturating_sub(150);
    let data: Vec<Option<f64>> = closes[start..].iter().map(|c| Some(*c)).collect();
    let times: Vec<i64> = p.candles[start..].iter().map(|c| c.time).collect();
    let (w, h) = (600.0, 280.0);
    let mut values: Vec<f64> = closes[start..].to_vec();
    values.extend(p.stop.filter(|v| *v != 0.0));
    values.extend(p.target.filter(|v| *v != 0.0));
    let min = values.iter().copied().fold(f64::INFINITY, f64::min) * 0.998;
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max) * 1.002;
    let n = data.len();
    let x = move |i: usize| i as f64 / (n.max(2) - 1) as f64 * w;
    let y = move |v: f64| h - (v - min) / (if max - min == 0.0 { 1.0 } else { max - min }) * h;
    let line = chart_path(&data, x, y);
    let t0 = times.first().copied().unwrap_or(0);
    html! {
        <svg viewBox={format!("0 0 {w} {h}")} class="price-chart" preserveAspectRatio="none" role="img" aria-label="Graphique des prix">
            <defs>
                <linearGradient id="app-area" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0" stop-color="rgba(0,240,255,.32)" />
                    <stop offset="1" stop-color="rgba(0,240,255,0)" />
                </linearGradient>
            </defs>
            { for [0.25, 0.5, 0.75].iter().map(|f| html! { <line x1="0" x2={w.to_string()} y1={(h * f).to_string()} y2={(h * f).to_string()} stroke="rgba(255,255,255,.06)" /> }) }
            <path d={format!("{line}L{w},{h}L0,{h}Z")} fill="url(#app-area)" />
            <path d={chart_path(&e50[start..], x, y)} stroke="#7d4dff" stroke-width="1.5" fill="none" vector-effect="non-scaling-stroke" />
            <path d={chart_path(&e20[start..], x, y)} stroke="#ff2bd6" stroke-width="1.5" fill="none" vector-effect="non-scaling-stroke" />
            <path d={line} stroke="#00f0ff" stroke-width="2.5" fill="none" class="glow-line" vector-effect="non-scaling-stroke" />
            if let Some(s) = p.stop.filter(|v| *v != 0.0) {
                <line x1="0" x2={w.to_string()} y1={y(s).to_string()} y2={y(s).to_string()} stroke="#ff3b5c" stroke-dasharray="6 6" vector-effect="non-scaling-stroke" />
            }
            if let Some(t) = p.target.filter(|v| *v != 0.0) {
                <line x1="0" x2={w.to_string()} y1={y(t).to_string()} y2={y(t).to_string()} stroke="#39ff88" stroke-dasharray="6 6" vector-effect="non-scaling-stroke" />
            }
            { for p.trades.iter().filter(|t| t.entry_time >= t0).filter_map(|t| times.iter().position(|x| *x == t.entry_time).map(|i| html! {
                <circle key={t.entry_time.to_string()} cx={x(i).to_string()} cy={y(t.entry_price).to_string()} r="4" fill="#39ff88" vector-effect="non-scaling-stroke" />
            })) }
        </svg>
    }
}

#[derive(Properties, PartialEq)]
pub struct SegmentedProps {
    pub value: AttrValue,
    /// (value, label)
    pub options: Vec<(AttrValue, AttrValue)>,
    pub on_change: Callback<AttrValue>,
    pub label: AttrValue,
}

/// Radio group of buttons ("1 h · 4 h · 1 j").
#[component]
pub fn Segmented(p: &SegmentedProps) -> Html {
    html! {
        <div class="segmented" role="radiogroup" aria-label={p.label.clone()}>
            { for p.options.iter().map(|(v, l)| {
                let on = *v == p.value;
                let cb = { let (cb, v) = (p.on_change.clone(), v.clone()); Callback::from(move |_| cb.emit(v.clone())) };
                html! { <button key={v.to_string()} role="radio" aria-checked={on.to_string()} class={if on { "on" } else { "" }} onclick={cb}>{ l.clone() }</button> }
            }) }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct PlaceholderProps {
    pub title: AttrValue,
}

/// A screen not yet ported to Rust (phase 1 of the migration): says so plainly instead of showing a blank page.
#[component]
pub fn NotPorted(p: &PlaceholderProps) -> Html {
    html! {
        <section class="card">
            <h1>{ p.title.clone() }</h1>
            <p class="muted">{ "Cet écran est en cours de portage vers la nouvelle version de l'application web." }</p>
        </section>
    }
}

// ---------- French dates (the browser's time zone, like toLocale*String("fr-FR")) ----------

fn opts(pairs: &[(&str, &str)]) -> JsValue {
    let o = js_sys::Object::new();
    for (k, v) in pairs {
        let _ = js_sys::Reflect::set(&o, &(*k).into(), &(*v).into());
    }
    o.into()
}

/// "14:05".
pub fn fr_time(ms: f64) -> String {
    js_sys::Date::new(&ms.into()).to_locale_time_string_with_options("fr-FR", &opts(&[("hour", "2-digit"), ("minute", "2-digit")])).into()
}

/// "14:05:09" (`toLocaleTimeString("fr-FR")`).
pub fn fr_time_seconds(ms: f64) -> String {
    js_sys::Date::new(&ms.into()).to_locale_time_string("fr-FR").into()
}

/// "29 sept.".
pub fn fr_day_month(ms: f64) -> String {
    js_sys::Date::new(&ms.into()).to_locale_date_string("fr-FR", &opts(&[("day", "numeric"), ("month", "short")])).into()
}

/// "29/09/2026 14:05" (`toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })`).
pub fn fr_date_time_short(ms: f64) -> String {
    js_sys::Date::new(&ms.into()).to_locale_string("fr-FR", &opts(&[("dateStyle", "short"), ("timeStyle", "short")])).into()
}
