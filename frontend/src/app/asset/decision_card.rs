//! The « Décision » card (DecisionCard.tsx): `DecisionView` renders a decision, `DecisionCard` loads it (every
//! 5 minutes while visible and when the connection returns) and shows the last one cached in this browser while
//! loading or when the server is unreachable, `DecisionBadge` is the Radar's verdict (from the cache, < 12 h).
use std::cell::Cell;
use std::rc::Rc;

use altim_core::calendar::CalendarEvent;
use altim_core::engine::decision_types::{Fundamentals, Veto};
use altim_core::types::Kind;
use altim_core::web::decision::config_changes::{ConfigTransition, latest_change};
use altim_core::web::decision::doc::{DecisionDoc, PersonalInput};
use altim_core::web::decision::format::*;
use altim_core::web::decision::rows;
use altim_core::web::insights::calendar::day_label;
use yew::prelude::*;

use super::decision_parts::*;
use super::store;
use super::track_details::TrackDetails;
use crate::app::news::{AgendaEvent, local_today};
use crate::app::simulation::SimulateBuy;

/// Where the decision shown comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionStatus {
    Fresh,
    /// The cached one (received at `at`) while the new one loads.
    Refreshing {
        at: f64,
    },
    /// The server did not answer: the cached one.
    Stale {
        at: f64,
        offline: bool,
        error: String,
    },
}

#[derive(Properties, PartialEq)]
pub struct DecisionViewProps {
    pub d: Rc<DecisionDoc>,
    #[prop_or(DecisionStatus::Fresh)]
    pub status: DecisionStatus,
    #[prop_or_default]
    pub on_retry: Option<Callback<()>>,
    /// Shows "Simuler cet achat" (paper trading) with the live price when known.
    #[prop_or_default]
    pub simulate: Option<Option<f64>>,
    /// The configuration change that led to this decision (this browser's history), explained.
    #[prop_or_default]
    pub change: Option<ConfigTransition>,
}

fn vetoes_section(vetoes: &[&Veto]) -> Html {
    html! {
        <ul class="dec-vetoes">
            { for vetoes.iter().map(|v| {
                let (cls, icon, word) = if v.active { ("active", "⛔", "ACTIVE") } else if v.verifiable { ("pass", "✓", "vérifié") } else { ("unknown", "?", "non vérifiable") };
                html! {
                    <li key={v.code.clone()} class={format!("veto-{cls}")}>
                        <span class="dec-veto-mark"><span aria-hidden="true">{ icon }</span>{ " " }{ word }</span>
                        <div><b>{ v.label.clone() }</b><small class="muted">{ v.detail.clone() }</small></div>
                    </li>
                }
            }) }
        </ul>
    }
}

#[component]
pub fn DecisionView(p: &DecisionViewProps) -> Html {
    let m = crate::money::use_money();
    let m = &*m;
    let doc = &*p.d;
    // DecisionCard shows only decisions whose whole type decodes (parse_decision_full, `cached_full`).
    let Some(d) = doc.full() else { return Html::default() };
    let (lv_icon, lv_label) = level_ui(d.level);
    let vetoes = sort_vetoes(&d.vetoes);
    let active = vetoes.iter().filter(|v| v.active).count();
    let fam = summary_families(&d.families);
    // The 6-level rating is the headline when the server gives it (older answers: the verdict).
    let rt = doc.rating().map(rating_ui);
    let regime = d.market_regime.as_ref();
    let confidence = altim_core::js::round(d.confidence);
    let label = match (doc.rating(), rt) {
        (Some(_), Some(rt)) => {
            if d.rating_label.is_empty() {
                rt.1.to_string()
            } else {
                d.rating_label.clone()
            }
        }
        _ => d.label.clone(),
    };
    let level_label = if d.level_label.is_empty() { lv_label.to_string() } else { d.level_label.clone() };
    let conf_width = altim_core::js::number_to_string(d.confidence.clamp(2.0, 100.0));
    let plan = d.plan.as_ref();
    html! {
        <article class={format!("card decision lv-{}", level_key(d.level))} aria-labelledby="dec-title">
            <p class="dec-date">
                <b class="mono">{ d.symbol.clone() }</b>{ format!(" — {}", long_date(d.as_of as f64)) }
                <span class="muted">{ format!(" · {}", d.name) }</span>
            </p>

            { match &p.status {
                DecisionStatus::Refreshing { at } => html! { <p class="muted small dec-status">{ format!("Décision du {} · mise à jour…", short_date_time(*at)) }</p> },
                DecisionStatus::Stale { at, offline, error } => html! {
                    <p class="notice warn small dec-status" role="status">
                        { if *offline { "Hors ligne".to_string() } else { format!("Serveur injoignable ({error})") } }
                        { format!(" : dernière décision connue, du {}.", short_date_time(*at)) }
                        if let Some(r) = &p.on_retry {
                            { " " }<button class="link-btn" onclick={let r = r.clone(); Callback::from(move |_| r.emit(()))}>{ "Réessayer" }</button>
                        }
                    </p>
                },
                DecisionStatus::Fresh => html! {},
            } }

            <div class="dec-verdict">
                <h2 id="dec-title" class="dec-label" data-tone={rt.map(|r| level_key(r.2))}>
                    <span aria-hidden="true">{ rt.map(|r| r.0).unwrap_or(lv_icon) }</span>{ " " }{ label }
                </h2>
                <span class="dec-level-label">{ level_label }</span>
                <div class="dec-confidence">
                    <span>{ "Confiance du modèle : " }<b class="mono">{ format!("{confidence}/100") }</b></span>
                    <div class="weight-track" role="img" aria-label={format!("Confiance {confidence} sur 100")}>
                        <i style={format!("width: {conf_width}%;")} />
                    </div>
                </div>
            </div>
            if rt.is_some() {
                <p class="small">{ "Verdict du plan : " }<b>{ d.label.clone() }</b>{ " " }<span class="muted">{ "· la note résume verdict, niveau et confiance" }</span></p>
            }
            if let Some(r) = d.rating_reason.as_ref().filter(|r| !r.is_empty()) {
                <p class="small muted">{ r.clone() }</p>
            }
            <p class="muted small">{ d.confidence_text.clone() }</p>
            if let Some(e) = doc.model_evidence() {
                <EvidenceLine e={e.clone()} />
            }
            if let Some(b) = doc.bot() {
                <BotLine b={b.clone()} />
            }
            <p class={classes!("dec-mode", (d.mode == "personal").then_some("personal"))}>{ mode_text(&d.mode) }</p>
            if let Some(regime) = regime {
                <p class="small">
                    <span class="muted">{ "Régime de marché : " }</span>
                    <b><span aria-hidden="true">{ regime_ui(regime.kind).0 }</span>{ " " }{ regime_ui(regime.kind).1 }</b>
                    if !regime.reasons.is_empty() {
                        <small class="muted">{ format!(" — {}", regime.reasons.join(" · ")) }</small>
                    }
                </p>
            }

            if let Some(dg) = doc.degraded().filter(|g| g.active) {
                <div class="notice danger" role="alert">
                    <b>{ dg.headline.clone() }</b>
                    if !dg.reasons.is_empty() {
                        <ul class="small">{ for dg.reasons.iter().map(|r| html! { <li key={r.clone()}>{ r.clone() }</li> }) }</ul>
                    }
                </div>
            }

            if let Some(n) = doc.no_trade() {
                { no_trade_banner(n) }
            }

            <p class="dec-headline">{ d.headline.clone() }</p>

            if let Some(t) = &p.change {
                { change_block(t) }
            }

            if !fam.is_empty() {
                <ul class="dec-chips" aria-label="Résumé par famille">
                    { for fam.iter().map(|(key, name, family)| {
                        let t = family_tone(family.status, family.score);
                        html! {
                            <li key={*key} class={format!("dec-chip tone-{}", t.tone)} title={family.summary.clone()}>
                                <span>{ *name }</span>{ " " }<span aria-hidden="true">{ t.icon }</span>{ " " }<small>{ t.text }</small>
                            </li>
                        }
                    }) }
                </ul>
            }

            if let Some(s) = doc.score() {
                { score_block(s) }
            }

            { match plan {
                Some(p) => html! {
                    <div class="dec-plan">
                        <div><small>{ "Zone d'achat" }</small><b>{ format!("{} – {}", usd(Some(p.zone_from), m), usd(Some(p.zone_to), m)) }</b></div>
                        <div><small>{ "Stop / invalidation" }</small><b class="sell">{ usd(Some(p.stop), m) }</b><small class="muted">{ pct(Some(-p.risk_pct), 1, false) }</small></div>
                        <div><small>{ "Objectif 1" }</small><b class="buy">{ usd(Some(p.target1), m) }</b><small class="muted">{ pct(Some(p.reward1_pct), 1, true) }</small></div>
                        <div>
                            <small>{ "Objectif 2" }</small>
                            { match p.target2 {
                                Some(t) => html! { <><b class="buy">{ usd(Some(t), m) }</b><small class="muted">{ pct(p.reward2_pct, 1, true) }</small></> },
                                None => html! { <b class="muted">{ "aucun" }</b> },
                            } }
                        </div>
                        <div title={p.target3_source.clone()}>
                            <small>{ "Objectif 3" }</small>
                            { match p.target3 {
                                Some(t) => html! { <><b class="buy">{ usd(Some(t), m) }</b><small class="muted">{ pct(p.reward3_pct, 1, true) }</small></> },
                                None => html! { <b class="muted">{ "aucun" }</b> },
                            } }
                        </div>
                        <div class={if p.acceptable { "rr-ok" } else { "rr-ko" }}>
                            <small>{ format!("Gain/risque (entrée {})", usd(Some(p.entry), m)) }</small>
                            <b>{ risk_reward_text(p) }</b>
                            <small>{ if p.acceptable { "✓ suffisant" } else { "✕ insuffisant" } }</small>
                        </div>
                        <p class="muted small dec-horizon">
                            { "Horizon : " }
                            { match &d.horizon {
                                Some(h) => html! { <><b class="dec-horizon-kind">{ h.label.clone() }</b>{ format!(" — {}", h.detail) }</> },
                                None => html! { p.horizon.clone() },
                            } }
                            if let Some(s) = p.target3_source.as_ref().filter(|s| !s.is_empty()) {
                                <br />{ format!("Objectif 3 : {}.", uncapitalize(s)) }
                            }
                        </p>
                    </div>
                },
                None => html! { <p class="muted small">{ "Pas de plan d'entrée : aucun niveau net (zone, stop et objectifs) sur cet actif pour l'instant." }</p> },
            } }
            if let Some(z) = &d.action_zones {
                { action_ladder(z, m) }
            }

            if let Some(pos) = &d.position {
                <div class="dec-block dec-position">
                    <h3>{ "Votre position" }</h3>
                    <p class="kv small"><span>{ "Prix d'achat moyen" }</span><b class="mono">{ usd(Some(pos.cost), m) }</b></p>
                    if let Some(pnl) = pos.pnl_pct {
                        <p class="kv small"><span>{ "Plus-value latente" }</span><b class={format!("mono {}", up_down(Some(pnl)))}>{ pct(Some(pnl), 1, true) }</b></p>
                    }
                    <p class="small">{ pos.advice.clone() }</p>
                    if !pos.exits.is_empty() {
                        <ul class="dec-exits">
                            { for pos.exits.iter().map(|e| html! {
                                <li key={format!("{}-{}", exit_kind_key(e.kind), e.trigger)} class={if e.now { "now" } else { "" }}>
                                    <div>
                                        <b>{ exit_text(e.share, &e.trigger) }</b>
                                        <small class="muted">{ format!("{}{}", exit_kind_label(e.kind), e.price.map(|p| format!(" · {}", usd(Some(p), m))).unwrap_or_default()) }</small>
                                    </div>
                                    if e.now {
                                        <span class="dec-now">{ "▶ maintenant" }</span>
                                    }
                                </li>
                            }) }
                        </ul>
                    }
                    <p class="muted small">{ "Sorties progressives : le reste de la position est conservé tant que le scénario tient." }</p>
                </div>
            }

            if let Some(x) = &d.exposure {
                { match &x.warning {
                    Some(w) if !w.is_empty() => html! {
                        <p class="notice warn small" role="note">
                            { format!("⚠ {w} Actifs concernés : {}", x.assets.join(", ")) }
                            { x.correlation.map(|c| format!(" ; corrélation de {} au {} : {}", d.symbol, x.factor, num(Some(c), 2))).unwrap_or_default() }
                            { "." }
                        </p>
                    },
                    _ => html! {
                        <p class="muted small">
                            { format!("Exposition au facteur {} : {} du portefeuille", x.factor, pct(Some(x.weight), 0, false)) }
                            { x.correlation.map(|c| format!(" (corrélation {})", num(Some(c), 2))).unwrap_or_default() }
                            { "." }
                        </p>
                    },
                } }
            }

            if !d.why_wait.is_empty() {
                <div class="dec-block">
                    <h3>{ "Pourquoi attendre ?" }</h3>
                    <ul class="dec-list">{ for d.why_wait.iter().map(|w| html! { <li key={w.clone()}>{ w.clone() }</li> }) }</ul>
                </div>
            }
            { conditions("Pour passer en ACHAT", &d.to_buy, m) }
            { conditions("Pour passer en VENTE", &d.to_sell, m) }
            if let Some(c) = doc.counter_argument() {
                { counter_block(c) }
            }

            <div class="dec-sections">
                if let Some(n) = doc.no_trade() {
                    { section(
                        "Quand ne pas trader",
                        Some(if n.active { format!("{} raison{}", n.reasons.len(), if n.reasons.len() > 1 { "s" } else { "" }) } else { "rien à signaler".into() }),
                        no_trade_list(n),
                    ) }
                }

                { section(
                    "Familles d'indices",
                    Some(format!("{}/{} disponibles", d.families.iter().filter(|f| f.status != altim_core::engine::decision_types::Status::Unavailable).count(), d.families.len())),
                    html! { <ul class="dec-families">{ for d.families.iter().map(family_row) }</ul> },
                ) }

                if let Some(st) = &d.structure {
                    { section(
                        "Structure technique",
                        Some(st.score.map(|s| format!("direction {}", signed_score(s))).unwrap_or_else(|| "non disponible".into())),
                        structure_list(st, m),
                    ) }
                }

                if let Some(events) = doc.events() {
                    { section(
                        "Agenda (7 jours)",
                        Some(match events {
                            None => "non vérifié".into(),
                            Some([]) => "rien de majeur".into(),
                            Some(e) => format!("{} événement{}", e.len(), if e.len() > 1 { "s" } else { "" }),
                        }),
                        match events {
                            None => html! { <p class="small muted">{ "Calendrier indisponible ou incomplet : les annonces à venir n'ont pas pu être vérifiées." }</p> },
                            Some([]) => html! {
                                <p class="small muted">
                                    { format!("Aucune annonce majeure (banques centrales, inflation, emploi, PIB){} dans les 7 jours.", if d.kind == Kind::Stock { ", ni résultats, dividende ou split" } else { "" }) }
                                </p>
                            },
                            Some(e) => {
                                let today = local_today();
                                html! {
                                    <ul class="news-list">
                                        { for e.iter().enumerate().map(|(i, e)| {
                                            let time = format!("{}{}", day_label(&e.day, &today), e.time.as_ref().map(|t| format!(" · {t}")).unwrap_or_default());
                                            html! { <AgendaEvent key={format!("{:?}:{}:{}:{i}", e.kind, e.title, e.day)} e={CalendarEvent { time: Some(time), ..e.clone() }} /> }
                                        }) }
                                    </ul>
                                }
                            }
                        },
                    ) }
                }

                { section(
                    "Interdictions d'achat",
                    Some(if active > 0 { format!("{active} active{}", if active > 1 { "s" } else { "" }) } else { "aucune active".into() }),
                    vetoes_section(&vetoes),
                ) }

                { section(
                    "Plan d'entrée recherché",
                    Some(format!("{}/{} étapes", d.setup.met, d.setup.total)),
                    html! {
                        <>
                            <p class="small"><b>{ d.setup.name.clone() }</b></p>
                            <ol class="dec-steps">
                                { for d.setup.steps.iter().map(|s| {
                                    let (icon, word) = step_ui(s.state);
                                    html! {
                                        <li key={s.label.clone()} class={format!("step-{}", step_key(s.state))}>
                                            <span class="dec-step-mark"><span aria-hidden="true">{ icon }</span>{ " " }{ word }</span>
                                            <div>
                                                <span>{ s.label.clone() }</span>
                                                if !s.detail.is_empty() && s.detail != "—" {
                                                    <small class="muted">{ s.detail.clone() }</small>
                                                }
                                            </div>
                                        </li>
                                    }
                                }) }
                            </ol>
                        </>
                    },
                ) }

                if !d.scenarios.is_empty() {
                    { section(
                        "Scénarios",
                        d.unfolding.as_ref().map(|u| format!("en cours : {}", scenario_ui(u.kind).1.to_lowercase())),
                        html! {
                            <>
                                if let Some(u) = &d.unfolding {
                                    <p class="small">{ u.text.clone() }</p>
                                }
                                <ul class="dec-scenarios">
                                    { for d.scenarios.iter().map(|s| {
                                        let n = s.conditions.len();
                                        html! {
                                            <li key={scenario_key(s.kind)} class={format!("sc-{}{}", scenario_key(s.kind), if s.unfolding { " sc-unfolding" } else { "" })} aria-current={s.unfolding.then_some("true")}>
                                                <div class="dec-family-head">
                                                    <b><span aria-hidden="true">{ scenario_ui(s.kind).0 }</span>{ format!(" {}", s.title) }</b>
                                                    if n > 0 {
                                                        <span class="mono small">{ format!("{}/{n} condition{}{}", s.met, if n > 1 { "s" } else { "" }, if s.unfolding { " · en cours" } else { "" }) }</span>
                                                    }
                                                </div>
                                                <p class="small">{ format!("Si {} → {}", uncapitalize(&s.condition), s.consequence) }</p>
                                                if let Some(l) = s.level {
                                                    <small class="muted mono">{ format!("niveau {}", usd(Some(l), m)) }</small>
                                                }
                                                if n > 0 {
                                                    <ul class="dec-checks">
                                                        { for s.conditions.iter().map(|c| {
                                                            let (icon, word, key) = check_ui(c.state);
                                                            html! {
                                                                <li key={c.text.clone()} class={format!("check-{key}")}>
                                                                    <span class="dec-step-mark"><span aria-hidden="true">{ icon }</span>{ " " }{ word }</span>
                                                                    <div><span>{ c.text.clone() }</span><small class="muted">{ c.detail.clone() }</small></div>
                                                                </li>
                                                            }
                                                        }) }
                                                    </ul>
                                                }
                                            </li>
                                        }
                                    }) }
                                </ul>
                                <p class="muted small">{ "Ce qu'il faut surveiller, pas une prévision." }</p>
                            </>
                        },
                    ) }
                }

                { section(
                    "Points favorables et défavorables",
                    Some(format!("{} / {}", d.pros.len(), d.cons.len())),
                    html! {
                        <div class="dec-proscons">
                            <div>
                                <h3 class="up">{ "＋ Points favorables" }</h3>
                                <ul class="dec-list">{ for d.pros.iter().map(|x| html! { <li key={x.clone()}>{ x.clone() }</li> }) }</ul>
                            </div>
                            <div>
                                <h3 class="down">{ "− Points défavorables" }</h3>
                                <ul class="dec-list">{ for d.cons.iter().map(|x| html! { <li key={x.clone()}>{ x.clone() }</li> }) }</ul>
                            </div>
                        </div>
                    },
                ) }

                { section(
                    "Pourquoi pas ?",
                    Some(format!("incertitude {}", uncertainty_label(d.why_not.uncertainty))),
                    html! {
                        <>
                            <p class="small muted">{ "Ce qui pourrait rendre cette décision fausse, cherché exprès." }</p>
                            <ul class="dec-list">{ for d.why_not.risks.iter().map(|r| html! { <li key={r.clone()}>{ r.clone() }</li> }) }</ul>
                            <p class="kv small"><span>{ "Incertitude" }</span><b>{ uncertainty_label(d.why_not.uncertainty) }</b></p>
                            if !d.why_not.invalidation.is_empty() {
                                <h3>{ "Invalidation" }</h3>
                                <ul class="dec-list">{ for d.why_not.invalidation.iter().map(|r| html! { <li key={r.clone()}>{ r.clone() }</li> }) }</ul>
                            }
                        </>
                    },
                ) }

                { section(
                    if matches!(d.fundamentals, Some(Fundamentals::Crypto(_))) { "Fondamentaux du réseau" } else { "Fondamentaux de l'entreprise" },
                    None,
                    match &d.fundamentals {
                        None => html! { <p class="muted small">{ "Non disponibles pour cet actif." }</p> },
                        Some(Fundamentals::Stock(f)) => stock_fund(f, m),
                        Some(Fundamentals::Crypto(f)) => crypto_fund(doc, f, m),
                    },
                ) }

                { section(
                    "Liquidité",
                    None,
                    match &d.liquidity {
                        Some(l) => html! { <>{ figures(rows::liquidity_rows(l, m)) }<small class="muted">{ format!("Source : {}", l.source) }</small></> },
                        None => html! { <p class="muted small">{ "Non disponible." }</p> },
                    },
                ) }

                { section(
                    "Historique du signal",
                    Some(d.track.as_ref().map(|t| format!("{} trades", t.trades)).unwrap_or_else(|| "non disponible".into())),
                    match &d.track {
                        Some(t) => html! {
                            <>
                                <p class="muted small">{ t.period.clone() }</p>
                                { figures(rows::track_rows(t)) }
                                if t.trades < 30 {
                                    <p class="notice warn small">{ "Moins de 30 trades : échantillon trop petit pour conclure." }</p>
                                }
                                <p class="small">{ t.note.clone() }</p>
                                if doc.track_details() {
                                    <TrackDetails track={t.clone()} spread={doc.track_has("spreadPct")} />
                                }
                            </>
                        },
                        None => html! { <p class="muted small">{ "Pas assez d'historique pour mesurer ce signal sur cet actif." }</p> },
                    },
                ) }

                { section(
                    "Sources",
                    Some(format!("{}/{} disponibles", d.sources.iter().filter(|s| s.ok).count(), d.sources.len())),
                    html! {
                        <ul class="source-list">
                            { for d.sources.iter().map(|s| html! {
                                <li key={s.name.clone()}>
                                    <span class={if s.ok { "up" } else { "down" }}>{ if s.ok { "● ok" } else { "○ indisponible" } }</span>{ format!(" {}", s.name) }
                                    <small class="muted">{ s.detail.clone() }</small>
                                </li>
                            }) }
                        </ul>
                    },
                ) }
            </div>

            if let Some(live_price) = p.simulate {
                <SimulateBuy d={p.d.clone()} {live_price} />
            }

            <p class="dec-disclaimer small">{ d.disclaimer.clone() }</p>
        </article>
    }
}

#[derive(Properties, PartialEq)]
pub struct DecisionCardProps {
    pub symbol: AttrValue,
    pub kind: Kind,
    /// None for an asset not held.
    pub personal: Option<PersonalInput>,
    /// false while the portfolio prices are loading (the personal decision waits for its weights).
    #[prop_or(true)]
    pub ready: bool,
    /// Live price of the asset, used by "Simuler cet achat" (else the decision's price).
    #[prop_or_default]
    pub live_price: Option<f64>,
}

/// The cached decision of the same mode, if any (`initial`).
fn initial(kind: Kind, symbol: &str, personal: bool) -> Option<Rc<altim_core::web::decision::CachedDecision>> {
    // Only a decision whose whole type decodes (as `api::decision_full` requires).
    store::cached(kind, symbol).filter(|c| c.personal == Some(personal) && c.decision.full().is_some())
}

#[component]
pub fn DecisionCard(p: &DecisionCardProps) -> Html {
    let is_personal = p.personal.as_ref().is_some_and(|x| x.cost.is_some() || !x.weights.is_empty());
    let app = crate::state::app::use_app_state();
    let state = store::use_transitions();
    let weights = app.score_weights;
    let url = altim_core::web::decision::doc::decision_url(&p.symbol, p.kind, if is_personal { p.personal.as_ref() } else { None }, Some(&weights));
    let d = {
        let (kind, symbol) = (p.kind, p.symbol.clone());
        use_state(move || initial(kind, &symbol, is_personal).map(|c| Rc::new(c.decision.clone())))
    };
    let status = {
        let (kind, symbol) = (p.kind, p.symbol.clone());
        use_state(move || initial(kind, &symbol, is_personal).map(|c| DecisionStatus::Refreshing { at: c.at }).unwrap_or(DecisionStatus::Fresh))
    };
    let error = use_state(|| None::<String>);
    let nonce = use_state(|| 0u32);
    let retry = {
        let nonce = nonce.clone();
        Callback::from(move |_: ()| nonce.set(*nonce + 1))
    };

    {
        let (d, status, error) = (d.clone(), status.clone(), error.clone());
        use_effect_with((p.symbol.clone(), p.kind, is_personal), move |(symbol, kind, personal)| {
            let c = initial(*kind, symbol, *personal);
            d.set(c.as_ref().map(|c| Rc::new(c.decision.clone())));
            status.set(c.map(|c| DecisionStatus::Refreshing { at: c.at }).unwrap_or(DecisionStatus::Fresh));
            error.set(None);
        });
    }

    {
        let (d, status, error) = (d.clone(), status.clone(), error.clone());
        let (symbol, kind, personal) = (p.symbol.to_string(), p.kind, if is_personal { p.personal.clone() } else { None });
        use_effect_with((url, p.ready, *nonce), move |(_, ready, _)| {
            let alive = Rc::new(Cell::new(true));
            let mut keep: Option<(gloo::timers::callback::Interval, gloo::events::EventListener)> = None;
            if *ready {
                let load = {
                    let alive = alive.clone();
                    Rc::new(move || {
                        let (d, status, error, alive, symbol, personal) =
                            (d.clone(), status.clone(), error.clone(), alive.clone(), symbol.clone(), personal.clone());
                        wasm_bindgen_futures::spawn_local(async move {
                            let weights = crate::state::app::app_state().score_weights;
                            let r = super::api::decision_full(&symbol, kind, personal.as_ref(), Some(&weights)).await;
                            if !alive.get() {
                                return;
                            }
                            match r {
                                Ok(x) => {
                                    store::cache_decision(&x, personal.is_some());
                                    d.set(Some(Rc::new(x)));
                                    status.set(DecisionStatus::Fresh);
                                    error.set(None);
                                }
                                Err(e) => {
                                    let msg = if e.0 == "Erreur 404" { "décision pas encore disponible sur ce serveur".to_string() } else { e.0 };
                                    let offline = super::api::offline();
                                    match initial(kind, &symbol, personal.is_some()) {
                                        Some(c) => {
                                            d.set(Some(Rc::new(c.decision.clone())));
                                            status.set(DecisionStatus::Stale { at: c.at, offline, error: msg });
                                        }
                                        None => error.set(Some(if offline {
                                            "Hors ligne : aucune décision enregistrée pour cet actif.".to_string()
                                        } else {
                                            format!("Décision indisponible : {msg}.")
                                        })),
                                    }
                                }
                            }
                        });
                    })
                };
                load();
                let tick = {
                    let load = load.clone();
                    gloo::timers::callback::Interval::new(300_000, move || {
                        if crate::hooks::visible() {
                            load();
                        }
                    })
                };
                let online = gloo::events::EventListener::new(&gloo::utils::window(), "online", move |_| load());
                keep = Some((tick, online));
            }
            move || {
                alive.set(false);
                drop(keep);
            }
        });
    }

    if let Some(x) = (*d).clone() {
        let change = latest_change(&state.transitions, &x.d.symbol, x.d.kind, x.d.verdict, x.d.level, &x.d.mode).cloned();
        return html! { <DecisionView d={x} status={(*status).clone()} on_retry={retry} simulate={Some(p.live_price)} {change} /> };
    }
    if let Some(e) = (*error).clone() {
        return html! {
            <div class="card decision">
                <h2 class="card-title">{ "Décision" }</h2>
                <p class="notice warn">{ format!("⚠ {e}") }</p>
                <button class="link-btn" onclick={Callback::from(move |_| retry.emit(()))}>{ "Réessayer" }</button>
                <p class="dec-disclaimer small">{ "Pas un conseil en investissement réglementé ; Altim ne passe aucun ordre." }</p>
            </div>
        };
    }
    html! { <div class="skeleton tall" aria-label="Chargement de la décision" role="status" /> }
}

#[derive(Properties, PartialEq)]
pub struct DecisionBadgeProps {
    pub kind: Kind,
    pub symbol: AttrValue,
}

/// The Radar's verdict: the full decision (the same as the asset's Décision card), from this browser's cache (less
/// than 12 h old; the Radar refreshes it every 15 minutes). The 4 h technical signal is only one of its inputs.
#[component]
pub fn DecisionBadge(p: &DecisionBadgeProps) -> Html {
    let _seen = store::use_decisions_seen();
    let Some(c) = store::fresh(p.kind, &p.symbol) else {
        return html! { <span class="badge hold dec-pending" title="Décision en cours de calcul">{ "Décision…" }</span> };
    };
    let doc = &c.decision;
    let d = &doc.d;
    let label = match doc.rating() {
        Some(r) => rating_ui(r).1.to_string(),
        None => d.label.clone(),
    };
    html! {
        <span
            class={format!("badge {}", rating_tone(doc.rating(), d.verdict))}
            title={format!("Décision Altim du {} : {} · confiance {}", short_date_time(c.at), level_ui(d.level).1, altim_core::js::round(d.confidence))}
        >
            { label }
        </span>
    }
}

/// The short reason under the Radar's chip when it reads ATTENDRE or AUCUNE POSITION (the decision's `chipNote`,
/// the same French text on iOS and Android): « zone d'achat 67 653,51 € (−10,1 %) », « veto : … ». Nothing otherwise.
#[component]
pub fn DecisionNote(p: &DecisionBadgeProps) -> Html {
    let _seen = store::use_decisions_seen();
    match store::fresh(p.kind, &p.symbol).and_then(|c| c.decision.d.chip_note.clone()) {
        Some(n) => html! { <small class="dec-note">{ n }</small> },
        None => html! {},
    }
}
