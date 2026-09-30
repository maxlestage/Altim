//! Full catalogue (AssetPicker.tsx): every crypto, every US-listed stock and ETF, browsable by category and
//! searchable. Several assets can be picked in one go. Also used by Réglages (batch D).
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use altim_core::js::fr;
use altim_core::types::Kind;
use altim_core::web::portfolio::wire::UniverseItem;
use yew::prelude::*;

use super::browser::{input_value, stop};
use super::search::{AssetSearch, pick_row};
use crate::ui::Segmented;

const PAGE: usize = 50;

/// `KIND_LABEL`.
pub fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Crypto => "Crypto",
        Kind::Stock => "Actions & ETF",
    }
}

#[derive(Properties, PartialEq)]
pub struct AssetPickerProps {
    /// A fixed category; without it the picker opens on the combined search (no list to scroll).
    #[prop_or_default]
    pub kind: Option<Kind>,
    pub selected: HashSet<String>,
    pub on_toggle: Callback<UniverseItem>,
    pub on_close: Callback<()>,
    #[prop_or(AttrValue::Static("Choisir des actifs"))]
    pub title: AttrValue,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    All,
    Of(Kind),
}

impl Mode {
    fn value(self) -> &'static str {
        match self {
            Mode::All => "all",
            Mode::Of(k) => k.as_str(),
        }
    }
}

#[component]
pub fn AssetPicker(p: &AssetPickerProps) -> Html {
    let mode = use_state(|| p.kind.map(Mode::Of).unwrap_or(Mode::All));
    let kind = match *mode {
        Mode::All => Kind::Crypto,
        Mode::Of(k) => k,
    };
    let q = use_state(String::new);
    let items = use_state(Vec::<UniverseItem>::new);
    let total = use_state(|| None::<usize>);
    let loading = use_state(|| false);
    let error = use_state(|| None::<String>);

    let load = {
        let (items, total, loading, error, q) = (items.clone(), total.clone(), loading.clone(), error.clone(), q.clone());
        Rc::new(move |offset: usize, prev: Vec<UniverseItem>, alive: Rc<Cell<bool>>| {
            let (items, total, loading, error, query) = (items.clone(), total.clone(), loading.clone(), error.clone(), q.trim().to_string());
            loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let r = super::api::universe(kind, &query, offset, PAGE).await;
                if !alive.get() {
                    return;
                }
                match r {
                    Ok(r) => {
                        let mut next = if offset == 0 { Vec::new() } else { prev };
                        next.extend(r.items);
                        items.set(next);
                        total.set(Some(r.total));
                        error.set(None);
                    }
                    Err(e) => error.set(Some(e.0)),
                }
                loading.set(false);
            });
        })
    };
    {
        let load = load.clone();
        use_effect_with((*mode, (*q).clone()), move |(mode, q)| {
            let alive = Rc::new(Cell::new(true));
            let mut timer = None;
            if *mode != Mode::All {
                let a = alive.clone();
                timer = Some(gloo::timers::callback::Timeout::new(if q.is_empty() { 0 } else { 250 }, move || load(0, Vec::new(), a)));
            }
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    let prefix = format!("{}:", kind.as_str());
    let count = p.selected.iter().filter(|k| k.starts_with(&prefix)).count();
    let on_close = {
        let c = p.on_close.clone();
        Callback::from(move |_: MouseEvent| c.emit(()))
    };
    let on_mode = {
        let (mode, items, total, q) = (mode.clone(), items.clone(), total.clone(), q.clone());
        Callback::from(move |v: AttrValue| {
            mode.set(match v.as_str() {
                "crypto" => Mode::Of(Kind::Crypto),
                "stock" => Mode::Of(Kind::Stock),
                _ => Mode::All,
            });
            items.set(Vec::new());
            total.set(None);
            q.set(String::new());
        })
    };
    let oninput = {
        let q = q.clone();
        Callback::from(move |e: InputEvent| q.set(input_value(&e)))
    };
    let label = match *total {
        None => "Chargement du catalogue…".to_string(),
        Some(t) if !q.trim().is_empty() => format!("{} résultat{}", fr(t as f64, 0, 3), if t > 1 { "s" } else { "" }),
        Some(t) if kind == Kind::Crypto => format!("Toutes les cryptos : {}", fr(t as f64, 0, 3)),
        Some(t) => format!("Toutes les actions et ETF cotés aux États-Unis : {}", fr(t as f64, 0, 3)),
    };
    let selected_text = if count > 0 { format!(" · {count} sélectionné{}", if count > 1 { "s" } else { "" }) } else { String::new() };
    let more = {
        let (load, items) = (load.clone(), items.clone());
        Callback::from(move |_: MouseEvent| load(items.len(), (*items).clone(), Rc::new(Cell::new(true))))
    };
    let options: Vec<(AttrValue, AttrValue)> =
        [("all", "Recherche"), ("crypto", "Cryptos"), ("stock", "Actions")].iter().map(|(v, l)| (AttrValue::from(*v), AttrValue::from(*l))).collect();
    html! {
        <div class="sheet-backdrop picker-backdrop" onclick={on_close.clone()}>
            <div class="sheet picker" role="dialog" aria-modal="true" aria-label={p.title.clone()} onclick={stop()}>
                <div class="sheet-handle" />
                <div class="sheet-head">
                    <h2>{ p.title.clone() }</h2>
                    <button class="btn btn-small" onclick={on_close}>
                        { format!("Terminé{}", if p.selected.is_empty() { String::new() } else { format!(" ({})", p.selected.len()) }) }
                    </button>
                </div>
                if p.kind.is_none() {
                    <Segmented label="Catégorie" value={AttrValue::from(mode.value())} on_change={on_mode} {options} />
                }
                if *mode == Mode::All {
                    <AssetSearch selected={p.selected.clone()} on_toggle={p.on_toggle.clone()} auto_focus=true />
                    <p class="muted small">{ "Tapez un symbole ou un nom : cryptos et actions sont cherchées ensemble. Les onglets permettent aussi de parcourir tout le catalogue." }</p>
                } else {
                    <label class="field">
                        <span>{ label }{ selected_text }</span>
                        <input
                            type="search"
                            placeholder={if kind == Kind::Crypto { "Filtrer : BTC, Solana, PEPE…" } else { "Filtrer : Apple, NVDA, S&P 500…" }}
                            value={(*q).clone()}
                            {oninput}
                        />
                    </label>
                    if let Some(e) = &*error {
                        <p class="notice warn">{ format!("⚠ {e}") }</p>
                    }
                    <ul class="search-results picker-list">
                        { for items.iter().map(|it| {
                            let on = p.selected.contains(&it.key());
                            let tag = match (it.rank.filter(|r| *r != 0), it.kind) {
                                (Some(r), _) => format!("#{r}"),
                                (None, Kind::Stock) => if it.etf == Some(true) { "ETF".into() } else { "Action".into() },
                                (None, Kind::Crypto) => {
                                    let n = it.exchanges.map(|n| n.to_string()).unwrap_or_else(|| "undefined".into());
                                    format!("{n} plateforme{}", if it.exchanges.unwrap_or(0) > 1 { "s" } else { "" })
                                }
                            };
                            let onclick = { let (t, it) = (p.on_toggle.clone(), it.clone()); Callback::from(move |_: MouseEvent| t.emit(it.clone())) };
                            pick_row(it, on, tag, onclick)
                        }) }
                    </ul>
                    if let Some(t) = (*total).filter(|t| items.len() < *t) {
                        <button class="btn btn-ghost" disabled={*loading} onclick={more}>
                            { if *loading { "Chargement…".to_string() } else { format!("Afficher plus ({} restants)", fr((t - items.len()) as f64, 0, 3)) } }
                        </button>
                    }
                    if *total == Some(0) {
                        <p class="muted small">{ "Aucun actif trouvé. Les actions cotées hors des États-Unis (en euros) ne sont pas encore prises en charge." }</p>
                    }
                }
            </div>
        </div>
    }
}
