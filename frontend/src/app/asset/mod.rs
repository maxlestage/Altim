//! Asset screen /app/actif/:kind/:symbol (phase 2, batch B: port of web/src/webapp/AssetScreen.tsx, DecisionCard.tsx,
//! decision.ts, config-changes.ts, TrackDetails.tsx, GuardCard.tsx, ZonesCard.tsx, WhyCard.tsx, StrategiesCard.tsx,
//! strategies.ts, AnomaliesCard.tsx, NoteCard.tsx). Sub-components go in files of this folder, declared below.
use altim_core::types::Kind;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct AssetScreenProps {
    pub kind: Kind,
    /// Upper case, validated by `app::asset_of`.
    pub symbol: AttrValue,
}

#[component]
pub fn AssetScreen(p: &AssetScreenProps) -> Html {
    html! { <crate::ui::NotPorted title={p.symbol.clone()} /> }
}
