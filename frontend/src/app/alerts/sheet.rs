//! « Alerte de prix » on an asset page (PriceAlertButton of Alerts.tsx): above / below a price, or a move of ±x %,
//! typed in the display currency, in a sheet over the page (a portal on <body>).
use altim_core::types::Kind;
use altim_core::web::insights::alerts::{PriceTarget, parse_typed, threshold_text};
use yew::prelude::*;

use super::notify::{ask_permission, permission};
use super::store::{set_alerts, use_alerts};
use crate::route::use_on_link;
use crate::ui::Segmented;

#[derive(Properties, PartialEq)]
pub struct PriceAlertButtonProps {
    pub symbol: AttrValue,
    pub kind: Kind,
    pub name: AttrValue,
    /// Current price in dollars.
    pub price: Option<f64>,
}

#[component]
pub fn PriceAlertButton(p: &PriceAlertButtonProps) -> Html {
    let open = use_state(|| false);
    let s = use_alerts();
    let count = s.armed_on(&p.symbol, p.kind);
    let show = {
        let open = open.clone();
        Callback::from(move |_| open.set(true))
    };
    let close = {
        let open = open.clone();
        Callback::from(move |_| open.set(false))
    };
    html! {
        <>
            <button class="btn btn-ghost btn-small" onclick={show} aria-haspopup="dialog">
                { "🔔 Alerte de prix" }{ if count > 0 { format!(" · {count}") } else { String::new() } }
            </button>
            if *open {
                <PriceAlertSheet symbol={p.symbol.clone()} kind={p.kind} name={p.name.clone()} price={p.price} on_close={close} />
            }
        </>
    }
}

#[derive(Properties, PartialEq)]
struct SheetProps {
    symbol: AttrValue,
    kind: Kind,
    name: AttrValue,
    price: Option<f64>,
    on_close: Callback<()>,
}

#[component]
fn PriceAlertSheet(p: &SheetProps) -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let cur = m.currency();
    let mode = use_state(|| AttrValue::Static("below"));
    let text = use_state(String::new);
    let done = use_state(|| false);
    // `autoFocus` of React: the field takes the focus when the sheet opens.
    let input = use_node_ref();
    {
        let input = input.clone();
        use_effect_with((), move |_| {
            if let Some(i) = input.cast::<web_sys::HtmlElement>() {
                let _ = i.focus();
            }
        });
    }
    let is_move = mode.as_str() == "move";
    let v = parse_typed(&text);
    let shown_price = p.price.map(|x| m.to_display(x));
    let valid = v.is_finite() && v > 0.0 && (!is_move || (v <= 100.0 && shown_price.is_some()));
    let save = {
        let (symbol, name, kind, mode, done) = (p.symbol.to_string(), p.name.to_string(), p.kind, mode.clone(), done.clone());
        Callback::from(move |_| {
            if !valid {
                return;
            }
            let is_move = mode.as_str() == "move";
            let t = PriceTarget {
                id: crate::state::random_uuid(),
                symbol: symbol.clone(),
                kind,
                name: name.clone(),
                above: mode.as_str() == "above",
                currency: cur,
                created: js_sys::Date::now(),
                triggered: None,
                price: if is_move { shown_price.unwrap_or_default() } else { v },
                move_pct: is_move.then_some(v),
            };
            set_alerts(|s| s.targets.push(t));
            let done = done.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if permission() == "default" {
                    ask_permission().await;
                }
                done.set(true);
            });
        })
    };
    let on_input = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| text.set(e.target_unchecked_into::<web_sys::HtmlInputElement>().value()))
    };
    let set_mode = {
        let mode = mode.clone();
        Callback::from(move |v: AttrValue| mode.set(v))
    };
    let close = {
        let c = p.on_close.clone();
        Callback::from(move |_: MouseEvent| c.emit(()))
    };
    let placeholder = if is_move {
        "ex. 5".to_string()
    } else {
        shown_price
            .filter(|x| *x != 0.0 && x.is_finite())
            .map(|x| threshold_text(x, cur).trim_end_matches(" €").trim_end_matches(" $").to_string())
            .unwrap_or_default()
    };
    let options: Vec<(AttrValue, AttrValue)> = [("below", "En dessous"), ("above", "Au-dessus"), ("move", "Variation")]
        .iter()
        .map(|(v, l)| (AttrValue::Static(v), AttrValue::Static(l)))
        .collect();
    let sheet = html! {
        <div class="sheet-backdrop" onclick={close.clone()}>
            <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="price-alert-title" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="sheet-handle" />
                <div class="sheet-head"><h2 id="price-alert-title">{ format!("Alerte de prix · {}", p.symbol) }</h2></div>
                if let Some(price) = p.price {
                    <p class="kv small"><span>{ "Prix actuel" }</span><b class="mono">{ crate::money::price(price) }</b></p>
                }
                <Segmented label="Type d'alerte" value={(*mode).clone()} on_change={set_mode} {options} />
                <label class="field">
                    <span>{ if is_move { "Variation (%, hausse ou baisse)".to_string() } else { format!("Prix ({})", altim_core::web::money::symbol(cur)) } }</span>
                    <input ref={input} inputmode="decimal" autofocus=true value={(*text).clone()} {placeholder} oninput={on_input} />
                </label>
                <p class="muted small">
                    { "Vérifiée toutes les 5 minutes tant qu'un onglet Altim est ouvert, et notifiée si vous l'autorisez ; visible dans Alertes. Un seuil en € est comparé au cours converti au taux du jour." }
                </p>
                if *done {
                    <p class="notice ok small" role="status">{ "✔ Alerte enregistrée. " }<a href="/app/alertes" onclick={on_link} class="link">{ "Voir les alertes" }</a></p>
                } else {
                    <button class="btn" disabled={!valid} onclick={save}>{ "Créer l'alerte" }</button>
                }
                <button class="btn btn-ghost" onclick={close}>{ if *done { "Fermer" } else { "Annuler" } }</button>
            </div>
        </div>
    };
    match gloo::utils::document().body() {
        Some(body) => create_portal(sheet, body.into()),
        None => sheet,
    }
}
