#!/usr/bin/env node
// No horizontal scrolling, anywhere: visits every page of the site and every screen of the web app at phone widths
// (iPhone user agent, touch, mobile viewport), opens the accordions, clicks the tabs and segmented controls (the
// timeframe chooser ends on « 1 sem. ») and fails when, at any step:
//   (a) the page is wider than the screen (document scrollWidth), measured WITHOUT the safety net of global.css
//       (html, body { overflow-x: clip }), so that the real causes show;
//   (b) an element sticks out of the screen (left < 0 or right > width), except a fixed off-canvas element and what an
//       ancestor (other than html/body) clips;
//   (c) an element scrolls horizontally by itself (overflow-x auto/scroll with content wider than its box): an inner
//       horizontal scroller is horizontal scrolling too.
//
// Usage: node scripts/no-hscroll.mjs [--url http://127.0.0.1:3000] [--widths 320,375] [--shots DIR]
//          [--record FILE.json.gz | --replay FILE.json.gz] [--only /app/bot,/]
// - The server: ALTIM_DEV_OPEN=1 (no login) serving web/dist (scripts/build-web.sh).
// - --record saves the answers of the data requests (fetch/XHR, the server's /api and any other host) seen during the
//   run; --replay answers them from that file and nothing else (an unknown request gets a 503, like an unreachable
//   source): the CI check needs no network and always sees the same data.
// - Browser: playwright-core (PLAYWRIGHT_CORE = its index.mjs, else resolved normally) driving Chrome/Chromium at
//   CHROME (default: /usr/bin/google-chrome when present, else Playwright's own).
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { gunzipSync, gzipSync } from "node:zlib";

const args = process.argv.slice(2);
const opt = (name, def) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : def;
};
const BASE = opt("url", "http://127.0.0.1:3000").replace(/\/$/, "");
const WIDTHS = opt("widths", "320,360,375,390,430").split(",").map(Number);
const SHOTS = opt("shots", "");
const RECORD = opt("record", "");
const REPLAY = opt("replay", "");
const ONLY = opt("only", "");
const STEP_MS = Number(opt("max-wait", REPLAY ? "4000" : "15000"));

const pw = await import(process.env.PLAYWRIGHT_CORE || "playwright-core");
const chromium = pw.chromium ?? pw.default.chromium;
const chrome = process.env.CHROME || (existsSync("/usr/bin/google-chrome") ? "/usr/bin/google-chrome" : undefined);

// Every page: the site (and its "not found" page), the login page, every app route (route.rs), an unknown app address,
// the asset screen of one stock and one crypto.
const ROUTES = [
  "/", "/mentions-legales", "/confidentialite", "/risques", "/page-inconnue", "/login",
  "/app", "/app/alertes", "/app/actif/stock/AAPL", "/app/actif/crypto/BTC", "/app/avoirs", "/app/simulation",
  "/app/journal", "/app/selection", "/app/opportunites", "/app/actu", "/app/reglages", "/app/lexique", "/app/bot",
  "/app/validation", "/app/inconnu",
  "/app#avertissement", // the disclaimer shown before the app (nothing accepted yet on this browser)
].filter((r) => !ONLY || ONLY.split(",").includes(r));

// The app's saved state: disclaimer accepted, a watchlist with long and short names, holdings with large amounts
// (long figures are what widens a row).
const now = Date.now();
const DAY = 86_400_000;
const STORAGE = {
  "altim.webapp.v1": {
    version: 1, acceptedDisclaimer: true, interval: "4h", horizon: "medium", risk: {},
    watchlist: [
      { symbol: "AAPL", kind: "stock", name: "Apple" }, { symbol: "BTC", kind: "crypto", name: "Bitcoin" },
      { symbol: "ETH", kind: "crypto", name: "Ethereum" }, { symbol: "NVDA", kind: "stock", name: "NVIDIA" },
      { symbol: "BRK.B", kind: "stock", name: "Berkshire Hathaway Inc. Class B" },
    ],
  },
  "altim.holdings.v1": {
    version: 1, cash: 1234567.89, cashCurrency: "EUR", updatedAt: now,
    holdings: [
      { id: "h1", symbol: "AAPL", kind: "stock", name: "Apple", quantity: 1250.5, averagePrice: 180.25 },
      { id: "h2", symbol: "BTC", kind: "crypto", name: "Bitcoin", quantity: 12.3456789, averagePrice: 45000 },
      { id: "h3", symbol: "ETH", kind: "crypto", name: "Ethereum", quantity: 345.678, averagePrice: 2500 },
      { id: "h4", symbol: "BRK.B", kind: "stock", name: "Berkshire Hathaway Inc. Class B", quantity: 77, averagePrice: 410.5 },
    ],
  },
  // A simulated portfolio with an open position and a closed trade (positions, results by decision).
  "altim.paper.v1": {
    version: 1, startCapital: 1000000, cash: 876543.21, startedAt: now - 20 * DAY,
    positions: [
      {
        id: "p1", symbol: "BTC", kind: "crypto", name: "Bitcoin", openedAt: now - 9 * DAY, entry: 98765.43, quantity: 1.2345678,
        invested: 121931.39, stop: 91234.56, target: 112345.67,
        decision: { verdict: "buy_zone", label: "ZONE D'ACHAT", confidence: 64, asOf: now - 9 * DAY },
      },
    ],
    trades: [
      {
        id: "t1", symbol: "BRK.B", kind: "stock", name: "Berkshire Hathaway Inc. Class B", openedAt: now - 18 * DAY, entry: 412.34,
        quantity: 240, invested: 98961.6, stop: 395, target: 450,
        decision: { verdict: "buy", label: "ACHETER", confidence: 71, asOf: now - 18 * DAY },
        closedAt: now - 4 * DAY, exit: 450, reason: "target", proceeds: 107892, pnl: 8930.4, pnlPct: 9.02,
      },
    ],
  },
};

const IPHONE_UA =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";

// Recorded answers: "METHOD url" → { status, type, body (base64) }.
const fixtures = REPLAY ? JSON.parse(gunzipSync(readFileSync(REPLAY)).toString()) : {};
const recorded = {};
const keyOf = (req) => `${req.method()} ${req.url().replace(BASE, "")}`;
// Data: what the page fetches from the server's /api or from another host (not the front's own .wasm and scripts).
const isData = (req) =>
  ["fetch", "xhr", "eventsource"].includes(req.resourceType()) && (!req.url().startsWith(BASE) || req.url().startsWith(`${BASE}/api/`));

/// The measure, in the page: (a), (b) and (c) of the header, against the width of the screen `W`.
function measure(W) {
  const out = [];
  const path = (el) => {
    const parts = [];
    for (let e = el; e && e !== document.body && parts.length < 5; e = e.parentElement) {
      const cls = [...e.classList].slice(0, 2).map((c) => `.${c}`).join("");
      parts.unshift(e.tagName.toLowerCase() + cls);
    }
    return parts.join(" > ");
  };
  const text = (el) => (el.textContent || "").replace(/\s+/g, " ").trim().slice(0, 60);
  const docW = Math.max(document.documentElement.scrollWidth, document.body.scrollWidth);
  if (docW > W) out.push({ kind: "page", what: `scrollWidth ${docW} > ${W}` });
  const hidden = (el) => {
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      const cs = getComputedStyle(e);
      if (cs.display === "none" || cs.visibility === "hidden") return true;
      if (cs.position === "fixed") {
        const r = e.getBoundingClientRect();
        if (r.left >= W || r.right <= 0) return true; // off-canvas on purpose
      }
    }
    return false;
  };
  // Clipped by an ancestor below body that is itself on screen: no scroll can reach it.
  const clipped = (el) => {
    for (let e = el.parentElement; e && e !== document.body; e = e.parentElement) {
      const ox = getComputedStyle(e).overflowX;
      if (ox === "hidden" || ox === "clip") {
        const r = e.getBoundingClientRect();
        return r.left >= -0.5 && r.right <= W + 0.5;
      }
    }
    return false;
  };
  const sticking = new Set();
  for (const el of document.body.querySelectorAll("*")) {
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    const cs = getComputedStyle(el);
    if ((cs.overflowX === "auto" || cs.overflowX === "scroll") && el.scrollWidth > el.clientWidth + 1 && !hidden(el)) {
      out.push({ kind: "scroller", what: path(el), detail: `scrollWidth ${el.scrollWidth} > ${el.clientWidth}`, text: text(el) });
    }
    if ((r.right > W + 0.5 || r.left < -0.5) && !el.classList.contains("sr-only") && !hidden(el) && !clipped(el)) sticking.add(el);
  }
  // The highest box wider than its parent's content box: where the overflow starts.
  const origin = (el) => {
    let found = null;
    for (let e = el; e && e.parentElement && e.parentElement !== document.documentElement; e = e.parentElement) {
      const p = e.parentElement;
      const pr = p.getBoundingClientRect();
      const ps = getComputedStyle(p);
      const inner = pr.right - parseFloat(ps.paddingRight) - parseFloat(ps.borderRightWidth);
      if (e.getBoundingClientRect().right > inner + 0.5 && ps.overflowX === "visible") found = e;
    }
    return found;
  };
  // Only the outermost element sticking out, with the deepest one inside it and where the overflow starts.
  for (const el of sticking) {
    if (el.parentElement && sticking.has(el.parentElement)) continue;
    let deep = el;
    for (const d of el.querySelectorAll("*")) if (sticking.has(d)) deep = d;
    const r = el.getBoundingClientRect();
    const o = origin(deep);
    out.push({
      kind: "element", what: path(el), detail: `left ${r.left.toFixed(1)} right ${r.right.toFixed(1)}`,
      cause: deep === el ? "" : path(deep), origin: o ? `${path(o)} (${getComputedStyle(o).display}, min-width ${getComputedStyle(o).minWidth})` : "",
      text: text(deep),
    });
  }
  return out;
}

/// Waits for the page's data requests to settle (no request for 700 ms), at most `max` ms.
async function settle(page, inflight, max = STEP_MS) {
  const end = Date.now() + max;
  let quietSince = Date.now();
  await page.waitForTimeout(150);
  while (Date.now() < end) {
    if (inflight.size > 0) quietSince = Date.now();
    else if (Date.now() - quietSince > 700) break;
    await page.waitForTimeout(100);
  }
}

const browser = await chromium.launch({ executablePath: chrome, args: ["--no-proxy-server"] });
const findings = [];
let steps = 0;
// One browser context per width, all at once (each its own storage): the run takes the time of one width.
async function runWidth(W) {
  const ctx = await browser.newContext({
    viewport: { width: W, height: 780 }, screen: { width: W, height: 780 }, deviceScaleFactor: 2,
    isMobile: true, hasTouch: true, userAgent: IPHONE_UA, locale: "fr-FR", timezoneId: "Europe/Paris",
  });
  await ctx.addInitScript((s) => {
    if (location.hash === "#avertissement") return;
    for (const [k, v] of Object.entries(s)) if (localStorage.getItem(k) === null) localStorage.setItem(k, JSON.stringify(v));
  }, STORAGE);
  // The safety net off: the measure sees the real overflow.
  await ctx.addInitScript(() => {
    const st = document.createElement("style");
    st.textContent = "html, body { overflow-x: visible !important; }";
    const add = () => document.head ? document.head.append(st) : requestAnimationFrame(add);
    add();
  });
  await ctx.route("**/*", async (route) => {
    const req = route.request();
    if (req.url().includes("/api/live")) return route.abort(); // the live stream: never ends, not layout
    if (!REPLAY || !isData(req)) return route.continue();
    const f = fixtures[keyOf(req)];
    if (!f) return route.fulfill({ status: 503, contentType: "application/json", body: '{"error":"hors ligne (audit)"}' });
    return route.fulfill({ status: f.status, contentType: f.type, body: Buffer.from(f.body, "base64") });
  });
  const page = await ctx.newPage();
  const inflight = new Set();
  page.on("request", (r) => isData(r) && !r.url().includes("/api/live") && inflight.add(r));
  page.on("requestfinished", (r) => inflight.delete(r));
  page.on("requestfailed", (r) => inflight.delete(r));
  if (RECORD) {
    page.on("response", async (res) => {
      const req = res.request();
      if (!isData(req) || req.url().includes("/api/live")) return;
      try {
        const body = await res.body();
        const k = keyOf(req);
        // A finished answer replaces a pending one (202), never the reverse.
        if (recorded[k] && recorded[k].status === 200 && res.status() !== 200) return;
        recorded[k] = { status: res.status(), type: res.headers()["content-type"] || "application/json", body: body.toString("base64") };
      } catch {}
    });
  }
  page.on("pageerror", (e) => console.log(`  [${W}] erreur JS : ${e.message}`));

  for (const route of ROUTES) {
    const t0 = Date.now();
    const check = async (step) => {
      steps++;
      for (const f of await page.evaluate(measure, W)) findings.push({ width: W, route, step, ...f });
    };
    const openDetails = () =>
      page.$$eval("details:not([open])", (ds) => ds.forEach((d) => (d.open = true))).catch(() => {});
    if (route.endsWith("#avertissement")) await page.evaluate(() => localStorage.clear()).catch(() => {});
    await page.goto(BASE + route, { waitUntil: "load" });
    await settle(page, inflight);
    await check("arrivée");
    await openDetails();
    await settle(page, inflight);
    await check("détails ouverts");
    // Tabs, segmented controls, choosers: each visible one, once (new ones revealed by a click are visited too).
    const done = new Set();
    for (let n = 0; n < 60; n++) {
      const next = await page.evaluate((done) => {
        const sel = ".segmented button, .portfolio-tabs button, .horizon-grid button, [role=tab], [role=radio], .val-chips button, .chip.pick";
        const els = [...document.querySelectorAll(sel)];
        for (let i = 0; i < els.length; i++) {
          const e = els[i];
          const r = e.getBoundingClientRect();
          if (r.width === 0 || e.disabled) continue;
          const key = `${e.parentElement?.className}|${(e.textContent || "").trim()}`;
          if (!done.includes(key)) return { i, key, sel };
        }
        return null;
      }, [...done]);
      if (!next) break;
      done.add(next.key);
      const el = (await page.$$(next.sel))[next.i];
      await el?.click({ timeout: 3000 }).catch(() => {});
      if (!page.url().startsWith(BASE + route)) await page.goto(BASE + route, { waitUntil: "load" });
      await settle(page, inflight);
      await openDetails();
      await check(`clic « ${next.key.split("|")[1].slice(0, 30)} »`);
    }
    // The timeframe chooser on its longest unit.
    const week = page.getByRole("button", { name: "1 sem.", exact: true }).first();
    if (await week.count()) {
      await week.click({ timeout: 3000 }).catch(() => {});
      await settle(page, inflight);
      await openDetails();
      await settle(page, inflight, 3000);
      await check("1 sem.");
    }
    if (SHOTS) {
      mkdirSync(SHOTS, { recursive: true });
      await page.screenshot({ path: `${SHOTS}/${W}${route.replaceAll("/", "_") || "_"}.png`, fullPage: true }).catch(() => {});
    }
    const mine = findings.filter((f) => f.width === W && f.route === route).length;
    console.log(`${mine ? "✗" : "✓"} ${W} px ${route}${mine ? ` : ${mine} débordement(s)` : ""} (${((Date.now() - t0) / 1000).toFixed(1)} s)`);
  }
  await ctx.close();
}
await Promise.all(WIDTHS.map(runWidth));
await browser.close();

if (RECORD) {
  writeFileSync(RECORD, gzipSync(JSON.stringify(recorded, null, 0)));
  console.log(`${Object.keys(recorded).length} réponses enregistrées dans ${RECORD}`);
}
// One line per distinct finding (the same element at several steps once, with its widths).
const seen = new Map();
for (const f of findings) {
  const k = `${f.route}|${f.kind}|${f.what}|${f.cause ?? ""}`;
  const s = seen.get(k) ?? { ...f, widths: new Set(), steps: new Set() };
  s.widths.add(f.width);
  s.steps.add(f.step);
  seen.set(k, s);
}
for (const f of seen.values()) {
  console.log(
    `\n${f.route} — ${[...f.widths].join(", ")} px — ${f.kind} : ${f.what}\n    ${f.detail ?? ""}${f.cause ? `\n    cause : ${f.cause}` : ""}${f.origin ? `\n    départ : ${f.origin}` : ""}${f.text ? `\n    texte : « ${f.text} »` : ""}\n    étapes : ${[...f.steps].slice(0, 4).join(" ; ")}`,
  );
}
console.log(`\n${steps} mesures, ${ROUTES.length} pages × ${WIDTHS.length} largeurs : ${seen.size} débordement(s) distinct(s)`);
process.exit(seen.size ? 1 : 0);
