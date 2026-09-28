//! Company / network figures, liquidity and the signal's own track record for `/api/decision`. What no free and
//! verifiable source gives stays None: the decision then shows the family as unavailable and the matching veto as
//! not verifiable (never estimated).
use crate::engine::decision_types::{Fundamentals, Liquidity, Track};
use crate::types::{Candle, Kind};

pub async fn extras(symbol: &str, kind: Kind, price: Option<f64>, daily: &[Candle]) -> (Option<Fundamentals>, Option<Liquidity>, Option<Track>) {
    let fundamentals = async {
        match kind {
            Kind::Stock => crate::fundamentals::stock_fundamentals(symbol, price).await.ok().map(Fundamentals::Stock),
            Kind::Crypto => crate::tokenomics::crypto_fundamentals(symbol).await.ok().map(Fundamentals::Crypto),
        }
    };
    let (fundamentals, liquidity) = tokio::join!(fundamentals, crate::liquidity::liquidity(symbol, kind, daily));
    (fundamentals, Some(liquidity), crate::engine::metrics::track(daily, kind))
}
