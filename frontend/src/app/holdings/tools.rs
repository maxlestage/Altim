//! Decision tools that only compute (ToolCards.tsx): comparison of 2 to 4 assets (`CompareCard`, Radar), position
//! size (`PositionCard`, asset screen), sale after fees and tax, projection and rebalancing (Mes avoirs). Amounts are
//! typed and shown in the display currency, computed in dollars.
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::history::Close;
use altim_core::js::{fr, number_to_string, round};
use altim_core::types::Kind;
use altim_core::web::money::NBSP;
use altim_core::web::portfolio::holdings::PortfolioAnalysis;
use altim_core::web::portfolio::tools::{
    ASSET_CLASSES, AssetClass, ByClass, Comparison, PROJECTION_RATES, PositionInput, RebalanceLine, SaleLine, compare_assets, position_size,
    projection, rebalance, sale_total,
};
use altim_core::web::portfolio::view::{REBALANCE_KEY, parse_decimal, parse_rebalance, plain, rebalance_json};
use yew::prelude::*;

use super::browser::input_value;
use super::history::{attr, line_path};
use crate::state::{local_get, local_set};
use crate::ui::Segmented;
use altim_core::web::sorting::Sorting;

fn usd(v: f64) -> String {
    crate::money::money_with(v, 0, 0, NBSP)
}

/// "+12,3 %" with a no-break space before "%" (ToolCards.tsx; the history card's has a plain space).
fn pct(v: f64) -> String {
    format!("{}{}\u{a0}%", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}

/// `Number(s.replace(/[\s ]/g, "").replace(",", "."))`.
fn num(s: &str) -> f64 {
    parse_decimal(s)
}

/// `num(s) || 0`: NaN and 0 give 0.
fn num0(s: &str) -> f64 {
    let v = num(s);
    if v.is_nan() { 0.0 } else { v }
}

fn options(o: &[(&'static str, &'static str)]) -> Vec<(AttrValue, AttrValue)> {
    o.iter().map(|(v, l)| (AttrValue::from(*v), AttrValue::from(*l))).collect()
}

fn text_input(h: &UseStateHandle<String>) -> Callback<InputEvent> {
    let h = h.clone();
    Callback::from(move |e: InputEvent| h.set(input_value(&e)))
}

// Categorical palette (fixed order, validated on the dark surface). The 4th is close to the 1st for deuteranopes
// (ΔE 6,4, allowed with a second cue): its line is dashed.
const COLORS: [&str; 4] = ["#3987e5", "#d95926", "#199e70", "#c24ec9"];

// ---------- Comparison ----------

#[derive(Properties, PartialEq)]
pub struct CompareCardProps {
    /// Assets of the radar (symbol, kind).
    pub assets: Vec<(String, Kind)>,
}

/// 2 to 4 assets of the radar over the same days: change, volatility, worst fall, correlation.
#[component]
pub fn CompareCard(p: &CompareCardProps) -> Html {
    let picked = use_state(|| p.assets.iter().take(2).map(|(s, k)| altim_core::web::store::asset_key(s, *k)).collect::<Vec<String>>());
    let days = use_state(|| "90".to_string());
    let series = use_state(|| None::<Rc<HashMap<String, Vec<Close>>>>);
    let error = use_state(|| None::<String>);
    let mut sorted = (*picked).clone();
    sorted.sort_dyn();
    let key = sorted.join(",");

    {
        let (series, error, picked) = (series.clone(), error.clone(), (*picked).clone());
        use_effect_with((key, (*days).clone()), move |(_, days)| {
            let alive = Rc::new(Cell::new(true));
            if picked.len() < 2 {
                series.set(None);
            } else {
                series.set(None);
                let items: Vec<(String, Kind)> =
                    picked.iter().filter_map(|id| id.split_once(':').and_then(|(k, s)| Kind::parse(k).map(|k| (s.to_string(), k)))).collect();
                let (a, d) = (alive.clone(), days.parse().unwrap_or(90));
                wasm_bindgen_futures::spawn_local(async move {
                    let r = super::api::history(&items, d).await;
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
            }
            move || alive.set(false)
        });
    }

    let c = (*series).as_ref().and_then(|s| compare_assets(s, &picked, days.parse().unwrap_or(90), js_sys::Date::now() as i64));
    let name = |id: &str| id.split(':').nth(1).unwrap_or(id).to_string();
    let color_of = |id: &str| picked.iter().position(|x| x == id).map(|i| COLORS[i]).unwrap_or("undefined");
    let on_days = {
        let days = days.clone();
        Callback::from(move |v: AttrValue| days.set(v.to_string()))
    };
    html! {
        <div class="card compare-card">
            <h2 class="card-title">{ "Comparer" }</h2>
            <p class="muted small">{ "Choisissez 2 à 4 actifs de votre radar." }</p>
            <div class="news-tags">
                { for p.assets.iter().map(|(symbol, kind)| {
                    let id = altim_core::web::store::asset_key(symbol, *kind);
                    let i = picked.iter().position(|x| *x == id);
                    let toggle = {
                        let (picked, id) = (picked.clone(), id.clone());
                        Callback::from(move |_: MouseEvent| {
                            let mut p = (*picked).clone();
                            if p.contains(&id) {
                                p.retain(|x| *x != id);
                            } else if p.len() < 4 {
                                p.push(id.clone());
                            }
                            picked.set(p);
                        })
                    };
                    html! {
                        <button
                            key={id.clone()}
                            class={classes!("chip", "pick", i.is_some().then_some("on"))}
                            aria-pressed={i.is_some().to_string()}
                            onclick={toggle}
                            style={i.map(|i| format!("border-color: {};", COLORS[i]))}
                        >
                            { symbol.clone() }
                        </button>
                    }
                }) }
            </div>
            <Segmented label="Période" value={AttrValue::from((*days).clone())} options={options(&[("30", "30 j"), ("90", "90 j"), ("365", "1 an")])} on_change={on_days} />
            if picked.len() < 2 {
                <p class="muted small">{ "Sélectionnez au moins 2 actifs." }</p>
            }
            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if picked.len() >= 2 && series.is_none() && error.is_none() {
                <p class="muted small">{ "Chargement de l'historique…" }</p>
            }
            if let Some(c) = c {
                // One colour per asset, in the order of the stats (an asset without history is left out).
                <CompareChart colors={c.stats.iter().map(|s| color_of(&s.id)).collect::<Vec<_>>()} c={Rc::new(c.clone())} />
                <div class="table-scroll">
                    <table class="compare-table small">
                        <thead>
                            <tr><th>{ "Actif" }</th><th>{ "Variation" }</th><th>{ "Volatilité" }</th><th>{ "Pire recul" }</th></tr>
                        </thead>
                        <tbody>
                            { for c.stats.iter().map(|s| html! {
                                <tr key={s.id.clone()}>
                                    <td><i class="dot" style={format!("background: {};", color_of(&s.id))} />{ name(&s.id) }</td>
                                    <td data-label="Variation" class={if s.change >= 0.0 { "up" } else { "down" }}>{ pct(s.change) }</td>
                                    <td data-label="Volatilité">{ format!("{}\u{a0}%/an", number_to_string(round(s.volatility))) }</td>
                                    <td data-label="Pire recul" class="down">{ pct(s.max_drawdown) }</td>
                                </tr>
                            }) }
                        </tbody>
                    </table>
                </div>
                if c.stats.len() >= 2 {
                    <p class="muted small">
                        { "Corrélation : " }
                        { c.stats.iter().enumerate().flat_map(|(i, a)| c.stats[i + 1..].iter().enumerate().map(move |(j, b)| (i, j, a, b))).map(|(i, j, a, b)| {
                            let v = c.correlation[i][i + 1 + j];
                            format!("{}/{} {}", name(&a.id), name(&b.id), v.map(|v| fr(v, 0, 2)).unwrap_or_else(|| "—".into()))
                        }).collect::<Vec<_>>().join(" · ") }
                        { ". Proche de 1 : ils montent et baissent ensemble (peu de diversification) ; proche de 0 : indépendants." }
                    </p>
                }
                <p class="muted small">
                    { format!(
                        "Mêmes jours pour tous{}. Volatilité = écart type annualisé des variations journalières. Le passé ne dit pas ce qui arrivera.",
                        if c.missing.is_empty() { String::new() } else { format!(" (sans historique : {})", c.missing.iter().map(|m| name(m)).collect::<Vec<_>>().join(", ")) }
                    ) }
                </p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct CompareChartProps {
    c: Rc<Comparison>,
    colors: Vec<&'static str>,
}

#[component]
fn CompareChart(p: &CompareChartProps) -> Html {
    let (w, h) = (320.0, 130.0);
    let c = &p.c;
    let mut all = vec![0.0];
    all.extend(c.stats.iter().flat_map(|s| s.pct.iter().copied()));
    let lo = all.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if hi - lo == 0.0 { 1.0 } else { hi - lo };
    let len = c.days.len();
    let x = move |i: usize| 34.0 + i as f64 / (len.max(2) - 1) as f64 * (w - 40.0);
    let y = move |v: f64| 6.0 + (1.0 - (v - lo) / span) * (h - 12.0);
    let label = c.stats.iter().map(|s| format!("{} {}", s.id.split(':').nth(1).unwrap_or(&s.id), pct(s.change))).collect::<Vec<_>>().join(", ");
    html! {
        <div class="history-chart">
            <svg viewBox={format!("0 0 {w} {h}")} role="img" aria-label={label}>
                <line x1="34" x2={attr(w - 6.0)} y1={attr(y(0.0))} y2={attr(y(0.0))} class="zero" />
                <text x="30" y={attr(y(hi) + 4.0)} class="axis" text-anchor="end">{ pct(hi) }</text>
                <text x="30" y={attr(y(lo))} class="axis" text-anchor="end">{ pct(lo) }</text>
                { for c.stats.iter().enumerate().map(|(k, s)| {
                    let color = p.colors.get(k).copied().unwrap_or("undefined");
                    html! {
                        <path key={s.id.clone()} d={line_path(&s.pct, x, y)} stroke={color} class="mine" stroke-dasharray={(color == COLORS[3]).then_some("6 4")} />
                    }
                }) }
            </svg>
        </div>
    }
}

// ---------- Position size ----------

#[derive(Properties, PartialEq)]
pub struct PositionCardProps {
    pub symbol: AttrValue,
    pub price: Option<f64>,
    pub stop: Option<f64>,
    pub stop_source: AttrValue,
    pub target: Option<f64>,
    pub capital: f64,
    pub risk_pct: f64,
}

/// How much to buy so that hitting the stop costs the chosen share of the capital.
#[component]
pub fn PositionCard(p: &PositionCardProps) -> Html {
    let _m = crate::money::use_money();
    // Capital, stop and target are shown and typed in the display currency, sized in dollars.
    let capital_of = |c: f64| if c > 0.0 { number_to_string(round(crate::money::to_display(c))) } else { String::new() };
    let price_of = |v: Option<f64>| v.filter(|v| *v != 0.0).map(|v| plain(crate::money::to_display(v))).unwrap_or_default();
    let capital_text = use_state(|| capital_of(p.capital));
    let risk_text = use_state(|| number_to_string(p.risk_pct).replace('.', ","));
    let stop_text = use_state(|| price_of(p.stop));
    let target_text = use_state(|| price_of(p.target));
    {
        let (c, s, t) = (capital_text.clone(), stop_text.clone(), target_text.clone());
        use_effect_with((p.stop, p.target, p.capital), move |&(stop, target, capital)| {
            if s.is_empty() && stop.is_some_and(|v| v != 0.0) {
                s.set(price_of(stop));
            }
            if t.is_empty() && target.is_some_and(|v| v != 0.0) {
                t.set(price_of(target));
            }
            if c.is_empty() && capital > 0.0 {
                c.set(capital_of(capital));
            }
        });
    }
    let from = crate::money::from_display;
    let size = p.price.filter(|v| *v != 0.0).and_then(|price| {
        position_size(&PositionInput {
            capital: from(num(&capital_text)),
            risk_pct: num(&risk_text),
            entry: price,
            stop: from(num(&stop_text)),
            target: (!target_text.is_empty()).then(|| from(num(&target_text))),
        })
    });
    let sym = crate::money::symbol();
    html! {
        <div class="card position-card">
            <h2 class="card-title">{ "Taille de position" }</h2>
            <div class="grid-2">
                <label class="field"><span>{ format!("Capital ({sym})") }</span><input inputmode="decimal" value={(*capital_text).clone()} oninput={text_input(&capital_text)} /></label>
                <label class="field"><span>{ "Risque accepté (%)" }</span><input inputmode="decimal" value={(*risk_text).clone()} oninput={text_input(&risk_text)} /></label>
                <label class="field"><span>{ format!("Stop ({sym})") }</span><input inputmode="decimal" value={(*stop_text).clone()} oninput={text_input(&stop_text)} /></label>
                <label class="field"><span>{ format!("Objectif ({sym}, facultatif)") }</span><input inputmode="decimal" value={(*target_text).clone()} oninput={text_input(&target_text)} /></label>
            </div>
            <p class="muted small">
                { format!(
                    "Entrée au prix actuel {} ; stop proposé : {}.",
                    p.price.filter(|v| *v != 0.0).map(crate::money::price).unwrap_or_else(|| "…".into()),
                    p.stop_source
                ) }
            </p>
            if size.is_none() && p.price.is_some_and(|v| v != 0.0) {
                <p class="muted small">{ "Le stop doit être sous le prix d'entrée, et le capital et le risque positifs." }</p>
            }
            if let Some(s) = size {
                <p class="kv"><span>{ "Acheter" }</span><b>{ format!("{} {} · {}", fr(s.quantity, 0, if s.quantity >= 1.0 { 2 } else { 6 }), p.symbol, usd(s.amount)) }</b></p>
                <p class="kv small"><span>{ "Part du capital" }</span><b>{ format!("{}\u{a0}%", fr(s.capital_share, 0, 1)) }</b></p>
                <p class="kv small"><span>{ format!("Perte si le stop est touché ({})", pct(-s.stop_distance)) }</span><b class="down">{ format!("−{}", usd(s.risk)) }</b></p>
                if let (Some(reward), Some(ratio)) = (s.reward, s.ratio) {
                    <p class="kv small"><span>{ "Gain à l'objectif · rapport gain/risque" }</span><b class="up">{ format!("+{} · {} R", usd(reward), fr(ratio, 0, 1)) }</b></p>
                }
                if s.capped {
                    <p class="notice warn small">{ "Stop très proche : la taille est limitée à votre capital, la perte au stop reste sous le risque choisi." }</p>
                }
                if s.ratio.is_some_and(|r| r < 1.5) {
                    <p class="notice warn small">{ "Rapport gain/risque sous 1,5 : l'idée rapporte peu au regard du risque." }</p>
                }
                <p class="muted small">{ "Calcul, pas conseil : un écart de prix (gap) peut faire perdre plus que prévu au stop. Altim ne passe aucun ordre." }</p>
            }
        </div>
    }
}

// ---------- Sale after fees and tax ----------

#[derive(Properties, PartialEq)]
pub struct AnalysisProps {
    pub analysis: Rc<PortfolioAnalysis>,
}

/// What selling would leave once the fees and the flat tax on the gains are paid.
#[component]
pub fn SaleCard(p: &AnalysisProps) -> Html {
    let _m = crate::money::use_money();
    let tax_text = use_state(|| "30".to_string());
    let fee_text = use_state(|| "0,1".to_string());
    let a = &p.analysis;
    let lines: Vec<SaleLine> = a
        .lines
        .iter()
        .filter(|l| l.value > 0.0)
        .map(|l| SaleLine { id: l.id.clone(), value: l.value, cost: (l.invested > 0.0).then_some(l.invested) })
        .collect();
    let t = sale_total(&lines, num0(&tax_text), num0(&fee_text));
    if lines.is_empty() {
        return html! {};
    }
    let symbol = |id: &str| a.lines.iter().find(|l| l.id == id).map(|l| l.symbol.clone()).unwrap_or_else(|| id.to_string());
    html! {
        <div class="card sale-card">
            <h2 class="card-title">{ "Si je vendais" }</h2>
            <div class="grid-2">
                <label class="field"><span>{ "Impôt sur la plus-value (%)" }</span><input inputmode="decimal" value={(*tax_text).clone()} oninput={text_input(&tax_text)} /></label>
                <label class="field"><span>{ "Frais de vente (%)" }</span><input inputmode="decimal" value={(*fee_text).clone()} oninput={text_input(&fee_text)} /></label>
            </div>
            <div class="table-scroll">
                <table class="compare-table small">
                    <thead>
                        <tr><th>{ "Ligne" }</th><th>{ "Valeur" }</th><th>{ "Plus-value" }</th><th>{ "Impôt" }</th><th>{ "Net" }</th></tr>
                    </thead>
                    <tbody>
                        { for t.lines.iter().map(|(id, l)| html! {
                            <tr key={id.clone()}>
                                <td>{ symbol(id) }</td>
                                <td data-label="Valeur">{ usd(l.gross) }</td>
                                <td data-label="Plus-value" class={match l.gain { None => "muted", Some(g) if g >= 0.0 => "up", _ => "down" }}>
                                    { match l.gain { None => "—".to_string(), Some(g) => format!("{}{}", if g >= 0.0 { "+" } else { "−" }, usd(g.abs())) } }
                                </td>
                                <td data-label="Impôt">{ if l.tax > 0.0 { format!("−{}", usd(l.tax)) } else { usd(0.0) } }</td>
                                <td data-label="Net"><b>{ usd(l.net) }</b></td>
                            </tr>
                        }) }
                    </tbody>
                </table>
            </div>
            <p class="kv"><span>{ "Tout vendre : vous garderiez" }</span><b>{ usd(t.total.net) }</b></p>
            <p class="kv small">
                <span>{ format!("Frais {} · impôt {}", usd(t.total.fees), usd(t.total.tax)) }</span>
                <b class={match t.total.gain { None => "", Some(g) if g >= 0.0 => "up", _ => "down" }}>
                    { t.total.gain.map(|g| format!("plus-value nette {}{}", if g >= 0.0 { "+" } else { "−" }, usd(g.abs()))).unwrap_or_default() }
                </b>
            </p>
            <p class="muted small">
                { format!(
                    "30 % = prélèvement forfaitaire unique en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux) ; les pertes de l'année compensent les gains. Cryptos : l'impôt se calcule sur l'ensemble du portefeuille à chaque cession et les cessions de moins de 305 € par an sont exonérées, donc ce calcul ligne par ligne est une estimation.{} Vérifiez votre situation (PEA, assurance-vie, option barème…) ; Altim ne passe aucun ordre.",
                    if t.unknown_cost > 0 { format!(" {} ligne(s) sans prix d'achat : plus-value non calculée.", t.unknown_cost) } else { String::new() }
                ) }
            </p>
        </div>
    }
}

// ---------- Projection ----------

#[derive(Properties, PartialEq)]
pub struct ProjectionCardProps {
    /// Today's total (USD).
    pub start: f64,
}

/// What the portfolio plus a monthly contribution would become, under three yearly returns (hypotheses).
#[component]
pub fn ProjectionCard(p: &ProjectionCardProps) -> Html {
    let _m = crate::money::use_money();
    let monthly_text = use_state(|| "0".to_string());
    let years = use_state(|| "10".to_string());
    let monthly = {
        let v = crate::money::from_display(num0(&monthly_text));
        if v.is_nan() { 0.0 } else { v }
    };
    let y: f64 = years.parse().unwrap_or(10.0);
    let runs: Vec<(f64, f64)> =
        PROJECTION_RATES.iter().map(|r| (*r, projection(p.start, monthly, y, *r).last().map(|x| x.value).unwrap_or(0.0))).collect();
    let paid = projection(p.start, monthly, y, PROJECTION_RATES[0]).last().map(|x| x.paid).unwrap_or(0.0);
    let on_years = {
        let years = years.clone();
        Callback::from(move |v: AttrValue| years.set(v.to_string()))
    };
    html! {
        <div class="card projection-card">
            <h2 class="card-title">{ "Projection" }</h2>
            <label class="field">
                <span>{ format!("Versement chaque mois, facultatif ({})", crate::money::symbol()) }</span>
                <input inputmode="decimal" value={(*monthly_text).clone()} oninput={text_input(&monthly_text)} />
            </label>
            <Segmented label="Durée" value={AttrValue::from((*years).clone())} options={options(&[("5", "5 ans"), ("10", "10 ans"), ("20", "20 ans")])} on_change={on_years} />
            <p class="kv small">
                <span>{ if monthly > 0.0 { format!("Aujourd'hui {} + versements", usd(p.start)) } else { "Vos avoirs aujourd'hui, sans rien ajouter".into() } }</span>
                <b>{ format!("{}{}", usd(paid), if monthly > 0.0 { " versés" } else { "" }) }</b>
            </p>
            { for runs.iter().map(|(rate, end)| {
                let gain = end - paid;
                html! {
                    <p key={number_to_string(*rate)} class="kv">
                        <span>{ format!("Si {} % par an", number_to_string(*rate)) }</span>
                        <b>{ format!("{} ", usd(*end)) }<small class={if gain >= 0.0 { "up" } else { "down" }}>{ format!("({}{})", if gain >= 0.0 { "+" } else { "−" }, usd(gain.abs())) }</small></b>
                    </p>
                }
            }) }
            <p class="muted small">
                { "Trois hypothèses de rendement à comparer, pas des prévisions : une année peut perdre 30 % ou plus (cryptos : davantage), et l'inflation réduit ce que ces montants achèteront. Sans frais ni impôts." }
            </p>
        </div>
    }
}

// ---------- Rebalancing ----------

fn class_label(k: AssetClass) -> &'static str {
    match k {
        AssetClass::Crypto => "Cryptos",
        AssetClass::Stock => "Actions",
        AssetClass::Cash => "Liquidités",
    }
}

/// Buys and sells to reach a target split crypto / stocks / cash (the target is remembered in "altim.rebalance").
#[component]
pub fn RebalanceCard(p: &AnalysisProps) -> Html {
    let _m = crate::money::use_money();
    let target = use_state(|| parse_rebalance(local_get(REBALANCE_KEY).as_deref()));
    let a = &p.analysis;
    let t = ByClass { crypto: num0(&target[0]), stock: num0(&target[1]), cash: num0(&target[2]) };
    let sum = t.crypto + t.stock + t.cash;
    let lines: Vec<RebalanceLine> = a.lines.iter().map(|l| RebalanceLine { id: l.id.clone(), kind: l.kind, value: l.value }).collect();
    let r = rebalance(&lines, a.cash, &t);
    let total = r.as_ref().map(|r| r.total).unwrap_or(0.0);
    let small = move |v: f64| v.abs() < (total * 0.01).max(10.0);
    html! {
        <div class="card rebalance-card">
            <h2 class="card-title">{ "Rééquilibrer" }</h2>
            <div class="grid-3">
                { for ASSET_CLASSES.iter().enumerate().map(|(i, k)| {
                    let set = {
                        let target = target.clone();
                        Callback::from(move |e: InputEvent| {
                            let mut next = (*target).clone();
                            next[i] = input_value(&e);
                            // Private browsing: not remembered.
                            local_set(REBALANCE_KEY, &rebalance_json(&next));
                            target.set(next);
                        })
                    };
                    html! {
                        <label key={k.key()} class="field">
                            <span>{ format!("{} (%)", class_label(*k)) }</span>
                            <input inputmode="decimal" value={target[i].clone()} oninput={set} />
                        </label>
                    }
                }) }
            </div>
            if (sum - 100.0).abs() > 0.01 {
                <p class="notice warn small">{ format!("La cible fait {} % : elle doit faire 100 %.", fr(sum, 0, 3)) }</p>
            }
            if let Some(r) = &r {
                { for ASSET_CLASSES.iter().map(|k| {
                    let mv = r.moves.get(*k);
                    let what = if mv > 0.0 { if *k == AssetClass::Cash { "mettre de côté" } else { "acheter" } } else if *k == AssetClass::Cash { "investir" } else { "vendre" };
                    html! {
                        <p key={k.key()} class="kv small">
                            <span>{ format!("{} : {} % → {} %", class_label(*k), fr(r.current.get(*k), 0, 0), fr(t.get(*k), 0, 3)) }</span>
                            <b class={if small(mv) { "" } else if mv > 0.0 { "up" } else { "down" }}>
                                { if small(mv) { "rien à faire".to_string() } else { format!("{what} {}", usd(mv.abs())) } }
                            </b>
                        </p>
                    }
                }) }
                if r.lines.iter().any(|(_, v)| !small(*v)) {
                    <ul class="rebalance-lines small">
                        { for r.lines.iter().filter(|(_, v)| !small(*v)).map(|(id, v)| {
                            let symbol = a.lines.iter().find(|x| x.id == *id).map(|x| x.symbol.clone()).unwrap_or_else(|| "undefined".into());
                            html! { <li key={id.clone()}>{ format!("{} {} de {symbol}", if *v > 0.0 { "Acheter" } else { "Vendre" }, usd(v.abs())) }</li> }
                        }) }
                    </ul>
                }
                <p class="muted small">
                    { format!(
                        "Réparti au prorata de vos lignes actuelles{}. Avant de vendre, pensez aux frais et à l'impôt sur les plus-values. Altim ne passe aucun ordre.",
                        if r.moves.crypto > 0.0 && !a.lines.iter().any(|l| l.kind == Kind::Crypto) { " (aucune crypto détenue : à répartir vous-même)" } else { "" }
                    ) }
                </p>
            }
        </div>
    }
}
