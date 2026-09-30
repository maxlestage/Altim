//! "Simuler cet achat" on the Décision card (PaperOrder.tsx): a virtual purchase, no real money, no order sent. The
//! simulated ledger is kept in dollars like the prices; amounts are typed and shown in the display currency.
use std::rc::Rc;

use altim_core::engine::decision_types::Decision;
use altim_core::js::{number_to_string, round};
use altim_core::web::trading::enum_str;
use altim_core::web::trading::journal::{JournalSide, NewEntry, SOURCE_PAPER};
use altim_core::web::trading::paper::{DEFAULT_CAPITAL, FEE_RATE, OpenOrder, PaperDecision, SLIPPAGE, new_paper, open_position, valuation};
use altim_core::web::trading::paper_ui::{
    against_decision, against_text, default_amount, fr_date, input_price, level_warnings, parse_amount, price, usd,
};
use yew::prelude::*;

use super::store::{get_paper, set_paper, use_paper};
use super::use_autofocus;
use crate::app::journal::JournalNote;
use crate::route::use_on_link;

#[derive(Properties, PartialEq)]
pub struct SimulateBuyProps {
    pub d: Rc<Decision>,
    /// Live price when the stream has one (else the decision's price).
    #[prop_or_default]
    pub live_price: Option<f64>,
}

/// "Simuler cet achat" on the Décision card: a virtual purchase, no real money, no order sent.
#[component]
pub fn SimulateBuy(p: &SimulateBuyProps) -> Html {
    let on_link = use_on_link();
    let open = use_state(|| false);
    let done = use_state(|| None::<String>);
    let d = &p.d;
    let px = p.live_price.or(d.price).filter(|v| *v > 0.0);
    let verdict = enum_str(&d.verdict);
    let against = against_decision(&verdict);
    let click = {
        let (open, done) = (open.clone(), done.clone());
        Callback::from(move |_| {
            done.set(None);
            open.set(true);
        })
    };
    html! {
        <div class="dec-block paper-cta">
            <button class="btn btn-ghost" onclick={click} disabled={px.is_none()}>{ "Simuler cet achat" }</button>
            <small class="muted">
                { "Portefeuille simulé : aucun argent réel, aucun ordre passé." }
                { if against { format!(" Décision actuelle : « {} ».", d.label) } else { String::new() } }
            </small>
            if let Some(msg) = &*done {
                <p class="notice ok small" role="status">
                    { format!("✔ {msg} ") }<a href="/app/simulation" onclick={on_link} class="link">{ "Voir la simulation" }</a>
                </p>
            }
            if let Some(px) = px.filter(|_| *open) {
                <OrderSheet
                    d={d.clone()}
                    {px}
                    live={p.live_price.is_some()}
                    on_close={{ let open = open.clone(); Callback::from(move |_| open.set(false)) }}
                    on_done={{ let (open, done) = (open.clone(), done.clone()); Callback::from(move |msg: String| { open.set(false); done.set(Some(msg)); }) }}
                />
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct OrderSheetProps {
    d: Rc<Decision>,
    px: f64,
    live: bool,
    on_close: Callback<()>,
    on_done: Callback<String>,
}

#[component]
fn OrderSheet(p: &OrderSheetProps) -> Html {
    let m = crate::money::use_money();
    let saved = use_paper();
    let d = &p.d;
    let px = p.px;
    let state = saved.state.as_ref();
    // The simulated ledger is kept in dollars like the prices; amounts are typed and shown in the display currency.
    let start_usd = m.from_display(DEFAULT_CAPITAL);
    let cash = state.map(|s| s.cash).unwrap_or(start_usd);
    let equity = state.map(|s| valuation(s, |_| None).equity).unwrap_or(start_usd);
    let amount_text = {
        let init = input_price(Some(m.to_display(default_amount(equity, cash))));
        use_state(move || init)
    };
    let stop_text = {
        let init = input_price(d.plan.as_ref().map(|pl| m.to_display(pl.stop)));
        use_state(move || init)
    };
    let target_text = {
        let init = input_price(d.plan.as_ref().map(|pl| m.to_display(pl.target1)));
        use_state(move || init)
    };
    let error = use_state(|| None::<String>);
    let note = use_state(String::new);
    let focus = use_autofocus();

    let usd_of = |v: Option<f64>| v.map(|v| m.from_display(v));
    let amount = usd_of(parse_amount(&amount_text));
    let stop = usd_of(parse_amount(&stop_text));
    let target = usd_of(parse_amount(&target_text));
    let fill = px * (1.0 + SLIPPAGE);
    let warnings = level_warnings(fill, stop, target);
    let bad_number = [stop, target].iter().any(|v| v.is_some_and(|v| !v.is_finite()));
    let verdict = enum_str(&d.verdict);
    let against = against_decision(&verdict);

    let confirm = {
        let (d, error, note, on_done) = (d.clone(), error.clone(), note.clone(), p.on_done.clone());
        Callback::from(move |_| {
            error.set(None);
            if bad_number {
                error.set(Some("Stop ou objectif illisible : laissez vide ou saisissez un nombre.".into()));
                return;
            }
            let now = js_sys::Date::now();
            let base = get_paper();
            let created = base.is_none();
            let s = base.unwrap_or_else(|| new_paper(start_usd, now));
            let order = OpenOrder {
                id: crate::state::random_uuid(),
                symbol: d.symbol.clone(),
                kind: d.kind,
                name: d.name.clone(),
                price: px,
                amount: amount.unwrap_or(f64::NAN),
                stop,
                target,
                decision: Some(PaperDecision {
                    verdict: enum_str(&d.verdict),
                    label: d.label.clone(),
                    confidence: d.confidence,
                    as_of: d.as_of as f64,
                }),
            };
            let next = match open_position(&s, &order, now) {
                Ok(n) => n,
                Err(e) => {
                    error.set(Some(e));
                    return;
                }
            };
            set_paper(next.clone());
            let Some(pos) = next.positions.last() else { return };
            crate::app::journal::store::record(NewEntry {
                now,
                source: SOURCE_PAPER,
                side: Some(JournalSide::Buy),
                symbol: d.symbol.clone(),
                kind: Some(d.kind),
                name: d.name.clone(),
                price: pos.entry,
                quantity: Some(pos.quantity),
                amount: Some(pos.invested),
                stop: pos.stop,
                targets: vec![pos.target],
                note: (*note).clone(),
                ref_id: Some(pos.id.clone()),
                decision: Some(d.as_ref()),
                ..Default::default()
            });
            let dm = crate::money::display();
            let fresh = if created {
                format!(" (portefeuille simulé créé avec {})", dm.money(start_usd, 0, 0, altim_core::web::money::NBSP))
            } else {
                String::new()
            };
            on_done.emit(format!("Achat simulé : {} de {}{fresh}. Inscrit au journal.", usd(&dm, amount.unwrap_or(f64::NAN)), d.name));
        })
    };
    let close = {
        let cb = p.on_close.clone();
        Callback::from(move |_: MouseEvent| cb.emit(()))
    };
    let input = |h: &UseStateHandle<String>| {
        let h = h.clone();
        Callback::from(move |e: InputEvent| h.set(e.target_unchecked_into::<web_sys::HtmlInputElement>().value()))
    };
    let fees = amount.filter(|a| *a > 0.0 && a.is_finite()).map(|a| format!(" Frais : {}.", usd(&m, a * FEE_RATE))).unwrap_or_default();
    let new_book = if state.is_none() {
        format!(" (nouveau portefeuille de {})", m.money(start_usd, 0, 0, altim_core::web::money::NBSP))
    } else {
        String::new()
    };
    let source = if p.live { "Cours en direct.".to_string() } else { format!("Cours de la décision du {}.", fr_date(d.as_of as f64, true)) };

    let sheet = html! {
        <div class="sheet-backdrop" onclick={close.clone()}>
            <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="paper-order-title" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="sheet-handle" />
                <div class="sheet-head"><h2 id="paper-order-title">{ format!("Simuler l'achat de {}", d.name) }</h2></div>
                <p class="notice small">{ "Simulation : aucun argent réel, aucun ordre passé. Le résultat sera suivi dans l'écran « Simulation »." }</p>
                if against {
                    <p class="notice warn small" role="alert">{ format!("⚠ {} C'est permis : utile pour comparer.", against_text(&d.label)) }</p>
                }
                <p class="kv small"><span>{ "Décision affichée" }</span><b>{ format!("{} · confiance {}/100", d.label, number_to_string(round(d.confidence))) }</b></p>
                <p class="kv small"><span>{ "Cours utilisé" }</span><b class="mono">{ price(&m, px) }</b></p>
                <p class="muted small">{ format!("{source} Achat simulé à {} (glissement 0,05 %), frais 0,1 %.", price(&m, fill)) }</p>
                <label class="field">
                    <span>{ format!("Montant à investir ({}, frais compris)", m.symbol()) }</span>
                    <input inputmode="decimal" ref={focus} value={(*amount_text).clone()} oninput={input(&amount_text)} aria-describedby="paper-cash" />
                </label>
                <small id="paper-cash" class="muted">
                    { format!("Liquidités simulées : {}{new_book} · par défaut 10 % de la valeur simulée.{fees}", usd(&m, cash)) }
                </small>
                <div class="grid-2">
                    <label class="field">
                        <span>{ "Stop (optionnel)" }</span>
                        <input inputmode="decimal" value={(*stop_text).clone()} oninput={input(&stop_text)} placeholder="aucun" />
                    </label>
                    <label class="field">
                        <span>{ "Objectif (optionnel)" }</span>
                        <input inputmode="decimal" value={(*target_text).clone()} oninput={input(&target_text)} placeholder="aucun" />
                    </label>
                </div>
                if d.plan.is_some() {
                    <small class="muted">{ "Préremplis avec le plan de la décision (stop et objectif 1). Vente automatique si une bougie journalière les atteint." }</small>
                }
                { for warnings.iter().map(|w| html! { <p key={*w} class="notice warn small">{ format!("⚠ {w}") }</p> }) }
                <JournalNote value={AttrValue::from((*note).clone())} on_change={{ let note = note.clone(); Callback::from(move |v| note.set(v)) }} />
                if let Some(e) = &*error {
                    <p class="notice danger small" role="alert">{ format!("✕ {e}") }</p>
                }
                <button class="btn" onclick={confirm}>{ "Confirmer l'achat simulé" }</button>
                <button class="btn btn-ghost" onclick={close}>{ "Annuler" }</button>
            </div>
        </div>
    };
    // Portal: the Décision card has a backdrop-filter, which would trap a position: fixed sheet inside it.
    match gloo::utils::document().body() {
        Some(body) => create_portal(sheet, body.into()),
        None => sheet,
    }
}
