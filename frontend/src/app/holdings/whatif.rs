//! « Et si… ? » on Mes avoirs (WhatIfCard.tsx): how the portfolio would move if one market factor (Nasdaq-100,
//! S&P 500, Bitcoin) fell by a given shock, each line through its beta to that factor. `daily`: the daily candles
//! already loaded (`kind:symbol`); the factor's own candles are fetched when missing.
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::signal::Candle;
use altim_core::js::{fr, to_fixed};
use altim_core::web::money::NBSP;
use altim_core::web::portfolio::holdings::{PortfolioAnalysis, market_key};
use altim_core::web::portfolio::portfolio_risk::{
    FACTOR_KEYS, FACTOR_SHOCKS, FactorBeta, FactorKey, MIN_BETA_DAYS, WEAK_CORRELATION, WHATIF_BETA_DAYS, factor_beta, what_if,
};
use altim_core::web::portfolio::view::parse_amount;
use yew::prelude::*;

use super::browser::input_value;

fn usd(v: f64) -> String {
    crate::money::money_with(v, 0, 0, NBSP)
}
fn signed_usd(loss: f64) -> String {
    format!(
        "{}{}",
        if loss > 0.0 {
            "−"
        } else if loss < 0.0 {
            "+"
        } else {
            ""
        },
        usd(loss.abs())
    )
}
fn signed_pct(v: f64) -> String {
    format!(
        "{}{} %",
        if v > 0.0 {
            "+"
        } else if v < 0.0 {
            "−"
        } else {
            ""
        },
        fr(v.abs(), 0, 1)
    )
}

/// Candles fetched for a factor (None: the request failed), merged as the answers come.
#[derive(Clone, PartialEq, Default)]
struct Fetched(HashMap<String, Option<Rc<Vec<Candle>>>>);

impl Reducible for Fetched {
    type Action = (String, Option<Rc<Vec<Candle>>>);
    fn reduce(self: Rc<Self>, (k, v): Self::Action) -> Rc<Self> {
        let mut m = self.0.clone();
        m.insert(k, v);
        Rc::new(Fetched(m))
    }
}

#[derive(Properties, PartialEq)]
pub struct WhatIfCardProps {
    pub analysis: Rc<PortfolioAnalysis>,
    pub daily: Rc<HashMap<String, Vec<Candle>>>,
}

#[component]
pub fn WhatIfCard(p: &WhatIfCardProps) -> Html {
    let _m = crate::money::use_money();
    let factor = use_state(|| FactorKey::Qqq);
    let shock = use_state(|| -10.0);
    let custom = use_state(String::new);
    let amount_text = use_state(String::new);
    let fetched = use_reducer(Fetched::default);
    let f = factor.factor();
    let f_key = market_key(f.kind, f.symbol);
    let own = p.daily.get(&f_key).filter(|c| !c.is_empty());

    {
        let (fetched, has) = (fetched.clone(), own.is_some());
        use_effect_with(f_key.clone(), move |k| {
            let alive = Rc::new(Cell::new(true));
            if !has && !fetched.0.contains_key(k) {
                let (a, k) = (alive.clone(), k.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let r = super::api::candles(f.symbol, f.kind, "1d").await;
                    if a.get() {
                        fetched.dispatch((k, r.ok().map(|s| Rc::new(s.candles))));
                    }
                });
            }
            move || alive.set(false)
        });
    }
    let factor_candles: Option<&[Candle]> = match own {
        Some(c) => Some(c),
        None => fetched.0.get(&f_key).and_then(|c| c.as_deref().map(Vec::as_slice)),
    };

    let custom_shock = parse_amount(custom.strip_prefix(['−', '-']).unwrap_or(&custom));
    let eff_shock = match custom_shock {
        Some(c) if !custom.trim().is_empty() && c.is_finite() && c > 0.0 && c <= 100.0 => -c,
        _ => *shock,
    };
    // Typed in the display currency; the engine works in dollars.
    let amount = parse_amount(&amount_text).map(crate::money::from_display).filter(|a| a.is_finite());
    let mut betas: HashMap<String, FactorBeta> = HashMap::new();
    if let Some(fc) = factor_candles.filter(|c| !c.is_empty()) {
        for l in &p.analysis.lines {
            let k = market_key(l.kind, &l.symbol);
            let daily = p.daily.get(&k).map(Vec::as_slice).unwrap_or(&[]);
            betas.entry(k).or_insert_with(|| factor_beta(daily, fc, WHATIF_BETA_DAYS));
        }
    }
    let loading = factor_candles.is_none() && !fetched.0.contains_key(&f_key);
    let r = what_if(&p.analysis, *factor, eff_shock, &betas, amount);
    let on_custom = {
        let custom = custom.clone();
        Callback::from(move |e: InputEvent| custom.set(input_value(&e)))
    };
    let on_amount = {
        let a = amount_text.clone();
        Callback::from(move |e: InputEvent| a.set(input_value(&e)))
    };
    html! {
        <div class="card whatif">
            <h2 class="card-title">{ "Et si… ?" }</h2>
            <p class="muted small">
                { format!(
                    "Comment le risque de ce portefeuille évolue si {} baisse de {} % ? Chaque ligne bouge selon son bêta face à ce marché.",
                    f.name,
                    fr(eff_shock.abs(), 0, 1)
                ) }
            </p>
            <div class="whatif-row">
                <span class="small muted">{ "Marché" }</span>
                <div class="agenda-chips" role="group" aria-label="Marché">
                    { for FACTOR_KEYS.iter().map(|k| {
                        let on = *k == *factor;
                        let pick = { let (factor, k) = (factor.clone(), *k); Callback::from(move |_: MouseEvent| factor.set(k)) };
                        html! {
                            <button key={k.factor().symbol} class={classes!("chip", "pick", on.then_some("on"))} aria-pressed={on.to_string()} onclick={pick}>
                                { k.factor().label }
                            </button>
                        }
                    }) }
                </div>
            </div>
            <div class="whatif-row">
                <span class="small muted">{ "Choc" }</span>
                <div class="agenda-chips" role="group" aria-label="Choc">
                    { for FACTOR_SHOCKS.iter().map(|s| {
                        let on = custom.trim().is_empty() && *s == *shock;
                        let pick = { let (shock, custom, s) = (shock.clone(), custom.clone(), *s); Callback::from(move |_: MouseEvent| { shock.set(s); custom.set(String::new()); }) };
                        html! {
                            <button key={s.to_string()} class={classes!("chip", "pick", on.then_some("on"))} aria-pressed={on.to_string()} onclick={pick}>
                                { signed_pct(*s) }
                            </button>
                        }
                    }) }
                </div>
            </div>
            <div class="grid-2">
                <label class="field">
                    <span>{ "Autre baisse (%)" }</span>
                    <input inputmode="decimal" value={(*custom).clone()} placeholder="ex. 15" oninput={on_custom} />
                </label>
                <label class="field">
                    <span>{ format!("Montant simulé ({}, facultatif)", crate::money::symbol()) }</span>
                    <input inputmode="decimal" value={(*amount_text).clone()} placeholder={usd(p.analysis.total)} oninput={on_amount} />
                </label>
            </div>
            if r.scaled {
                <p class="muted small">{ format!("Simulé sur {} répartis selon les poids actuels (liquidités comprises : {}).", usd(r.base), usd(r.cash)) }</p>
            }
            if loading {
                <p class="muted small">{ format!("Chargement des cours de {}…", f.label) }</p>
            } else if factor_candles.is_none_or(|c| c.is_empty()) {
                <p class="notice warn small">{ format!("⚠ Cours journaliers de {} indisponibles : simulation non couverte pour l'instant.", f.label) }</p>
            } else {
                <p class="whatif-total">
                    <span>{ "Perte estimée" }</span>
                    <b class={if r.loss > 0.0 { "down" } else { "up" }}>{ signed_usd(r.loss) }</b>
                    <small class="muted">{ format!("soit {} de {}", signed_pct(-r.loss_percent), if r.scaled { "ce montant" } else { "votre patrimoine" }) }</small>
                </p>
                if let Some(w) = &r.worst {
                    <p class="small">
                        { "Ligne la plus touchée : " }<b>{ w.symbol.clone() }</b>
                        { format!(" ({}, {}).", signed_pct(w.move_percent.unwrap_or(0.0)), signed_usd(w.loss.unwrap_or(0.0))) }
                    </p>
                }
                <ul class="whatif-lines">
                    { for r.lines.iter().map(|l| {
                        let detail = if l.reference {
                            " · c'est ce marché lui-même (bêta 1)".to_string()
                        } else {
                            match l.beta {
                                None => format!(" · moins de {MIN_BETA_DAYS} jours communs avec {} : bêta non mesurable", f.label),
                                Some(beta) => format!(
                                    " · bêta {}{} · {}{}",
                                    to_fixed(beta, 2),
                                    l.correlation.map(|c| format!(", corrélation {}", to_fixed(c, 2))).unwrap_or_default(),
                                    signed_pct(l.move_percent.unwrap_or(0.0)),
                                    if l.correlation.is_some_and(|c| c.abs() < WEAK_CORRELATION) {
                                        format!(" · lien faible avec {} : ce bêta explique mal les mouvements de la ligne, résultat peu fiable", f.label)
                                    } else {
                                        String::new()
                                    }
                                ),
                            }
                        };
                        html! {
                            <li key={l.key.clone()}>
                                <div class="whatif-line-head">
                                    <b>{ l.symbol.clone() }</b>
                                    <span class={match l.loss { None => "muted", Some(x) if x > 0.0 => "down", _ => "up" }}>
                                        { l.loss.map(signed_usd).unwrap_or_else(|| "non couvert".into()) }
                                    </span>
                                </div>
                                <small class="muted">{ format!("{} · {} %{detail}", usd(l.value), fr(l.weight, 0, 1)) }</small>
                            </li>
                        }
                    }) }
                </ul>
                if !r.uncovered.is_empty() {
                    <p class="muted small">{ format!("Non couvert : {} ({}) — laissé hors du total, jamais estimé.", r.uncovered.join(", "), usd(r.uncovered_value)) }</p>
                }
                <p class="muted small">
                    { format!(
                        "{} ; hypothèse : choc instantané, relation stable — rarement vrai en crise (les corrélations montent quand tout baisse). Les liquidités ne bougent pas. Ce n'est pas une prévision.",
                        match (r.min_days, r.max_days) {
                            (Some(lo), hi) => format!(
                                "Bêta estimé sur {lo}{} jours de rendements journaliers communs",
                                hi.filter(|h| *h != lo).map(|h| format!(" à {h}")).unwrap_or_default()
                            ),
                            _ => "Aucun bêta mesuré".into(),
                        }
                    ) }
                </p>
            }
        </div>
    }
}
