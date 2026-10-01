//! Several holdings entered at once (AddHoldings.tsx), in two sections: cryptos and stocks / ETF. Costs are typed in
//! the display currency and saved with it (`costCurrency`); an asset already held is merged (weighted average cost).
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use altim_core::engine::format::format_price;
use altim_core::types::Kind;
use altim_core::web::portfolio::view::parse_decimal;
use altim_core::web::portfolio::wire::UniverseItem;
use altim_core::web::store::StoredHolding;
use altim_core::web::trading::journal::JournalSide;
use altim_core::web::trading::journal_store::RealTrade;
use yew::prelude::*;

use super::browser::{input_value, stop};
use super::picker::{AssetPicker, kind_label};
use super::search::AssetSearch;
use crate::app::journal::JournalToggle;
use crate::app::journal::store::record_real_trade;
use crate::state::holdings::{add_holdings, use_holdings};

#[derive(Clone, PartialEq)]
struct Line {
    asset: UniverseItem,
    qty: String,
    pru: String,
}

enum LinesAction {
    Toggle(UniverseItem),
    Qty(String, String),
    Pru(String, String),
}

#[derive(Clone, PartialEq, Default)]
struct Lines(Vec<Line>);

impl Reducible for Lines {
    type Action = LinesAction;
    fn reduce(self: Rc<Self>, action: LinesAction) -> Rc<Self> {
        let mut ls = self.0.clone();
        match action {
            LinesAction::Toggle(asset) => {
                let k = asset.key();
                if ls.iter().any(|l| l.asset.key() == k) {
                    ls.retain(|l| l.asset.key() != k);
                } else {
                    ls.push(Line { asset, qty: String::new(), pru: String::new() });
                }
            }
            LinesAction::Qty(k, v) => ls.iter_mut().filter(|l| l.asset.key() == k).for_each(|l| l.qty = v.clone()),
            LinesAction::Pru(k, v) => ls.iter_mut().filter(|l| l.asset.key() == k).for_each(|l| l.pru = v.clone()),
        }
        Rc::new(Lines(ls))
    }
}

/// Current consensus price of every chosen asset (0 when unknown), merged as the answers come.
#[derive(Clone, PartialEq, Default)]
struct Prices(HashMap<String, f64>);

impl Reducible for Prices {
    type Action = Vec<(String, f64)>;
    fn reduce(self: Rc<Self>, add: Self::Action) -> Rc<Self> {
        let mut m = self.0.clone();
        m.extend(add);
        Rc::new(Prices(m))
    }
}

#[derive(Properties, PartialEq)]
pub struct AddHoldingsProps {
    pub on_close: Callback<()>,
}

#[component]
pub fn AddHoldings(p: &AddHoldingsProps) -> Html {
    let _m = crate::money::use_money();
    let usd = use_holdings();
    let lines = use_reducer(Lines::default);
    let picking = use_state(|| None::<Kind>);
    let prices = use_reducer(Prices::default);
    // A first entry of holdings is usually past purchases: not journaled unless asked; later additions are.
    let journal = {
        let first = usd.holdings.is_empty();
        use_state(move || !first)
    };
    let note = use_state(String::new);
    let keys: Vec<String> = lines.0.iter().map(|l| l.asset.key()).collect();
    let selected: HashSet<String> = keys.iter().cloned().collect();

    {
        let (lines, prices) = (lines.clone(), prices.clone());
        use_effect_with(keys.join(","), move |_| {
            let missing: Vec<UniverseItem> = lines.0.iter().map(|l| l.asset.clone()).filter(|a| !prices.0.contains_key(&a.key())).collect();
            if !missing.is_empty() {
                wasm_bindgen_futures::spawn_local(async move {
                    let items: Vec<(String, Kind)> = missing.iter().map(|a| (a.symbol.clone(), a.kind)).collect();
                    let found = super::api::quotes(&items).await.unwrap_or_default();
                    let add = missing
                        .iter()
                        .map(|a| (a.key(), found.iter().find(|q| q.symbol == a.symbol && q.kind == a.kind).map(|q| q.price).unwrap_or(0.0)))
                        .collect();
                    prices.dispatch(add);
                });
            }
        });
    }

    let toggle = {
        let lines = lines.clone();
        Callback::from(move |a: UniverseItem| lines.dispatch(LinesAction::Toggle(a)))
    };
    // Costs are typed in the display currency and saved with it.
    let cur = crate::money::currency();
    struct Parsed<'a> {
        l: &'a Line,
        price: f64,
        quantity: f64,
        average_price: f64,
        valid: bool,
    }
    let parsed: Vec<Parsed> = lines
        .0
        .iter()
        .map(|l| {
            let price = prices.0.get(&l.asset.key()).copied().unwrap_or(0.0);
            let quantity = parse_decimal(&l.qty);
            let average_price = if l.pru.trim().is_empty() { crate::money::to_display(price) } else { parse_decimal(&l.pru) };
            let valid = quantity > 0.0 && average_price.is_finite() && average_price > 0.0;
            Parsed { l, price, quantity, average_price, valid }
        })
        .collect();
    let ready: Vec<&Parsed> = parsed.iter().filter(|p| p.valid).collect();
    let incomplete = parsed.len() - ready.len();
    let invested: f64 = ready.iter().map(|p| p.quantity * p.average_price).sum();

    let save = {
        let items: Vec<StoredHolding> = ready
            .iter()
            .map(|p| StoredHolding {
                id: String::new(),
                symbol: p.l.asset.symbol.clone(),
                kind: p.l.asset.kind,
                name: p.l.asset.name.clone(),
                quantity: p.quantity,
                average_price: p.average_price,
                stop: None,
                cost_currency: Some(cur),
                stop_currency: None,
                extra: Default::default(),
            })
            .collect();
        // The journal gets dollars.
        let trades: Vec<RealTrade> = if *journal {
            ready
                .iter()
                .map(|p| RealTrade {
                    side: JournalSide::Buy,
                    symbol: p.l.asset.symbol.clone(),
                    kind: p.l.asset.kind,
                    name: p.l.asset.name.clone(),
                    price: crate::money::from_display(p.average_price),
                    quantity: p.quantity,
                    stop: None,
                    note: (*note).clone(),
                    ref_id: None,
                })
                .collect()
        } else {
            vec![]
        };
        let on_close = p.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            add_holdings(items.clone());
            for t in &trades {
                record_real_trade(t);
            }
            on_close.emit(());
        })
    };

    let section = |kind: Kind| {
        let rows: Vec<&Parsed> = parsed.iter().filter(|p| p.l.asset.kind == kind).collect();
        let browse = {
            let picking = picking.clone();
            Callback::from(move |_: MouseEvent| picking.set(Some(kind)))
        };
        html! {
            <fieldset class="entry-section">
                <legend class="section-label">{ kind_label(kind) }{ if rows.is_empty() { String::new() } else { format!(" · {}", rows.len()) } }</legend>
                { for rows.iter().map(|r| {
                    let key = r.l.asset.key();
                    let held = usd.holdings.iter().any(|h| h.symbol == r.l.asset.symbol && h.kind == r.l.asset.kind);
                    let remove = { let (t, a) = (toggle.clone(), r.l.asset.clone()); Callback::from(move |_: MouseEvent| t.emit(a.clone())) };
                    let on_qty = { let (ls, k) = (lines.clone(), key.clone()); Callback::from(move |e: InputEvent| ls.dispatch(LinesAction::Qty(k.clone(), input_value(&e)))) };
                    let on_pru = { let (ls, k) = (lines.clone(), key.clone()); Callback::from(move |e: InputEvent| ls.dispatch(LinesAction::Pru(k.clone(), input_value(&e)))) };
                    let price = (r.price != 0.0).then_some(r.price);
                    html! {
                        <div key={key.clone()} class={classes!("entry-line", "card", (!r.valid).then_some("incomplete"))}>
                            <div class="entry-head">
                                <div class="holding-name">
                                    <b>{ r.l.asset.name.clone() }</b>
                                    <small class="muted mono">{ r.l.asset.symbol.clone() }{ price.map(|p| format!(" · cours {}", crate::money::price(p))).unwrap_or_default() }</small>
                                </div>
                                <button class="link-btn danger" aria-label={format!("Retirer {}", r.l.asset.name)} onclick={remove}>{ "Retirer" }</button>
                            </div>
                            <div class="entry-fields">
                                <label class="field">
                                    <span>{ "Quantité" }</span>
                                    <input inputmode="decimal" value={r.l.qty.clone()} placeholder="ex. 0,5" oninput={on_qty} />
                                </label>
                                <label class="field">
                                    <span>{ format!("Prix moyen payé ({})", crate::money::symbol()) }</span>
                                    <input
                                        inputmode="decimal"
                                        value={r.l.pru.clone()}
                                        placeholder={price.map(|p| format_price(crate::money::to_display(p))).unwrap_or_else(|| "ex. 100".into())}
                                        oninput={on_pru}
                                    />
                                </label>
                            </div>
                            if held {
                                <p class="muted small">{ "Déjà dans vos avoirs : la quantité sera ajoutée et le prix moyen recalculé." }</p>
                            }
                        </div>
                    }
                }) }
                <button class="btn btn-ghost" onclick={browse}>
                    { if kind == Kind::Crypto { "Parcourir toutes les cryptos" } else { "Parcourir toutes les actions / ETF" } }
                </button>
            </fieldset>
        }
    };

    let on_close = {
        let c = p.on_close.clone();
        Callback::from(move |_: MouseEvent| c.emit(()))
    };
    let n = ready.len();
    html! {
        <>
            <div class="sheet-backdrop" onclick={on_close.clone()}>
                <div class="sheet" role="dialog" aria-modal="true" aria-label="Ajouter des avoirs" onclick={stop()}>
                    <div class="sheet-handle" />
                    <div class="sheet-head"><h2>{ "Ajouter des avoirs" }</h2></div>
                    <AssetSearch selected={selected.clone()} on_toggle={toggle.clone()} auto_focus=true clear_on_pick=true />
                    <p class="muted small">
                        { "Cherchez chaque crypto ou action et touchez-la pour l'ajouter, puis indiquez la quantité et le prix d'achat moyen (laissé vide = cours actuel). Vous pouvez aussi parcourir tout le catalogue." }
                    </p>
                    { section(Kind::Crypto) }
                    { section(Kind::Stock) }
                    if n > 0 {
                        <p class="kv small"><span>{ "Montant investi" }</span><b>{ crate::money::money(crate::money::from_display(invested)) }</b></p>
                        <JournalToggle
                            checked={*journal}
                            on_change={{ let journal = journal.clone(); Callback::from(move |v| journal.set(v)) }}
                            note={AttrValue::from((*note).clone())}
                            on_note={{ let note = note.clone(); Callback::from(move |v| note.set(v)) }}
                            text="Achats faits aujourd'hui : les inscrire au journal"
                        />
                    }
                    if incomplete > 0 {
                        <p class="muted small">{ format!("{incomplete} ligne{s} sans quantité : ignorée{s}.", s = if incomplete > 1 { "s" } else { "" }) }</p>
                    }
                    <button class="btn" disabled={n == 0} onclick={save}>
                        { if n > 0 { format!("Enregistrer {n} avoir{}", if n > 1 { "s" } else { "" }) } else { "Enregistrer".into() } }
                    </button>
                    <button class="btn btn-ghost" onclick={on_close}>{ "Annuler" }</button>
                </div>
            </div>
            if let Some(kind) = *picking {
                <AssetPicker
                    kind={Some(kind)}
                    title={if kind == Kind::Crypto { "Toutes les cryptos" } else { "Toutes les actions et ETF" }}
                    {selected}
                    on_toggle={toggle}
                    on_close={{ let picking = picking.clone(); Callback::from(move |_| picking.set(None)) }}
                />
            }
        </>
    }
}
