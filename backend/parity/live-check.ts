/**
 * Live comparison with the Rust port (network): run at the same time as
 *   ALTIM_LIVE_OUT=/tmp/rust.json cargo test --test live_network live_snapshot_and_quotes -- --ignored
 *   bun parity/live-check.ts /tmp/ts.json
 * then compare the two files (same shape).
 */
import { writeFileSync } from "node:fs";
import { snapshot } from "../../web/server/market";
import { ASSETS, consensusQuotes } from "../../web/server/quotes";

const err = (e: Error) => ({ error: e.message });
const [btc1h, aapl1d, aapl1h, quotes] = await Promise.all([
  snapshot("BTC", "crypto", "1h").catch(err),
  snapshot("AAPL", "stock", "1d").catch(err),
  snapshot("AAPL", "stock", "1h").catch(err),
  consensusQuotes(ASSETS),
]);
writeFileSync(process.argv[2] ?? "ts-live.json", JSON.stringify({ btc1h, aapl1d, aapl1h, quotes }));
