//! « Pourquoi ça bouge ? » on the asset screen (WhyCard.tsx, /api/why): co-occurring observations, never presented
//! as causes, and the optional question on these data (/api/ask).
use altim_core::types::Kind;
use altim_core::web::decision::reports::{AskAnswer, WhyReport};
use yew::prelude::*;

/// The `value` of the element an input event comes from (textarea, input).
pub fn event_value(e: &Event) -> String {
    e.target().and_then(|t| js_sys::Reflect::get(&t, &"value".into()).ok()).and_then(|v| v.as_string()).unwrap_or_default()
}

fn dir_icon(d: &str) -> &'static str {
    match d {
        "up" => "↗",
        "down" => "↘",
        _ => "→",
    }
}
fn dir_word(d: &str) -> &'static str {
    match d {
        "up" => "haussier",
        "down" => "baissier",
        _ => "neutre",
    }
}
fn mag_word(m: &str) -> &'static str {
    match m {
        "low" => "faible",
        "medium" => "moyen",
        _ => "fort",
    }
}
fn cert_class(c: &str) -> &'static str {
    match c {
        "observed" => "good",
        "possibleCorrelation" => "info",
        _ => "warning",
    }
}

#[derive(Properties, PartialEq)]
pub struct WhyCardProps {
    pub symbol: AttrValue,
    pub kind: Kind,
}

#[component]
pub fn WhyCard(p: &WhyCardProps) -> Html {
    let report = use_state(|| None::<WhyReport>);
    let error = use_state(|| None::<String>);
    let question = use_state(String::new);
    let asking = use_state(|| false);
    let answer = use_state(|| None::<AskAnswer>);
    let ask_error = use_state(|| None::<String>);

    {
        let (report, error) = (report.clone(), error.clone());
        use_effect_with((p.symbol.clone(), p.kind), move |(symbol, kind)| {
            let alive = std::rc::Rc::new(std::cell::Cell::new(true));
            report.set(None);
            error.set(None);
            let (a, symbol, kind) = (alive.clone(), symbol.to_string(), *kind);
            wasm_bindgen_futures::spawn_local(async move {
                let r = super::api::why(&symbol, kind).await;
                if a.get() {
                    match r {
                        Ok(r) => report.set(Some(r)),
                        Err(e) => error.set(Some(e.0)),
                    }
                }
            });
            move || alive.set(false)
        });
    }

    let ask = {
        let (question, asking, answer, ask_error) = (question.clone(), asking.clone(), answer.clone(), ask_error.clone());
        let (symbol, kind) = (p.symbol.to_string(), p.kind);
        Callback::from(move |_: MouseEvent| {
            let q = question.trim().to_string();
            if q.chars().count() < 3 || *asking {
                return;
            }
            asking.set(true);
            ask_error.set(None);
            answer.set(None);
            let (asking, answer, ask_error, symbol) = (asking.clone(), answer.clone(), ask_error.clone(), symbol.clone());
            wasm_bindgen_futures::spawn_local(async move {
                match super::api::ask(&symbol, kind, &q).await {
                    Ok(a) => answer.set(Some(a)),
                    Err(e) => ask_error.set(Some(e.0)),
                }
                asking.set(false);
            });
        })
    };
    let on_input = {
        let question = question.clone();
        Callback::from(move |e: InputEvent| question.set(event_value(&e)))
    };
    let too_short = question.trim().chars().count() < 3;

    html! {
        <div class="card why">
            <h2 class="card-title">{ "Pourquoi ça bouge ?" }</h2>
            if let Some(e) = &*error {
                <p class="notice warn small">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <div class="skeleton" aria-label="Chargement des observations" />
            }
            if let Some(r) = &*report {
                <p class="why-summary">{ r.summary.clone() }</p>
                <ul class="why-factors">
                    { for r.factors.iter().map(|f| html! {
                        <li key={f.key.clone()} class={format!("insight {}", cert_class(&f.certainty))}>
                            <span aria-label={dir_word(&f.direction)}>{ dir_icon(&f.direction) }</span>
                            <span>
                                <b>{ f.label.clone() }</b>{ " " }<span class="chip muted">{ f.certainty_label.clone() }</span>
                                <br />
                                { f.detail.clone() }
                                <br />
                                <small class="muted">{ format!("Sens {}, ampleur {} · source : {}", dir_word(&f.direction), mag_word(&f.magnitude), f.source) }</small>
                            </span>
                        </li>
                    }) }
                </ul>
                if !r.not_covered.is_empty() {
                    <p class="muted small">
                        { format!(
                            "Non couvert : {}.",
                            r.not_covered.iter().map(|n| format!("{} ({})", n.label.to_lowercase(), n.reason.strip_suffix('.').unwrap_or(&n.reason))).collect::<Vec<_>>().join(" ; ")
                        ) }
                    </p>
                }
                <p class="muted small">{ r.disclaimer.clone() }</p>

                if r.ask_enabled {
                    <div class="why-ask">
                        <label class="field">
                            <span>{ "Poser une question sur ces données" }</span>
                            <textarea rows="2" maxlength="500" value={(*question).clone()} placeholder="ex. La baisse vient-elle du marché ou de l'actif ?" oninput={on_input} />
                        </label>
                        <button class="btn btn-ghost" onclick={ask} disabled={*asking || too_short}>{ if *asking { "Réponse en cours…" } else { "Demander" } }</button>
                        <small class="muted">
                            { "Réponse d'un modèle d'IA (Claude, Anthropic) à partir des seules données ci-dessus, envoyées à Anthropic avec votre question (jamais vos avoirs). Elle peut se tromper ; ce n'est pas un conseil. Quelques questions par minute au plus." }
                        </small>
                        if let Some(e) = &*ask_error {
                            <p class="notice warn small">{ format!("⚠ {e}") }</p>
                        }
                        if let Some(a) = &*answer {
                            <div class="why-answer" role="status">
                                <p>{ a.answer.clone() }</p>
                                <small class="muted">
                                    { format!("Données utilisées : les observations ci-dessus{}.", if a.used_decision() { " et la dernière décision calculée" } else { "" }) }
                                </small>
                            </div>
                        }
                    </div>
                }
            }
        </div>
    }
}
