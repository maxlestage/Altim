//! Phase 2, batch C: pure logic of Mes avoirs and its tools, ported from web/src (engine/holdings.ts,
//! portfolio-risk.ts, tools.ts, dca.ts, risk.ts, advice.ts, sectors.ts), plus the server replies they read (`wire`)
//! and the helpers of the screens (`view`). Tests next to the code (`cargo test -p altim-core`).
pub mod advice;
pub mod dca;
pub mod holdings;
pub mod portfolio_risk;
pub mod risk;
pub mod sectors;
pub mod tools;
pub mod view;
pub mod wire;
