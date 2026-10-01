//! Building blocks of the decision card (DecisionCard.tsx): sections, figures, families, fundamentals, composite
//! score, technical structure, when not to trade, action zones, counter-argument, why the signal changed, the model's
//! evidence and the bot's line. Plain functions of their data (the rows and texts come from
//! `altim_core::web::decision`); the amounts use the display currency passed in.
use altim_core::engine::bot::{BotGroup, BotView};
use altim_core::engine::decision_types::{
    ActionZones, CheckState, CompositeScore, Condition, CounterArgument, CryptoFundamentals, Family, ModelEvidence, NoTrade, StockFundamentals,
};
use altim_core::engine::structure::{Bias, Structure};
use altim_core::engine::validation::Verdict as ProofVerdict;
use altim_core::web::bot::screen::{action_ui, pct0};
use altim_core::web::decision::config_changes::{ConfigTransition, transition_title};
use altim_core::web::decision::doc::DecisionDoc;
use altim_core::web::decision::format::*;
use altim_core::web::decision::rows::{self, Extra, Row};
use altim_core::web::money::MoneyDisplay;
use yew::prelude::*;

use crate::route::use_on_link;

/// A collapsible section: title, a small badge, the body.
pub fn section(title: &str, badge: Option<String>, body: Html) -> Html {
    html! {
        <details class="dec-section">
            <summary>
                <span>{ title.to_string() }</span>
                if let Some(b) = badge {
                    <small class="dec-section-badge">{ b }</small>
                }
            </summary>
            <div class="dec-section-body">{ body }</div>
        </details>
    }
}

/// Label / value pairs; a missing value says "non disponible" rather than a blank.
pub fn figures(rows: Vec<Row>) -> Html {
    html! {
        <dl class="dec-figures">
            { for rows.into_iter().map(|(k, v, cls)| {
                let v = v.filter(|v| v != "—");
                html! {
                    <div key={k.clone()}>
                        <dt>{ k }</dt>
                        { match v {
                            None => html! { <dd class="muted">{ "non disponible" }</dd> },
                            Some(v) => html! { <dd class={format!("mono {cls}")}>{ v }</dd> },
                        } }
                    </div>
                }
            }) }
        </dl>
    }
}

pub fn up_down(v: Option<f64>) -> &'static str {
    match v {
        None => "",
        Some(v) if v >= 0.0 => "up",
        Some(_) => "down",
    }
}

pub fn conditions(title: &str, items: &[Condition], m: &MoneyDisplay) -> Html {
    if items.is_empty() {
        return html! {};
    }
    html! {
        <div class="dec-block">
            <h3>{ title.to_string() }</h3>
            <ul class="dec-list">
                { for items.iter().map(|c| html! {
                    <li key={c.text.clone()}>
                        { c.text.clone() }
                        if let Some(l) = c.level {
                            { " " }<span class="dec-level mono">{ format!("niveau {}", usd(Some(l), m)) }</span>
                        }
                    </li>
                }) }
            </ul>
        </div>
    }
}

/// The centred bar of a score −100 … +100 (families, composite score).
fn bar_style(s: f64) -> String {
    let w = s.abs().min(100.0) / 2.0;
    let n = altim_core::js::number_to_string;
    if s >= 0.0 { format!("width: {}%; left: 50%;", n(w)) } else { format!("width: {}%; left: {}%;", n(w), n(50.0 - w)) }
}

pub fn family_row(f: &Family) -> Html {
    let t = family_tone(f.status, f.score);
    let s = f.score;
    let sign = |s: f64| {
        if s > 0.0 {
            "+"
        } else if s < 0.0 {
            "−"
        } else {
            ""
        }
    };
    html! {
        <li class="dec-family" key={f.key.clone()}>
            <div class="dec-family-head">
                <b>{ f.label.clone() }</b>
                <span class={format!("dec-tag tone-{}", t.tone)}>
                    <span aria-hidden="true">{ t.icon }</span>{ " " }{ t.text }
                    if let Some(s) = s {
                        <span class="mono">{ format!(" · {}{}", sign(s), altim_core::js::round(s).abs()) }</span>
                    }
                </span>
            </div>
            { match s {
                Some(s) => html! {
                    <div class="bar" role="img" aria-label={format!("Score {} sur une échelle de −100 à +100", altim_core::js::round(s))}>
                        <i class={if s >= 0.0 { "pos" } else { "neg" }} style={bar_style(s)} />
                    </div>
                },
                None => html! { <p class="muted small">{ "Non disponible : aucune source gratuite et vérifiable, rien n'est estimé." }</p> },
            } }
            <p class="small">{ f.summary.clone() }</p>
            if !f.points.is_empty() {
                <ul class="dec-list small">{ for f.points.iter().map(|p| html! { <li key={p.clone()}>{ p.clone() }</li> }) }</ul>
            }
            <small class="muted">{ format!("Source : {}", f.source) }</small>
        </li>
    }
}

pub fn stock_fund(f: &StockFundamentals, m: &MoneyDisplay) -> Html {
    let e = f.next_earnings.as_ref();
    let mut hist = rows::history_rows("PER", f.valuation_history.as_ref().and_then(|h| h.per.as_ref()));
    hist.extend(rows::history_rows("P/S", f.valuation_history.as_ref().and_then(|h| h.ps.as_ref())));
    html! {
        <>
            <p class="muted small">{ f.period.clone() }</p>
            if let (Some(pe), Some(fa)) = (f.period_end, f.filed_at) {
                <p class="muted small">{ format!("Comptes arrêtés au {}, déposés à la SEC le {}.", ny_date(pe as f64), ny_date(fa as f64)) }</p>
            }
            if let Some(s) = &f.sector {
                <p class="small">
                    <span class="muted">{ "Secteur : " }</span>{ format!("{} · {} (code SIC {})", s.label, s.sic_description, s.sic) }
                </p>
            }
            { figures(rows::stock_rows(f, m)) }
            <p class="kv small">
                <span>{ "Prochains résultats" }</span>
                <b>
                    { match e {
                        Some(e) => html! { <>{ ny_date(e.date as f64) }if e.estimated { <span class="dec-estimated">{ " · date estimée" }</span> }</> },
                        None => html! { "non communiqués" },
                    } }
                </b>
            </p>
            if !f.surprises.is_empty() {
                <div class="dec-block">
                    <h3>{ "Surprises sur les résultats" }</h3>
                    <ul class="dec-list small">
                        { for f.surprises.iter().map(|s| html! {
                            <li key={s.quarter.clone()}>
                                { format!("{} : BPA {} contre {} attendus, ", s.quarter, usd(Some(s.eps), m), usd(Some(s.consensus), m)) }
                                <span class={up_down(Some(s.surprise_pct))}>{ pct(Some(s.surprise_pct), 1, true) }</span>
                            </li>
                        }) }
                    </ul>
                </div>
            }
            if let Some(r) = &f.revisions {
                <p class="small">
                    { format!("Révisions des analystes (BPA de l'exercice) : {} il y a un mois, {} aujourd'hui (", usd(Some(r.month_ago), m), usd(Some(r.now), m)) }
                    <span class={up_down(Some(r.change_pct))}>{ pct(Some(r.change_pct), 1, true) }</span>{ ")." }
                </p>
            }
            if let Some(h) = &f.valuation_history {
                <div class="dec-block">
                    <h3>{ "Valorisation par rapport à sa propre histoire" }</h3>
                    if let Some(v) = f.valuation_verdict.as_ref().filter(|v| !v.is_empty()) {
                        <p class="small">{ format!("{v}.") }</p>
                    }
                    { figures(hist) }
                    <small class="muted">{ format!("{}. Source : {}.", h.method, h.source) }</small>
                </div>
            }
            { match &f.peers {
                Some(c) => html! {
                    <div class="dec-block">
                        <h3>{ "Comparaison sectorielle" }</h3>
                        <p class="small">{ f.sector_note.clone() }</p>
                        <ul class="dec-list small">
                            { for c.peers.iter().map(|p| html! {
                                <li key={p.symbol.clone()}>
                                    { format!(
                                        "{} ({}) : PER {}, P/S {}, marge opérationnelle {}, chiffre d'affaires {} sur un an",
                                        p.name, p.symbol, num(p.per, 1), num(p.ps, 1), pct(p.operating_margin, 1, false), pct(p.revenue_growth, 1, true)
                                    ) }
                                </li>
                            }) }
                        </ul>
                        <small class="muted">{ format!("Cours du {}. Source : {}.", ny_date(c.date as f64), c.source) }</small>
                    </div>
                },
                None => html! { <p class="muted small">{ f.sector_note.clone() }</p> },
            } }
            if !f.guidance.is_empty() {
                <p class="muted small">{ f.guidance.clone() }</p>
            }
            <small class="muted">{ format!("Source : {}", f.source) }</small>
        </>
    }
}

pub fn crypto_fund(doc: &DecisionDoc, f: &CryptoFundamentals, m: &MoneyDisplay) -> Html {
    let mut stable = rows::stable_rows(f.stablecoins.as_ref(), "tous réseaux", m);
    let chain_label = format!("réseau {}", f.chain_stablecoins.as_ref().map(|c| c.scope.as_str()).unwrap_or(""));
    stable.extend(rows::stable_rows(f.chain_stablecoins.as_ref(), &chain_label, m));
    html! {
        <>
            { figures(rows::crypto_rows(f, m)) }
            <p class="small"><span class="muted">{ "Déblocages de jetons : " }</span>{ f.unlocks.clone() }</p>
            if f.stablecoins.is_some() || f.chain_stablecoins.is_some() {
                <div class="dec-block">
                    <h3>{ "Flux de stablecoins" }</h3>
                    { figures(stable) }
                    <small class="muted">
                        { format!("Liquidité disponible sur le marché crypto. Source : {}.", f.stablecoins.as_ref().or(f.chain_stablecoins.as_ref()).map(|s| s.source.as_str()).unwrap_or("")) }
                    </small>
                </div>
            }
            if let Some(dev) = doc.dev_activity() {
                <div class="dec-block">
                    <h3>{ "Activité de développement" }</h3>
                    { match dev {
                        Some(a) => html! { <>{ figures(rows::dev_rows(a)) }<small class="muted">{ format!("Source : {}", a.source) }</small></> },
                        None => html! { <p class="muted small">{ "Non disponible (CoinGecko ne la publie plus et le dépôt GitHub du projet n'a pas répondu)." }</p> },
                    } }
                </div>
            }
            if !f.not_covered.is_empty() {
                <p class="muted small">{ format!("{}.", f.not_covered) }</p>
            }
            <small class="muted">{ format!("Source : {}", f.source) }</small>
        </>
    }
}

// ---------- Composite score and technical structure ----------

/// Direction of an item: an arrow and a word, never colour alone.
pub fn bias_tag(b: Bias) -> Html {
    let (icon, label) = bias_ui(b);
    html! { <span class={format!("dec-bias bias-{}", bias_key(b))}><span aria-hidden="true">{ icon }</span>{ " " }{ label }</span> }
}

/// Score −100 … +100 on one axis centred on 0 (same bar as the families).
fn score_bar(v: f64, label: &str) -> Html {
    html! {
        <div class="bar dec-thin" role="img" aria-label={format!("{label} : {} sur une échelle de −100 à +100", signed_score(v))}>
            <i class={if v >= 0.0 { "pos" } else { "neg" }} style={bar_style(v)} />
        </div>
    }
}

pub fn score_block(s: &CompositeScore) -> Html {
    let Some(value) = s.value else { return html! { <p class="muted small">{ s.text.clone() }</p> } };
    html! {
        <div class="dec-block dec-score">
            <p class="kv"><span>{ "Score composite" }</span><b><span class="mono">{ signed_score(value) }</span>{ format!(" · {}", s.label) }</b></p>
            { score_bar(value, "Score composite") }
            <ul class="dec-score-factors" aria-label="Score par facteur">
                { for s.factors.iter().map(|f| html! {
                    <li key={f.key.clone()}>
                        <span>
                            { f.label.clone() }{ " " }
                            <small class="muted">{ if f.value.is_none() { "non mesuré".to_string() } else { format!("poids {} %", num(Some(f.applied), 1)) } }</small>
                        </span>
                        <span class="mono small">{ f.value.map(signed_score).unwrap_or_else(|| "—".into()) }</span>
                        if let Some(v) = f.value {
                            { score_bar(v, &f.label) }
                        }
                    </li>
                }) }
            </ul>
            <p class="muted small">{ s.text.clone() }</p>
        </div>
    }
}

/// Every item of the structure, measured or not (a missing one says why rather than disappearing).
pub fn structure_list(st: &Structure, m: &MoneyDisplay) -> Html {
    html! {
        <>
            <p class="muted small">
                { format!("{}.", st.timeframe) }
                if let Some(s) = rows::structure_score(st) {
                    { " Direction d'ensemble : " }<span class="mono">{ s }</span>{ "/100." }
                }
            </p>
            <ul class="dec-structure">
                { for rows::structure_rows(st, m).into_iter().map(|r| html! {
                    <li key={r.name.clone()}>
                        <div class="dec-family-head"><b>{ r.name.clone() }</b>if let Some(b) = r.bias { { bias_tag(b) } }</div>
                        <p class="small">{ r.reading.clone() }</p>
                        { match r.extra {
                            Extra::None => html! {},
                            Extra::Text(t) => html! { <small class="muted">{ t }</small> },
                            Extra::List(l) => html! { <ul class="dec-list small">{ for l.into_iter().map(|x| html! { <li key={x.clone()}>{ x }</li> }) }</ul> },
                            Extra::Levels(l) => html! {
                                <ul class="dec-list small">
                                    { for l.into_iter().map(|(k, p, rest)| html! {
                                        <li key={format!("{k}-{p}")}>{ format!("{k} ") }<span class="mono">{ p }</span>{ format!(" {rest}") }</li>
                                    }) }
                                </ul>
                            },
                        } }
                    </li>
                }) }
            </ul>
        </>
    }
}

// ---------- Guidance: when not to trade, action zones, counter-argument, why the signal changed ----------

/// Compact banner near the top: the headline and the reasons' names (the detail is in its section).
pub fn no_trade_banner(n: &NoTrade) -> Html {
    if !n.active {
        return html! {};
    }
    html! {
        <div class="notice warn dec-notrade" role="status">
            <b>{ n.headline.clone() }</b>
            <ul class="dec-chips" aria-label="Raisons">
                { for n.reasons.iter().map(|r| html! { <li key={r.code.clone()} class="dec-chip">{ r.label.clone() }</li> }) }
            </ul>
        </div>
    }
}

pub fn no_trade_list(n: &NoTrade) -> Html {
    html! {
        <>
            if n.reasons.is_empty() {
                <p class="small">{ "Aucune raison mesurée de s'abstenir maintenant, ce qui ne garantit rien pour la suite." }</p>
            } else {
                <ul class="dec-list">{ for n.reasons.iter().map(|r| html! { <li key={r.code.clone()}><b>{ r.label.clone() }</b>{ format!(" — {}", r.detail) }</li> }) }</ul>
            }
            if !n.unchecked.is_empty() {
                <p class="muted small">{ format!("Non vérifié faute de données : {}.", n.unchecked.join(" ; ")) }</p>
            }
            <p class="muted small">{ "Volatilité, liquidité, écart achat/vente, résultats (avant et 1 à 2 séances après), annonces, marché sans direction, signal faible ou dégradé, séance de Wall Street (actions). N'interdit rien : signale un mauvais moment." }</p>
        </>
    }
}

/// Vertical ladder, highest price at the top; bands in the kind's colour, the price marked where it sits.
pub fn action_ladder(z: &ActionZones, m: &MoneyDisplay) -> Html {
    let items = rows::ladder(z).into_iter().map(|item| match item {
        rows::LadderItem::Marker => {
            html! { <li key="here" class="az-marker" aria-current="true"><span class="mono">{ rows::marker_text(z) }</span></li> }
        }
        rows::LadderItem::Zone { zone: r, here } => html! {
            <li key={r.kind.clone()} class={format!("az az-{}{}", r.kind, if here { " here" } else { "" })} aria-current={here.then_some("true")}>
                <span class="az-band" aria-hidden="true" />
                <div>
                    <b class="az-label">{ r.label.clone() }</b>
                    <span class="mono small">{ if r.from == r.to { usd(Some(r.from), m) } else { format!("{} – {}", usd(Some(r.from), m), usd(Some(r.to), m)) } }</span>
                    if here {
                        <span class="az-here mono">{ format!("◀ vous êtes ici · {}", usd(Some(z.price), m)) }</span>
                    }
                    <small class="muted">{ r.note.clone() }</small>
                </div>
            </li>
        },
    });
    html! {
        <div class="dec-block">
            <h3>{ "Zones d'action" }</h3>
            <p class="small"><b>{ z.here_text.clone() }</b></p>
            <ol class="dec-ladder" aria-label="Zones d'action, du prix le plus haut au plus bas">{ for items }</ol>
        </div>
    }
}

pub fn counter_block(c: &CounterArgument) -> Html {
    html! {
        <div class="dec-block dec-counter">
            <h3>{ "Contre-argument" }</h3>
            <p class="dec-counter-counts">
                <span>{ "🟢 Raisons favorables : " }<b class="mono">{ c.favourable }</b></span>
                <span>{ "🔴 Raisons défavorables : " }<b class="mono">{ c.unfavourable }</b></span>
            </p>
            <small class="muted">{ c.families_text.clone() }</small>
            if !c.invalidators.is_empty() {
                <p class="small"><b>{ "Points qui pourraient invalider le scénario" }</b></p>
                <ul class="dec-list small">{ for c.invalidators.iter().map(|i| html! { <li key={i.text.clone()}>{ i.text.clone() }</li> }) }</ul>
            }
        </div>
    }
}

/// "Pourquoi le signal a changé depuis le 28/09 à 14:02": what the measurements say changed.
pub fn change_block(t: &ConfigTransition) -> Html {
    let title = transition_title(t);
    let head = format!("🚨 {} — changement de configuration : ", t.symbol);
    let what = title.strip_prefix(&head).map(String::from).unwrap_or(title.clone());
    html! {
        <div class="dec-block dec-change">
            <h3>{ format!("Pourquoi le signal a changé depuis le {}", short_date_time(t.since)) }</h3>
            <p class="small">{ what }</p>
            if t.changes.is_empty() {
                <p class="muted small">{ "Mesures de la décision précédente non enregistrées (vue avant cette version) : changement non détaillé." }</p>
            } else {
                <ul class="dec-list small">{ for t.changes.iter().map(|c| html! { <li key={c.clone()}>{ c.clone() }</li> }) }</ul>
            }
        </div>
    }
}

/// (icon, word) of a scenario condition.
pub fn check_ui(s: CheckState) -> (&'static str, &'static str, &'static str) {
    match s {
        CheckState::Met => ("✓", "remplie", "met"),
        CheckState::Unmet => ("✕", "non remplie", "unmet"),
        CheckState::Unknown => ("?", "inconnue", "unknown"),
    }
}

#[derive(Properties, PartialEq)]
pub struct EvidenceProps {
    pub e: ModelEvidence,
}

/// « Preuve du modèle »: the cross-asset validation of the signal on this asset's class, next to the confidence.
#[component]
pub fn EvidenceLine(p: &EvidenceProps) -> Html {
    let on_link = use_on_link();
    let e = &p.e;
    let tone = if !e.available {
        "na"
    } else if e.weak {
        "weak"
    } else {
        match e.class_verdict {
            Some(ProofVerdict::Edge) => "edge",
            Some(ProofVerdict::Negative) => "weak",
            _ => "unproven",
        }
    };
    let link = if e.link.is_empty() { "/app/validation".to_string() } else { e.link.clone() };
    html! {
        <div class={format!("dec-proof {tone}")}>
            <p class="small dec-proof-head">
                <b>{ "Preuve du modèle" }</b>
                if let (true, Some(b)) = (e.available, e.beat_hold.as_ref().filter(|b| !b.is_empty())) {
                    <span class="dec-chip">{ format!("bat la détention : {b}") }</span>
                }
            </p>
            <p class="small">{ e.text.clone() }</p>
            <p class="small">
                <a href={link} onclick={on_link} class="link">{ "Voir la validation du modèle →" }</a>
                if let Some(t) = e.as_of {
                    <span class="muted">{ format!(" · calculée le {}", short_date_time(t as f64)) }</span>
                }
            </p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct BotProps {
    pub b: BotView,
}

/// « Bot Altim »: the learned model's action, its probabilities and whether it counts in this decision.
#[component]
pub fn BotLine(p: &BotProps) -> Html {
    let on_link = use_on_link();
    let b = &p.b;
    let action = b.action.filter(|_| b.available);
    let tone = match action {
        None => "na",
        Some(_) if b.counts => "edge",
        Some(_) => "unproven",
    };
    let link = if b.link.is_empty() { "/app/bot".to_string() } else { b.link.clone() };
    html! {
        <div class={format!("dec-proof {tone}")}>
            <p class="small dec-proof-head">
                <b>{ "Bot Altim" }</b>
                if let Some(a) = action {
                    <span class={format!("chip bot-action {}", action_ui(a).1)}>{ action_ui(a).0 }</span>
                    <span class="dec-chip">{ if b.counts { "compte" } else { "ne compte pas" } }</span>
                }
            </p>
            if action.is_some() {
                <p class="small">
                    { "Probabilités à 20 jours : hausse " }<b>{ pct0(b.up) }</b>{ format!(" (seuil {}), baisse ", pct0(b.threshold_up)) }<b>{ pct0(b.down) }</b>
                    { format!(" (seuil {})", pct0(b.threshold_down)) }
                    if !b.in_basket {
                        <span class="muted">{ format!(" · modèle des {}, non testé sur cet actif", if b.group == BotGroup::Crypto { "cryptos" } else { "actions" }) }</span>
                    }
                </p>
            } else {
                <p class="small">{ b.text.clone() }</p>
            }
            if !b.contributions.is_empty() {
                <p class="small muted">{ b.contributions.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" · ") }</p>
            }
            <p class="small">{ b.note.clone() }</p>
            if let Some(v4) = b.v4.as_ref().filter(|_| b.available) {
                <crate::app::bot::V4Avis v={v4.clone()} />
            }
            <p class="small">
                <a href={link} onclick={on_link} class="link">{ "Voir le bot et ses résultats →" }</a>
                if let Some(t) = b.as_of {
                    <span class="muted">{ format!(" · entraîné le {}", short_date_time(t as f64)) }</span>
                }
            </p>
        </div>
    }
}
