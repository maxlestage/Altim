//! Pure logic of the bot, validation, alerts, news and agenda screens (bot (the rest of model-bot.ts, in `web::bot::screen`), model-validation.ts, alerts-store.ts, notify.ts (pure part), news-summary.ts, calendar.ts).
//! One file per TypeScript module, declared here; tests next to the code (`cargo test -p altim-core`).
pub mod alerts;
pub mod calendar;
pub mod news;
pub mod news_summary;
pub mod validation;
