/**
 * Private access: the whole site, the web app and the API are reserved to the owner.
 *
 * Secrets live only in the environment (Heroku config vars), never in the code:
 * - ALTIM_USER            login name
 * - ALTIM_PASSWORD_HASH   argon2id hash of the password (the password itself is stored nowhere)
 * - ALTIM_TOTP_SECRET     base32 secret of the 6-digit code from an authenticator app (second factor)
 * - ALTIM_SESSION_SECRET  key that signs the session cookie (changing it logs every device out)
 * - ALTIM_API_TOKEN       token for trading bots: `Authorization: Bearer <token>` on /api/*
 *
 * Protections: argon2id password hash, TOTP (RFC 6238) with anti-replay, HMAC-SHA256 signed cookie (HttpOnly,
 * Secure, SameSite=Strict, 7 days, revoked at logout), constant-time comparisons, lock-out after repeated failures per
 * IP (IPv6: per /64) plus a global cap, one attempt at a time per IP and at most 2 password checks at once on the
 * server (argon2id takes 64 MiB each: a flood of parallel logins cannot exhaust the memory), Origin check on forms
 * (CSRF), no indexing. Fails closed: in production, without configuration, nothing is served.
 * Generate the values with `bun run secrets` (web/scripts/secrets.ts).
 */
import { createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import type { NextFunction, Request, Response } from "express";

export interface AuthConfig {
  user: string;
  passwordHash: string;
  totpSecret: string | null;
  sessionSecret: string;
  apiToken: string | null;
}

export const SESSION_COOKIE = "altim_session";
const SESSION_DAYS = 7;
const MAX_FAILURES = 5;
const LOCK_MS = 15 * 60_000;
/** Failures from all addresses together within LOCK_MS before every new login is paused (distributed guessing). */
const GLOBAL_FAILURES = 30;
/** Password checks running at the same time (argon2id: 64 MiB each). */
const MAX_VERIFYING = 2;

/** Configuration from the environment; null when access control is not configured. Throws on a weak configuration. */
export function authConfig(env: Record<string, string | undefined> = process.env): AuthConfig | null {
  const { ALTIM_USER, ALTIM_PASSWORD_HASH, ALTIM_TOTP_SECRET, ALTIM_SESSION_SECRET, ALTIM_API_TOKEN } = env;
  if (!ALTIM_USER && !ALTIM_PASSWORD_HASH && !ALTIM_SESSION_SECRET) return null;
  if (!ALTIM_USER || !ALTIM_PASSWORD_HASH || !ALTIM_SESSION_SECRET) throw new Error("ALTIM_USER, ALTIM_PASSWORD_HASH et ALTIM_SESSION_SECRET sont tous nécessaires");
  if (!ALTIM_PASSWORD_HASH.startsWith("$argon2id$")) throw new Error("ALTIM_PASSWORD_HASH doit être un hachage argon2id (bun run secrets)");
  if (ALTIM_SESSION_SECRET.length < 64) throw new Error("ALTIM_SESSION_SECRET trop court (64 caractères minimum)");
  if (ALTIM_API_TOKEN && ALTIM_API_TOKEN.length < 40) throw new Error("ALTIM_API_TOKEN trop court (40 caractères minimum)");
  if (ALTIM_TOTP_SECRET && base32(ALTIM_TOTP_SECRET).length < 20) throw new Error("ALTIM_TOTP_SECRET trop court (160 bits minimum)");
  return { user: ALTIM_USER, passwordHash: ALTIM_PASSWORD_HASH, totpSecret: ALTIM_TOTP_SECRET || null, sessionSecret: ALTIM_SESSION_SECRET, apiToken: ALTIM_API_TOKEN || null };
}

/** Equality in constant time (length difference included) against timing attacks. */
export function safeEqual(a: string, b: string): boolean {
  const ha = createHmac("sha256", "altim-compare").update(a).digest();
  const hb = createHmac("sha256", "altim-compare").update(b).digest();
  return timingSafeEqual(ha, hb) && a.length === b.length;
}

// ---------- TOTP (RFC 6238: HMAC-SHA1, 30 s, 6 digits) ----------

const B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
export function base32(s: string): Buffer {
  const clean = s.toUpperCase().replace(/[\s=-]/g, "");
  let bits = "";
  for (const ch of clean) {
    const v = B32.indexOf(ch);
    if (v < 0) throw new Error("base32 invalide");
    bits += v.toString(2).padStart(5, "0");
  }
  const out: number[] = [];
  for (let i = 0; i + 8 <= bits.length; i += 8) out.push(parseInt(bits.slice(i, i + 8), 2));
  return Buffer.from(out);
}
export function toBase32(buf: Buffer): string {
  let bits = "", out = "";
  for (const b of buf) bits += b.toString(2).padStart(8, "0");
  for (let i = 0; i < bits.length; i += 5) out += B32[parseInt(bits.slice(i, i + 5).padEnd(5, "0"), 2)];
  return out;
}

export function totp(secret: string, counter: number): string {
  const msg = Buffer.alloc(8);
  msg.writeBigUInt64BE(BigInt(counter));
  const h = createHmac("sha1", base32(secret)).update(msg).digest();
  const o = h[h.length - 1]! & 0xf;
  const code = ((h.readUInt32BE(o) & 0x7fffffff) % 1_000_000).toString();
  return code.padStart(6, "0");
}

/** Accepts the current 30 s step and the neighbouring ones (clock drift); a step already used is refused (replay). */
export function checkTotp(secret: string, code: string, now: number, lastUsed: number): number | null {
  if (!/^\d{6}$/.test(code)) return null;
  const step = Math.floor(now / 30_000);
  for (const c of [step, step - 1, step + 1]) if (c > lastUsed && safeEqual(totp(secret, c), code)) return c;
  return null;
}

// ---------- Signed session cookie ----------

const b64 = (b: Buffer | string) => Buffer.from(b).toString("base64url");
const sign = (secret: string, data: string) => createHmac("sha256", secret).update(data).digest("base64url");

export function makeSession(cfg: AuthConfig, now = Date.now()): string {
  const payload = b64(JSON.stringify({ u: cfg.user, iat: now, exp: now + SESSION_DAYS * 86_400_000, n: randomBytes(12).toString("base64url") }));
  return `${payload}.${sign(cfg.sessionSecret, payload)}`;
}

/** Valid session → its payload (nonce and expiry, for revocation); otherwise null. */
export function sessionPayload(cfg: AuthConfig, cookie: string | undefined, now = Date.now()): { n: string; exp: number } | null {
  if (!cookie || cookie.length > 512) return null;
  const [payload, mac] = cookie.split(".");
  if (!payload || !mac || !safeEqual(sign(cfg.sessionSecret, payload), mac)) return null;
  try {
    const p = JSON.parse(Buffer.from(payload, "base64url").toString());
    return p.u === cfg.user && typeof p.exp === "number" && p.exp > now && typeof p.n === "string" ? { n: p.n, exp: p.exp } : null;
  } catch {
    return null;
  }
}

export function readSession(cfg: AuthConfig, cookie: string | undefined, now = Date.now(), revoked?: Map<string, number>): boolean {
  const p = sessionPayload(cfg, cookie, now);
  return !!p && !revoked?.has(p.n);
}

export function cookies(req: Request): Record<string, string> {
  const decode = (v: string) => {
    try {
      return decodeURIComponent(v);
    } catch {
      return v; // malformed escape (%E0…): kept raw, it will simply not match
    }
  };
  return Object.fromEntries((req.headers.cookie ?? "").split(";").map((c) => c.trim().split("=")).filter((kv) => kv.length === 2).map(([k, v]) => [k!, decode(v!)]));
}

// ---------- Login page ----------

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

export function loginPage(opts: { error?: string; next?: string; totp: boolean; configured: boolean }): string {
  const body = !opts.configured
    ? `<p>Accès privé non configuré. Ajoutez les variables ALTIM_* dans Heroku → Settings → Config Vars (voir DEPLOIEMENT.md).</p>`
    : `<form method="post" action="/login" autocomplete="on">
      <input type="hidden" name="next" value="${esc(opts.next ?? "/app")}">
      <label>Identifiant<input name="user" autocomplete="username" required autocapitalize="none" spellcheck="false"></label>
      <label>Mot de passe<input name="password" type="password" autocomplete="current-password" required></label>
      ${opts.totp ? `<label>Code à 6 chiffres (application d'authentification)<input name="code" inputmode="numeric" pattern="[0-9]{6}" maxlength="6" autocomplete="one-time-code" required></label>` : ""}
      ${opts.error ? `<p class="err" role="alert">${esc(opts.error)}</p>` : ""}
      <button type="submit">Se connecter</button>
    </form>`;
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow"><title>Altim — Accès privé</title>
<style>
:root{color-scheme:dark}*{box-sizing:border-box}body{margin:0;min-height:100svh;display:grid;place-items:center;padding:16px;background:#05070f;color:#e8ecff;font:16px/1.5 system-ui,-apple-system,sans-serif}
main{width:100%;max-width:380px;padding:28px 22px;border:1px solid rgba(0,240,255,.25);border-radius:20px;background:#0b0f1f;box-shadow:0 0 40px rgba(0,240,255,.08)}
h1{margin:0 0 4px;font-size:22px;letter-spacing:.2em;color:#00f0ff}p{color:#9aa3c7;margin:0 0 18px;font-size:14px}
form{display:grid;gap:14px}label{display:grid;gap:6px;font-size:13px;color:#9aa3c7}
input{width:100%;min-height:48px;padding:10px 12px;border-radius:12px;border:1px solid #232a45;background:#060913;color:#e8ecff;font-size:16px}
input:focus{outline:2px solid #00f0ff;outline-offset:1px}button{min-height:48px;border:0;border-radius:12px;background:#00f0ff;color:#05070f;font-weight:700;font-size:16px;cursor:pointer}
.err{color:#ff3b5c;margin:0}
</style></head><body><main><h1>ALTIM</h1><p>Accès privé.</p>${body}</main></body></html>`;
}

// ---------- Middleware ----------

/** Paths reachable without a session: health check (Heroku) and the login page itself. */
const PUBLIC_PATHS = new Set(["/health", "/login", "/robots.txt"]);

export function createAuth(cfg: AuthConfig | null, production: boolean) {
  const failures = new Map<string, { n: number; first: number; until: number; busy: boolean }>();
  /** Times of the recent failures, all addresses together. */
  let globalFailures: number[] = [];
  let globalUntil = 0;
  let verifying = 0;
  /** Sessions closed by "Se déconnecter" (nonce → expiry), until they would have expired anyway. */
  const revoked = new Map<string, number>();
  let lastTotp = 0;
  const secureCookie = production;

  const cookieHeader = (value: string, maxAge: number) =>
    `${SESSION_COOKIE}=${value}; Path=/; HttpOnly; SameSite=Strict; Max-Age=${maxAge}${secureCookie ? "; Secure" : ""}`;

  /** IPv4 as is; IPv6 by /64 (one subscriber owns a whole /64 and could rotate addresses in it). */
  const clientKey = (ip: string) => {
    const v6 = ip.replace(/^::ffff:/, "");
    if (!v6.includes(":")) return v6;
    const parts = v6.split("::")[0]!.split(":");
    return parts.slice(0, 4).join(":") + "::/64";
  };
  const locked = (key: string, now: number) => (failures.get(key)?.until ?? 0) > now || globalUntil > now;
  const fail = (key: string, now: number) => {
    const f = failures.get(key) ?? { n: 0, first: now, until: 0, busy: false };
    // Failures are counted over a 15-minute window, not forever.
    if (now - f.first > LOCK_MS) {
      f.n = 0;
      f.first = now;
    }
    f.n++;
    if (f.n >= MAX_FAILURES) {
      f.until = now + LOCK_MS;
      f.n = 0;
      f.first = now;
    }
    failures.set(key, f);
    globalFailures = globalFailures.filter((t) => now - t < LOCK_MS);
    globalFailures.push(now);
    if (globalFailures.length >= GLOBAL_FAILURES) {
      globalUntil = now + LOCK_MS;
      globalFailures = [];
    }
  };
  /** Forgets expired entries (called on each attempt, bounded work). */
  const sweep = (now: number) => {
    if (failures.size > 1_000) for (const [k, v] of failures) if (!v.busy && Math.max(v.until, v.first + LOCK_MS) < now) failures.delete(k);
    if (revoked.size > 1_000) for (const [n, exp] of revoked) if (exp < now) revoked.delete(n);
  };

  /** Same-origin form only (CSRF): the Origin (or Referer) must be this host. */
  const sameOrigin = (req: Request) => {
    const origin = req.headers.origin ?? req.headers.referer;
    if (!origin) return false;
    try {
      return new URL(origin).host === req.headers.host;
    } catch {
      return false;
    }
  };

  const safeNext = (v: unknown) => (typeof v === "string" && /^\/(?!\/)[\w\-./?=&%]*$/.test(v) && !v.startsWith("/login") ? v : "/app");

  function isAuthenticated(req: Request): boolean {
    if (!cfg) return !production;
    if (readSession(cfg, cookies(req)[SESSION_COOKIE], Date.now(), revoked)) return true;
    const bearer = /^Bearer (.+)$/.exec(req.headers.authorization ?? "")?.[1];
    return !!(cfg.apiToken && bearer && req.path.startsWith("/api/") && safeEqual(bearer, cfg.apiToken));
  }

  async function login(req: Request, res: Response) {
    const now = Date.now();
    const ip = clientKey(req.ip ?? "?");
    sweep(now);
    res.setHeader("X-Robots-Tag", "noindex, nofollow");
    res.setHeader("Cache-Control", "no-store");
    const page = (status: number, error: string) =>
      res.status(status).type("html").send(loginPage({ error, next: safeNext(req.body?.next), totp: !!cfg?.totpSecret, configured: !!cfg }));
    if (!cfg) return page(503, "Accès privé non configuré.");
    if (!sameOrigin(req)) return page(403, "Requête refusée.");
    if (locked(ip, now)) return page(429, "Trop d'essais. Réessayez dans 15 minutes.");
    // One attempt at a time per address, reserved before any await: parallel requests cannot all slip past the
    // lock-out check before the first failure is counted.
    const entry = failures.get(ip) ?? { n: 0, first: now, until: 0, busy: false };
    if (entry.busy) return page(429, "Une connexion est déjà en cours. Réessayez dans un instant.");
    if (verifying >= MAX_VERIFYING) return page(503, "Serveur occupé. Réessayez dans un instant.");
    entry.busy = true;
    failures.set(ip, entry);
    try {
      const { user = "", password = "", code = "" } = (req.body ?? {}) as Record<string, string>;
      // Every check runs whatever the result of the previous ones (no hint on which one failed).
      const userOk = safeEqual(String(user), cfg.user);
      verifying++;
      let passOk = false;
      try {
        passOk = await Bun.password.verify(String(password).slice(0, 256), cfg.passwordHash).catch(() => false);
      } finally {
        verifying--;
      }
      const step = cfg.totpSecret ? checkTotp(cfg.totpSecret, String(code), Date.now(), lastTotp) : 0;
      if (!userOk || !passOk || step === null) {
        fail(ip, Date.now());
        // Slows guessing down; the address stays "busy" meanwhile, but the password-check slot is already free.
        await new Promise((r) => setTimeout(r, 400 + Math.random() * 400));
        return page(401, "Identifiant, mot de passe ou code incorrect.");
      }
      if (step) lastTotp = step;
    } finally {
      const f = failures.get(ip);
      if (f) f.busy = false;
    }
    failures.delete(ip);
    res.setHeader("Set-Cookie", cookieHeader(makeSession(cfg), SESSION_DAYS * 86_400));
    res.redirect(303, safeNext(req.body?.next));
  }

  function logout(req: Request, res: Response) {
    if (!sameOrigin(req)) return res.status(403).type("text").send("Requête refusée.");
    // The cookie is revoked on the server too: a copy of it (stolen, or on another device) stops working.
    const p = cfg ? sessionPayload(cfg, cookies(req)[SESSION_COOKIE]) : null;
    if (p) revoked.set(p.n, p.exp);
    res.setHeader("Set-Cookie", cookieHeader("", 0));
    res.redirect(303, "/login");
  }

  function guard(req: Request, res: Response, next: NextFunction) {
    res.setHeader("X-Robots-Tag", "noindex, nofollow");
    if (PUBLIC_PATHS.has(req.path)) return next();
    if (isAuthenticated(req)) {
      res.setHeader("Cache-Control", "private, no-store");
      return next();
    }
    if (req.path.startsWith("/api/")) return res.status(401).json({ error: "authentification requise" });
    if (req.method !== "GET" && req.method !== "HEAD") return res.status(401).type("text").send("authentification requise");
    return res.redirect(302, `/login?next=${encodeURIComponent(req.originalUrl)}`);
  }

  function loginForm(req: Request, res: Response) {
    if (cfg && isAuthenticated(req)) return res.redirect(302, safeNext(req.query.next));
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("X-Robots-Tag", "noindex, nofollow");
    res.type("html").send(loginPage({ next: safeNext(req.query.next), totp: !!cfg?.totpSecret, configured: !!cfg }));
  }

  return { guard, login, logout, loginForm, isAuthenticated, enabled: !!cfg || production };
}
