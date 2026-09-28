//! `/api/strategies`: the strategy comparator of one asset, on its long daily history (`long()`, ≈ 3 years of
//! multi-source daily candles) and, for a stock, its daily P/E (SEC EDGAR). Cached one hour per asset.
use std::sync::Arc;

use crate::cache::cached;
use crate::engine::strategies::{PerInput, StrategiesReport, compare};
use crate::http::{Error, Result};
use crate::js::now_ms;
use crate::types::Kind;

pub async fn strategies_for(symbol: &str, kind: Kind) -> Result<Arc<StrategiesReport>> {
    let s = symbol.to_string();
    cached(&format!("strategies:{}:{symbol}", kind.as_str()), 3_600_000, move || async move {
        let hist = super::data::long(&s, kind).await?;
        let per = match kind {
            Kind::Crypto => None,
            Kind::Stock => Some(crate::fundamentals::per_history(&s, &hist.candles).await),
        };
        let input = match &per {
            None => PerInput::NotApplicable,
            Some(Ok((series, source))) => PerInput::Series(series, source.clone()),
            Some(Err(e)) => PerInput::Missing(format!("PER indisponible ({e})")),
        };
        compare(&s, kind, &hist.candles, &hist.source, input, now_ms())
            .ok_or_else(|| Error(format!("historique journalier trop court ({} bougies, il en faut 260)", hist.candles.len())))
    })
    .await
}
