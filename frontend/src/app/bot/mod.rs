//! « Bot Altim » (Bot.tsx): candidate models trained on long histories of the validation's basket and an extra
//! universe (/api/bot, the server's `BotReport`), chosen at each retraining on an inner validation and tested
//! walk-forward on periods they had not seen, saying ACHETER / ATTENDRE / VENDRE; today's view of the watched assets
//! (/api/bot/views). v3 (`v3::BotV3Section`) first when present, then v2's selection as the reference. Also the
//! Radar's cards: `BotRadarCard` and `BriefCard` (« Point du jour »). Texts in
//! `altim_core::web::bot::screen`.
mod brief;
mod radar_card;
mod report;
mod v3;

use altim_core::engine::bot::BotReport;
use altim_core::web::bot::screen::BOT_URL;
use yew::prelude::*;

pub use brief::BriefCard;
pub use radar_card::{BotRadarCard, BotRadarView, RadarState};
pub use report::{ActionChip, BotReportView};

use crate::app::validation::{Load, use_heavy_report};

#[component]
pub fn Bot() -> Html {
    let state = use_heavy_report::<BotReport>(BOT_URL);
    let pending = matches!(state, Load::Pending);
    html! {
        <section class="app-screen validation bot">
            <div class="screen-top">
                <div>
                    <h1>{ "Bot Altim" }</h1>
                    <p class="muted small">
                        { "Des modèles appris sur de longs historiques (actions depuis 1990, cryptos depuis leur cotation), qui disent ACHETER, ATTENDRE ou VENDRE à 20 et 60 jours. Jugés seulement sur des périodes qu'ils n'avaient pas vues, sur 34 actifs fixés d'avance, avec un seuil corrigé des essais multiples. Altim ne passe aucun ordre." }
                    </p>
                </div>
            </div>
            { match state {
                Load::Error(e) => html! { <p class="notice warn">{ format!("⚠ {e}") }</p> },
                Load::Loading | Load::Pending => html! {
                    <div class="card">
                        <p class="muted">
                            { if pending { "Téléchargement des historiques, entraînement et test en cours (plusieurs minutes la première fois)…" } else { "Chargement…" } }
                        </p>
                        <div class="skeleton" />
                    </div>
                },
                Load::Ready(r) => html! { <BotReportView report={r} /> },
            } }
        </section>
    }
}
