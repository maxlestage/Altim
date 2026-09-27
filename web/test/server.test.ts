import { afterAll, beforeAll, expect, test } from "bun:test";
import type { Server } from "node:http";
import { createApp } from "../server/app";

let server: Server;
let base = "";

beforeAll(async () => {
  server = createApp().listen(0);
  await new Promise((r) => server.once("listening", r));
  const addr = server.address();
  base = `http://127.0.0.1:${typeof addr === "object" && addr ? addr.port : 0}`;
});
afterAll(() => server.close());

test("santé", async () => {
  const r = await fetch(`${base}/health`);
  expect(r.status).toBe(200);
  expect(await r.text()).toBe("ok");
});

test("en-têtes de sécurité (helmet) et pas de x-powered-by", async () => {
  const r = await fetch(`${base}/health`);
  expect(r.headers.get("content-security-policy")).toContain("default-src 'self'");
  expect(r.headers.get("content-security-policy")).toContain("frame-ancestors 'none'");
  expect(r.headers.get("strict-transport-security")).toContain("max-age=63072000");
  expect(r.headers.get("x-content-type-options")).toBe("nosniff");
  expect(r.headers.get("x-powered-by")).toBeNull();
});

test("redirection HTTPS derrière le routeur Heroku", async () => {
  const r = await fetch(`${base}/app`, { headers: { "x-forwarded-proto": "http", host: "altim.example" }, redirect: "manual" });
  expect(r.status).toBe(301);
  expect(r.headers.get("location")).toBe("https://altim.example/app");
});

test("validation des paramètres de l'API", async () => {
  for (const path of [
    "/api/candles?symbol=../etc&interval=1h",
    "/api/candles?symbol=BTC&interval=5m",
    "/api/candles?symbol=BTC&interval=1h&kind=forex",
    "/api/radar?symbols=AAPL:bond",
    "/api/alerts?symbols=BTC:crypto,../x:stock",
    "/api/sentiment?symbol=<script>",
    "/api/zones?symbol=../x&kind=crypto",
    "/api/zones?symbol=AAPL&kind=bond",
    "/api/selection?horizon=forever",
    "/api/selection?horizon=medium&kind=forex",
  ]) {
    const r = await fetch(base + path);
    expect(r.status).toBe(400);
    expect((await r.json()).error).toBeTruthy();
  }
  expect((await fetch(`${base}/api/inconnue`)).status).toBe(404);
  expect(await (await fetch(`${base}/api/search?q=`)).json()).toEqual([]);
});

test("routes de l'application (SPA) servies par index.html", async () => {
  for (const path of ["/", "/app", "/app/actif/BTC", "/mentions-legales"]) {
    const r = await fetch(base + path);
    // 200 after `bun run build`, 500 with an explicit message otherwise (tests run before the build in CI).
    expect([200, 500]).toContain(r.status);
    if (r.status === 200) expect(await r.text()).toContain('<div id="root">');
  }
});

test("fichier absent : vrai 404, jamais la page HTML", async () => {
  const r = await fetch(`${base}/app/index-abcdef12.js`);
  expect(r.status).toBe(404);
});

test("limitation de débit de l'API", async () => {
  let limited = false;
  for (let i = 0; i < 260 && !limited; i++) {
    const r = await fetch(`${base}/api/search?q=`);
    if (r.status === 429) limited = true;
  }
  expect(limited).toBe(true);
});
