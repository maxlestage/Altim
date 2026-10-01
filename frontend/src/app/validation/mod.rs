//! « Validation du modèle » (Validation.tsx): the signal's backtest (the decision's own track record, same costs) on
//! a basket fixed in advance, pooled by asset class, by market regime and overall (/api/validation, the server's
//! `ValidationReport`). Stacked cards, mobile first; the per-asset list follows the basket's order by default, never
//! ranked by performance. Texts in `altim_core::web::insights::validation`.
use std::rc::Rc;

use altim_core::engine::backtest::Regime;
use altim_core::engine::validation::{AssetResult, GroupStat, RegimeGroup, ValidationReport, Verdict};
use altim_core::web::insights::validation::{
    SORTS, SortKey, VALIDATION_URL, beat_text, class_short, group_short, history_tile, month_year, plain, pooled_text, regime_days_text, regimes_of,
    signed_pct, sort_assets, verdict_tone, years,
};
use yew::prelude::*;

use crate::api::Pending;
use crate::route::use_on_link;

/// A heavy report polled every 5 s while the server computes it (`pending`), as Validation.tsx and Bot.tsx do.
#[derive(Clone)]
pub(crate) enum Load<T> {
    Loading,
    Pending,
    Error(String),
    Ready(Rc<T>),
}

#[hook]
pub(crate) fn use_heavy_report<T: serde::de::DeserializeOwned + Clone + 'static>(url: &'static str) -> Load<T> {
    let state = use_state(|| Load::<T>::Loading);
    {
        let state = state.clone();
        use_effect_with((), move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let a = alive.clone();
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    let r = crate::api::get_pending::<T>(url).await;
                    if !a.get() {
                        return;
                    }
                    match r {
                        Ok(Pending::Ready(r)) => return state.set(Load::Ready(Rc::new(r))),
                        Ok(Pending::Pending) => state.set(Load::Pending),
                        Err(e) => return state.set(Load::Error(e.0)),
                    }
                    gloo_timers::future::TimeoutFuture::new(5_000).await;
                    if !a.get() {
                        return;
                    }
                }
            });
            move || alive.set(false)
        });
    }
    (*state).clone()
}

#[component]
pub fn Validation() -> Html {
    let state = use_heavy_report::<ValidationReport>(VALIDATION_URL);
    let pending = matches!(state, Load::Pending);
    html! {
        <section class="app-screen validation">
            <div class="screen-top">
                <div>
                    <h1>{ "Validation du modèle" }</h1>
                    <p class="muted small">
                        { "Le signal d'Altim rejoué jour par jour sur un panier d'actions, de bitcoin, d'ether et d'altcoins choisi à l'avance, avec les mêmes réglages et les mêmes coûts que l'historique de la carte Décision. Un test du passé, pas une promesse." }
                    </p>
                </div>
            </div>
            { match state {
                Load::Error(e) => html! { <p class="notice warn">{ format!("⚠ {e}") }</p> },
                Load::Loading | Load::Pending => html! {
                    <div class="card">
                        <p class="muted">{ if pending { "Calcul sur tout le panier en cours (environ 30 secondes la première fois)…" } else { "Chargement…" } }</p>
                        <div class="skeleton" />
                    </div>
                },
                Load::Ready(r) => html! { <ValidationView report={r} /> },
            } }
        </section>
    }
}

#[derive(Properties, PartialEq)]
pub struct ValidationViewProps {
    pub report: Rc<ValidationReport>,
}

#[component]
pub fn ValidationView(p: &ValidationViewProps) -> Html {
    let r = &p.report;
    let regime_group = use_state(|| "all".to_string());
    let sort = use_state(|| SortKey::Class);
    let on_link = use_on_link();
    let groups: Vec<&GroupStat> = std::iter::once(&r.overall).chain(r.classes.iter()).collect();
    let shown_regimes = groups.iter().find(|g| g.id == *regime_group).copied().unwrap_or(&r.overall);
    html! {
        <>
            <div class="card val-head">
                <p class="val-headline">{ &r.headline }</p>
                <div class="val-tiles">
                    <Tile label="Actifs testés" value={format!("{} / {}", r.overall.assets, r.basket.len())} />
                    <Tile label="Trades" value={r.overall.pooled.trades.to_string()} />
                    <Tile label="Historique" value={history_tile(r.years, r.min_years, r.max_years)} />
                </div>
                <BeatMeter beat={r.overall.beat_hold} n={r.overall.assets} share={r.overall.beat_share} />
                <p class="small"><VerdictChip verdict={r.overall.verdict} label={r.overall.verdict_label.clone()} /></p>
                <p class="muted small">{ format!("Tous les trades ensemble : {}", pooled_text(&r.overall.pooled)) }</p>
                <p class="muted small">
                    { format!(
                        "Période : {} – {} (bougies journalières). Calculé le {}, mis à jour toutes les 12 h.",
                        month_year(r.from),
                        month_year(r.to),
                        crate::ui::fr_date_time_short(r.as_of as f64)
                    ) }
                </p>
                if !r.failures.is_empty() {
                    <div class="notice warn small">
                        { format!("{} actif{s} non testé{s} :", r.failures.len(), s = if r.failures.len() > 1 { "s" } else { "" }) }
                        <ul class="reasons">{ for r.failures.iter().map(|f| html! { <li key={f.symbol.clone()}><b>{ &f.symbol }</b>{ format!(" — {}", f.error) }</li> }) }</ul>
                    </div>
                }
            </div>

            <h2 class="section-label">{ "Par classe d'actifs" }</h2>
            <div class="val-grid">{ for r.classes.iter().map(|g| html! { <GroupCard key={g.id.clone()} g={g.clone()} tax_rate={r.parameters.tax_rate_pct} /> }) }</div>

            <h2 class="section-label">{ "Par régime de marché" }</h2>
            <div class="val-chips" role="group" aria-label="Actifs pris en compte">
                { for groups.iter().map(|g| {
                    let on = g.id == *regime_group;
                    let pick = {
                        let (rg, id) = (regime_group.clone(), g.id.clone());
                        Callback::from(move |_| rg.set(id.clone()))
                    };
                    html! {
                        <button key={g.id.clone()} type="button" class={classes!("chip", "pick", on.then_some("on"))} aria-pressed={on.to_string()} onclick={pick}>
                            { group_short(&g.id) }
                        </button>
                    }
                }) }
            </div>
            <div class="val-grid">{ for regimes_of(shown_regimes).into_iter().map(|g| html! { <RegimeCard key={format!("{:?}", g.regime)} g={g.clone()} /> }) }</div>
            <p class="muted small">
                { format!("{} Régime lu la veille, sans données futures ; « inconnu » tant que l'historique est trop court pour le classer.", r.parameters.regime_rule) }
            </p>

            <h2 class="section-label">{ "Actif par actif" }</h2>
            <div class="val-chips" role="group" aria-label="Ordre des actifs">
                { for SORTS.iter().map(|(k, l)| {
                    let on = *k == *sort;
                    let pick = {
                        let (sort, k) = (sort.clone(), *k);
                        Callback::from(move |_| sort.set(k))
                    };
                    html! { <button key={k.id()} type="button" class={classes!("chip", "pick", on.then_some("on"))} aria-pressed={on.to_string()} onclick={pick}>{ *l }</button> }
                }) }
            </div>
            <ul class="val-assets">
                { for sort_assets(&r.assets, *sort).into_iter().map(|a| html! { <AssetRow key={a.symbol.clone()} a={a.clone()} on_link={on_link.clone()} /> }) }
            </ul>

            <div class="card">
                <h2 class="card-title">{ "Protections contre les biais" }</h2>
                <ul class="reasons">{ for r.protections.iter().map(|x| html! { <li key={x.clone()}>{ x }</li> }) }</ul>
                <p class="small"><b>{ "Hors échantillon ?" }</b>{ format!(" {}", r.out_of_sample.note) }</p>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Biais et limites" }</h2>
                <ul class="reasons">{ for r.limits.iter().map(|x| html! { <li key={x.clone()}>{ x }</li> }) }</ul>
                <p class="muted small">
                    { format!(
                        "Coûts par ordre : frais {}\u{202f}%, glissement {}\u{202f}%, écart achat/vente supposé {}\u{202f}% (actions) ou {}\u{202f}% (cryptos), moitié payée à chaque ordre. Panier fixé le {}. Source : {}.",
                        plain(Some(r.parameters.fees_pct), 3),
                        plain(Some(r.parameters.slippage_pct), 3),
                        plain(Some(r.parameters.spread_stock_pct), 3),
                        plain(Some(r.parameters.spread_crypto_pct), 3),
                        altim_core::web::bot::fr_iso(&r.basket_fixed_on),
                        r.source
                    ) }
                </p>
            </div>
        </>
    }
}

#[derive(Properties, PartialEq)]
struct TileProps {
    label: AttrValue,
    value: AttrValue,
}

#[component]
fn Tile(p: &TileProps) -> Html {
    html! {
        <div class="val-tile">
            <span class="muted small">{ p.label.clone() }</span>
            <b>{ p.value.clone() }</b>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct BeatMeterProps {
    pub beat: usize,
    pub n: usize,
    pub share: Option<f64>,
}

/// Share of the assets where the signal beat buy-and-hold: one thin bar, the number written next to it.
#[component]
pub fn BeatMeter(p: &BeatMeterProps) -> Html {
    let width = p.share.unwrap_or(0.0).clamp(0.0, 100.0);
    html! {
        <div class="val-meter">
            <p class="kv small"><span>{ "A battu la simple détention" }</span><b>{ beat_text(p.beat, p.n, p.share) }</b></p>
            <div class="val-bar" role="img" aria-label={format!("A battu la détention sur {} actifs sur {}", p.beat, p.n)}>
                <span style={format!("width: {}%;", altim_core::js::number_to_string(width))} />
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct VerdictChipProps {
    verdict: Verdict,
    label: AttrValue,
}

#[component]
fn VerdictChip(p: &VerdictChipProps) -> Html {
    html! { <span class={classes!("chip", "val-verdict", verdict_tone(p.verdict))}>{ p.label.clone() }</span> }
}

/// A figure: label, value, green / red when it has a sign.
fn kv(label: String, value: String, tone: Option<f64>) -> Html {
    let cls = tone.map(|t| if t >= 0.0 { "up" } else { "down" });
    html! { <p class="kv small"><span>{ label }</span><b class={cls}>{ value }</b></p> }
}

#[derive(Properties, PartialEq)]
pub struct GroupCardProps {
    pub g: GroupStat,
    pub tax_rate: f64,
}

#[component]
pub fn GroupCard(p: &GroupCardProps) -> Html {
    let g = &p.g;
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ &g.label }</h3>
            <p class="small"><VerdictChip verdict={g.verdict} label={g.verdict_label.clone()} /></p>
            <p class="muted small">{ format!("{} actif{} · historique médian {}", g.assets, if g.assets > 1 { "s" } else { "" }, years(g.years)) }</p>
            <p class="small">{ pooled_text(&g.pooled) }</p>
            <BeatMeter beat={g.beat_hold} n={g.assets} share={g.beat_share} />
            { kv("Rendement médian du signal".into(), signed_pct(g.median_return, 1), g.median_return) }
            { kv("Détention médiane".into(), signed_pct(g.median_hold, 1), g.median_hold) }
            if let Some(w) = &g.worst_return {
                { kv(format!("Pire actif ({})", w.symbol), signed_pct(Some(w.value), 1), Some(w.value)) }
            }
            { kv("Recul max médian (signal / détention)".into(), format!("{} / {}", signed_pct(g.median_drawdown, 1), signed_pct(g.median_hold_drawdown, 1)), None) }
            if let Some(w) = &g.worst_drawdown {
                { kv(format!("Pire recul ({})", w.symbol), signed_pct(Some(w.value), 1), None) }
            }
            { kv("Sharpe / Sortino médians".into(), format!("{} / {}", plain(g.median_sharpe, 2), plain(g.median_sortino, 2)), None) }
            { kv("Multiple de R moyen".into(), g.pooled.avg_r.map(|r| format!("{} R", plain(Some(r), 2))).unwrap_or_else(|| "—".into()), None) }
            { kv("Temps investi médian".into(), g.median_exposure.map(|e| format!("{}\u{202f}%", plain(Some(e), 0))).unwrap_or_else(|| "—".into()), None) }
            { kv(
                format!("Après impôt {}\u{202f}% (signal / détention)", plain(Some(p.tax_rate), 0)),
                format!("{} / {}", signed_pct(g.median_after_tax, 1), signed_pct(g.median_hold_after_tax, 1)),
                None,
            ) }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct RegimeCardProps {
    pub g: RegimeGroup,
}

#[component]
pub fn RegimeCard(p: &RegimeCardProps) -> Html {
    let g = &p.g;
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ &g.label }</h3>
            <p class="small"><VerdictChip verdict={g.verdict} label={g.verdict_label.clone()} /></p>
            <p class="small">{ pooled_text(&g.pooled) }</p>
            <p class="muted small">{ format!("{} jours-actifs dans ce régime.", altim_core::js::fr(g.days as f64, 0, 3)) }</p>
            if g.regime != Regime::Unknown {
                <p class="small">{ regime_days_text(g) }</p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AssetRowProps {
    pub a: AssetResult,
    pub on_link: Callback<MouseEvent>,
}

#[component]
pub fn AssetRow(p: &AssetRowProps) -> Html {
    let a = &p.a;
    let gap = a.total_return - a.buy_and_hold;
    let tone = |v: f64| if v >= 0.0 { "up" } else { "down" };
    let kind = a.kind.map(|k| k.as_str()).unwrap_or_default();
    let class = a.class.map(class_short).unwrap_or_default();
    let win = a.win_rate.map(|w| format!("{}\u{202f}%", plain(Some(w), 0))).unwrap_or_else(|| "—".into());
    let span = years(Some((a.to - a.from) as f64 / (365.25 * 86_400_000.0)));
    html! {
        <li class="val-asset">
            <div class="val-asset-top">
                <a href={format!("/app/actif/{kind}/{}", a.symbol)} onclick={p.on_link.clone()} class="val-sym"><b>{ &a.symbol }</b>{ " " }<span class="muted small">{ &a.name }</span></a>
                <span class="chip muted">{ format!("{class} · {}", a.group) }</span>
                if a.low_sample {
                    <span class="chip muted">{ "échantillon trop faible" }</span>
                }
            </div>
            <p class="small">
                { "Signal " }<b class={tone(a.total_return)}>{ signed_pct(Some(a.total_return), 1) }</b>{ " · détention " }<b class={tone(a.buy_and_hold)}>{ signed_pct(Some(a.buy_and_hold), 1) }</b>
                { " " }<span class="muted">{ format!("({}, écart {})", if a.beat_hold { "mieux" } else { "moins bien" }, signed_pct(Some(gap), 1)) }</span>
            </p>
            <p class="muted small">
                { format!(
                    "{} trade{} · réussite {win} · facteur de profit {} · espérance {} · recul max {} (détention {}) · Sharpe {} · Sortino {} · {span} ({})",
                    a.trades,
                    if a.trades > 1 { "s" } else { "" },
                    plain(a.profit_factor, 2),
                    signed_pct(a.expectancy, 2),
                    signed_pct(Some(a.max_drawdown), 1),
                    signed_pct(Some(a.hold_max_drawdown), 1),
                    plain(a.sharpe, 2),
                    plain(a.sortino, 2),
                    a.source
                ) }
            </p>
        </li>
    }
}
