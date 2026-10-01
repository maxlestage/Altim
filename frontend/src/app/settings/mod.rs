//! Réglages (Settings.tsx): radar list, display currency, investment
//! horizon, prudence of the advice, weights of the composite score, links to the other screens, logout.
use std::collections::HashSet;

use altim_core::engine::fibonacci::HORIZONS;
use altim_core::js::{number_to_string, to_fixed};
use altim_core::web::money::Currency;
use altim_core::web::portfolio::wire::UniverseItem;
use altim_core::web::store::{
    DEFAULT_RISK, DEFAULT_SCORE_WEIGHTS, HorizonPref, RiskSettings, SCORE_FACTORS, ScoreWeights, WatchItem, asset_key, default_watchlist,
};
use yew::prelude::*;

use crate::app::common::FxNote;
use crate::app::holdings::AssetPicker;
use crate::route::use_on_link;
use crate::state::app::{set_app_state, use_app_state};
use crate::ui::Segmented;

/// (label, min, max, step, unit, getter, setter) of each prudence stepper (`RISK_FIELDS`).
struct RiskField {
    label: &'static str,
    min: f64,
    max: f64,
    step: f64,
    unit: &'static str,
    get: fn(&RiskSettings) -> f64,
    set: fn(&mut RiskSettings, f64),
}

const RISK_FIELDS: [RiskField; 5] = [
    RiskField {
        label: "Risque accepté par idée",
        min: 0.25,
        max: 5.0,
        step: 0.25,
        unit: " %",
        get: |r| r.risk_per_trade_percent,
        set: |r, v| r.risk_per_trade_percent = v,
    },
    RiskField {
        label: "Taille max d'une ligne",
        min: 5.0,
        max: 100.0,
        step: 5.0,
        unit: " %",
        get: |r| r.max_position_percent,
        set: |r, v| r.max_position_percent = v,
    },
    RiskField { label: "Gain/risque min", min: 1.0, max: 5.0, step: 0.25, unit: "", get: |r| r.min_risk_reward, set: |r, v| r.min_risk_reward = v },
    RiskField {
        label: "Perte max du jour",
        min: 0.5,
        max: 10.0,
        step: 0.5,
        unit: " %",
        get: |r| r.daily_loss_limit_percent,
        set: |r, v| r.daily_loss_limit_percent = v,
    },
    RiskField {
        label: "Part crypto max",
        min: 0.0,
        max: 100.0,
        step: 5.0,
        unit: " %",
        get: |r| r.max_crypto_percent,
        set: |r, v| r.max_crypto_percent = v,
    },
];

const NNBSP: &str = "\u{202f}";

fn set_weight(w: &mut ScoreWeights, key: &str, v: u32) {
    match key {
        "tech" => w.tech = v,
        "mom" => w.mom = v,
        "fund" => w.fund = v,
        "sent" => w.sent = v,
        "news" => w.news = v,
        _ => w.macro_ = v,
    }
}

fn horizon_index(h: HorizonPref) -> usize {
    match h {
        HorizonPref::Short => 0,
        HorizonPref::Medium => 1,
        HorizonPref::Long => 2,
    }
}

#[component]
pub fn Settings() -> Html {
    let s = use_app_state();
    let on_link = use_on_link();
    // Opens the asset search of « Mes avoirs » (AssetPicker).
    let picking = use_state(|| false);
    let weight_total: u32 = SCORE_FACTORS.iter().map(|(k, ..)| s.score_weights.get(k)).sum();
    let cfg = &HORIZONS[horizon_index(s.horizon)];

    let watch = s.watchlist.iter().map(|w| {
        let key = w.key();
        let remove = {
            let key = key.clone();
            Callback::from(move |_| set_app_state(|s| s.watchlist.retain(|x| x.key() != key)))
        };
        html! {
            <li key={key}>
                <span><b>{ w.symbol.clone() }</b>{ " " }<span class="muted">{ w.name.clone() }</span></span>
                <button aria-label={format!("Retirer {}", w.name)} onclick={remove}>{ "Retirer" }</button>
            </li>
        }
    });

    let steppers = RISK_FIELDS.iter().enumerate().map(|(i, f)| {
        let v = (f.get)(&s.risk);
        let down = Callback::from(move |_| {
            let f = &RISK_FIELDS[i];
            set_app_state(|s| {
                let x = to_fixed((f.get)(&s.risk) - f.step, 2).parse::<f64>().unwrap_or(f.min);
                (f.set)(&mut s.risk, f.min.max(x));
            })
        });
        let up = Callback::from(move |_| {
            let f = &RISK_FIELDS[i];
            set_app_state(|s| {
                let x = to_fixed((f.get)(&s.risk) + f.step, 2).parse::<f64>().unwrap_or(f.max);
                (f.set)(&mut s.risk, f.max.min(x));
            })
        });
        html! {
            <div class="stepper" key={f.label}>
                <span>{ f.label }</span>
                <div>
                    <button aria-label={format!("Diminuer {}", f.label)} onclick={down}>{ "−" }</button>
                    <b class="mono">{ number_to_string(v) }{ f.unit }</b>
                    <button aria-label={format!("Augmenter {}", f.label)} onclick={up}>{ "+" }</button>
                </div>
            </div>
        }
    });

    let weights = SCORE_FACTORS.iter().map(|(key, label, hint, _)| {
        let value = s.score_weights.get(key);
        let key = *key;
        let oninput = Callback::from(move |e: InputEvent| {
            let raw = e.target_unchecked_into::<web_sys::HtmlInputElement>().value();
            // Math.min(100, Math.max(0, Math.round(Number(value)) || 0))
            let n = altim_core::js::round(altim_core::web::trading::js_number(&raw));
            let v = if n.is_finite() && n != 0.0 { n.clamp(0.0, 100.0) } else { 0.0 };
            set_app_state(|s| set_weight(&mut s.score_weights, key, v as u32));
        });
        html! {
            <label key={key} class="score-weight">
                <span><b>{ *label }</b>{ " " }<small class="muted">{ *hint }</small></span>
                <b class="mono">{ format!("{value}{NNBSP}%") }</b>
                <input type="range" min="0" max="100" step="1" value={value.to_string()} aria-label={format!("Poids {label}")} {oninput} />
            </label>
        }
    });

    let currency_options = vec![(AttrValue::from("EUR"), AttrValue::from("Euro (€)")), (AttrValue::from("USD"), AttrValue::from("Dollar ($)"))];
    let on_currency = Callback::from(|v: AttrValue| set_app_state(|s| s.currency = if v == "USD" { Currency::Usd } else { Currency::Eur }));
    let horizon_options: Vec<(AttrValue, AttrValue)> =
        [("short", 0), ("medium", 1), ("long", 2)].iter().map(|(k, i)| (AttrValue::from(*k), AttrValue::from(HORIZONS[*i].label))).collect();
    let on_horizon = Callback::from(|v: AttrValue| {
        set_app_state(|s| {
            s.horizon = match v.as_str() {
                "short" => HorizonPref::Short,
                "long" => HorizonPref::Long,
                _ => HorizonPref::Medium,
            }
        })
    });
    let horizon_value = match s.horizon {
        HorizonPref::Short => "short",
        HorizonPref::Medium => "medium",
        HorizonPref::Long => "long",
    };

    html! {
        <section class="app-screen">
            <div class="screen-top"><h1>{ "Réglages" }</h1></div>

            <div class="card">
                <h2 class="card-title">{ "Radar" }</h2>
                <p class="muted small">{ "Suivez autant d'actifs que vous voulez, parmi toutes les cryptos et toutes les actions et ETF cotés aux États-Unis." }</p>
                <button class="btn btn-ghost" onclick={{ let picking = picking.clone(); Callback::from(move |_| picking.set(true)) }}>{ "+ Ajouter : rechercher une crypto ou une action" }</button>
                <ul class="watch-edit">{ for watch }</ul>
                <button class="link-btn" onclick={Callback::from(|_| set_app_state(|s| s.watchlist = default_watchlist()))}>{ "Liste par défaut" }</button>
            </div>
            if *picking {
                <AssetPicker
                    title="Actifs du radar"
                    selected={s.watchlist.iter().map(|w| asset_key(&w.symbol, w.kind)).collect::<HashSet<String>>()}
                    on_toggle={Callback::from(|item: UniverseItem| set_app_state(move |s| {
                        let k = item.key();
                        if s.watchlist.iter().any(|w| asset_key(&w.symbol, w.kind) == k) {
                            s.watchlist.retain(|w| asset_key(&w.symbol, w.kind) != k);
                        } else {
                            s.watchlist.push(WatchItem { symbol: item.symbol, kind: item.kind, name: item.name });
                        }
                    }))}
                    on_close={{ let picking = picking.clone(); Callback::from(move |_| picking.set(false)) }}
                />
            }

            <div class="card">
                <h2 class="card-title">{ "Devise d'affichage" }</h2>
                <Segmented label="Devise d'affichage" value={AttrValue::from(s.currency.as_str())} options={currency_options} on_change={on_currency} />
                <FxNote />
                <p class="muted small">
                    { "Les cours viennent en dollars des sources (bourses américaines, plateformes crypto en USD ou USDT) ; Altim les convertit au taux EUR/USD du moment (Yahoo Finance, sinon taux de référence de la BCE). Les montants que vous saisissez (prix de revient, budget, liquidités) le sont dans cette devise." }
                </p>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Mon horizon d'investissement" }</h2>
                <p class="muted small">
                    { "Les zones d'achat ne sont pas les mêmes selon la durée pendant laquelle vous comptez garder l'actif. Votre horizon est mis en avant sur chaque fiche ; les autres restent consultables." }
                </p>
                <Segmented label="Horizon d'investissement" value={AttrValue::from(horizon_value)} options={horizon_options} on_change={on_horizon} />
                <p class="muted small">{ format!("{} : bougies {}, détention de {}.", cfg.label, cfg.unit, cfg.holding) }</p>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Prudence des conseils" }</h2>
                <p class="muted small">{ "Ces réglages déterminent les montants conseillés : la part de votre patrimoine qu'un conseil d'achat accepte de risquer si le stop est touché, et la taille maximale d'une ligne." }</p>
                { for steppers }
                <p class="muted small">{ "Règle professionnelle : ne jamais risquer plus de 1 à 2 % de son patrimoine sur une seule idée." }</p>
                <p class="muted small">
                    { "Perte max du jour : si votre patrimoine a déjà perdu ce pourcentage depuis la clôture de la veille, Mes avoirs vous conseille de ne plus ouvrir de position aujourd'hui. Part crypto max : au-delà, Mes avoirs signale une surexposition aux cryptos, qui peuvent perdre 50 % ou plus ensemble (60 % par défaut ; 10 à 30 % est plus courant pour un patrimoine prudent)." }
                </p>
                <button class="link-btn" onclick={Callback::from(|_| set_app_state(|s| s.risk = DEFAULT_RISK))}>{ "Valeurs recommandées" }</button>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Score composite" }</h2>
                <p class="muted small">
                    { "Poids de chaque famille dans le score de −100 à +100 de la carte Décision. Seuls les facteurs mesurés comptent : leurs poids sont ramenés à 100 %. Le verdict, lui, ne change pas." }
                </p>
                <div class="score-weights">{ for weights }</div>
                <p class="muted small">
                    { "Total : " }<span class="mono">{ weight_total }</span>{ " (ramené à 100 %)." }{ if weight_total == 0 { " Tous à 0 : les poids par défaut sont utilisés." } else { "" } }
                </p>
                <button class="link-btn" onclick={Callback::from(|_| set_app_state(|s| s.score_weights = DEFAULT_SCORE_WEIGHTS))}>{ "Poids par défaut (32 / 18 / 20 / 10 / 10 / 10)" }</button>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Simulation (sans argent réel)" }</h2>
                <p class="muted small">{ "Un portefeuille virtuel pour tester les décisions d'Altim : achats simulés depuis la carte Décision, ventes au stop ou à l'objectif, résultats par décision. Aucun argent réel, aucun ordre passé, tout reste dans ce navigateur." }</p>
                <a href="/app/simulation" onclick={on_link.clone()} class="btn btn-ghost">{ "Ouvrir la simulation" }</a>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Validation du modèle" }</h2>
                <p class="muted small">{ "Le signal testé sur 34 actions, cryptos et ETF choisis à l'avance, par classe d'actifs et par régime de marché, avec ses biais et limites." }</p>
                <a href="/app/validation" onclick={on_link.clone()} class="btn btn-ghost">{ "Voir la validation" }</a>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Bot Altim" }</h2>
                <p class="muted small">{ "Un modèle appris qui dit ACHETER, ATTENDRE ou VENDRE, jugé seulement sur des périodes qu'il n'avait pas vues, avec ses résultats réels et ses limites." }</p>
                <a href="/app/bot" onclick={on_link.clone()} class="btn btn-ghost">{ "Voir le bot" }</a>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Comprendre" }</h2>
                <p class="muted small">{ "Signal, zone d'achat, stop, volatilité, flat tax… les mots d'Altim expliqués simplement." }</p>
                <a href="/app/lexique" onclick={on_link} class="btn btn-ghost">{ "Ouvrir le lexique" }</a>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Accès privé" }</h2>
                <p class="muted small">{ "Votre session reste ouverte 7 jours sur cet appareil. Déconnectez-vous sur un appareil partagé." }</p>
                <form method="post" action="/logout">
                    <button type="submit" class="btn btn-ghost">{ "Se déconnecter" }</button>
                </form>
            </div>

            <div class="card">
                <h2 class="card-title">{ "À propos" }</h2>
                <p class="kv small"><span>{ "Sources crypto" }</span><b>{ "8 recoupées" }</b></p>
                <p class="kv small"><span>{ "Sources actions" }</span><b>{ "Yahoo, Nasdaq, Cboe" }</b></p>
                <p class="muted small">
                    { "Altim est un outil d'aide à la décision, pas un conseil en investissement. " }<a href="/risques" class="link">{ "Avertissement sur les risques" }</a>{ " · " }
                    <a href="/confidentialite" class="link">{ "Confidentialité" }</a>
                </p>
                <p class="muted small">{ "Conception & développement : Maxime Nathan Lestage" }</p>
            </div>
        </section>
    }
}
