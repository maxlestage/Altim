//! More of the signal's track record (TrackDetails.tsx, inside "Historique du signal"): spread cost, expectancy,
//! R multiples, results by market regime, how the test avoids flattering itself, and the tax note. The card shows
//! it only when the answer has these fields (older answers show nothing more).
use altim_core::engine::backtest::Regime;
use altim_core::engine::decision_types::Track;
use altim_core::web::decision::format::{num, pct};
use altim_core::web::decision::rows::{FLAT_TAX, after_tax};
use yew::prelude::*;

use crate::route::use_on_link;

#[derive(Properties, PartialEq)]
pub struct TrackDetailsProps {
    pub track: Track,
    /// The spread was sent (`t.spreadPct != null`).
    #[prop_or(true)]
    pub spread: bool,
}

#[component]
pub fn TrackDetails(p: &TrackDetailsProps) -> Html {
    let on_link = use_on_link();
    let t = &p.track;
    let x = &t.details;
    let after_tax = after_tax(t.total_return);
    html! {
        <div class="track-details">
            <p class="kv small">
                <span>{ "Espérance par trade (coûts inclus)" }</span>
                <b class={if x.expectancy.unwrap_or(0.0) >= 0.0 { "up" } else { "down" }}>{ pct(x.expectancy, 2, true) }</b>
            </p>
            <p class="kv small">
                <span>{ "Multiple de R moyen (gain ÷ risque jusqu'au stop)" }</span>
                <b>{ x.avg_r.map(|r| format!("{} R", num(Some(r), 2))).unwrap_or_else(|| "—".into()) }</b>
            </p>
            if p.spread {
                <p class="kv small">
                    <span>{ format!("Écart achat/vente {}", if x.spread_measured { "mesuré" } else { "supposé" }) }</span>
                    <b>{ pct(Some(x.spread_pct), 3, false) }</b>
                </p>
            }
            if !x.spread_note.is_empty() {
                <p class="muted small">{ x.spread_note.clone() }</p>
            }

            if !x.regimes.is_empty() {
                <p class="small"><b>{ "Selon le régime de marché" }</b></p>
                <ul class="insights">
                    { for x.regimes.iter().map(|g| {
                        let cls = if g.low_sample { "info" } else if g.avg_return.unwrap_or(0.0) >= 0.0 { "good" } else { "warning" };
                        let icon = match g.regime {
                            Regime::Bull => "↗",
                            Regime::Bear => "↘",
                            Regime::Crisis => "⚠",
                            _ => "→",
                        };
                        html! {
                            <li key={altim_core::engine::backtest::regime_label(g.regime)} class={format!("insight {cls}")}>
                                <span aria-hidden="true">{ icon }</span>
                                <span class="small">
                                    <b>{ g.label.clone() }</b>
                                    <br />
                                    { format!("{} trade{}", g.trades, if g.trades > 1 { "s" } else { "" }) }
                                    if g.trades > 0 {
                                        { format!(" · réussite {} · moyenne {}", pct(g.win_rate, 0, false), pct(g.avg_return, 1, true)) }
                                    }
                                    if g.low_sample {
                                        { " · " }<span class="chip muted">{ "échantillon trop faible" }</span>
                                    }
                                </span>
                            </li>
                        }
                    }) }
                </ul>
                <p class="muted small">
                    { "Haussier : clôture au-dessus d'une moyenne 200 jours qui monte (sur 20 jours) ; baissier : sous une moyenne qui baisse ; crise : plus de 30 % sous le plus haut de l'année. Régime lu à la date du signal, sans données futures." }
                </p>
            }

            if !x.bias_notes.is_empty() {
                <p class="small"><b>{ "Comment ce test évite de se flatter" }</b></p>
                <ul class="reasons">{ for x.bias_notes.iter().map(|n| html! { <li key={n.clone()}>{ n.clone() }</li> }) }</ul>
            }

            <p class="muted small">
                { format!(
                    "Impôt (hypothèse : flat tax de {} % sur le gain net, payée à la fin, pertes compensées) : rendement du signal après impôt ≈ {}. Votre situation fiscale peut différer.",
                    FLAT_TAX, pct(Some(after_tax), 1, true)
                ) }
            </p>
            <p class="small">
                <a href="/app/validation" onclick={on_link} class="link">{ "Validation du modèle : le même test sur 34 actifs →" }</a>
            </p>
        </div>
    }
}
