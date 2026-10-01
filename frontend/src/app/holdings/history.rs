//! « Évolution de mes lignes » (HistoryCard.tsx): value of today's lines over the period, against Bitcoin and the
//! S&P 500 (`altim_core::engine::history::portfolio_history`).
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::history::{Close, HistoryLine, PortfolioHistory, portfolio_history};
use altim_core::js::{fr, number_to_string, to_fixed};
use altim_core::types::Kind;
use altim_core::web::money::{Currency, NBSP};
use altim_core::web::store::Holding;
use yew::prelude::*;

use super::browser::utc_day;
use crate::ui::Segmented;
use altim_core::web::sorting::Sorting;

// Categorical palette validated for the allocation (dark surface, colour-blind readers): same entity, same colour.
pub(super) fn color(id: &str) -> &'static str {
    match id {
        "crypto:BTC" => "#d95926",
        "stock:SPY" => "#199e70",
        _ => "#3987e5",
    }
}
const PORTFOLIO: &str = "#3987e5";

fn usd(v: f64) -> String {
    crate::money::money_with(v, 0, 0, NBSP)
}

/// "+12,3 %" / "−4 %".
pub(super) fn pct(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}

/// A number of an SVG attribute, as React writes it (`String(n)`).
pub(super) fn attr(v: f64) -> String {
    number_to_string(v)
}

#[derive(Properties, PartialEq)]
pub struct HistoryCardProps {
    pub holdings: Vec<Holding>,
}

#[component]
pub fn HistoryCard(p: &HistoryCardProps) -> Html {
    let m = crate::money::use_money();
    let days = use_state(|| 90u32);
    let series = use_state(|| None::<Rc<HashMap<String, Vec<Close>>>>);
    let error = use_state(|| None::<String>);
    let hover = use_state(|| None::<usize>);
    let mut keys: Vec<String> = p.holdings.iter().map(|h| altim_core::web::store::asset_key(&h.symbol, h.kind)).collect();
    keys.sort_dyn();
    let key = keys.join(",");

    {
        let (series, error, holdings) = (series.clone(), error.clone(), p.holdings.clone());
        use_effect_with((key, *days), move |(_, days)| {
            let alive = Rc::new(Cell::new(true));
            series.set(None);
            let mut unique: Vec<(String, Kind)> = Vec::new();
            for h in &holdings {
                if !unique.iter().any(|(s, k)| *s == h.symbol && *k == h.kind) {
                    unique.push((h.symbol.clone(), h.kind));
                }
            }
            let (a, days) = (alive.clone(), *days);
            wasm_bindgen_futures::spawn_local(async move {
                let r = super::api::history(&unique, days).await;
                if !a.get() {
                    return;
                }
                match r {
                    Ok(r) => {
                        series.set(Some(Rc::new(r.by_id())));
                        error.set(None);
                    }
                    Err(e) => error.set(Some(e.0)),
                }
            });
            move || alive.set(false)
        });
    }

    let h = {
        // Quantity held per asset (lines of the same asset added), in the order of the holdings.
        let mut lines: Vec<HistoryLine> = Vec::new();
        for x in &p.holdings {
            let id = altim_core::web::store::asset_key(&x.symbol, x.kind);
            match lines.iter_mut().find(|l| l.id == id) {
                Some(l) => l.quantity += x.quantity,
                None => lines.push(HistoryLine { id, quantity: x.quantity }),
            }
        }
        use_memo(((*series).clone(), lines, *days), |(series, lines, days)| {
            series.as_ref().and_then(|s| portfolio_history(lines, s, *days as usize, js_sys::Date::now() as i64))
        })
    };

    let on_days = {
        let days = days.clone();
        Callback::from(move |v: AttrValue| days.set(v.parse().unwrap_or(90)))
    };
    let options: Vec<(AttrValue, AttrValue)> =
        [("30", "30 j"), ("90", "90 j"), ("365", "1 an")].iter().map(|(v, l)| (AttrValue::from(*v), AttrValue::from(*l))).collect();
    html! {
        <div class="card history-card">
            <h2 class="card-title">{ "Évolution de mes lignes" }</h2>
            <Segmented label="Période" value={AttrValue::from(days.to_string())} {options} on_change={on_days} />
            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if series.is_none() && error.is_none() {
                <p class="muted small">{ "Chargement de l'historique…" }</p>
            }
            if series.is_some() && h.is_none() {
                <p class="muted small">{ "Pas assez d'historique pour vos lignes sur cette période." }</p>
            }
            if let Some(h) = &*h {
                <p class="kv">
                    <span>{ format!("{} → {}", usd(h.points[0].value), usd(h.points[h.points.len() - 1].value)) }</span>
                    <b class={if h.change >= 0.0 { "up" } else { "down" }}>{ pct(h.change) }</b>
                </p>
                <Chart h={h.clone()} hover={*hover} set_hover={{ let hover = hover.clone(); Callback::from(move |i| hover.set(i)) }} />
                <ul class="history-legend">
                    <li><i style={format!("background: {PORTFOLIO};")} />{ "Mes lignes " }<b>{ pct(h.change) }</b></li>
                    { for h.benchmarks.iter().map(|b| html! {
                        <li key={b.id.clone()}><i style={format!("background: {};", color(&b.id))} />{ format!("{} ", b.label) }<b>{ pct(b.change) }</b></li>
                    }) }
                </ul>
                <p class="kv small"><span>{ "Pire recul depuis un sommet" }</span><b class="down">{ pct(h.max_drawdown) }</b></p>
                if let Some(b) = h.best {
                    <p class="kv small"><span>{ format!("Meilleure journée ({})", utc_day(b.t, false)) }</span><b class="up">{ pct(b.change) }</b></p>
                }
                if let Some(w) = h.worst {
                    <p class="kv small"><span>{ format!("Pire journée ({})", utc_day(w.t, false)) }</span><b class="down">{ pct(w.change) }</b></p>
                }
                <p class="muted small">
                    { "Valeur chaque jour des quantités que vous détenez aujourd'hui (cours de clôture, liquidités non comprises) : vos achats et ventes passés ne sont pas connus, ce n'est donc pas la performance de votre compte." }
                    { if h.shortened { " La courbe commence plus tard : une de vos lignes a un historique plus court." } else { "" } }
                    { if h.missing.is_empty() { String::new() } else {
                        format!(" Sans historique : {}.", h.missing.iter().map(|m| m.split(':').nth(1).unwrap_or(m)).collect::<Vec<_>>().join(", "))
                    } }
                    { if m.currency() == Currency::Eur {
                        " Cours en dollars convertis au taux du jour, pas au taux de chaque date : l'effet de change passé n'est pas compté (les % restent exacts en dollars)."
                    } else { "" } }
                </p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ChartProps {
    h: PortfolioHistory,
    hover: Option<usize>,
    set_hover: Callback<Option<usize>>,
}

const W: f64 = 320.0;
const H: f64 = 150.0;
const PAD_L: f64 = 38.0;
const PAD_R: f64 = 6.0;
const PAD_T: f64 = 8.0;
const PAD_B: f64 = 18.0;

/// "M34.0,60.1L…" (`toFixed(1)` coordinates).
pub(super) fn line_path(vals: &[f64], x: impl Fn(usize) -> f64, y: impl Fn(f64) -> f64) -> String {
    vals.iter().enumerate().map(|(i, v)| format!("{}{},{}", if i == 0 { "M" } else { "L" }, to_fixed(x(i), 1), to_fixed(y(*v), 1))).collect()
}

#[component]
fn Chart(p: &ChartProps) -> Html {
    let h = &p.h;
    let base = h.points[0].value;
    let mine: Vec<f64> = h.points.iter().map(|x| (x.value / base - 1.0) * 100.0).collect();
    let mut all = mine.clone();
    all.extend(h.benchmarks.iter().flat_map(|b| b.points.iter().map(|x| x.pct)));
    all.push(0.0);
    let lo = all.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if hi - lo == 0.0 { 1.0 } else { hi - lo };
    let len = mine.len();
    let x = move |i: usize| PAD_L + i as f64 / (len - 1) as f64 * (W - PAD_L - PAD_R);
    let y = move |v: f64| PAD_T + (1.0 - (v - lo) / span) * (H - PAD_T - PAD_B);
    let at = p.hover.unwrap_or(len - 1).min(len - 1);
    let pick = {
        let set = p.set_hover.clone();
        Callback::from(move |e: PointerEvent| {
            let Some(el) = e.current_target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()) else { return };
            let r = el.get_bounding_client_rect();
            let px = (e.client_x() as f64 - r.left()) / r.width() * W;
            let i = ((px - PAD_L) / (W - PAD_L - PAD_R) * (len - 1) as f64).round();
            set.emit(Some(i.clamp(0.0, (len - 1) as f64) as usize));
        })
    };
    let leave = {
        let set = p.set_hover.clone();
        Callback::from(move |_: PointerEvent| set.emit(None))
    };
    let year = h.points.len() > 200;
    let label = format!(
        "Mes lignes {} sur la période{}",
        pct(h.change),
        h.benchmarks.iter().map(|b| format!(", {} {}", b.label, pct(b.change))).collect::<String>()
    );
    html! {
        <div class="history-chart">
            <p class="history-read small" aria-live="polite">
                <b>{ utc_day(h.points[at].t, false) }</b>
                { format!(" · Mes lignes {} ({})", usd(h.points[at].value), pct(mine[at])) }
                { for h.benchmarks.iter().map(|b| html! {
                    <span key={b.id.clone()}>{ format!(" · {} {}", b.label.split(" (").next().unwrap_or(&b.label), pct(b.points[at].pct)) }</span>
                }) }
            </p>
            <svg viewBox={format!("0 0 {W} {H}")} role="img" aria-label={label} onpointermove={pick.clone()} onpointerdown={pick} onpointerleave={leave}>
                <line x1={attr(PAD_L)} x2={attr(W - PAD_R)} y1={attr(y(0.0))} y2={attr(y(0.0))} class="zero" />
                <text x={attr(PAD_L - 4.0)} y={attr(y(hi) + 4.0)} class="axis" text-anchor="end">{ pct(hi) }</text>
                <text x={attr(PAD_L - 4.0)} y={attr(y(lo))} class="axis" text-anchor="end">{ pct(lo) }</text>
                <text x={attr(PAD_L)} y={attr(H - 4.0)} class="axis">{ utc_day(h.points[0].t, year) }</text>
                <text x={attr(W - PAD_R)} y={attr(H - 4.0)} class="axis" text-anchor="end">{ utc_day(h.points[len - 1].t, year) }</text>
                { for h.benchmarks.iter().map(|b| html! {
                    <path key={b.id.clone()} d={line_path(&b.points.iter().map(|x| x.pct).collect::<Vec<_>>(), x, y)} stroke={color(&b.id)} class="bench" />
                }) }
                <path d={line_path(&mine, x, y)} stroke={PORTFOLIO} class="mine" />
                if p.hover.is_some() {
                    <line x1={attr(x(at))} x2={attr(x(at))} y1={attr(PAD_T)} y2={attr(H - PAD_B)} class="cross" />
                    <circle cx={attr(x(at))} cy={attr(y(mine[at]))} r="4" fill={PORTFOLIO} class="dot" />
                }
            </svg>
        </div>
    }
}
