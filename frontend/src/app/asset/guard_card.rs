//! Market guard (GuardCard.tsx): background regime, shock risk, counter-trend reversal risk, and what a short-term
//! bot should do.
use altim_core::engine::guard::{Direction, FactorStatus, GuardFactor, Scalping, ShockLevel, Trend};
use altim_core::web::decision::reports::GuardReport;
use yew::prelude::*;

fn status_text(s: FactorStatus) -> &'static str {
    match s {
        FactorStatus::Verified => "vérifié sur cet actif",
        FactorStatus::Unproven => "peu d'historique : compté à moitié",
        FactorStatus::Rejected => "jamais prédictif ici : ignoré",
        FactorStatus::Unverifiable => "sans historique : non vérifié",
    }
}

fn meter(label: &str, value: f64, tone: &str) -> Html {
    let v = altim_core::js::number_to_string(value);
    html! {
        <div class="guard-meter" role="img" aria-label={format!("{label} : {v} sur 100")}>
            <div class="guard-meter-head"><span>{ label.to_string() }</span><b class="mono">{ format!("{v}/100") }</b></div>
            <div class="weight-track"><i class={format!("guard-{tone}")} style={format!("width: {}%;", altim_core::js::number_to_string(value.max(2.0)))} /></div>
        </div>
    }
}

fn factors(list: &[GuardFactor]) -> Html {
    if list.is_empty() {
        return html! { <p class="muted small">{ "Aucun signal." }</p> };
    }
    let r = |x: f64| altim_core::js::round(x);
    html! {
        <ul class="guard-factors">
            { for list.iter().map(|f| html! {
                <li key={f.code.clone()} class={if f.status == FactorStatus::Rejected { "muted" } else { "" }}>
                    <span>{ f.text.clone() }</span>
                    <small class="muted">
                        { format!(" +{} · {}", altim_core::js::number_to_string(f.points), status_text(f.status)) }
                        if let Some(e) = f.evidence.filter(|e| e.samples > 0.0) {
                            { format!(" ({} % des {} cas passés contre {} % d'habitude)", r(e.rate), altim_core::js::number_to_string(e.samples), r(e.base)) }
                        }
                    </small>
                </li>
            }) }
        </ul>
    }
}

/// `toLocaleString("fr-FR")` of a multiplier (3 decimals at most).
fn fr_num(v: f64) -> String {
    altim_core::js::fr(v, 0, 3)
}

#[derive(Properties, PartialEq)]
pub struct GuardCardProps {
    pub g: GuardReport,
}

#[component]
pub fn GuardCard(p: &GuardCardProps) -> Html {
    let g = &p.g;
    let r = &g.result;
    let shock_tone = match r.shock.level {
        ShockLevel::Shock => "bad",
        ShockLevel::Agitated => "warn",
        ShockLevel::Calm => "ok",
    };
    let rev_tone = if r.reversal.score >= 50.0 {
        "bad"
    } else if r.reversal.score >= 25.0 {
        "warn"
    } else {
        "ok"
    };
    let level = match r.shock.level {
        ShockLevel::Calm => "Calme",
        ShockLevel::Agitated => "Agité",
        ShockLevel::Shock => "Choc",
    };
    let trend = match r.regime.trend {
        Trend::Up => "Haussière",
        Trend::Down => "Baissière",
        Trend::Range => "Sans direction",
    };
    let policy = match r.policy.scalping {
        Scalping::Ok => "Autorisé",
        Scalping::Reduce => "Taille réduite",
        Scalping::Pause => "Suspendu",
    };
    let rev_label = format!(
        "Risque de retournement{}",
        match r.reversal.direction {
            Some(Direction::Down) => " à la baisse",
            Some(Direction::Up) => " à la hausse",
            None => "",
        }
    );
    html! {
        <div class="card guard">
            <h2 class="card-title">{ "Garde-fou marché" }</h2>
            <p class="kv"><span>{ "Tendance de fond" }</span><b>{ format!("{trend} · force {}/100", altim_core::js::number_to_string(r.regime.strength)) }</b></p>
            <p class="muted small">{ r.regime.text.clone() }</p>
            { meter(&format!("Risque de choc · {level}"), r.shock.score, shock_tone) }
            { factors(&r.shock.factors) }
            { meter(&rev_label, r.reversal.score, rev_tone) }
            { factors(&r.reversal.factors) }
            <div class="guard-policy">
                <p class="kv"><span>{ "Trading court terme (bots)" }</span><b>{ policy }</b></p>
                <p class="kv small"><span>{ format!("Taille × {}", fr_num(r.policy.size_multiplier)) }</span><span>{ format!("Stop × {}", fr_num(r.policy.stop_multiplier)) }</span></p>
                <ul>{ for r.policy.notes.iter().map(|n| html! { <li key={n.clone()}>{ n.clone() }</li> }) }</ul>
            </div>
            if !g.inputs.headlines.is_empty() {
                <details>
                    <summary class="small">{ format!("Dernières actualités ({} en 24 h)", g.inputs.news24h) }</summary>
                    <ul class="guard-news">
                        { for g.inputs.headlines.iter().map(|h| html! {
                            <li key={h.title.clone()} class="small">{ format!("{} ", h.title) }<span class="muted">{ format!("· {}", crate::ui::fr_time(h.time)) }</span></li>
                        }) }
                    </ul>
                </details>
            }
            <p class="muted small">
                { "Aucun outil ne prévoit une vraie surprise. Le garde-fou mesure les conditions où les grands mouvements et les retournements sont plus probables, et chaque signal technique est vérifié sur l'historique de l'actif. API pour vos bots : " }
                <code>{ format!("/api/guard?symbol={}&kind={}", g.symbol, g.kind.as_str()) }</code>
            </p>
        </div>
    }
}
