//! « Si j'avais investi » (DcaCard.tsx, asset screen): one purchase by default (the user does not want to spend every
//! month), regular purchases as an option, replayed on the real daily closes of the asset.
use std::cell::Cell;
use std::rc::Rc;

use altim_core::engine::history::Close;
use altim_core::types::Kind;
use altim_core::web::money::{Currency, NBSP};
use altim_core::web::portfolio::dca::{DcaResult, ONCE, simulate_dca};
use altim_core::web::portfolio::view::parse_decimal;
use yew::prelude::*;

use super::browser::{input_value, utc_day};
use super::history::{line_path, pct};
use crate::ui::Segmented;

fn usd(v: f64) -> String {
    crate::money::money_with(v, 0, 0, NBSP)
}

fn date(t: i64) -> String {
    utc_day(t, true)
}

fn options(o: &[(&'static str, &'static str)]) -> Vec<(AttrValue, AttrValue)> {
    o.iter().map(|(v, l)| (AttrValue::from(*v), AttrValue::from(*l))).collect()
}

#[derive(Properties, PartialEq)]
pub struct DcaCardProps {
    pub symbol: AttrValue,
    pub kind: Kind,
}

#[component]
pub fn DcaCard(p: &DcaCardProps) -> Html {
    let m = crate::money::use_money();
    let amount_text = use_state(|| "1000".to_string());
    let every = use_state(|| "once".to_string());
    let period = use_state(|| "365".to_string());
    let once = *every == "once";
    let closes = use_state(|| None::<Rc<Vec<Close>>>);
    let error = use_state(|| None::<String>);

    {
        let (closes, error) = (closes.clone(), error.clone());
        use_effect_with((p.symbol.clone(), p.kind, (*period).clone()), move |(symbol, kind, period)| {
            let alive = Rc::new(Cell::new(true));
            closes.set(None);
            let (a, symbol, kind) = (alive.clone(), symbol.to_string(), *kind);
            let days = if period == "730" { 730 } else { 365 };
            wasm_bindgen_futures::spawn_local(async move {
                let r = super::api::history(&[(symbol.clone(), kind)], days).await;
                if !a.get() {
                    return;
                }
                match r {
                    Ok(r) => {
                        let c = r.series.into_iter().find(|s| s.symbol == symbol && s.kind == kind).map(|s| s.closes).unwrap_or_default();
                        closes.set(Some(Rc::new(c)));
                        error.set(None);
                    }
                    Err(e) => error.set(Some(e.0)),
                }
            });
            move || alive.set(false)
        });
    }

    // Typed in the display currency, replayed in dollars on the dollar closes.
    let amount = m.from_display(parse_decimal(&amount_text));
    let every_days = if once { ONCE } else { every.parse().unwrap_or(ONCE) };
    let r = (*closes)
        .as_ref()
        .filter(|_| amount > 0.0)
        .and_then(|c| simulate_dca(c, amount, every_days, period.parse().unwrap_or(365), js_sys::Date::now() as i64));
    let set = |h: &UseStateHandle<String>| {
        let h = h.clone();
        Callback::from(move |v: AttrValue| h.set(v.to_string()))
    };
    let on_amount = {
        let a = amount_text.clone();
        Callback::from(move |e: InputEvent| a.set(input_value(&e)))
    };
    let sym = m.symbol();
    html! {
        <div class="card dca-card">
            <h2 class="card-title">{ "Si j'avais investi" }</h2>
            <label class="field">
                <span>{ if once { format!("Montant investi ({sym})") } else { format!("Montant par achat ({sym})") } }</span>
                <input inputmode="decimal" value={(*amount_text).clone()} oninput={on_amount} />
            </label>
            <Segmented label="Achat" value={AttrValue::from((*every).clone())} options={options(&[("once", "Une fois"), ("7", "Chaque semaine"), ("30", "Chaque mois")])} on_change={set(&every)} />
            <Segmented label="Depuis" value={AttrValue::from((*period).clone())} options={options(&[("182", "6 mois"), ("365", "1 an"), ("730", "2 ans")])} on_change={set(&period)} />
            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if closes.is_none() && error.is_none() {
                <p class="muted small">{ "Chargement de l'historique…" }</p>
            }
            if closes.is_some() && r.is_none() {
                <p class="muted small">
                    { if amount > 0.0 { format!("Pas assez d'historique pour {} sur cette période.", p.symbol) } else { "Indiquez un montant.".into() } }
                </p>
            }
            if let Some(r) = r {
                <p class="kv">
                    <span>{ if once { format!("{} investis le {}", usd(r.invested), date(r.first)) } else { format!("{} achats · {} investis", altim_core::js::number_to_string(r.buys), usd(r.invested)) } }</span>
                    <b class={if r.gain >= 0.0 { "up" } else { "down" }}>{ format!("{} ({})", usd(r.value), pct(r.gain)) }</b>
                </p>
                <DcaChart r={Rc::new(r.clone())} />
                if !once {
                    <p class="kv small">
                        <span>{ format!("Tout investi le {}", date(r.first)) }</span>
                        <b class={if r.lump_sum.gain >= 0.0 { "up" } else { "down" }}>{ format!("{} ({})", usd(r.lump_sum.value), pct(r.lump_sum.gain)) }</b>
                    </p>
                }
                <p class="kv small"><span>{ if once { "Prix d'achat" } else { "Prix moyen payé" } }</span><b>{ crate::money::price(r.average_price) }</b></p>
                <p class="kv small"><span>{ "Prix à la dernière clôture" }</span><b>{ crate::money::price(r.last_price) }</b></p>
                if m.currency() == Currency::Eur {
                    <p class="muted small">{ "Rejoué en $ sur les cours en dollars, puis converti au taux du jour : l'effet de change passé (EUR/USD) n'est pas compté." }</p>
                }
                <p class="muted small">
                    { if once {
                        "Un seul achat, à la clôture de ce jour-là."
                    } else if r.lump_sum.gain > r.gain {
                        "Sur cette période, tout acheter le premier jour a mieux rendu : le prix a surtout monté."
                    } else if r.lump_sum.gain < r.gain {
                        "Sur cette période, étaler les achats a mieux rendu : ils ont profité des baisses."
                    } else {
                        "Sur cette période, les deux façons d'investir reviennent au même."
                    } }
                    { " Rejoué sur les vraies clôtures journalières, sans frais ni impôts ; le passé ne dit pas ce qui arrivera." }
                </p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct DcaChartProps {
    r: Rc<DcaResult>,
}

#[component]
fn DcaChart(p: &DcaChartProps) -> Html {
    let (w, h) = (320.0, 110.0);
    let r = &p.r;
    let max = r.path.iter().map(|x| x.value.max(x.invested)).fold(1.0, f64::max);
    let len = r.path.len();
    let x = move |i: usize| i as f64 / (len - 1) as f64 * w;
    let y = move |v: f64| 6.0 + (1.0 - v / max) * (h - 12.0);
    let invested: Vec<f64> = r.path.iter().map(|x| x.invested).collect();
    let value: Vec<f64> = r.path.iter().map(|x| x.value).collect();
    html! {
        <div class="history-chart">
            <svg viewBox={format!("0 0 {w} {h}")} role="img" aria-label={format!("{} investis, valeur {}", usd(r.invested), usd(r.value))}>
                <path d={line_path(&invested, x, y)} stroke="rgba(255,255,255,0.45)" stroke-dasharray="4 3" class="bench" />
                <path d={line_path(&value, x, y)} stroke="#3987e5" class="mine" />
            </svg>
            <ul class="history-legend">
                <li><i style="background: #3987e5;" />{ "Valeur" }</li>
                <li><i style="background: rgba(255,255,255,0.45);" />{ "Somme investie" }</li>
            </ul>
        </div>
    }
}
