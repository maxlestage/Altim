//! Unusual readings on this asset (AnomaliesCard.tsx, /api/anomalies): volume, price/volume, z-score; derivatives
//! for cryptos, with what they may mean (a measured gap, never a forecast).
use altim_core::js::{fr, round};
use altim_core::types::Kind;
use altim_core::web::decision::reports::{Anomaly, AnomalyReport, Derivatives, compact_usd};
use altim_core::web::money::{MoneyDisplay, NBSP};
use yew::prelude::*;

fn fr1(v: f64, d: usize) -> String {
    fr(v, 0, d)
}

fn signed(v: f64, d: usize) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr1(v.abs(), d))
}

fn opts(pairs: &[(&str, &str)]) -> wasm_bindgen::JsValue {
    let o = js_sys::Object::new();
    for (k, v) in pairs {
        let _ = js_sys::Reflect::set(&o, &(*k).into(), &(*v).into());
    }
    o.into()
}

/// "29 sept., 14:05" (`toLocaleString("fr-FR", { day, month: "short", hour, minute })`, the browser's time zone).
fn time(t: f64) -> String {
    js_sys::Date::new(&t.into())
        .to_locale_string("fr-FR", &opts(&[("day", "numeric"), ("month", "short"), ("hour", "2-digit"), ("minute", "2-digit")]))
        .into()
}

fn alert(a: &Anomaly) -> Html {
    html! {
        <li key={a.code.clone()} class={format!("anom anom-{}", a.severity)}>
            <b>{ format!("⚠️ {}", a.title) }</b>
            <small class="mono">{ a.measured.clone() }</small>
            <p class="small">{ a.meaning.clone() }</p>
            <small class="muted">{ format!("Source : {}", a.source) }</small>
        </li>
    }
}

fn derivatives_block(d: &Derivatives, m: &MoneyDisplay) -> Html {
    let c = |v: f64| compact_usd(v, m);
    html! {
        <div class="anom-deriv">
            <h3 class="section-label">{ "Dérivés et gros mouvements" }</h3>
            { for d.errors.iter().map(|e| html! { <p key={e.clone()} class="notice warn small">{ e.clone() }</p> }) }
            if let Some(l) = &d.liquidations {
                <div class="anom-block">
                    <p class="kv">
                        <span>{ format!("Liquidations {}", if l.complete { "24 h".to_string() } else { format!("{} h (lecture partielle)", fr1(l.hours, 1)) }) }</span>
                        <b>{ c(l.long_usd + l.short_usd) }</b>
                    </p>
                    <p class="kv small">
                        <span>{ format!("Acheteurs liquidés ({})", l.long_count) }</span>
                        <b class="sell">{ format!("{}{}", c(l.long_usd), l.long_share().map(|s| format!(" · {} %", round(s))).unwrap_or_default()) }</b>
                    </p>
                    <p class="kv small"><span>{ format!("Vendeurs liquidés ({})", l.short_count) }</span><b class="buy">{ c(l.short_usd) }</b></p>
                    if let Some(x) = &l.largest {
                        <p class="kv small">
                            <span>{ "Plus grosse" }</span>
                            <b>{ format!("{} · {} à {} · {}", c(x.usd), if x.long { "acheteur" } else { "vendeur" }, m.money_fmt(x.price, |v| fr1(v, 2), NBSP), time(x.time)) }</b>
                        </p>
                    }
                    <p class="muted small">{ l.scope.clone() }</p>
                </div>
            }
            if let Some(oi) = &d.open_interest {
                <p class="kv">
                    <span>{ "Open interest" }</span>
                    <b>
                        { c(oi.usd) }
                        { oi.change24h.map(|v| format!(" · {} 24 h", signed(v, 1))).unwrap_or_default() }
                        { oi.change7d.map(|v| format!(" · {} 7 j", signed(v, 1))).unwrap_or_default() }
                    </b>
                </p>
            }
            if let Some(f) = &d.funding {
                <p class="kv">
                    <span>{ format!("Funding (dernier règlement{})", f.period_hours.filter(|h| *h != 0.0).map(|h| format!(", toutes les {} h", fr1(h, 0))).unwrap_or_default()) }</span>
                    <b>{ format!("{} · habituel {} à {}", signed(f.rate, 4), signed(f.p5, 4), signed(f.p95, 4)) }</b>
                </p>
            }
            if let Some(ls) = &d.long_short {
                <p class="kv">
                    <span>{ "Ratio comptes acheteurs / vendeurs" }</span>
                    <b>{ format!("{} · habituel {} à {}", fr1(ls.ratio, 2), fr1(ls.p5, 2), fr1(ls.p95, 2)) }</b>
                </p>
            }
            <p class="muted small">
                { format!("Source : {} ; plages habituelles = 5 à 95 % des valeurs récentes (funding : ≈ 100 derniers règlements ; ratio : 30 j).", d.source) }
            </p>
            <ul class="small anom-nc">
                { for d.not_covered.iter().map(|n| html! { <li key={n.label.clone()}><b>{ format!("Non couvert — {}", n.label) }</b>{ format!(" : {}.", n.reason) }</li> }) }
            </ul>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AnomaliesCardProps {
    pub symbol: AttrValue,
    pub kind: Kind,
}

#[component]
pub fn AnomaliesCard(p: &AnomaliesCardProps) -> Html {
    let m = crate::money::use_money();
    let report = use_state(|| None::<AnomalyReport>);
    let error = use_state(|| None::<String>);
    {
        let (report, error) = (report.clone(), error.clone());
        use_effect_with((p.symbol.clone(), p.kind), move |(symbol, kind)| {
            let alive = std::rc::Rc::new(std::cell::Cell::new(true));
            report.set(None);
            error.set(None);
            let (a, symbol, kind) = (alive.clone(), symbol.to_string(), *kind);
            wasm_bindgen_futures::spawn_local(async move {
                let r = super::api::anomalies(&symbol, kind).await;
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
    html! {
        <div class="card anomalies">
            <h2 class="card-title">{ "Détection d'anomalies" }</h2>
            if let Some(e) = &*error {
                <p class="notice warn small">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <div class="skeleton" aria-label="Chargement des anomalies" />
            }
            if let Some(r) = &*report {
                if r.anomalies.is_empty() {
                    <p class="small">
                        { format!(
                            "Rien d'inhabituel sur les mesures ci-dessous{}.",
                            r.session.filter(|s| *s != 0.0).map(|s| format!(" (séance du {})", String::from(js_sys::Date::new(&s.into()).to_locale_date_string("fr-FR", &wasm_bindgen::JsValue::UNDEFINED)))).unwrap_or_default()
                        ) }
                    </p>
                } else {
                    <ul class="anom-list">{ for r.anomalies.iter().map(alert) }</ul>
                }
                { for r.errors.iter().map(|e| html! { <p key={e.clone()} class="notice warn small">{ e.clone() }</p> }) }
                if !r.normal.is_empty() {
                    <details>
                        <summary class="small">{ format!("Mesures dans la normale · {}", r.normal.len()) }</summary>
                        <ul class="anom-normal small">
                            { for r.normal.iter().map(|a| html! { <li key={a.code.clone()}><span>{ a.title.clone() }</span>{ " " }<small class="muted mono">{ a.measured.clone() }</small></li> }) }
                        </ul>
                    </details>
                }
                if let Some(d) = &r.derivatives {
                    { derivatives_block(d, &m) }
                }
                <p class="muted small">
                    { "Une anomalie est un écart mesuré, pas une prévision : son sens reste à confirmer. " }
                    { if r.source.is_empty() { String::new() } else { format!("{}.", r.source) } }
                </p>
            }
        </div>
    }
}
