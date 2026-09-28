/** Production entry point (Heroku): Express on Bun. */
import { createApp, warmSelections } from "./server/app";
import { LiveHub } from "./server/live";

const port = Number(process.env.PORT ?? 3000);
const live = new LiveHub();
const server = createApp({ live }).listen(port, () => console.log(`Altim web en ligne sur http://localhost:${port}`));
if (process.env.NODE_ENV === "production") warmSelections();

// Heroku restarts every dyno at least once a day with SIGTERM, then kills it 30 s later: new requests are refused,
// live streams are closed (the apps and the browser reconnect at once to the new dyno) and the exchanges' sockets
// are shut before leaving.
let stopping = false;
function shutdown(signal: string) {
  if (stopping) return;
  stopping = true;
  console.log(`${signal} : arrêt propre`);
  server.close(() => process.exit(0));
  (server as unknown as { closeAllConnections?: () => void }).closeAllConnections?.();
  live.close();
  // Streams are cut above; Bun does not always call the close callback: 3 s of grace, well inside Heroku's 30 s.
  setTimeout(() => process.exit(0), 3_000).unref?.();
}
process.on("SIGTERM", () => shutdown("SIGTERM"));
process.on("SIGINT", () => shutdown("SIGINT"));

// A failing upstream call that nobody awaited must not bring the whole server down: logged, the server keeps going.
process.on("unhandledRejection", (reason) => console.error("Promesse rejetée non gérée :", reason instanceof Error ? reason.message : reason));
