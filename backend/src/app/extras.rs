//! Company / network figures, liquidity and the signal's own track record for `/api/decision`.
//!
//! Wiring point of the modules written separately: `crate::fundamentals::stock_fundamentals(symbol, price)`,
//! `crate::tokenomics::crypto_fundamentals(symbol)`, `crate::liquidity::liquidity(symbol, kind, &daily)` and
//! `crate::engine::metrics::track(&daily, kind)`. Until they are merged, nothing is returned: the decision then shows
//! these families as "Données indisponibles" and the matching vetoes as not verifiable (never estimated).
use crate::engine::decision_types::{Fundamentals, Liquidity, Track};
use crate::types::{Candle, Kind};

pub async fn extras(symbol: &str, kind: Kind, price: Option<f64>, daily: &[Candle]) -> (Option<Fundamentals>, Option<Liquidity>, Option<Track>) {
    // To wire at merge:
    //   let fundamentals = match kind {
    //       Kind::Stock => crate::fundamentals::stock_fundamentals(symbol, price).await.map(Fundamentals::Stock),
    //       Kind::Crypto => crate::tokenomics::crypto_fundamentals(symbol).await.map(Fundamentals::Crypto),
    //   };
    //   let liquidity = crate::liquidity::liquidity(symbol, kind, daily).await;
    //   let track = crate::engine::metrics::track(daily, kind);
    let _ = (symbol, kind, price, daily);
    (None, None, None)
}
