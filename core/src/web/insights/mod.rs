//! Phase 2, batch A: pure logic ported from web/src (bot (the rest of model-bot.ts, in `web::bot::screen`), model-validation.ts, alerts-store.ts, notify.ts (pure part), news-summary.ts, calendar.ts).
//! One file per TypeScript module, declared here; tests next to the code (`cargo test -p altim-core`).
pub mod alerts;
pub mod calendar;
pub mod money;
pub mod news;
pub mod news_summary;
pub mod validation;
