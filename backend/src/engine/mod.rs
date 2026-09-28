//! Pure engines, ported line for line from `web/src/engine/*.ts` (the web app keeps using the TypeScript ones).
pub mod alerts;
pub mod backtest;
pub mod brief;
pub mod decision;
pub mod decision_types;
pub mod fibonacci;
pub mod format;
pub mod guard;
pub mod history;
pub mod macro_ctx;
pub mod metrics;
pub mod news;
pub mod news_summary;
pub mod reliability;
pub mod screener;
pub mod signal;
pub mod strategies;
pub mod structure;
pub mod synthesis;

use serde::{Deserialize, Serialize};

/// How often a factor was followed by the move it warns about on this asset's own history (`Evidence` in guard.ts,
/// shared with fibonacci.ts and macro.ts).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub samples: f64,
    pub rate: f64,
    pub base: f64,
    pub lift: f64,
}
