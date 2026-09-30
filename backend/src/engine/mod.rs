//! Pure engines, ported line for line from `web/src/engine/*.ts` (the web app keeps using the TypeScript ones).
pub mod alerts;
pub mod anomalies;
pub mod ask;
pub mod backtest;
pub mod bot;
pub mod bot_trees;
pub mod bot_v3;
pub mod brief;
pub mod decision;
pub mod decision_types;
pub mod fibonacci;
pub mod format;
pub mod guard;
pub mod guidance;
pub mod history;
pub mod macro_ctx;
pub mod metrics;
pub mod model_evidence;
pub mod news;
pub mod news_summary;
pub mod opportunities;
pub mod reliability;
pub mod screener;
pub mod signal;
pub mod strategies;
pub mod structure;
pub mod synthesis;
pub mod validation;
pub mod why;

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
