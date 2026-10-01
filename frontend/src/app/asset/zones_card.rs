//! Buy zones by horizon (ZonesCard.tsx, Fibonacci): the user's horizon first and open, the others one tap away; the
//! status and the distance follow the live price. `MacroBlock` is the macro and geopolitical context under them.
use altim_core::engine::fibonacci::{FibZone, Horizon, Trend, ZoneStatus, zone_state};
use altim_core::engine::guard::{FactorStatus, weigh};
use altim_core::engine::macro_ctx::MacroLevel;
use altim_core::js::{number_to_string, round, to_fixed};
use altim_core::web::decision::reports::{HorizonZone, MacroInfo, ZonesReport};
use altim_core::web::sorting::Sorting;
use altim_core::web::store::HorizonPref;
use yew::prelude::*;

fn usd(v: f64) -> String {
    crate::money::price(v)
}

fn pct(v: f64) -> String {
    format!("{} %", round(v))
}

/// (label, badge tone) of a zone status.
fn status_ui(s: ZoneStatus) -> (&'static str, &'static str) {
    match s {
        ZoneStatus::Above => ("Attendre le repli", "hold"),
        ZoneStatus::InZone => ("Dans la zone", "buy"),
        ZoneStatus::Golden => ("Zone d'or", "buy"),
        ZoneStatus::Deep => ("Repli profond", "hold"),
        ZoneStatus::Broken => ("Zone invalidée", "sell"),
        ZoneStatus::Downtrend => ("Tendance baissière", "sell"),
        ZoneStatus::None => ("Pas de niveau net", "unknown"),
    }
}

pub fn horizon_of(h: HorizonPref) -> Horizon {
    match h {
        HorizonPref::Short => Horizon::Short,
        HorizonPref::Medium => Horizon::Medium,
        HorizonPref::Long => Horizon::Long,
    }
}

fn evidence(z: &FibZone) -> Html {
    let Some(e) = &z.evidence else {
        return html! { <p class="muted small">{ "Historique : aucun repli comparable sur cet actif pour cet horizon, zone non vérifiée." }</p> };
    };
    let (_, status) = weigh(Some(e), true);
    let verdict = match status {
        FactorStatus::Verified => "ces zones ont mieux tenu qu'une entrée au hasard sur cet actif",
        FactorStatus::Unproven => "trop peu de cas pour conclure",
        FactorStatus::Rejected => "ces zones n'ont pas fait mieux qu'une entrée au hasard sur cet actif",
        FactorStatus::Unverifiable => "",
    };
    html! {
        <p class={format!("small {}", if status == FactorStatus::Verified { "up" } else { "muted" })}>
            { format!(
                "Historique : sur {} repli{} dans la zone, {} sont remontés au plus haut avant de casser le plus bas, contre {} pour une entrée au hasard : {verdict}.",
                number_to_string(e.samples), if e.samples > 1.0 { "s" } else { "" }, pct(e.rate), pct(e.base)
            ) }
        </p>
    }
}

/// Horizontal ladder: low of the move → high, buy zone, golden pocket and current price.
fn ladder(z: &FibZone, price: f64) -> Html {
    let (Some(s), Some(zone), Some(golden)) = (&z.swing, &z.zone, &z.golden) else { return html! {} };
    let (lo, hi) = (s.low.min(price), s.high.max(price));
    let span = if hi - lo == 0.0 { 1.0 } else { hi - lo };
    let x = |v: f64| format!("{}%", number_to_string((v - lo) / span * 100.0));
    let w = |a: f64, b: f64| format!("{}%", number_to_string((b - a).abs() / span * 100.0));
    html! {
        <div class="fib-ladder" role="img" aria-label={format!("Prix {}, zone d'achat {} – {}", usd(price), usd(zone.from), usd(zone.to))}>
            <div class="fib-track">
                <i class="fib-zone" style={format!("left: {}; width: {};", x(zone.from), w(zone.from, zone.to))} />
                <i class="fib-golden" style={format!("left: {}; width: {};", x(golden.from), w(golden.from, golden.to))} />
                <i class="fib-price" style={format!("left: {};", x(price))} />
            </div>
            <div class="fib-ends mono small"><span>{ usd(s.low) }</span><span>{ usd(s.high) }</span></div>
        </div>
    }
}

fn zone_detail(z: &FibZone, macro_note: Option<&str>, price: Option<f64>) -> Html {
    html! {
        <div class="fib-detail">
            <p class="small">{ z.text.clone() }</p>
            if let Some(p) = price.filter(|p| *p != 0.0 && z.swing.is_some() && z.zone.is_some()) {
                { ladder(z, p) }
            }
            if let (Some(zone), Some(golden)) = (&z.zone, &z.golden) {
                <dl class="fib-levels">
                    <div>
                        <dt>{ "Zone d'achat (38,2 – 61,8 %)" }</dt>
                        <dd class="mono">{ format!("{} – {}", z.levels.iter().find(|l| l.ratio == 0.618).map(|l| usd(l.price)).unwrap_or_default(), usd(zone.to)) }</dd>
                    </div>
                    <div><dt>{ "Zone d'or (61,8 – 65 %)" }</dt><dd class="mono">{ format!("{} – {}", usd(golden.from), usd(golden.to)) }</dd></div>
                    <div><dt>{ "Invalidation (plus bas)" }</dt><dd class="mono sell">{ z.invalidation.map(usd).unwrap_or_default() }</dd></div>
                    <div><dt>{ "Objectifs" }</dt><dd class="mono buy">{ z.targets.iter().map(|t| usd(*t)).collect::<Vec<_>>().join(" · ") }</dd></div>
                </dl>
            }
            if z.status != ZoneStatus::None && z.status != ZoneStatus::Downtrend {
                { evidence(z) }
            }
            if let Some(n) = macro_note.filter(|n| !n.is_empty()) {
                <p class="notice warn small">{ format!("⚠ {n}") }</p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct MacroBlockProps {
    pub m: MacroInfo,
}

#[component]
pub fn MacroBlock(p: &MacroBlockProps) -> Html {
    let m = &p.m;
    let r = &m.report;
    let (level, key) = match r.level {
        MacroLevel::Calm => ("Calme", "calm"),
        MacroLevel::Tense => ("Tendu", "tense"),
        MacroLevel::High => ("Très tendu", "high"),
    };
    html! {
        <div class={format!("macro macro-{key}")}>
            <p class="kv"><span>{ "Contexte macro et géopolitique" }</span><b>{ format!("{level} · {}/100", number_to_string(r.score)) }</b></p>
            if let Some(g) = &m.regime {
                <p class="small">
                    <span class="muted">{ "Régime de marché : " }</span><b>{ g.label.clone() }</b>
                    if !g.reasons.is_empty() {
                        <small class="muted">{ format!(" — {}", g.reasons.join(" · ")) }</small>
                    }
                </p>
            }
            if r.factors.is_empty() {
                <p class="muted small">{ "Aucun signe de stress sur la peur (VIX), le S&P 500, le pétrole, l'or, le dollar ni les taux." }</p>
            } else {
                <ul class="guard-factors">
                    { for r.factors.iter().map(|f| html! { <li key={f.code.clone()}>{ format!("{} ", f.text) }<small class="muted">{ format!("+{}", number_to_string(f.points)) }</small></li> }) }
                </ul>
            }
            if let Some(e) = m.evidence.filter(|e| e.samples >= 20.0) {
                <p class="muted small">
                    { format!(
                        "Sur cet actif, les jours de stress macro ont été suivis d'une forte baisse dans {} des cas en 5 jours, contre {} d'habitude{}",
                        pct(e.rate), pct(e.base), if e.lift >= 1.1 { " : à prendre au sérieux." } else { " : pas d'effet mesurable ici." }
                    ) }
                </p>
            }
            if !r.themes.is_empty() {
                <details>
                    <summary class="small">{ format!("Sujets du jour ({})", r.themes.iter().map(|t| format!("{} {}", t.label, t.count)).collect::<Vec<_>>().join(" · ")) }</summary>
                    <ul class="small">
                        { for r.themes.iter().flat_map(|t| t.examples.iter().take(2)).map(|x| html! { <li key={x.clone()}>{ x.clone() }</li> }) }
                    </ul>
                </details>
            }
            <p class="muted small">
                { "Personne ne peut prévoir une guerre ou une crise. Altim mesure le stress qu'elle crée dès qu'elle commence et vous dit d'être plus prudent." }
            </p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ZonesCardProps {
    pub report: ZonesReport,
    pub price: Option<f64>,
    pub horizon: HorizonPref,
}

#[component]
pub fn ZonesCard(p: &ZonesCardProps) -> Html {
    let _m = crate::money::use_money();
    let mine = horizon_of(p.horizon);
    let open = use_state(|| Some(mine));
    let price = p.price.or(p.report.price);
    // Status and distance follow the live price.
    let mut zones: Vec<HorizonZone> = p
        .report
        .zones
        .iter()
        .map(|z| {
            let mut z = z.clone();
            if let (Some(pr), Some(s)) = (price.filter(|v| *v != 0.0), z.zone.swing.filter(|s| s.trend == Trend::Up)) {
                let st = zone_state(&s, pr);
                z.zone.status = st.status;
                z.zone.distance = st.distance;
                z.zone.text = st.text;
            }
            z
        })
        .collect();
    zones.sort_by_key_dyn(|z| z.zone.horizon != mine);
    html! {
        <div class="card zones">
            <h2 class="card-title">{ "Zones d'achat par horizon" }</h2>
            <p class="muted small">{ "Retracements de Fibonacci du dernier mouvement haussier : là où les traders attendent un repli pour acheter." }</p>
            <ul class="zone-list">
                { for zones.iter().map(|hz| {
                    let z = &hz.zone;
                    let (label, tone) = status_ui(z.status);
                    let is_open = *open == Some(z.horizon);
                    let toggle = {
                        let (open, h) = (open.clone(), z.horizon);
                        Callback::from(move |_| open.set(if is_open { None } else { Some(h) }))
                    };
                    html! {
                        <li key={z.horizon.as_str()} class={format!("zone-row {}", if z.horizon == mine { "mine" } else { "" })}>
                            <button class="zone-head" aria-expanded={is_open.to_string()} onclick={toggle}>
                                <span>
                                    <b>{ z.label.clone() }</b>
                                    <small class="muted">{ format!(" · {}{}", z.unit, if z.horizon == mine { " · votre horizon" } else { "" }) }</small>
                                </span>
                                <span class={format!("badge {tone}")}>
                                    { format!("{label}{}", z.distance.map(|d| format!(" −{} %", to_fixed(d, 1).replace('.', ","))).unwrap_or_default()) }
                                </span>
                            </button>
                            if is_open {
                                { zone_detail(z, hz.macro_note.as_deref(), price) }
                            }
                        </li>
                    }
                }) }
            </ul>
            if let Some(m) = &p.report.macro_info {
                <MacroBlock m={m.clone()} />
            }
        </div>
    }
}
