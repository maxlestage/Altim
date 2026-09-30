//! Risk cards of Mes avoirs (RiskCards.tsx): market shocks through each line's beta (`StressCard`) and the portfolio
//! against the user's own limits (`LimitsCard`).
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::js::{fr, to_fixed};
use altim_core::types::Kind;
use altim_core::web::money::NBSP;
use altim_core::web::portfolio::holdings::{LineAnalysis, PortfolioAnalysis, market_key};
use altim_core::web::portfolio::portfolio_risk::{BetaEstimate, LimitCheck, LimitLevel, MIN_BETA_DAYS, StressResult, benchmark};
use yew::prelude::*;

use crate::route::use_on_link;

fn usd(v: f64) -> String {
    crate::money::money_with(v.abs(), 0, 0, NBSP)
}

fn pc(v: f64) -> String {
    format!("{} %", fr(v.abs(), 0, 1))
}

#[derive(Properties, PartialEq)]
pub struct StressCardProps {
    pub analysis: Rc<PortfolioAnalysis>,
    pub results: Rc<Vec<StressResult>>,
    pub betas: Rc<HashMap<String, BetaEstimate>>,
}

/// Market shocks passed through each line's beta: loss in $ and %, worst line, what the cash cushions.
#[component]
pub fn StressCard(p: &StressCardProps) -> Html {
    let _m = crate::money::use_money();
    let a = &p.analysis;
    // One entry per asset (first place, last line), as `new Map(lines.map(…)).values()`.
    let mut assets: Vec<(String, &LineAnalysis)> = Vec::new();
    for l in &a.lines {
        let k = market_key(l.kind, &l.symbol);
        match assets.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.1 = l,
            None => assets.push((k, l)),
        }
    }
    let beta = |k: &str| p.betas.get(k);
    let estimated: Vec<&(String, &LineAnalysis)> = assets.iter().filter(|(k, _)| beta(k).is_some_and(|b| b.estimated)).collect();
    let reference: Vec<&(String, &LineAnalysis)> = assets.iter().filter(|(k, _)| beta(k).is_some_and(|b| b.reference)).collect();
    let fallback: Vec<&(String, &LineAnalysis)> = assets.iter().filter(|(k, _)| !beta(k).is_some_and(|b| b.estimated || b.reference)).collect();
    let days: Vec<usize> = estimated.iter().filter_map(|(k, _)| beta(k).map(|b| b.days)).collect();
    let symbols = |v: &[&(String, &LineAnalysis)]| v.iter().map(|(_, l)| l.symbol.as_str()).collect::<Vec<_>>().join(", ");
    let mut note = String::from("Hypothèse : choc instantané, bêta constant. ");
    if !estimated.is_empty() {
        let (lo, hi) = (days.iter().min().copied().unwrap_or(0), days.iter().max().copied().unwrap_or(0));
        let list: Vec<String> =
            estimated.iter().map(|(k, l)| format!("{} {}", l.symbol, to_fixed(beta(k).map(|b| b.beta).unwrap_or(1.0), 2))).collect();
        note += &format!(
            "Bêta estimé sur {lo}{} jours de rendements journaliers ({}). ",
            if hi != lo { format!(" à {hi}") } else { String::new() },
            list.join(", ")
        );
    }
    if !reference.is_empty() {
        note += &format!("{} : référence elle-même, bêta 1. ", symbols(&reference));
    }
    if !fallback.is_empty() {
        note +=
            &format!("Historique trop court (moins de {MIN_BETA_DAYS} jours communs) ou indisponible pour {} : bêta 1 retenu. ", symbols(&fallback));
    }
    note += "Une vraie crise peut aller plus loin : les corrélations montent quand tout baisse.";
    html! {
        <div class="card">
            <h2 class="card-title">{ "Scénarios de crise" }</h2>
            <p class="muted small">
                { format!(
                    "Ce que perdrait votre portefeuille si les marchés chutaient d'un coup : chaque ligne bouge selon son bêta face à sa référence ({} pour les cryptos, {} pour les actions). Vos liquidités ({}) ne bougent pas.",
                    benchmark(Kind::Crypto).label,
                    benchmark(Kind::Stock).label,
                    usd(a.cash)
                ) }
            </p>
            <ul class="insights">
                { for p.results.iter().map(|r| {
                    let level = if r.loss_percent >= 10.0 { "danger" } else if r.loss > 0.0 { "warning" } else { "good" };
                    html! {
                        <li key={r.scenario.key} class={classes!("insight", level)}>
                            <span aria-hidden="true">{ if r.loss > 0.0 { "↘" } else { "→" } }</span>
                            <span>
                                <b>{ r.scenario.label }</b>
                                <br />
                                { format!("{} ≈ ", if r.loss >= 0.0 { "Perte" } else { "Gain" }) }
                                <b class={if r.loss > 0.0 { "down" } else { "up" }}>{ format!("{}{}", if r.loss > 0.0 { "−" } else { "+" }, usd(r.loss)) }</b>
                                { format!(", soit {} du patrimoine", pc(r.loss_percent)) }
                                if a.cash > 0.0 && r.loss > 0.0 {
                                    { format!(
                                        " ({} de vos placements : les liquidités amortissent {})",
                                        pc(r.invested_loss_percent),
                                        pc(r.invested_loss_percent - r.loss_percent)
                                    ) }
                                }
                                { "." }
                                if let Some(w) = &r.worst {
                                    <br />
                                    <small class="muted">
                                        { format!(
                                            "Ligne la plus touchée : {} ({}{}, −{})",
                                            w.symbol,
                                            if w.move_percent > 0.0 { "+" } else { "−" },
                                            pc(w.move_percent),
                                            usd(w.loss)
                                        ) }
                                    </small>
                                }
                            </span>
                        </li>
                    }
                }) }
            </ul>
            <p class="muted small">{ note }</p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct LimitsCardProps {
    pub checks: Rc<Vec<LimitCheck>>,
}

/// The portfolio against the user's own limits (Réglages).
#[component]
pub fn LimitsCard(p: &LimitsCardProps) -> Html {
    let on_link = use_on_link();
    if p.checks.is_empty() {
        return html! {};
    }
    html! {
        <div class="card">
            <h2 class="card-title">{ "Vos limites de risque" }</h2>
            <ul class="insights">
                { for p.checks.iter().map(|c| {
                    let (icon, class) = match c.level {
                        LimitLevel::Danger => ("⛔", "danger"),
                        LimitLevel::Warning => ("⚠", "warning"),
                        LimitLevel::Ok => ("✔", "good"),
                        LimitLevel::Na => ("?", "info"),
                    };
                    html! {
                        <li key={c.code} class={classes!("insight", class)}>
                            <span aria-hidden="true">{ icon }</span>
                            <span><b>{ c.label.clone() }</b><br />{ c.detail.clone() }</span>
                        </li>
                    }
                }) }
            </ul>
            <p class="muted small">
                { "Limites réglables dans " }
                <a href="/app/reglages" onclick={on_link} class="link">{ "Réglages → Prudence des conseils" }</a>
                { ". Hypothèse : stop que vous avez saisi, sinon stop de protection à 2 × la volatilité journalière ; variation du jour mesurée depuis la clôture de la veille (journée UTC pour les cryptos, dernière séance pour les actions)." }
            </p>
        </div>
    }
}
