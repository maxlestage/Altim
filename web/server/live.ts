/**
 * Live prices: nothing frozen.
 *
 * - Crypto: real-time WebSocket feeds from 7 exchanges (OKX, Coinbase, Kraken, Bitfinex, Bitget, Gate, Crypto.com),
 *   one connection per exchange shared by every visitor, subscriptions added / removed on demand. The live price is
 *   the median of the exchanges that agree (an exchange more than 1 % away from the others is ignored).
 * - Stocks: no free real-time feed exists; the fast quote sources are polled every 5 s while someone watches.
 *   Cryptos listed on none of these exchanges are polled the same way from the REST sources.
 *
 * Browsers receive ticks through Server-Sent Events (/api/live), several per second when the market moves.
 */
import { QUOTE_SOURCES, type Asset } from "./quotes";
import { nyOpen } from "./market";

export type Kind = "crypto" | "stock";
export interface Tick {
  symbol: string;
  kind: Kind;
  price: number;
  /** 24 h change in %, median of the sources that give it. */
  change: number | null;
  /** Sources agreeing / sources with a live price. */
  agreeing: number;
  total: number;
  sources: string[];
  time: number;
  /** Stocks: whether the US regular session is open. */
  market?: "open" | "closed";
}

type Quote = { price: number; change: number | null; time: number };
type Parsed = { base: string; price: number; change?: number | null }[];

const median = (v: number[]) => {
  const s = [...v].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2]! : (s[s.length / 2 - 1]! + s[s.length / 2]!) / 2;
};
const num = (v: unknown) => {
  const n = typeof v === "string" ? Number(v) : typeof v === "number" ? v : NaN;
  return Number.isFinite(n) ? n : null;
};

// ---------- Exchange feeds (message formats checked on real streams, see test/live.test.ts) ----------

export interface FeedSpec {
  name: string;
  url: string;
  subscribe: (bases: string[]) => unknown[];
  unsubscribe: (bases: string[]) => unknown[];
  parse: (msg: any, feed: { chanIds: Map<number, string> }) => Parsed;
  /** Messages to send periodically to keep the connection open. */
  ping?: { every: number; message: () => unknown };
  /** Reply required by the exchange to some messages (heartbeats). */
  reply?: (msg: any) => unknown | null;
}

const bfxSymbol = (b: string) => (b.length > 3 ? `t${b}:USD` : `t${b}USD`);

export const FEEDS: FeedSpec[] = [
  {
    name: "OKX",
    url: "wss://ws.okx.com/ws/v5/public",
    subscribe: (b) => [{ op: "subscribe", args: b.map((x) => ({ channel: "tickers", instId: `${x}-USDT` })) }],
    unsubscribe: (b) => [{ op: "unsubscribe", args: b.map((x) => ({ channel: "tickers", instId: `${x}-USDT` })) }],
    parse: (m) =>
      m?.arg?.channel === "tickers" && Array.isArray(m.data)
        ? m.data.flatMap((d: any) => {
          const price = num(d.last), open = num(d.open24h);
          return price ? [{ base: String(d.instId).replace(/-USDT$/, ""), price, change: open ? (price / open - 1) * 100 : null }] : [];
        })
        : [],
    ping: { every: 25_000, message: () => "ping" },
  },
  {
    name: "Coinbase",
    url: "wss://ws-feed.exchange.coinbase.com",
    subscribe: (b) => [{ type: "subscribe", product_ids: b.map((x) => `${x}-USD`), channels: ["ticker"] }],
    unsubscribe: (b) => [{ type: "unsubscribe", product_ids: b.map((x) => `${x}-USD`), channels: ["ticker"] }],
    parse: (m) => {
      if (m?.type !== "ticker") return [];
      const price = num(m.price), open = num(m.open_24h);
      return price ? [{ base: String(m.product_id).replace(/-USD$/, ""), price, change: open ? (price / open - 1) * 100 : null }] : [];
    },
  },
  {
    name: "Kraken",
    url: "wss://ws.kraken.com/v2",
    subscribe: (b) => [{ method: "subscribe", params: { channel: "ticker", symbol: b.map((x) => `${x}/USD`) } }],
    unsubscribe: (b) => [{ method: "unsubscribe", params: { channel: "ticker", symbol: b.map((x) => `${x}/USD`) } }],
    parse: (m) =>
      m?.channel === "ticker" && Array.isArray(m.data)
        ? m.data.flatMap((d: any) => {
          const price = num(d.last);
          return price ? [{ base: String(d.symbol).replace(/\/USD$/, ""), price, change: num(d.change_pct) }] : [];
        })
        : [],
    ping: { every: 30_000, message: () => ({ method: "ping" }) },
  },
  {
    name: "Bitfinex",
    url: "wss://api-pub.bitfinex.com/ws/2",
    subscribe: (b) => b.map((x) => ({ event: "subscribe", channel: "ticker", symbol: bfxSymbol(x) })),
    unsubscribe: () => [], // handled by closing the channel ids (see LiveHub)
    parse: (m, feed) => {
      if (m?.event === "subscribed" && m.channel === "ticker") {
        feed.chanIds.set(m.chanId, String(m.symbol).slice(1).replace(/:?USD$/, ""));
        return [];
      }
      if (Array.isArray(m) && Array.isArray(m[1]) && feed.chanIds.has(m[0])) {
        const d = m[1];
        const price = num(d[6]);
        return price ? [{ base: feed.chanIds.get(m[0])!, price, change: num(d[5]) !== null ? num(d[5])! * 100 : null }] : [];
      }
      return [];
    },
  },
  {
    name: "Bitget",
    url: "wss://ws.bitget.com/v2/ws/public",
    subscribe: (b) => [{ op: "subscribe", args: b.map((x) => ({ instType: "SPOT", channel: "ticker", instId: `${x}USDT` })) }],
    unsubscribe: (b) => [{ op: "unsubscribe", args: b.map((x) => ({ instType: "SPOT", channel: "ticker", instId: `${x}USDT` })) }],
    parse: (m) =>
      m?.arg?.channel === "ticker" && Array.isArray(m.data)
        ? m.data.flatMap((d: any) => {
          const price = num(d.lastPr), open = num(d.open24h);
          return price ? [{ base: String(d.instId).replace(/USDT$/, ""), price, change: open ? (price / open - 1) * 100 : null }] : [];
        })
        : [],
    ping: { every: 25_000, message: () => "ping" },
  },
  {
    name: "Gate.io",
    url: "wss://api.gateio.ws/ws/v4/",
    subscribe: (b) => [{ time: Math.floor(Date.now() / 1000), channel: "spot.tickers", event: "subscribe", payload: b.map((x) => `${x}_USDT`) }],
    unsubscribe: (b) => [{ time: Math.floor(Date.now() / 1000), channel: "spot.tickers", event: "unsubscribe", payload: b.map((x) => `${x}_USDT`) }],
    parse: (m) => {
      if (m?.channel !== "spot.tickers" || m.event !== "update" || !m.result) return [];
      const price = num(m.result.last);
      return price ? [{ base: String(m.result.currency_pair).replace(/_USDT$/, ""), price, change: num(m.result.change_percentage) }] : [];
    },
    ping: { every: 20_000, message: () => ({ time: Math.floor(Date.now() / 1000), channel: "spot.ping" }) },
  },
  {
    name: "Crypto.com",
    url: "wss://stream.crypto.com/exchange/v1/market",
    subscribe: (b) => [{ id: Date.now(), method: "subscribe", params: { channels: b.map((x) => `ticker.${x}_USDT`) } }],
    unsubscribe: (b) => [{ id: Date.now(), method: "unsubscribe", params: { channels: b.map((x) => `ticker.${x}_USDT`) } }],
    parse: (m) => {
      const r = m?.result;
      if (m?.method !== "subscribe" || r?.channel !== "ticker" || !Array.isArray(r.data)) return [];
      return r.data.flatMap((d: any) => {
        const price = num(d.a);
        return price ? [{ base: String(r.instrument_name).replace(/_USDT$/, ""), price, change: num(d.c) !== null ? num(d.c)! * 100 : null }] : [];
      });
    },
    reply: (m) => (m?.method === "public/heartbeat" ? { id: m.id, method: "public/respond-heartbeat" } : null),
  },
];

// ---------- Consensus of live quotes ----------

/** Median of the fresh quotes; a source more than `tolerance` % from the median is ignored. */
export function liveConsensus(quotes: Map<string, Quote>, now: number, maxAge = 600_000, tolerance = 1): Omit<Tick, "symbol" | "kind"> | null {
  const fresh = [...quotes.entries()].filter(([, q]) => now - q.time <= maxAge && q.price > 0);
  if (!fresh.length) return null;
  const ref = median(fresh.map(([, q]) => q.price));
  const ok = fresh.filter(([, q]) => Math.abs(q.price / ref - 1) * 100 <= tolerance);
  const changes = ok.map(([, q]) => q.change).filter((c): c is number => c !== null && Number.isFinite(c));
  return {
    price: median(ok.map(([, q]) => q.price)),
    change: changes.length ? median(changes) : null,
    agreeing: ok.length,
    total: fresh.length,
    sources: ok.map(([n]) => n),
    time: Math.max(...ok.map(([, q]) => q.time)),
  };
}

/** US regular session (9:30 – 16:00 New York, weekdays). Holidays are not known: the price simply stops moving. */
export function usMarketOpen(now = Date.now()): boolean {
  const d = new Date(now);
  const parts = new Intl.DateTimeFormat("en-US", { timeZone: "America/New_York", weekday: "short", year: "numeric", month: "numeric", day: "numeric" })
    .formatToParts(d);
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? "";
  if (["Sat", "Sun"].includes(get("weekday"))) return false;
  const open = nyOpen(Number(get("year")), Number(get("month")), Number(get("day")));
  return now >= open && now < open + 6.5 * 3_600_000;
}

// ---------- Hub ----------

type Listener = (t: Tick) => void;

class ExchangeConnection {
  ws: WebSocket | null = null;
  bases = new Set<string>();
  chanIds = new Map<number, string>();
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private retry = 0;
  private closedByUs = false;

  constructor(private spec: FeedSpec, private onQuote: (exchange: string, p: Parsed[number]) => void) {}

  private send(msgs: unknown[]) {
    if (this.ws?.readyState !== WebSocket.OPEN) return;
    for (const m of msgs) this.ws.send(typeof m === "string" ? m : JSON.stringify(m));
  }

  add(bases: string[]) {
    const fresh = bases.filter((b) => !this.bases.has(b));
    fresh.forEach((b) => this.bases.add(b));
    if (!fresh.length) return;
    if (!this.ws) this.connect();
    else this.send(this.spec.subscribe(fresh));
  }

  remove(bases: string[]) {
    const gone = bases.filter((b) => this.bases.delete(b));
    if (!gone.length) return;
    if (!this.bases.size) return this.close();
    if (this.spec.name === "Bitfinex") {
      for (const [id, b] of this.chanIds) if (gone.includes(b)) { this.send([{ event: "unsubscribe", chanId: id }]); this.chanIds.delete(id); }
    } else this.send(this.spec.unsubscribe(gone));
  }

  private connect() {
    this.closedByUs = false;
    let ws: WebSocket;
    try {
      ws = new WebSocket(this.spec.url);
    } catch {
      return this.scheduleReconnect();
    }
    this.ws = ws;
    ws.onopen = () => {
      this.retry = 0;
      this.chanIds.clear();
      if (this.bases.size) this.send(this.spec.subscribe([...this.bases]));
      if (this.spec.ping) this.pingTimer = setInterval(() => this.send([this.spec.ping!.message()]), this.spec.ping.every);
    };
    ws.onmessage = (e) => {
      const text = typeof e.data === "string" ? e.data : "";
      if (!text || text === "pong") return;
      let msg: any;
      try { msg = JSON.parse(text); } catch { return; }
      const r = this.spec.reply?.(msg);
      if (r) this.send([r]);
      for (const p of this.spec.parse(msg, this)) if (this.bases.has(p.base)) this.onQuote(this.spec.name, p);
    };
    ws.onclose = () => {
      if (this.pingTimer) clearInterval(this.pingTimer);
      this.pingTimer = null;
      this.ws = null;
      if (!this.closedByUs && this.bases.size) this.scheduleReconnect();
    };
    ws.onerror = () => { try { ws.close(); } catch {} };
  }

  private scheduleReconnect() {
    const delay = Math.min(30_000, 1000 * 2 ** this.retry++);
    setTimeout(() => { if (!this.ws && this.bases.size) this.connect(); }, delay);
  }

  close() {
    this.closedByUs = true;
    if (this.pingTimer) clearInterval(this.pingTimer);
    this.pingTimer = null;
    try { this.ws?.close(); } catch {}
    this.ws = null;
  }
}

/** Stock quote sources fast enough to poll every few seconds. */
const FAST_STOCK = ["Robinhood", "TradingView", "Zacks", "Webull"];

export class LiveHub {
  private listeners = new Map<string, Set<Listener>>();
  private quotes = new Map<string, Map<string, Quote>>(); // key → source → quote
  private last = new Map<string, Tick>();
  private lastEmit = new Map<string, number>();
  private pending = new Map<string, ReturnType<typeof setTimeout>>();
  private connections: ExchangeConnection[];
  private timer: ReturnType<typeof setInterval> | null = null;
  private polling = false;
  /** Stocks: US session state at the last poll. */
  private market = new Map<string, "open" | "closed">();

  constructor(
    feeds = FEEDS,
    private restSources = QUOTE_SOURCES.filter((s) => s.kind === "crypto" || FAST_STOCK.includes(s.name)),
    private pollEvery = 5_000,
  ) {
    this.connections = feeds.map((f) => new ExchangeConnection(f, (exchange, p) => this.onQuote(`crypto:${p.base}`, exchange, p)));
  }

  /** Last known tick of each key (sent right away to a new subscriber). */
  snapshot(keys: string[]): Tick[] {
    return keys.map((k) => this.last.get(k)).filter((t): t is Tick => !!t);
  }

  subscribe(keys: string[], listener: Listener): () => void {
    const added: string[] = [];
    for (const k of keys) {
      if (!this.listeners.has(k)) {
        this.listeners.set(k, new Set());
        added.push(k);
      }
      this.listeners.get(k)!.add(listener);
    }
    const crypto = added.filter((k) => k.startsWith("crypto:")).map((k) => k.slice(7));
    if (crypto.length) this.connections.forEach((c) => c.add(crypto));
    if (added.length) void this.poll();
    this.timer ??= setInterval(() => void this.poll(), this.pollEvery);
    return () => {
      const removed: string[] = [];
      for (const k of keys) {
        const set = this.listeners.get(k);
        set?.delete(listener);
        if (set && !set.size) {
          this.listeners.delete(k);
          removed.push(k);
        }
      }
      const gone = removed.filter((k) => k.startsWith("crypto:")).map((k) => k.slice(7));
      if (gone.length) this.connections.forEach((c) => c.remove(gone));
      if (this.timer && !this.listeners.size) {
        clearInterval(this.timer);
        this.timer = null;
      }
    };
  }

  /** Number of watched symbols (for monitoring). */
  get watched() {
    return this.listeners.size;
  }

  onQuote(key: string, source: string, p: { price: number; change?: number | null }, now = Date.now()) {
    this.store(key, source, p.price, p.change ?? null, now);
    this.schedule(key);
  }

  private store(key: string, source: string, price: number, change: number | null, now: number) {
    if (!this.quotes.has(key)) this.quotes.set(key, new Map());
    this.quotes.get(key)!.set(source, { price, change, time: now });
  }

  /** At most 4 ticks per second per symbol: the latest state is always the one sent. */
  private schedule(key: string) {
    if (this.pending.has(key)) return;
    const wait = Math.max(0, 250 - (Date.now() - (this.lastEmit.get(key) ?? 0)));
    this.pending.set(key, setTimeout(() => {
      this.pending.delete(key);
      this.emit(key);
    }, wait));
  }

  private emit(key: string) {
    const stock = key.startsWith("stock:");
    const q = this.quotes.get(key);
    const c = q && liveConsensus(q, Date.now(), stock ? 3_600_000 : 120_000, stock ? 1.5 : 1);
    if (!c) return;
    const [kind, symbol] = key.split(":") as [Kind, string];
    const market = this.market.get(key);
    const tick: Tick = { symbol, kind, ...c, ...(market ? { market } : {}) };
    const prev = this.last.get(key);
    this.last.set(key, tick);
    this.lastEmit.set(key, Date.now());
    if (prev && prev.price === tick.price && prev.agreeing === tick.agreeing && prev.total === tick.total && prev.market === tick.market) return;
    this.listeners.get(key)?.forEach((l) => l(tick));
  }

  /** A WebSocket quote younger than this makes REST polling useless for that crypto. */
  static WS_FRESH = 15_000;

  /**
   * REST polling: every stock (no free real-time feed), plus the cryptos that no WebSocket exchange lists or whose
   * feeds went silent. Same median / tolerance rules as /api/tickers.
   */
  async poll() {
    if (this.polling) return;
    const now = Date.now();
    const wsNames = new Set(FEEDS.map((f) => f.name));
    const assets: Asset[] = [...this.listeners.keys()].flatMap((k): Asset[] => {
      const [kind, symbol] = k.split(":") as [Kind, string];
      if (kind === "crypto") {
        const q = this.quotes.get(k);
        const liveWs = q && [...q.entries()].some(([n, x]) => wsNames.has(n) && now - x.time < LiveHub.WS_FRESH);
        if (liveWs) return [];
      }
      return [{ symbol, name: symbol, kind }];
    });
    if (!assets.length) return;
    this.polling = true;
    try {
      const results = await Promise.all(this.restSources
        .filter((s) => assets.some((a) => a.kind === s.kind))
        .map(async (s) => ({ name: s.name, quotes: await s.fetch(assets.filter((a) => a.kind === s.kind)).catch(() => null) })));
      const t = Date.now();
      const market = usMarketOpen(t) ? "open" : "closed";
      for (const r of results) {
        for (const [sym, q] of r.quotes ?? []) {
          const kind = assets.find((a) => a.symbol === sym && this.restSources.find((s) => s.name === r.name)?.kind === a.kind)?.kind;
          // Same name as a WebSocket feed: one exchange, one vote (the most recent quote wins).
          if (kind) this.store(`${kind}:${sym}`, r.name, q.price, q.change ?? null, t);
        }
      }
      for (const a of assets) {
        const key = `${a.kind}:${a.symbol}`;
        if (a.kind === "stock") this.market.set(key, market);
        this.schedule(key);
      }
    } finally {
      this.polling = false;
    }
  }

  close() {
    this.connections.forEach((c) => c.close());
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
    this.pending.forEach((t) => clearTimeout(t));
  }
}
