//! « Exposition sectorielle » (SectorCard.tsx): stocks by sector (Nasdaq / SEC, /api/sectors), cryptos and cash as
//! their own blocks, what no source covers said as such.
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use altim_core::js::{fr, number_to_string};
use altim_core::types::Kind;
use altim_core::web::portfolio::holdings::{InsightLevel, PortfolioAnalysis, pc};
use altim_core::web::portfolio::sectors::{DEFAULT_FAILURE, ExposureLine, SectorItem, sector_exposure, sector_insight_text, source_line};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct SectorCardProps {
    pub analysis: Rc<PortfolioAnalysis>,
}

#[derive(Clone, PartialEq)]
struct Loaded {
    key: String,
    items: Option<Rc<HashMap<String, SectorItem>>>,
    error: Option<String>,
}

#[component]
pub fn SectorCard(p: &SectorCardProps) -> Html {
    let a = &p.analysis;
    let stocks: Vec<String> =
        a.lines.iter().filter(|l| l.kind == Kind::Stock).map(|l| l.symbol.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let key = stocks.join(",");
    let state = use_state(|| None::<Loaded>);
    {
        let state = state.clone();
        use_effect_with(key.clone(), move |key| {
            let alive = Rc::new(Cell::new(true));
            if stocks.is_empty() {
                state.set(Some(Loaded { key: key.clone(), items: Some(Rc::new(HashMap::new())), error: None }));
            } else {
                let (a, key) = (alive.clone(), key.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let r = super::api::sectors(&stocks).await;
                    if !a.get() {
                        return;
                    }
                    state.set(Some(match r {
                        Ok(r) => Loaded { key, items: Some(Rc::new(r.items.into_iter().map(|i| (i.symbol.clone(), i)).collect())), error: None },
                        Err(e) => Loaded { key, items: None, error: Some(e.0) },
                    }));
                });
            }
            move || alive.set(false)
        });
    }

    let exposure = (*state).as_ref().filter(|s| s.key == key).map(|s| {
        let lines: Vec<ExposureLine> = a.lines.iter().map(|l| ExposureLine { symbol: l.symbol.clone(), kind: l.kind, value: l.value }).collect();
        let failure = s.error.as_ref().map(|e| format!("classement indisponible ({e})")).unwrap_or_else(|| DEFAULT_FAILURE.into());
        sector_exposure(&lines, a.cash, s.items.as_deref(), &failure)
    });
    html! {
        <div class="card sector-card">
            <h2 class="card-title">{ "Exposition sectorielle" }</h2>
            if let Some(e) = exposure {
                <ul class="sector-bars" aria-label="Poids de chaque secteur dans le patrimoine">
                    { for e.blocks.iter().map(|b| html! {
                        <li key={b.key.clone()} class={classes!("sector-row", format!("sector-{}", b.kind.as_str()))}>
                            <div class="sector-head">
                                <span class="sector-label">{ b.label.clone() }</span>
                                <b class="mono">{ pc(b.weight) }</b>
                            </div>
                            <div class="sector-track" aria-hidden="true"><i style={format!("width: {}%;", number_to_string(b.weight.min(100.0)))} /></div>
                            if !b.symbols.is_empty() || b.stock_weight.is_some() {
                                <small class="muted">
                                    { b.symbols.join(", ") }
                                    { match b.stock_weight {
                                        Some(sw) if e.stock_value < e.total => format!(" · {} des actions", pc(sw)),
                                        _ => String::new(),
                                    } }
                                </small>
                            }
                        </li>
                    }) }
                </ul>
                // The effective number of sectors is shown as a figure below.
                if e.insights.iter().any(|i| i.code != "sector_effective") {
                    <ul class="insights">
                        { for e.insights.iter().filter(|i| i.code != "sector_effective").map(|i| html! {
                            <li key={i.code.clone()} class={classes!("insight", i.level.as_str())}>
                                <span aria-hidden="true">{ if i.level == InsightLevel::Warning { "⚠" } else { "ℹ" } }</span>
                                <span>{ sector_insight_text(i) }</span>
                            </li>
                        }) }
                    </ul>
                }
                if let Some(eff) = e.effective_sectors {
                    <p class="kv small"><span>{ "Secteurs effectifs (actions classées)" }</span><b>{ fr(eff, 0, 1) }</b></p>
                }
                if !e.unknown.is_empty() {
                    <ul class="reasons small">
                        { for e.unknown.iter().map(|u| html! { <li key={u.symbol.clone()}><b>{ u.symbol.clone() }</b>{ format!(" : {}", u.reason) }</li> }) }
                    </ul>
                }
                <p class="muted small">
                    { format!(
                        "Sources : {}. Crypto et liquidités : vos avoirs. Les secteurs SEC (codes SIC) sont de grandes divisions, affichés à part de ceux du Nasdaq.",
                        Some(source_line(&e.by_source)).filter(|s| !s.is_empty()).unwrap_or_else(|| "aucune action à classer".into())
                    ) }
                </p>
            } else {
                <p class="muted small">{ "Lecture des secteurs de vos actions…" }</p>
            }
        </div>
    }
}
