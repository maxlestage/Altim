//! Agenda (Agenda.tsx): the coming days' economic releases, central bank decisions, earnings, dividends, splits and
//! IPOs, each with its source, the 7-day risk calendar on top, and what no free source covers at the bottom. Pure
//! helpers in `altim_core::web::insights::calendar`; `AgendaEvent` is also the decision card's.
use std::rc::Rc;

use altim_core::web::insights::calendar::{
    AGENDA_FILTERS, AgendaFilter, CalendarEvent, CalendarReport, EventKind, FILTER_KEY, Importance, MINE_KEY, RiskLevel, calendar_url,
    category_label, day_label, failed_days, failed_sources_text, filter_events, group_by_day, mine_text, risk_days, stock_symbols,
};
use yew::prelude::*;

use crate::api::ApiError;
use crate::hooks::every_visible;
use crate::state::{local_get, local_set};
use crate::ui::Segmented;

async fn calendar(days: u32, symbols: Option<Vec<String>>, top: bool) -> Result<CalendarReport, ApiError> {
    crate::api::get(&calendar_url(days, symbols.as_deref(), top)).await
}

/// The viewer's local day, "YYYY-MM-DD" (`dayLabel`'s "today").
pub fn local_today() -> String {
    let d = js_sys::Date::new_0();
    format!("{}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

fn filled(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

#[derive(Properties, PartialEq)]
struct FiguresProps {
    e: CalendarEvent,
}

#[component]
fn Figures(p: &FiguresProps) -> Html {
    let e = &p.e;
    let (actual, consensus, previous) = (filled(&e.actual), filled(&e.consensus), filled(&e.previous));
    if actual.is_none() && consensus.is_none() && previous.is_none() {
        return html! {};
    }
    let earnings = e.kind == EventKind::Earnings;
    html! {
        <div class="agenda-figures small">
            if let Some(a) = actual {
                <span>{ "Publié " }<b>{ a }</b></span>
            }
            if let Some(c) = consensus {
                <span>{ if earnings { "Consensus " } else { "Attendu " } }<b>{ c }</b></span>
            }
            if let Some(v) = previous {
                if earnings {
                    <span class="muted">{ v }</span>
                } else {
                    <span>{ "Précédent " }<b>{ v }</b></span>
                }
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AgendaEventProps {
    pub e: CalendarEvent,
}

/// One event: importance, time, title, category chips, figures, detail and note, link to its source.
#[component]
pub fn AgendaEvent(p: &AgendaEventProps) -> Html {
    let e = &p.e;
    let high = e.importance == Importance::High;
    let importance = if high { "Importance haute" } else { "Importance moyenne" };
    html! {
        <li class={classes!("agenda-item", high.then_some("high"))}>
            <div class="agenda-head">
                <i class={classes!("agenda-dot", if high { "high" } else { "medium" })} role="img" aria-label={importance} title={importance} />
                <span class="agenda-time small">{ e.time.clone().unwrap_or_else(|| "Journée".into()) }</span>
                <b class="agenda-title">{ &e.title }</b>
            </div>
            <div class="news-tags">
                <span class="chip muted">{ category_label(e.category) }</span>
                if let Some(c) = filled(&e.country) {
                    <span class="chip muted">{ c }</span>
                }
                if let Some(s) = filled(&e.symbol) {
                    <span class="chip">{ s }</span>
                }
                if let Some(o) = filled(&e.original_name).filter(|o| *o != e.title) {
                    <span class="chip muted" title="Nom publié par la source">{ o }</span>
                }
            </div>
            <Figures e={e.clone()} />
            if let Some(d) = filled(&e.detail) {
                <p class="small muted">{ d }</p>
            }
            if let Some(n) = filled(&e.note) {
                <p class="small agenda-note">{ n }</p>
            }
            <a class="small link agenda-source" href={e.url.clone()} target="_blank" rel="noopener noreferrer nofollow">
                { format!("Source : {}", e.source) }
            </a>
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct RiskWeekProps {
    held: Vec<String>,
    watched: Vec<String>,
}

/// Risk of the next 7 days, one stacked row per day (weekends included): its own request, with the user's stocks
/// and the largest companies together (`top=1`), whatever the list's filters.
#[component]
fn RiskWeek(p: &RiskWeekProps) -> Html {
    let report = use_state(|| None::<Rc<CalendarReport>>);
    let error = use_state(|| None::<String>);
    let mut stocks: Vec<String> = Vec::new();
    for s in p.held.iter().chain(p.watched.iter()) {
        if !stocks.contains(s) {
            stocks.push(s.clone());
        }
    }
    stocks.truncate(50);
    let key = stocks.join(",");
    {
        let (report, error) = (report.clone(), error.clone());
        use_effect_with(key, move |key| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let a = alive.clone();
            let symbols = (!key.is_empty()).then(|| key.split(',').map(String::from).collect::<Vec<_>>());
            wasm_bindgen_futures::spawn_local(async move {
                let r = calendar(7, symbols, true).await;
                if !a.get() {
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
            move || alive.set(false)
        });
    }
    let failed = report.as_ref().map(|r| failed_days(r)).unwrap_or_default();
    let days = report.as_ref().map(|r| risk_days(&r.events, &r.from, &p.held, &p.watched, 7, &failed)).unwrap_or_default();
    html! {
        <div class="card">
            <h2 class="card-title">{ "Calendrier de risque · 7 jours" }</h2>
            if let Some(e) = &*error {
                <p class="notice warn small">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <p class="muted small">{ "Chargement…" }</p>
            }
            if !days.is_empty() {
                <ol class="risk-days">
                    { for days.iter().map(|d| {
                        let unknown = d.incomplete && d.level == RiskLevel::Low;
                        let extra = if d.main.len() > 3 { format!(" +{}", d.main.len() - 3) } else { String::new() };
                        let label = if unknown { "Risque non évalué" } else { d.level.label() };
                        let events = if unknown {
                            "Sources incomplètes : risque non évalué".to_string()
                        } else if d.level == RiskLevel::Low {
                            "Aucun événement majeur".to_string()
                        } else {
                            d.main.iter().take(3).cloned().collect::<Vec<_>>().join(" · ") + &extra
                        };
                        html! {
                            <li key={d.day.clone()} class={classes!("risk-day", (!unknown).then_some(d.level.id()))}>
                                <div class="risk-day-head">
                                    <b>{ &d.label }</b>
                                    if d.weekend {
                                        <span class="chip muted">{ "week-end" }</span>
                                    }
                                    <span role="img" aria-label={label} title={label}>{ if unknown { "⚪" } else { d.level.icon() } }</span>
                                    <span class="risk-day-events">{ events }</span>
                                </div>
                                if d.incomplete && !unknown {
                                    <span class="small muted">{ "Sources incomplètes ce jour-là." }</span>
                                }
                            </li>
                        }
                    }) }
                </ol>
            }
            <details class="small">
                <summary>{ "Règle" }</summary>
                <p class="muted">
                    { "🔴 décision de taux d'une banque centrale, inflation (CPI), emploi ou PIB d'importance haute, ou résultats d'une action de vos avoirs ou de votre radar. 🟠 autres publications économiques et banques centrales, résultats des grandes capitalisations américaines, dividende ou split d'une action détenue. 🟢 aucun de ces événements. Heures et jours de Paris ; seules les sources de l'Agenda sont prises en compte (voir « Non couvert »)." }
                </p>
            </details>
        </div>
    }
}

fn filter_id(f: AgendaFilter) -> &'static str {
    AGENDA_FILTERS.iter().find(|(v, _)| AgendaFilter::parse(v) == f).map(|(v, _)| *v).unwrap_or("all")
}

/// Agenda: the coming days' events, each with its source; what no free source covers is listed at the bottom.
#[component]
pub fn Agenda() -> Html {
    let app = crate::state::app::use_app_state();
    let stored = crate::state::holdings::use_stored_holdings();
    let report = use_state(|| None::<Rc<CalendarReport>>);
    let error = use_state(|| None::<String>);
    let loading = use_state(|| true);
    let filter = use_state(|| AgendaFilter::parse(&local_get(FILTER_KEY).unwrap_or_default()));
    let mine = use_state(|| local_get(MINE_KEY).as_deref() == Some("1"));
    let days = use_state(|| AttrValue::Static("14"));

    let watch = app.watchlist.iter().map(|w| (w.symbol.as_str(), w.kind));
    let holds = stored.holdings.iter().map(|h| (h.symbol.as_str(), h.kind));
    let stocks = stock_symbols(watch.clone().chain(holds.clone()));
    let held = stock_symbols(holds);
    let watched = stock_symbols(watch);
    // "Mes actifs": the server gives these stocks' events (even small companies); without any stock, the whole
    // calendar is filtered on the device.
    let asked_key = if *mine && !stocks.is_empty() { stocks.join(",") } else { String::new() };
    {
        let (report, error, loading) = (report.clone(), error.clone(), loading.clone());
        use_effect_with(((*days).clone(), asked_key), move |(days, key)| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let n: u32 = days.parse().unwrap_or(14);
            let symbols = (!key.is_empty()).then(|| key.split(',').map(String::from).collect::<Vec<_>>());
            let load = {
                let alive = alive.clone();
                move || {
                    let (report, error, loading, alive, symbols) = (report.clone(), error.clone(), loading.clone(), alive.clone(), symbols.clone());
                    loading.set(true);
                    wasm_bindgen_futures::spawn_local(async move {
                        let r = calendar(n, symbols, false).await;
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
                        loading.set(false);
                    });
                }
            };
            load();
            let timer = every_visible(900_000, load);
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    let set_days = {
        let days = days.clone();
        Callback::from(move |v: AttrValue| days.set(v))
    };
    let toggle_mine = {
        let mine = mine.clone();
        Callback::from(move |_| {
            local_set(MINE_KEY, if *mine { "0" } else { "1" });
            mine.set(!*mine);
        })
    };
    let shown: Vec<&CalendarEvent> =
        report.as_ref().map(|r| filter_events(&r.events, *filter, (*mine).then_some(stocks.as_slice()))).unwrap_or_default();
    let groups = group_by_day(&shown);
    let today = local_today();
    let period: Vec<(AttrValue, AttrValue)> =
        [("7", "7 jours"), ("14", "14 jours"), ("30", "30 jours")].iter().map(|(v, l)| (AttrValue::Static(v), AttrValue::Static(l))).collect();
    let current = filter_id(*filter);

    html! {
        <div class="agenda">
            <RiskWeek {held} {watched} />
            <div class="card">
                <h2 class="card-title">{ "Agenda" }</h2>
                <p class="muted small">
                    { "Publications économiques majeures, décisions des banques centrales, résultats, dividendes, splits et introductions en bourse. Heures de Paris ; chiffres tels que publiés par la source." }
                </p>
                <Segmented label="Période" value={(*days).clone()} options={period} on_change={set_days} />
                <div class="agenda-chips" role="radiogroup" aria-label="Type d'événement">
                    { for AGENDA_FILTERS.iter().map(|(v, l)| {
                        let on = *v == current;
                        let choose = {
                            let (filter, v) = (filter.clone(), *v);
                            Callback::from(move |_| {
                                filter.set(AgendaFilter::parse(v));
                                local_set(FILTER_KEY, v);
                            })
                        };
                        html! { <button key={*v} role="radio" aria-checked={on.to_string()} class={classes!("chip", "pick", on.then_some("on"))} onclick={choose}>{ *l }</button> }
                    }) }
                    <button class={classes!("chip", "pick", "mine", mine.then_some("on"))} aria-pressed={mine.to_string()} onclick={toggle_mine}>
                        { if *mine { "✓ Mes actifs" } else { "Mes actifs" } }
                    </button>
                </div>
                if *mine {
                    <p class="muted small">{ mine_text(stocks.len()) }</p>
                }
            </div>

            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <p class="muted">{ "Chargement de l'agenda…" }</p>
            }
            if report.is_some() && *loading {
                <p class="muted small">{ "Mise à jour…" }</p>
            }
            if let Some(t) = report.as_ref().and_then(|r| failed_sources_text(r)) {
                <p class="notice warn small">{ t }</p>
            }
            if report.is_some() && groups.is_empty() {
                <p class="muted">{ "Aucun événement de ce type sur la période." }</p>
            }

            { for groups.iter().map(|(day, events)| html! {
                <section key={day.clone()} class="agenda-day">
                    <h3 class="section-label">{ day_label(day, &today) }</h3>
                    <ul class="news-list">
                        { for events.iter().enumerate().map(|(i, e)| html! {
                            <AgendaEvent key={format!("{:?}:{}:{}:{}:{i}", e.kind, e.title, e.time.as_deref().unwrap_or(""), e.symbol.as_deref().unwrap_or(""))} e={(*e).clone()} />
                        }) }
                    </ul>
                </section>
            }) }

            if let Some(r) = &*report {
                <div class="card small">
                    <h2 class="card-title">{ "Non couvert" }</h2>
                    <ul class="agenda-gaps">{ for r.not_covered.iter().map(|t| html! { <li key={t.clone()}>{ t }</li> }) }</ul>
                    <p class="muted">
                        { "« · » sépare plusieurs séries publiées sous le même nom par la source (souvent la variation sur un mois et sur un an), dans l'ordre de la source." }
                    </p>
                    <details>
                        <summary>{ format!("Sources · {}/{} en ligne", r.sources.iter().filter(|s| s.ok).count(), r.sources.len()) }</summary>
                        <ul class="news-sources">
                            { for r.sources.iter().map(|s| {
                                let why = if s.ok {
                                    String::new()
                                } else {
                                    let days = if s.failed.is_empty() { String::new() } else { format!(" ({})", s.failed.join(", ")) };
                                    format!(" · {}{days}", s.error.as_deref().unwrap_or("indisponible"))
                                };
                                html! {
                                    <li key={s.name.clone()} class={if s.ok { "" } else { "muted" }}>
                                        { format!("{} {}{why}", if s.ok { "✔" } else { "✕" }, s.name) }
                                    </li>
                                }
                            }) }
                        </ul>
                    </details>
                </div>
            }
        </div>
    }
}
