//! Editing one existing line (`HoldingForm` of MyHoldings.tsx; new lines are added with AddHoldings). Amounts are
//! typed in the display currency; a field left untouched keeps its saved value and currency (a dollar cost basis is
//! not silently re-based in euros).
use altim_core::js::number_to_string;
use altim_core::web::money::Currency;
use altim_core::web::portfolio::view::{cost_note, input_text, parse_decimal};
use altim_core::web::store::{Holding, StoredHolding, shown};
use yew::prelude::*;

use super::browser::{input_value, stop, use_auto_focus};
use crate::state::holdings::set_holdings;

#[derive(Properties, PartialEq)]
pub struct HoldingFormProps {
    /// The line as saved (its own currencies).
    pub stored: StoredHolding,
    /// The same line in dollars.
    pub initial: Holding,
    /// Price of the line in the analysis (the journal's fallback when the consensus quote fails).
    pub last_price: Option<f64>,
    pub on_close: Callback<()>,
}

/// `upsertHolding` of an existing line: its fields replaced, anything else it carries kept (a cleared stop drops its
/// currency tag too).
fn update_line(id: &str, quantity: f64, average_price: f64, cost: Option<Currency>, stop: Option<f64>, stop_cur: Option<Currency>) {
    let id = id.to_string();
    set_holdings(move |s| {
        for h in s.holdings.iter_mut().filter(|h| h.id == id) {
            h.quantity = quantity;
            h.average_price = average_price;
            h.cost_currency = cost;
            h.stop = stop;
            h.stop_currency = stop.and(stop_cur);
        }
    });
}

#[component]
pub fn HoldingForm(p: &HoldingFormProps) -> Html {
    let m = crate::money::use_money();
    let cur = m.currency();
    let sym = m.symbol();
    let (stored, initial) = (&p.stored, &p.initial);
    let pru_initial = input_text(shown(stored.average_price, stored.cost_currency, &m));
    let stop_initial = stored.stop.filter(|s| *s != 0.0).map(|s| input_text(shown(s, stored.stop_currency, &m))).unwrap_or_default();
    let qty = use_state(|| number_to_string(initial.quantity));
    let pru = use_state(|| pru_initial.clone());
    let stop_text = use_state(|| stop_initial.clone());
    let price = use_state(|| None::<f64>);
    let focus = use_auto_focus(true);
    {
        let price = price.clone();
        use_effect_with((initial.symbol.clone(), initial.kind), move |(symbol, kind)| {
            let item = vec![(symbol.clone(), *kind)];
            wasm_bindgen_futures::spawn_local(async move {
                price.set(super::api::quotes(&item).await.ok().and_then(|r| r.first().map(|q| q.price)));
            });
        });
    }

    let quantity = parse_decimal(&qty);
    let keep_cost = *pru == pru_initial;
    let keep_stop = *stop_text == stop_initial;
    // What is saved (typed currency), and the same in dollars for the engines.
    let average_price = if keep_cost { stored.average_price } else { parse_decimal(&pru) };
    let cost_currency = if keep_cost { stored.cost_currency } else { Some(cur) };
    let stop_v = if keep_stop {
        stored.stop
    } else if stop_text.trim().is_empty() {
        None
    } else {
        Some(parse_decimal(&stop_text))
    };
    let stop_currency = if keep_stop { stored.stop_currency } else { Some(cur) };
    let valid = quantity > 0.0 && average_price.is_finite() && average_price >= 0.0 && stop_v.is_none_or(|s| s.is_finite() && s > 0.0);
    let cost_usd = if keep_cost { initial.average_price } else { m.from_display(average_price) };
    // TODO(phase 3): D `holdingChange` (engine/journal.ts, from `initial`, the new quantity and `cost_usd`, priced at
    // the consensus `price` or `p.last_price`) → `crate::app::journal::JournalToggle` (checked by default, with a note)
    // and `altim_core::web::trading::journal_store` (`recordRealTrade`, stop in dollars for a buy) on save:
    // « Achat / Vente de … à … : l'inscrire au journal ».

    let save = {
        let (id, on_close) = (initial.id.clone(), p.on_close.clone());
        Callback::from(move |_: MouseEvent| {
            if !valid {
                return;
            }
            update_line(&id, quantity, average_price, cost_currency, stop_v, stop_currency);
            on_close.emit(());
        })
    };
    let on_close = {
        let c = p.on_close.clone();
        Callback::from(move |_: MouseEvent| c.emit(()))
    };
    let set = |h: &UseStateHandle<String>| {
        let h = h.clone();
        Callback::from(move |e: InputEvent| h.set(input_value(&e)))
    };
    let note = cost_note(stored, &m).filter(|_| keep_cost);
    html! {
        <div class="sheet-backdrop" onclick={on_close.clone()}>
            <div class="sheet" role="dialog" aria-modal="true" aria-label={format!("Modifier {}", initial.name)} onclick={stop()}>
                <div class="sheet-handle" />
                <div class="sheet-head"><h2>{ format!("Modifier {}", initial.name) }</h2></div>
                <div class="chosen"><b>{ initial.name.clone() }</b>{ " " }<small class="muted mono">{ initial.symbol.clone() }</small></div>
                if let Some(v) = (*price).filter(|v| *v != 0.0) {
                    <p class="muted small">{ format!("Cours actuel (consensus) : {}", crate::money::price(v)) }</p>
                }
                <label class="field">
                    <span>{ "Quantité détenue" }</span>
                    <input ref={focus} inputmode="decimal" value={(*qty).clone()} oninput={set(&qty)} />
                </label>
                <label class="field">
                    <span>{ format!("Prix d'achat moyen ({sym}, PRU)") }</span>
                    <input inputmode="decimal" value={(*pru).clone()} oninput={set(&pru)} />
                </label>
                if let Some(n) = note {
                    <p class="muted small">{ format!("{n} Le modifier l'enregistre en {sym}.") }</p>
                }
                <label class="field">
                    <span>{ format!("Mon stop ({sym}, facultatif)") }</span>
                    <input inputmode="decimal" value={(*stop_text).clone()} placeholder="aucun" oninput={set(&stop_text)} />
                </label>
                <p class="muted small">
                    { "Prix auquel vous comptez vendre pour limiter la perte. Altim vous alerte quand le cours s'en approche (moins d'une volatilité journalière) ou le casse. Aucun ordre n'est passé." }
                </p>
                if quantity > 0.0 && cost_usd > 0.0 {
                    <p class="kv small"><span>{ "Montant investi" }</span><b>{ crate::money::money(quantity * cost_usd) }</b></p>
                }
                <button class="btn" disabled={!valid} onclick={save}>{ "Enregistrer" }</button>
                <button class="btn btn-ghost" onclick={on_close}>{ "Annuler" }</button>
            </div>
        </div>
    }
}
