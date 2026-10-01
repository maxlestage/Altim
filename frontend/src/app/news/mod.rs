//! Actu (News.tsx): every feed in one place, stories told by several sources merged, the user's assets first, the
//! day's summary on top; the « Agenda » view (Agenda.tsx) in `agenda`. Contracts and pure helpers in
//! `altim_core::web::insights::{news, news_summary, calendar}`.
mod agenda;

use std::rc::Rc;

use altim_core::engine::news::{Lang, NewsItem, NewsTone};
use altim_core::engine::news_summary::StorySummary;
use altim_core::types::Kind;
use altim_core::web::insights::alerts::uniq;
use altim_core::web::insights::news::{FILTER_KEY, FILTERS, NewsReport, VIEW_KEY, ago, parse_filter};
use altim_core::web::insights::news_summary::{
    asset_link, basis_label, consensus_text, impact_id, impact_label, move_text, rule_text, summary_heading, tone_mark,
};
use altim_core::web::store::asset_key;
use yew::prelude::*;

pub use agenda::{Agenda, AgendaEvent, local_today};

use crate::route::use_on_link;
use crate::state::{local_get, local_set};
use crate::ui::Segmented;

/// The radar's assets and the holdings, once each, 20 at most (the news and the brief ask for them).
#[hook]
pub(crate) fn use_my_assets() -> Vec<(String, Kind)> {
    let app = crate::state::app::use_app_state();
    let stored = crate::state::holdings::use_stored_holdings();
    let all: Vec<(String, Kind)> =
        app.watchlist.iter().map(|w| (w.symbol.clone(), w.kind)).chain(stored.holdings.iter().map(|h| (h.symbol.clone(), h.kind))).collect();
    let mut v = uniq(&all, |(s, k)| asset_key(s, *k));
    v.truncate(20);
    v
}

#[derive(Properties, PartialEq)]
struct StoryProps {
    n: NewsItem,
    #[prop_or_default]
    featured: bool,
}

#[component]
fn Story(p: &StoryProps) -> Html {
    let n = &p.n;
    let now = js_sys::Date::now();
    html! {
        <li class={classes!("news-item", p.featured.then_some("featured"), n.alert.then_some("alert"))}>
            <a href={n.link.clone()} target="_blank" rel="noopener noreferrer nofollow">
                <b>{ &n.title }</b>
            </a>
            if let Some(s) = n.summary.as_ref().filter(|s| p.featured && !s.is_empty()) {
                <p class="small muted">{ s }</p>
            }
            <div class="news-meta small">
                <span>{ &n.source }</span>
                <span class="muted">{ ago(n.time as f64, now) }</span>
                if !n.also_in.is_empty() {
                    <span class="muted" title={n.also_in.join(", ")}>{ format!("+{} source{}", n.also_in.len(), if n.also_in.len() > 1 { "s" } else { "" }) }</span>
                }
                if n.lang == Lang::En {
                    <span class="muted">{ "EN" }</span>
                }
                if n.tone != NewsTone::Neutral {
                    <span class={if n.tone == NewsTone::Negative { "down" } else { "up" }}>{ if n.tone == NewsTone::Negative { "▼ négatif" } else { "▲ positif" } }</span>
                }
            </div>
            if !n.assets.is_empty() || !n.themes.is_empty() || n.alert {
                <div class="news-tags">
                    if n.alert {
                        <span class="badge sell">{ "ALERTE" }</span>
                    }
                    { for n.assets.iter().map(|a| html! { <span key={a.clone()} class="chip">{ a.split(':').nth(1).unwrap_or_default() }</span> }) }
                    { for n.themes.iter().map(|t| html! { <span key={t.label()} class="chip muted">{ t.label() }</span> }) }
                </div>
            }
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct SummaryEventProps {
    s: StorySummary,
}

#[component]
fn SummaryEvent(p: &SummaryEventProps) -> Html {
    let s = &p.s;
    let on_link = use_on_link();
    html! {
        <li class={classes!("news-item", "summary-item", s.alert.then_some("alert"))}>
            <a href={s.link.clone()} target="_blank" rel="noopener noreferrer nofollow"><b>{ &s.title }</b></a>
            <div class="news-tags">
                <span class={classes!("chip", "impact", impact_id(s.impact))}>{ format!("Impact potentiel {}", impact_label(s.impact)) }</span>
                <span class="chip muted">{ basis_label(s) }</span>
            </div>
            if !s.assets.is_empty() {
                <div class="news-tags small">
                    <span class="muted">{ "Actifs concernés" }</span>
                    { for s.assets.iter().map(|id| {
                        let a = asset_link(id);
                        html! { <a key={id.clone()} href={a.href} onclick={on_link.clone()} class="chip link">{ a.symbol }</a> }
                    }) }
                </div>
            }
            <p class="small"><span class="muted">{ "Sources " }</span>{ consensus_text(s) }</p>
            { for s.moves.iter().map(|m| html! {
                <p key={m.asset.clone()} class="small">
                    <span class={if m.change_pct >= 0.0 { "up" } else { "down" }}>{ move_text(m) }</span>{ " " }
                    <span class="muted">{ "(clôtures horaires, pas forcément dues à cette actualité)" }</span>
                </p>
            }) }
            { for s.technical.iter().map(|t| html! {
                <p key={t.asset.clone()} class="small summary-tech">
                    <span class="muted">
                        { format!("Impact sur le signal technique{} : ", if s.technical.len() > 1 { format!(" ({})", asset_link(&t.asset).symbol) } else { String::new() }) }
                    </span>
                    { &t.text }
                </p>
            }) }
            <details class="small">
                <summary>{ "Sources et calcul" }</summary>
                <ul class="news-sources">
                    { for s.links.iter().map(|l| {
                        let (mark, cls, label) = tone_mark(l.tone);
                        html! {
                            <li key={l.link.clone()}>
                                <span class={cls} aria-label={label}>{ mark }</span>{ " " }
                                <a class="link" href={l.link.clone()} target="_blank" rel="noopener noreferrer nofollow">{ &l.source }</a>{ " " }
                                <span class="muted">{ format!("· {}", l.title) }</span>
                            </li>
                        }
                    }) }
                </ul>
                <p class="muted">{ rule_text(s) }</p>
            </details>
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct SummaryProps {
    list: Rc<Vec<StorySummary>>,
}

#[component]
fn Summary(p: &SummaryProps) -> Html {
    let (title, others) = summary_heading(&p.list);
    html! {
        <div class="card news-summary">
            <h2 class="card-title">{ title }</h2>
            if let Some(o) = others {
                <p class="small">{ o }</p>
            }
            <p class="muted small">
                { "Sujets des dernières 24 h repris par plusieurs sources indépendantes, escalades graves, ou articles citant vos actifs sur un sujet sensible. Impact potentiel indicatif, pas un signal." }
            </p>
            if !p.list.is_empty() {
                <ol class="news-list">{ for p.list.iter().map(|s| html! { <SummaryEvent key={s.id.clone()} s={s.clone()} /> }) }</ol>
            }
            <details class="small">
                <summary>{ "Comment l'impact est estimé" }</summary>
                <p class="muted">
                    { "Par règle : sources indépendantes, un même titre repris mot pour mot comptant pour une (2–3 : +1, 4 et plus : +2), thème (escalade grave +2 ; banques centrales, régulation ou piratage +1), cite un de vos actifs (+1). 0–1 point : faible, 2–3 : moyen, 4 et plus : important." }
                </p>
                <p class="muted">
                    { "Mesuré : si les bougies horaires d'un actif cité sont déjà en mémoire sur le serveur, la plus forte variation depuis la clôture précédant la publication (action : 1 % moyen, 3 % important ; crypto : 2 % et 5 %). Une variation après un article ne prouve pas qu'il en est la cause." }
                </p>
                <p class="muted">
                    { "Consensus : ton des titres de chaque source (repérage par mots-clés) ; divergent dès qu'un titre est négatif et un autre positif." }
                </p>
            </details>
        </div>
    }
}

/// News section: every feed in one place, stories told by several sources merged, the user's assets first.
#[component]
pub fn News() -> Html {
    let assets = use_my_assets();
    let report = use_state(|| None::<Rc<NewsReport>>);
    let error = use_state(|| None::<String>);
    let filter = use_state(|| local_get(FILTER_KEY).filter(|f| !f.is_empty()).unwrap_or_else(|| "all".into()));
    let french_only = use_state(|| false);
    let view = use_state(|| if local_get(VIEW_KEY).as_deref() == Some("agenda") { "agenda" } else { "articles" });
    let choose_view = {
        let view = view.clone();
        Callback::from(move |v: AttrValue| {
            let v = if v == "agenda" { "agenda" } else { "articles" };
            view.set(v);
            local_set(VIEW_KEY, v);
        })
    };
    let assets_key = assets.iter().map(|(s, k)| asset_key(s, *k)).collect::<Vec<_>>().join(",");
    {
        let (report, error, assets) = (report.clone(), error.clone(), assets.clone());
        use_effect_with((assets_key, *view), move |(_, view)| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let mut timer = None;
            if *view == "articles" {
                let load = {
                    let alive = alive.clone();
                    move || {
                        let (report, error, assets, alive) = (report.clone(), error.clone(), assets.clone(), alive.clone());
                        wasm_bindgen_futures::spawn_local(async move {
                            let r = crate::app::alerts::news_of(&assets).await;
                            if !alive.get() {
                                return;
                            }
                            match r {
                                Ok(r) => {
                                    report.set(Some(Rc::new(r)));
                                    error.set(None);
                                }
                                Err(e) => error.set(Some(e.0)),
                            }
                        });
                    }
                };
                load();
                timer = Some(crate::hooks::every_visible(300_000, load));
            }
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }
    let choose = {
        let filter = filter.clone();
        Callback::from(move |f: AttrValue| {
            filter.set(f.to_string());
            local_set(FILTER_KEY, &f);
        })
    };
    let toggle_french = {
        let french_only = french_only.clone();
        Callback::from(move |e: Event| french_only.set(e.target_unchecked_into::<web_sys::HtmlInputElement>().checked()))
    };

    let all = filter.as_str() == "all";
    let category = parse_filter(&filter);
    let shown: Vec<NewsItem> = match &*report {
        // An unknown saved rubric shows nothing (`n.category === filter`), like the TypeScript.
        Some(r) if all || category.is_some() => r.shown(category, *french_only).into_iter().cloned().collect(),
        _ => Vec::new(),
    };
    let top: Vec<NewsItem> = report.as_ref().map(|r| r.top_items().into_iter().cloned().collect()).unwrap_or_default();
    let up_sources = report.as_ref().map(|r| r.sources_up()).unwrap_or(0);
    let now = js_sys::Date::now();
    let subtitle = if *view == "agenda" {
        "Les événements à venir, chacun avec sa source.".to_string()
    } else {
        match &*report {
            Some(r) => format!("{} articles de {up_sources} sources sur 48 h, mis à jour {}.", r.items.len(), ago(r.as_of, now)),
            None => "Chargement des sources…".into(),
        }
    };
    let views: Vec<(AttrValue, AttrValue)> = vec![("articles".into(), "Articles".into()), ("agenda".into(), "Agenda".into())];
    let filters: Vec<(AttrValue, AttrValue)> = FILTERS.iter().map(|(v, l)| (AttrValue::Static(v), AttrValue::Static(l))).collect();

    html! {
        <section class="app-screen news">
            <header class="screen-top">
                <div>
                    <h1>{ "Actualités" }</h1>
                    <p class="muted small">{ subtitle }</p>
                </div>
            </header>

            <Segmented label="Vue" value={AttrValue::Static(*view)} options={views} on_change={choose_view} />

            if *view == "agenda" {
                <Agenda />
            } else {
                if let Some(e) = &*error {
                    <p class="notice warn">{ format!("⚠ {e}") }</p>
                }
                if let Some(list) = report.as_ref().and_then(|r| r.summary.clone()) {
                    <Summary list={Rc::new(list)} />
                }
                if !top.is_empty() && all {
                    <div class="card">
                        <h2 class="card-title">{ "À la une" }</h2>
                        <p class="muted small">{ "Les sujets repris par plusieurs sources, et toute escalade grave (guerre, panique bancaire…)." }</p>
                        <ul class="news-list">{ for top.iter().map(|n| html! { <Story key={n.id.clone()} n={n.clone()} featured=true /> }) }</ul>
                    </div>
                }
                if let Some(r) = report.as_ref().filter(|r| r.digest.total > 0 && all) {
                    <Digest report={r.clone()} />
                }
                <Segmented label="Rubrique" value={AttrValue::from((*filter).clone())} options={filters} on_change={choose} />
                <label class="check small">
                    <input type="checkbox" checked={*french_only} onchange={toggle_french} />{ " Articles en français seulement" }
                </label>
                if report.is_some() && shown.is_empty() {
                    <p class="muted">{ "Aucun article dans cette rubrique pour le moment." }</p>
                }
                <ul class="news-list">{ for shown.iter().map(|n| html! { <Story key={n.id.clone()} n={n.clone()} /> }) }</ul>
                if let Some(r) = &*report {
                    <details class="card small">
                        <summary>{ format!("Sources · {up_sources}/{} en ligne", r.sources.len()) }</summary>
                        <ul class="news-sources">
                            { for r.sources.iter().map(|s| html! {
                                <li key={s.name.clone()} class={if s.ok { "" } else { "muted" }}>
                                    { if s.ok { format!("✔ {} · {}", s.name, s.count) } else { format!("✕ {} · {}", s.name, s.error.as_deref().unwrap_or("indisponible")) } }
                                </li>
                            }) }
                        </ul>
                        <p class="muted">{ "Les titres sont affichés tels que publiés (non traduits) ; les liens ouvrent l'article chez sa source." }</p>
                    </details>
                }
            }
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct DigestProps {
    report: Rc<NewsReport>,
}

/// "Ce qui domine (24 h)": themes and the tone of the headlines.
#[component]
fn Digest(p: &DigestProps) -> Html {
    let d = &p.report.digest;
    let t = d.tone;
    let flex = |n: usize| format!("flex: {};", if n == 0 { "0.0001".to_string() } else { n.to_string() });
    html! {
        <div class="card">
            <h2 class="card-title">{ "Ce qui domine (24 h)" }</h2>
            <div class="news-tags">
                { for d.themes.iter().map(|th| html! { <span key={th.label.clone()} class="chip">{ format!("{} · {}", th.label, th.count) }</span> }) }
            </div>
            <div class="tone-bar" role="img" aria-label={format!("Ton des titres : {} négatifs, {} neutres, {} positifs", t.negative, t.neutral, t.positive)}>
                <i class="neg" style={flex(t.negative)} />
                <i class="neu" style={flex(t.neutral)} />
                <i class="pos" style={flex(t.positive)} />
            </div>
            <p class="muted small">
                { format!("Ton des titres : {} négatifs, {} neutres, {} positifs (repérage par mots-clés, indicatif).", t.negative, t.neutral, t.positive) }
            </p>
        </div>
    }
}
