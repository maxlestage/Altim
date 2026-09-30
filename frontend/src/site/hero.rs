//! Hero of the home page and the phone mockup reproducing the Radar (Hero.tsx, PhoneMockup.tsx).
use std::rc::Rc;

use altim_core::types::Kind;
use altim_core::web::market::{Tick, format_percent};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct TicksProps {
    pub ticks: Rc<Vec<Tick>>,
}

#[component]
pub fn Hero(p: &TicksProps) -> Html {
    html! {
        <section class="hero">
            <div class="hero-text">
                <p class="chip">
                    <span class="dot" />{ " Conseiller crypto & actions · iPhone, Apple Watch, Android et web" }
                </p>
                <h1>
                    { "Le marché," }
                    <br />
                    <span class="gradient glitch" data-text="décodé.">{ "décodé." }</span>
                </h1>
                <p class="lead">
                    { "Altim scanne en continu vos cryptos et vos actions, croise " }<strong>{ "7 familles d'indicateurs" }</strong>
                    { " sur plusieurs unités de temps et vous dit clairement quand " }<em class="buy">{ "acheter" }</em>{ ", quand" }{ " " }
                    <em class="sell">{ "vendre" }</em>{ " — et surtout quand " }<em class="hold">{ "attendre" }</em>{ "." }
                </p>
                <div class="cta">
                    <a class="btn" href="/app">{ "Ouvrir l'app web" }</a>
                    <a class="btn btn-ghost" href="#live">{ "Voir le signal en direct" }</a>
                </div>
                <dl class="stats">
                    <div><dt>{ "7" }</dt><dd>{ "indicateurs croisés" }</dd></div>
                    <div><dt>{ "4" }</dt><dd>{ "unités de temps" }</dd></div>
                    <div><dt>{ "16" }</dt><dd>{ "sources de prix recoupées" }</dd></div>
                </dl>
            </div>
            <PhoneMockup ticks={p.ticks.clone()} />
        </section>
    }
}

const BADGES: [(&str, &str); 4] = [("ACHAT", "buy"), ("ATTENDRE", "hold"), ("ACHAT FORT", "buy"), ("VENTE", "sell")];

fn fallback() -> Vec<Tick> {
    [("BTC", "Bitcoin"), ("ETH", "Ethereum"), ("SOL", "Solana"), ("BNB", "BNB")]
        .iter()
        .map(|(s, n)| Tick { symbol: (*s).into(), name: (*n).into(), kind: Kind::Crypto, price: 0.0, change: Some(0.0), agreeing: 0, total: 0 })
        .collect()
}

/// Phone mockup reproducing the Radar screen of the web app.
#[component]
pub fn PhoneMockup(p: &TicksProps) -> Html {
    let _m = crate::money::use_money();
    let rows: Vec<Tick> = if p.ticks.is_empty() { fallback() } else { p.ticks.iter().filter(|t| t.kind == Kind::Crypto).cloned().collect() };
    html! {
        <div class="phone-wrap" aria-label="Aperçu de l'application Altim">
            <div class="phone">
                <div class="notch" />
                <div class="screen">
                    <div class="screen-head">
                        <span class="screen-title">{ "Radar" }</span>
                        <span class="env">{ "CONSEIL" }</span>
                    </div>
                    <div class="seg">
                        <span>{ "15 min" }</span>
                        <span>{ "1 h" }</span>
                        <span class="on">{ "4 h" }</span>
                        <span>{ "1 j" }</span>
                    </div>
                    <div class="gauge-card">
                        <svg viewBox="0 0 200 110" class="gauge">
                            <defs>
                                <linearGradient id="g" x1="0" x2="1">{ crate::ui::gauge_stops() }</linearGradient>
                            </defs>
                            <path d="M20 100 A80 80 0 0 1 180 100" stroke="rgba(255,255,255,.08)" stroke-width="14" fill="none" stroke-linecap="round" />
                            <path d="M20 100 A80 80 0 0 1 180 100" stroke="url(#g)" stroke-width="14" fill="none" stroke-linecap="round" />
                            <line x1="100" y1="100" x2="100" y2="34" stroke="#fff" stroke-width="4" stroke-linecap="round" class="needle" />
                        </svg>
                        <div class="gauge-label">
                            <b>{ "+41" }</b>
                            <small>{ "confiance 46 %" }</small>
                        </div>
                    </div>
                    { for rows.iter().take(4).enumerate().map(|(i, t)| {
                        let (label, kind) = BADGES[i % BADGES.len()];
                        let has = t.price != 0.0 && !t.price.is_nan();
                        html! {
                            <div class={format!("row row-{kind}")} key={t.symbol.clone()} style={format!("animation-delay: {}ms;", i * 120)}>
                                <div>
                                    <b>{ &t.name }</b>
                                    <small>{ &t.symbol }</small>
                                    <span class={classes!("badge", kind)}>{ label }</span>
                                </div>
                                <div class="num">
                                    <b>{ if has { crate::money::price_sep(t.price, " ") } else { "—".into() } }</b>
                                    <small class={if t.change.unwrap_or(0.0) >= 0.0 { "up" } else { "down" }}>
                                        { match t.change { Some(c) if has => format_percent(c), _ => String::new() } }
                                    </small>
                                </div>
                            </div>
                        }
                    }) }
                </div>
            </div>
            <div class="phone-glow" />
        </div>
    }
}
