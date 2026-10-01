//! One search box for everything (AssetSearch.tsx): cryptos and stocks / ETF together, by symbol or name
//! (BTC, Solana, Apple, NVDA, S&P 500…). No list to scroll through.
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use altim_core::types::Kind;
use altim_core::web::portfolio::wire::UniverseItem;
use yew::prelude::*;

use super::browser::{input_value, use_auto_focus};

#[derive(Properties, PartialEq)]
pub struct AssetSearchProps {
    /// Keys "crypto:BTC" already chosen.
    pub selected: HashSet<String>,
    pub on_toggle: Callback<UniverseItem>,
    #[prop_or_default]
    pub auto_focus: bool,
    #[prop_or_default]
    pub clear_on_pick: bool,
    #[prop_or(AttrValue::Static("Rechercher une crypto ou une action"))]
    pub label: AttrValue,
}

/// A row of the results (also the catalogue's rows of AssetPicker): check box, symbol, name, tag.
pub fn pick_row(it: &UniverseItem, on: bool, tag: String, onclick: Callback<MouseEvent>) -> Html {
    html! {
        <li key={it.key()}>
            <button class={if on { "picked" } else { "" }} aria-pressed={on.to_string()} {onclick}>
                <span class="pick-box" aria-hidden="true">{ if on { "✓" } else { "+" } }</span>
                <b>{ it.symbol.clone() }</b>
                <span class="muted pick-name">{ it.name.clone() }</span>
                <small class="tag">{ tag }</small>
            </button>
        </li>
    }
}

#[component]
pub fn AssetSearch(p: &AssetSearchProps) -> Html {
    let q = use_state(String::new);
    let results = use_state(Vec::<UniverseItem>::new);
    let searching = use_state(|| false);
    let error = use_state(|| None::<String>);
    let input = use_auto_focus(p.auto_focus);

    {
        let (results, searching, error) = (results.clone(), searching.clone(), error.clone());
        use_effect_with((*q).clone(), move |q| {
            let query = q.trim().to_string();
            let alive = Rc::new(Cell::new(true));
            let mut timer = None;
            if query.is_empty() {
                results.set(Vec::new());
            } else {
                searching.set(true);
                let a = alive.clone();
                timer = Some(gloo::timers::callback::Timeout::new(200, move || {
                    wasm_bindgen_futures::spawn_local(async move {
                        let r = super::api::search(&query, 20).await;
                        if !a.get() {
                            return;
                        }
                        match r {
                            Ok(r) => {
                                results.set(r);
                                error.set(None);
                            }
                            Err(e) => error.set(Some(e.0)),
                        }
                        searching.set(false);
                    });
                }));
            }
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    let pick = |it: &UniverseItem| {
        let (on_toggle, it, q, clear, input) = (p.on_toggle.clone(), it.clone(), q.clone(), p.clear_on_pick, input.clone());
        Callback::from(move |_: MouseEvent| {
            on_toggle.emit(it.clone());
            if clear {
                q.set(String::new());
                if let Some(el) = input.cast::<web_sys::HtmlElement>() {
                    let _ = el.focus();
                }
            }
        })
    };
    let oninput = {
        let q = q.clone();
        Callback::from(move |e: InputEvent| q.set(input_value(&e)))
    };
    let query = q.trim();
    html! {
        <div class="asset-search">
            <label class="field">
                <span>{ p.label.clone() }</span>
                <input
                    ref={input.clone()}
                    type="search"
                    enterkeyhint="search"
                    autocomplete="off"
                    placeholder="BTC, Solana, Apple, NVDA, S&P 500…"
                    value={(*q).clone()}
                    {oninput}
                />
            </label>
            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if !query.is_empty() && !*searching && results.is_empty() && error.is_none() {
                <p class="muted small">{ format!("Aucun résultat pour « {query} ». Les actions cotées en euros ne sont pas encore prises en charge.") }</p>
            }
            if !results.is_empty() {
                <ul class="search-results picker-list">
                    { for results.iter().map(|it| {
                        let on = p.selected.contains(&it.key());
                        let tag = if it.kind == Kind::Crypto { "Crypto" } else if it.etf == Some(true) { "ETF" } else { "Action" };
                        pick_row(it, on, tag.into(), pick(it))
                    }) }
                </ul>
            }
        </div>
    }
}
