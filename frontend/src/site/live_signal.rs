//! "Signal technique en direct" (LiveSignal.tsx): the app's technical signal (altim-core `signal::analyze`, the
//! same engine as the server) on the last closed candles of the site's coins.
use altim_core::engine::signal::{Action, AnalyzeOptions, Candle, Signal, analyze, ema};
use altim_core::js::{round, to_fixed};
use altim_core::web::market::{COINS, Coin, SourceStatus};
use yew::prelude::*;

use crate::hooks::use_reveal;
use crate::ui::{chart_path, technical_text};

const INTERVALS: [(&str, &str); 3] = [("1h", "1 h"), ("4h", "4 h"), ("1d", "1 j")];

fn kind(s: &Signal) -> &'static str {
    match s.action {
        Action::Buy | Action::StrongBuy => "buy",
        Action::Sell | Action::StrongSell => "sell",
        Action::Hold => "hold",
    }
}

#[derive(Clone, PartialEq)]
struct Loaded {
    candles: Vec<Candle>,
    signal: Option<Signal>,
    sources: Vec<SourceStatus>,
    agreeing: u32,
}

#[component]
pub fn LiveSignal() -> Html {
    let r = use_reveal();
    let _m = crate::money::use_money();
    let coin = use_state(|| 0usize);
    let interval = use_state(|| "4h");
    let state = use_state(|| None::<Loaded>);
    let error = use_state(|| None::<&'static str>);
    {
        let (state, error) = (state.clone(), error.clone());
        use_effect_with((*coin, *interval), move |(coin, interval)| {
            let alive = std::rc::Rc::new(std::cell::Cell::new(true));
            error.set(None);
            let (c, iv, a) = (COINS[*coin], *interval, alive.clone());
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::site_candles(&c, iv).await {
                    Ok(d) if a.get() => {
                        let signal = analyze(&d.candles, &AnalyzeOptions::default());
                        state.set(Some(Loaded { candles: d.candles, signal, sources: d.sources, agreeing: d.agreeing }));
                    }
                    Err(_) if a.get() => error.set(Some("Marchés injoignables depuis votre réseau. Réessayez dans un instant.")),
                    _ => {}
                }
            });
            move || alive.set(false)
        });
    }
    let signal = state.as_ref().and_then(|s| s.signal.clone());
    let idle = error.is_some();
    html! {
        <section class="section live reveal" id="live" ref={r}>
            <div class="section-head">
                <p class="eyebrow">{ "Signal technique en direct" }</p>
                <h2>{ "Le moteur Altim tourne " }<span class="gradient">{ "dans votre navigateur" }</span></h2>
                <p class="muted">
                    { "Le signal technique de l'application (vérifié par des tests automatiques), appliqué aux dernières bougies clôturées. Dans l'application, ce n'est qu'un indice parmi d'autres : la décision y ajoute les interdictions d'achat, la zone d'achat, le gain/risque, l'agenda et la preuve du modèle." }
                </p>
            </div>

            <div class="live-controls">
                <div class="pills" role="tablist" aria-label="Actif">
                    { for COINS.iter().take(4).enumerate().map(|(i, c): (usize, &Coin)| {
                        let set = { let coin = coin.clone(); Callback::from(move |_| coin.set(i)) };
                        html! { <button key={c.symbol} class={if i == *coin { "pill on" } else { "pill" }} onclick={set}>{ c.base() }</button> }
                    }) }
                </div>
                <div class="pills" role="tablist" aria-label="Unité de temps">
                    { for INTERVALS.iter().map(|(v, l)| {
                        let set = { let interval = interval.clone(); let v = *v; Callback::from(move |_| interval.set(v)) };
                        html! { <button key={*v} class={if *v == *interval { "pill on" } else { "pill" }} onclick={set}>{ *l }</button> }
                    }) }
                </div>
            </div>

            if let Some(e) = *error {
                <p class="warn">{ e }</p>
            }

            <div class="live-grid">
                <div class="card chart-card">
                    if let Some(s) = &*state {
                        <MiniChart candles={s.candles.clone()} signal={s.signal.clone()} />
                        <div class="sources">
                            <span class={if s.agreeing >= 2 { "rel high" } else { "rel low" }}>
                                { format!("{} {} source{} concordante{}", if s.agreeing >= 2 { "✔" } else { "!" }, s.agreeing, if s.agreeing > 1 { "s" } else { "" }, if s.agreeing > 1 { "s" } else { "" }) }
                            </span>
                            { for s.sources.iter().map(|src| html! {
                                <span key={src.name.clone()} class={if src.ok { "src ok" } else { "src ko" }} title={src.error.clone().unwrap_or_default()}>
                                    { format!("{} {}{}", if src.ok { "●" } else { "○" }, src.name, match src.deviation { Some(d) if src.ok && d.is_finite() => format!(" {} %", to_fixed(d, 3)), _ => String::new() }) }
                                </span>
                            }) }
                        </div>
                    } else {
                        <div class={if idle { "skeleton idle" } else { "skeleton" }} />
                    }
                </div>

                <div class={classes!("card", "signal-card", signal.as_ref().map(kind))}>
                    if let Some(s) = &signal {
                        <div class="signal-top">
                            <crate::ui::Gauge score={s.score} gradient="lg" app={false} />
                            <div class="signal-meta">
                                <span class={classes!("badge", "big", kind(s))}>{ format!("Technique : {}", technical_text(s.action)) }</span>
                                <span class="mono">{ crate::money::price_sep(s.price, " ") }</span>
                                <small class="muted">{ format!("confiance {} %", round(s.confidence)) }</small>
                            </div>
                        </div>
                        <ul class="factors">
                            { for s.factors.iter().map(|f| {
                                let wd = f.score.abs() * 50.0;
                                let left = if f.score >= 0.0 { "50%".to_string() } else { format!("{}%", 50.0 - wd) };
                                html! {
                                    <li key={f.name.clone()}>
                                        <div class="factor-head">
                                            <span>{ &f.name }</span>
                                            <small class="muted">{ &f.detail }</small>
                                        </div>
                                        <div class="bar">
                                            <i class={if f.score >= 0.0 { "pos" } else { "neg" }} style={format!("width: {wd}%; left: {left};")} />
                                        </div>
                                    </li>
                                }
                            }) }
                        </ul>
                        if s.action != Action::Hold && s.has_plan {
                            <div class="plan">
                                <div><small>{ "STOP" }</small><b class="sell">{ crate::money::price_sep(s.stop_loss, " ") }</b></div>
                                <div><small>{ "ENTRÉE" }</small><b>{ crate::money::price_sep(s.price, " ") }</b></div>
                                <div><small>{ "OBJECTIF" }</small><b class="buy">{ crate::money::price_sep(s.take_profit, " ") }</b></div>
                            </div>
                        }
                    } else {
                        <div class={if idle { "skeleton tall idle" } else { "skeleton tall" }} />
                    }
                </div>
            </div>
            <p class="disclaimer">
                { "Démonstration pédagogique, pas un conseil en investissement. Dans l'app, ce signal est complété par la confirmation de l'unité de temps supérieure et par un backtest sur l'actif." }
            </p>
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct MiniChartProps {
    candles: Vec<Candle>,
    signal: Option<Signal>,
}

#[component]
fn MiniChart(p: &MiniChartProps) -> Html {
    let closes: Vec<f64> = p.candles.iter().map(|c| c.close).collect();
    let (e20, e50) = (ema(&closes, 20), ema(&closes, 50));
    let start = closes.len().saturating_sub(140);
    let data: Vec<Option<f64>> = closes[start..].iter().map(|c| Some(*c)).collect();
    let (w, h) = (600.0, 300.0);
    let plan = p.signal.as_ref().filter(|s| s.action != Action::Hold);
    let mut values: Vec<f64> = closes[start..].to_vec();
    if let Some(s) = plan {
        values.extend([s.stop_loss, s.take_profit]);
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min) * 0.998;
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max) * 1.002;
    let n = data.len();
    let x = move |i: usize| i as f64 / (n.max(2) - 1) as f64 * w;
    let y = move |v: f64| h - (v - min) / (if max - min == 0.0 { 1.0 } else { max - min }) * h;
    let line = chart_path(&data, x, y);
    html! {
        <svg viewBox={format!("0 0 {w} {h}")} class="mini-chart" preserveAspectRatio="none" role="img" aria-label="Graphique des prix">
            <defs>
                <linearGradient id="area" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0" stop-color="rgba(0,240,255,.35)" />
                    <stop offset="1" stop-color="rgba(0,240,255,0)" />
                </linearGradient>
            </defs>
            { for [0.25, 0.5, 0.75].iter().map(|f| html! { <line x1="0" x2={w.to_string()} y1={(h * f).to_string()} y2={(h * f).to_string()} stroke="rgba(255,255,255,.06)" /> }) }
            <path d={format!("{line}L{w},{h}L0,{h}Z")} fill="url(#area)" />
            <path d={chart_path(&e50[start..], x, y)} stroke="#7d4dff" stroke-width="1.5" fill="none" vector-effect="non-scaling-stroke" />
            <path d={chart_path(&e20[start..], x, y)} stroke="#ff2bd6" stroke-width="1.5" fill="none" vector-effect="non-scaling-stroke" />
            <path d={line} stroke="#00f0ff" stroke-width="2.5" fill="none" class="glow-line" vector-effect="non-scaling-stroke" />
            if let Some(s) = plan {
                <line x1="0" x2={w.to_string()} y1={y(s.stop_loss).to_string()} y2={y(s.stop_loss).to_string()} stroke="#ff3b5c" stroke-dasharray="6 6" vector-effect="non-scaling-stroke" />
                <line x1="0" x2={w.to_string()} y1={y(s.take_profit).to_string()} y2={y(s.take_profit).to_string()} stroke="#39ff88" stroke-dasharray="6 6" vector-effect="non-scaling-stroke" />
            }
        </svg>
    }
}
