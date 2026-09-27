/**
 * Altim web server (Express on Bun): showcase site, /app web application and multi-source APIs.
 */
import express, { type NextFunction, type Request, type Response } from "express";
import compression from "compression";
import helmet from "helmet";
import { join } from "node:path";
import { existsSync } from "node:fs";
import { analyze } from "../src/engine/signal";
import { gate, type Kind } from "../src/engine/reliability";
import { HIGHER, STEP, longDaily, snapshot, type Interval } from "./market";
import { ASSETS, consensusQuotes, makeAsset, type Asset } from "./quotes";
import { cached } from "./cache";
import { guardReport } from "./guard";
import { LiveHub } from "./live";
import { authConfig, createAuth, type AuthConfig } from "./auth";
import { macro, macroSeries } from "./macro";
import { screen } from "./screener";
import type { ScreenHorizon } from "../src/engine/screener";
import { fibZones } from "../src/engine/fibonacci";
import { macroAdvice, macroEvidence } from "../src/engine/macro";
import { cryptoUniverse, searchAll, searchUniverse, stockUniverse, universe, type UniverseEntry } from "./universe";

const ROOT = join(import.meta.dir, "..");
const DIST = join(ROOT, "dist");
const PUBLIC = join(ROOT, "public");
const INTERVALS: Interval[] = ["1h", "4h", "1d"];

// ---------- Validation ----------

class BadRequest extends Error {}

function parseKind(v: unknown): Kind {
  if (v === undefined || v === "crypto") return "crypto";
  if (v === "stock") return "stock";
  throw new BadRequest("kind invalide (crypto | stock)");
}

function parseSymbol(v: unknown, kind: Kind): string {
  const s = String(v ?? "").toUpperCase().trim();
  const ok = kind === "crypto" ? /^[A-Z0-9]{1,12}$/.test(s) : /^[A-Z][A-Z0-9.\-]{0,9}$/.test(s);
  if (!ok) throw new BadRequest("symbole invalide");
  return s;
}

function parseInterval(v: unknown): Interval {
  if (INTERVALS.includes(v as Interval)) return v as Interval;
  throw new BadRequest("interval invalide (1h | 4h | 1d)");
}

/** "BTC:crypto,AAPL:stock" → list of assets (20 max). */
function parseAssets(v: unknown): Asset[] {
  if (v === undefined || v === "") return ASSETS;
  const items = String(v).split(",").slice(0, 20);
  return items.map((item) => {
    const [sym, kindRaw] = item.split(":");
    const kind = parseKind(kindRaw ?? "crypto");
    return makeAsset(parseSymbol(sym, kind), kind);
  });
}

// ---------- Minimal rate limiting (per IP) ----------

function rateLimit(max: number, windowMs: number) {
  const hits = new Map<string, { count: number; reset: number }>();
  return (req: Request, res: Response, next: NextFunction) => {
    const now = Date.now();
    const key = req.ip ?? "?";
    const h = hits.get(key);
    if (!h || h.reset < now) hits.set(key, { count: 1, reset: now + windowMs });
    else if (++h.count > max) {
      res.setHeader("Retry-After", Math.ceil((h.reset - now) / 1000));
      return res.status(429).json({ error: "Trop de requêtes, réessayez dans un instant." });
    }
    if (hits.size > 10_000) for (const [k, v] of hits) if (v.reset < now) hits.delete(k);
    next();
  };
}

const wrap = (fn: (req: Request, res: Response) => Promise<unknown>) => (req: Request, res: Response, next: NextFunction) =>
  fn(req, res).catch(next);

// ---------- Data ----------

/** Only closed candles are analysed: a snapshot changes at most once per candle. */
const SNAP_TTL: Record<Interval, number> = { "1h": 60_000, "4h": 120_000, "1d": 300_000 };
const snap = (symbol: string, kind: Kind, interval: Interval) =>
  cached(`snap:${kind}:${symbol}:${interval}`, SNAP_TTL[interval], () => snapshot(symbol, kind, interval));

/** Long daily history (≈ 3 years), for the long-term horizon and the macro evidence. */
const long = (symbol: string, kind: Kind) => cached(`long:${kind}:${symbol}`, 3_600_000, () => longDaily(symbol, kind));
const macroNow = () => cached("macro:report", 300_000, () => macro());

/** Macro context + what its stress announced on this asset (its long daily history). */
async function macroContext(symbol: string, kind: Kind) {
  const [report, series, hist] = await Promise.all([macroNow(), macroSeries(), long(symbol, kind).catch(() => null)]);
  return { report, evidence: hist ? cached(`macroEv:${kind}:${symbol}`, 3_600_000, async () => macroEvidence(hist.candles, series)) : null };
}

/** Full market guard of an asset (cached 60 s). */
async function guardFor(symbol: string, kind: Kind) {
  const list = await universe(kind).catch(() => [] as UniverseEntry[]);
  const name = list.find((e) => e[0] === symbol)?.[1] ?? makeAsset(symbol, kind).name;
  return cached(`guard:${kind}:${symbol}`, 60_000, () =>
    guardReport(symbol, kind, name, async (i) => (await snap(symbol, kind, i)).candles, Date.now(), async () => {
      const ctx = await macroContext(symbol, kind);
      return { report: ctx.report, evidence: await ctx.evidence };
    }));
}

/** Selection of a market and horizon (screener.ts), finalists checked with the consensus and the guard. Cached 30 min. */
const selection = (h: ScreenHorizon, market: Kind = "stock") =>
  cached(`selection:${market}:${h}`, 30 * 60_000, () =>
    screen(h, async (symbol) => {
      const [d, g] = await Promise.all([snap(symbol, market, "1d"), guardFor(symbol, market).catch(() => null)]);
      return {
        reliability: d.reliability.level,
        shock: g?.shock.level ?? "calm",
        reversalDown: !!g && g.reversal.direction === "down" && g.reversal.score >= 50,
        daily: d.candles,
      };
    }, 150, 10, market));

/** Buy zones by horizon (Fibonacci) with the macro context. */
async function zones(symbol: string, kind: Kind) {
  const [h4, d, hist, q, ctx] = await Promise.all([
    snap(symbol, kind, "4h"),
    snap(symbol, kind, "1d"),
    long(symbol, kind).catch(() => null),
    quotes([makeAsset(symbol, kind)]).catch(() => []),
    macroContext(symbol, kind).catch(() => null),
  ]);
  const price = q[0]?.price ?? h4.candles[h4.candles.length - 1]?.close ?? null;
  const computed = await cached(`zones:${kind}:${symbol}`, 300_000, async () => fibZones(h4.candles, d.candles, hist?.candles ?? [], price));
  // Status and distance follow the current price (the levels only change with the candles).
  const current = price ? fibZones(h4.candles, d.candles, hist?.candles ?? [], price, false).map((z, i) => ({ ...z, evidence: computed[i]!.evidence })) : computed;
  const evidence = ctx ? await ctx.evidence : null;
  const level = ctx?.report.level ?? "calm";
  return {
    symbol, kind, price, asOf: Date.now(),
    zones: current.map((z) => ({ ...z, macroNote: macroAdvice(level, z.horizon) })),
    macro: ctx ? { ...ctx.report, evidence } : null,
  };
}

const quotes = (assets: Asset[]) =>
  cached(`quotes:${assets.map((a) => `${a.kind}:${a.symbol}`).sort().join(",")}`, 15_000, () => consensusQuotes(assets));

async function radarItem(asset: Asset, interval: Interval) {
  try {
    const hi = HIGHER[interval];
    const [s, higher] = await Promise.all([
      snap(asset.symbol, asset.kind, interval),
      hi ? snap(asset.symbol, asset.kind, hi).catch(() => null) : Promise.resolve(null),
    ]);
    const raw = analyze(s.candles, { higher: higher?.candles, intervalMs: STEP[interval] });
    const signal = raw ? gate(raw, s.reliability, s.quality.issues) : null;
    return {
      symbol: asset.symbol,
      kind: asset.kind,
      signal: signal && { action: signal.action, score: signal.score, confidence: signal.confidence },
      reliability: s.reliability,
      agreeing: s.agreeing,
      sources: s.sources.length,
      sparkline: s.candles.slice(-48).map((c) => c.close),
      lastClose: s.candles[s.candles.length - 1]?.close ?? null,
    };
  } catch (e) {
    return { symbol: asset.symbol, kind: asset.kind, error: e instanceof Error ? e.message : "indisponible" };
  }
}

/** Search across the full universe (every crypto, every US-listed stock / ETF): exact symbols first, then the largest. */
async function searchAssets(q: string, limit = 20) {
  const [crypto, stock] = await Promise.all([cryptoUniverse().catch(() => []), stockUniverse().catch(() => [])]);
  return searchAll(crypto, stock, q, limit).map(({ e, kind }) => toItem(e, kind));
}

const toItem = (e: UniverseEntry, kind: Kind) => ({
  symbol: e[0], name: e[1], kind, rank: e[2] || null,
  ...(kind === "crypto" ? { exchanges: e[3] } : { etf: e[3] === 1 }),
});

async function sentiment(symbol: string, kind: Kind) {
  const out: { fearGreed?: { value: number; label: string }; social?: { bullishPercent: number | null; sample: number } } = {};
  const tasks: Promise<void>[] = [];
  if (kind === "crypto") {
    tasks.push(
      cached("fng", 600_000, async () => {
        const r = await fetch("https://api.alternative.me/fng/?limit=1", { signal: AbortSignal.timeout(8000) });
        const d = (await r.json()) as { data: { value: string }[] };
        const v = Number(d.data[0]!.value);
        const label = v < 25 ? "Peur extrême" : v < 45 ? "Peur" : v <= 55 ? "Neutre" : v <= 75 ? "Avidité" : "Avidité extrême";
        return { value: v, label };
      }).then((v) => void (out.fearGreed = v)).catch(() => {}),
    );
  }
  tasks.push(
    cached(`st:${kind}:${symbol}`, 300_000, async () => {
      const id = kind === "crypto" ? `${symbol}.X` : symbol;
      const r = await fetch(`https://api.stocktwits.com/api/2/streams/symbol/${encodeURIComponent(id)}.json`, {
        headers: { "User-Agent": "Mozilla/5.0 Altim/1.0" },
        signal: AbortSignal.timeout(8000),
      });
      const d = (await r.json()) as { messages?: { entities?: { sentiment?: { basic?: string } | null } }[] };
      const tags = (d.messages ?? []).map((m) => m.entities?.sentiment?.basic).filter(Boolean);
      const bull = tags.filter((t) => t === "Bullish").length;
      return { bullishPercent: tags.length ? (bull / tags.length) * 100 : null, sample: tags.length };
    }).then((v) => void (out.social = v)).catch(() => {}),
  );
  await Promise.all(tasks);
  return out;
}

// ---------- Application ----------

/** Production: the three selections are computed at start-up and every 25 minutes, so nobody waits. */
export function warmSelections() {
  const jobs = (["stock", "crypto"] as Kind[]).flatMap((m) => (["medium", "long", "short"] as ScreenHorizon[]).map((h) => () => selection(h, m)));
  const run = () => jobs.reduce((p, job) => p.then(() => job().then(() => {}, () => {})), Promise.resolve());
  setTimeout(run, 5_000);
  setInterval(run, 25 * 60_000).unref?.();
}

export function createApp({ live, auth = { cfg: authConfig(), production: process.env.NODE_ENV === "production" } }: { live?: LiveHub; auth?: { cfg: AuthConfig | null; production: boolean } } = {}) {
  const app = express();
  const access = createAuth(auth.cfg, auth.production);
  let hub = live ?? null;
  const liveHub = () => (hub ??= new LiveHub());
  app.disable("x-powered-by");
  app.set("trust proxy", 1); // Heroku router

  // HTTPS behind the Heroku router.
  app.use((req, res, next) => {
    if (req.headers["x-forwarded-proto"] === "http") return res.redirect(301, `https://${req.headers.host}${req.originalUrl}`);
    next();
  });

  app.use(
    helmet({
      contentSecurityPolicy: {
        useDefaults: false,
        directives: {
          defaultSrc: ["'self'"],
          scriptSrc: ["'self'"],
          styleSrc: ["'self'", "'unsafe-inline'", "https://fonts.googleapis.com"],
          fontSrc: ["https://fonts.gstatic.com"],
          imgSrc: ["'self'", "data:"],
          connectSrc: ["'self'", "https://api.binance.com", "https://api.coingecko.com"],
          frameAncestors: ["'none'"],
          baseUri: ["'self'"],
          formAction: ["'self'"],
          objectSrc: ["'none'"],
        },
      },
      strictTransportSecurity: { maxAge: 63_072_000, includeSubDomains: true },
      referrerPolicy: { policy: "strict-origin-when-cross-origin" },
      crossOriginEmbedderPolicy: false,
    }),
  );
  app.use((_req, res, next) => {
    res.setHeader("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
    next();
  });
  // Event streams are never compressed: compression buffers and would freeze the live prices.
  app.use(compression({ filter: (req, res) => !String(res.getHeader("Content-Type") ?? "").includes("text/event-stream") && compression.filter(req, res) }));

  app.get("/health", (_req, res) => res.type("text").send("ok"));

  // ----- Private access (auth.ts): everything below requires a session, or the bot token on /api -----
  app.get("/robots.txt", (_req, res) => res.type("text").send("User-agent: *\nDisallow: /\n"));
  app.get("/login", access.loginForm);
  app.post("/login", express.urlencoded({ extended: false, limit: "2kb" }), (req, res, next) => access.login(req, res).catch(next));
  app.post("/logout", access.logout);
  app.use(access.guard);

  // ----- API -----
  const api = express.Router();
  api.use(rateLimit(240, 60_000));
  api.use((_req, res, next) => {
    res.setHeader("Cache-Control", "private, max-age=15");
    next();
  });

  api.get("/tickers", wrap(async (req, res) => {
    const data = await quotes(parseAssets(req.query.symbols));
    if (!data.length) throw new Error("aucune source disponible");
    res.json(data);
  }));

  api.get("/candles", wrap(async (req, res) => {
    const kind = parseKind(req.query.kind);
    const symbol = parseSymbol(req.query.symbol ?? req.query.base, kind);
    const interval = parseInterval(req.query.interval);
    res.json(await snap(symbol, kind, interval));
  }));

  api.get("/radar", wrap(async (req, res) => {
    const assets = parseAssets(req.query.symbols);
    const interval = parseInterval(req.query.interval ?? "4h");
    const [items, q] = await Promise.all([
      Promise.all(assets.map((a) => radarItem(a, interval))),
      quotes(assets).catch(() => []),
    ]);
    res.json(
      items.map((it) => {
        const quote = q.find((x) => x.symbol === it.symbol && x.kind === it.kind);
        return { ...it, name: makeAsset(it.symbol, it.kind).name, price: quote?.price ?? ("lastClose" in it ? it.lastClose : null), change: quote?.change ?? null, priceSources: quote ? `${quote.agreeing}/${quote.total}` : null };
      }),
    );
  }));

  api.get("/search", wrap(async (req, res) => {
    const q = String(req.query.q ?? "").slice(0, 30);
    const limit = Math.min(50, Math.max(1, Math.floor(Number(req.query.limit) || 20)));
    res.json(q.trim().length < 1 ? [] : await cached(`search:${limit}:${q.toUpperCase()}`, 600_000, () => searchAssets(q, limit)));
  }));

  // Full catalogue, paginated: /api/universe?kind=crypto|stock&q=&offset=0&limit=50
  api.get("/universe", wrap(async (req, res) => {
    const kind = parseKind(req.query.kind);
    const q = String(req.query.q ?? "").slice(0, 30);
    const offset = Math.max(0, Math.floor(Number(req.query.offset) || 0));
    const limit = Math.min(200, Math.max(1, Math.floor(Number(req.query.limit) || 50)));
    const list = await universe(kind);
    const matches = q.trim() ? searchUniverse(list, q, list.length) : list;
    res.setHeader("Cache-Control", "private, max-age=3600");
    res.json({ total: matches.length, offset, items: matches.slice(offset, offset + limit).map((e) => toItem(e, kind)) });
  }));

  // Market guard (regime, shock risk, reversal risk, policy for bots). Readable from any origin: trading bots poll it.
  api.get("/guard", wrap(async (req, res) => {
    const kind = parseKind(req.query.kind);
    const symbol = parseSymbol(req.query.symbol, kind);
    const report = await guardFor(symbol, kind);
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Cache-Control", "private, max-age=30");
    res.json(report);
  }));

  // Live prices (Server-Sent Events): last known price right away, then every change, several times per second.
  api.get("/live", (req, res, next) => {
    let assets: Asset[];
    try {
      assets = parseAssets(req.query.symbols);
    } catch (e) {
      return next(e);
    }
    res.status(200).set({
      "Content-Type": "text/event-stream; charset=utf-8",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
      "X-Accel-Buffering": "no",
      "Access-Control-Allow-Origin": "*",
    });
    res.flushHeaders();
    const send = (t: unknown) => res.write(`data: ${JSON.stringify(t)}\n\n`);
    res.write("retry: 3000\n\n");
    const h = liveHub();
    const keys = assets.map((a) => `${a.kind}:${a.symbol}`);
    const unsubscribe = h.subscribe(keys, send);
    h.snapshot(keys).forEach(send);
    // Comment every 15 s: keeps the connection open through the Heroku router (55 s idle limit) and proxies.
    const beat = setInterval(() => res.write(": ok\n\n"), 15_000);
    req.on("close", () => {
      clearInterval(beat);
      unsubscribe();
    });
  });

  // Buy zones by horizon (Fibonacci, short / medium / long term) with the macro context.
  api.get("/zones", wrap(async (req, res) => {
    const kind = parseKind(req.query.kind);
    const z = await zones(parseSymbol(req.query.symbol, kind), kind);
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.json(z);
  }));

  // Which stocks or cryptos to buy: ranked (relative strength for stocks, technical signal for cryptos), checked finalists.
  api.get("/selection", wrap(async (req, res) => {
    const h = String(req.query.horizon ?? "medium");
    if (!["short", "medium", "long"].includes(h)) throw new BadRequest("horizon invalide (short | medium | long)");
    const market = parseKind(req.query.kind ?? "stock");
    res.setHeader("Access-Control-Allow-Origin", "*");
    // The first computation scans 150 stocks (≈ 30 s): the Heroku router cuts at 30 s, so after 20 s the client is
    // told to come back; the computation keeps going and lands in the cache.
    const result = await Promise.race([selection(h as ScreenHorizon, market), new Promise<null>((r) => setTimeout(() => r(null), 20_000))]);
    if (!result) return res.status(202).json({ pending: true });
    res.setHeader("Cache-Control", "private, max-age=300");
    res.json(result);
  }));

  // Macro / geopolitical context (market-wide), readable by bots.
  api.get("/macro", wrap(async (_req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Cache-Control", "private, max-age=60");
    res.json(await macroNow());
  }));

  api.get("/sentiment", wrap(async (req, res) => {
    const kind = parseKind(req.query.kind);
    res.json(await sentiment(parseSymbol(req.query.symbol, kind), kind));
  }));

  api.use((_req, res) => res.status(404).json({ error: "route inconnue" }));
  app.use("/api", api);

  // ----- Static files -----
  const staticOptions = {
    index: false,
    setHeaders: (res: Response, path: string) => {
      res.setHeader("Cache-Control", /-[a-z0-9]{8,}\.(js|css|svg|png)$/.test(path) ? "private, max-age=31536000, immutable" : "private, max-age=3600");
    },
  };
  app.use(express.static(DIST, staticOptions));
  app.use(express.static(PUBLIC, staticOptions));

  // SPA: site, legal pages and /app/* served by index.html.
  app.use((req, res, next) => {
    if (req.method !== "GET" && req.method !== "HEAD") return next();
    // A missing file (with an extension) is a real 404, never the HTML page.
    if (/\.[a-z0-9]{2,5}$/i.test(req.path)) return res.status(404).type("text").send("Introuvable");
    const index = join(DIST, "index.html");
    if (!existsSync(index)) return res.status(500).type("text").send("Build manquant : lancez `bun run build`.");
    res.setHeader("Cache-Control", "no-cache");
    res.sendFile(index);
  });

  app.use((err: unknown, _req: Request, res: Response, _next: NextFunction) => {
    if (err instanceof BadRequest) return res.status(400).json({ error: err.message });
    const message = err instanceof Error ? err.message : "erreur";
    res.status(502).json({ error: message });
  });

  return app;
}
