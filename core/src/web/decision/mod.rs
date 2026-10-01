//! Pure logic of the Radar and the asset screen (from decision.ts,
//! config-changes.ts, strategies.ts, the helpers of Radar.tsx, NoteCard.tsx and DecisionCard.tsx). One file per
//! TypeScript module, tests next to the code (`cargo test -p altim-core`). Nothing here touches the browser: the
//! frontend passes the stored text, the clock and the money display.
//!
//! - `doc`: the decision of `/api/decision` (the server's own `Decision`, plus the answer as received: what the cache
//!   keeps and what an older server did not send), its checks, the personal-mode inputs and the URL.
//! - `format`: French formats and labels of the decision card (prices, %, dates in Paris / New York time).
//! - `cache`: the offline cache "altim.decision.v1" (`cached_decision` is what other screens read).
//! - `config_changes`: "altim.configChanges.v1", the configuration diff and why the signal changed.
//! - `rows`: label / value rows of the card (fundamentals, structure, track record, liquidity).
//! - `strategies`: the strategy comparator's chart geometry and formats (/api/strategies).
//! - `reports`: JSON contracts of the other routes the two screens read (radar, quotes, candles, guard, zones…).
//! - `radar`: the Radar's sort (remembered in "altim.radar.sort") and its lists.
//! - `notes`: personal notes "altim.notes.v1".
pub mod cache;
pub mod config_changes;
pub mod doc;
pub mod format;
pub mod notes;
pub mod radar;
pub mod reports;
pub mod rows;
pub mod strategies;

pub use cache::{CACHE_KEY, CachedDecision, cached_decision};
pub use doc::{DecisionDoc, parse_decision};
