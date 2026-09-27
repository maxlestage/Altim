import { afterAll, beforeAll, expect, test } from "bun:test";
import type { Server } from "node:http";
import { authConfig, base32, checkTotp, makeSession, readSession, safeEqual, SESSION_COOKIE, toBase32, totp, type AuthConfig } from "../server/auth";
import { createApp } from "../server/app";

// RFC 6238 appendix B (SHA1): secret "12345678901234567890", T = 59 s → 94287082 (6 digits: 287082).
const RFC_SECRET = toBase32(Buffer.from("12345678901234567890"));

test("TOTP : vecteurs officiels de la RFC 6238 et anti-rejeu", () => {
  expect(RFC_SECRET).toBe("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
  expect(base32(RFC_SECRET).toString()).toBe("12345678901234567890");
  expect(totp(RFC_SECRET, Math.floor(59 / 30))).toBe("287082");
  expect(totp(RFC_SECRET, Math.floor(1111111109 / 30))).toBe("081804");
  expect(totp(RFC_SECRET, Math.floor(2000000000 / 30))).toBe("279037");
  const now = 1111111109_000;
  const step = Math.floor(now / 30_000);
  expect(checkTotp(RFC_SECRET, "081804", now, 0)).toBe(step);
  // Same code a second time: refused (replay).
  expect(checkTotp(RFC_SECRET, "081804", now, step)).toBeNull();
  // Previous step still accepted (clock drift), two steps away refused.
  expect(checkTotp(RFC_SECRET, totp(RFC_SECRET, step - 1), now, 0)).toBe(step - 1);
  expect(checkTotp(RFC_SECRET, totp(RFC_SECRET, step - 2), now, 0)).toBeNull();
  expect(checkTotp(RFC_SECRET, "12345", now, 0)).toBeNull();
});

test("comparaison en temps constant et configuration robuste obligatoire", () => {
  expect(safeEqual("abc", "abc")).toBe(true);
  expect(safeEqual("abc", "abd")).toBe(false);
  expect(safeEqual("abc", "abcd")).toBe(false);
  expect(authConfig({})).toBeNull();
  expect(() => authConfig({ ALTIM_USER: "max" })).toThrow();
  const ok = { ALTIM_USER: "max", ALTIM_PASSWORD_HASH: "$argon2id$v=19$…", ALTIM_SESSION_SECRET: "x".repeat(64) };
  expect(authConfig(ok)?.user).toBe("max");
  expect(() => authConfig({ ...ok, ALTIM_PASSWORD_HASH: "$2b$10$bcrypt" })).toThrow("argon2id");
  expect(() => authConfig({ ...ok, ALTIM_SESSION_SECRET: "court" })).toThrow("64");
  expect(() => authConfig({ ...ok, ALTIM_API_TOKEN: "court" })).toThrow("40");
  expect(() => authConfig({ ...ok, ALTIM_TOTP_SECRET: "ABCD" })).toThrow("160");
});

let cfg: AuthConfig;
let server: Server;
let base = "";
const PASSWORD = "Tr3s-Long-Mot-De-Passe-Pour-Test";
const TOTP = toBase32(Buffer.from("altim-test-secret-20"));
const API = `altim_${"t".repeat(43)}`;

beforeAll(async () => {
  cfg = {
    user: "max",
    passwordHash: await Bun.password.hash(PASSWORD, { algorithm: "argon2id", memoryCost: 4096, timeCost: 2 }),
    totpSecret: TOTP,
    sessionSecret: "s".repeat(128),
    apiToken: API,
  };
  server = createApp({ auth: { cfg, production: true } }).listen(0);
  await new Promise((r) => server.once("listening", r));
  const addr = server.address();
  base = `http://127.0.0.1:${typeof addr === "object" && addr ? addr.port : 0}`;
});
afterAll(() => server.close());

const form = (fields: Record<string, string>, headers: Record<string, string> = {}) =>
  fetch(`${base}/login`, {
    method: "POST",
    redirect: "manual",
    headers: { "Content-Type": "application/x-www-form-urlencoded", Origin: base, "X-Forwarded-For": fields.ip ?? "203.0.113.1", ...headers },
    body: new URLSearchParams(fields).toString(),
  });
const code = (offset = 0) => totp(TOTP, Math.floor(Date.now() / 30_000) + offset);

test("session signée : infalsifiable, expire, liée à l'identifiant", () => {
  const s = makeSession(cfg);
  expect(readSession(cfg, s)).toBe(true);
  expect(readSession(cfg, s.replace(/.$/, (c) => (c === "A" ? "B" : "A")))).toBe(false);
  expect(readSession({ ...cfg, sessionSecret: "z".repeat(128) }, s)).toBe(false);
  expect(readSession(cfg, s, Date.now() + 8 * 86_400_000)).toBe(false);
  expect(readSession({ ...cfg, user: "autre" }, s)).toBe(false);
  expect(readSession(cfg, undefined)).toBe(false);
});

test("tout est privé : site, application et API ; santé et page de connexion accessibles", async () => {
  for (const path of ["/", "/app", "/app/actif/crypto/BTC", "/mentions-legales"]) {
    const r = await fetch(base + path, { redirect: "manual" });
    expect(r.status).toBe(302);
    expect(r.headers.get("location")).toBe(`/login?next=${encodeURIComponent(path)}`);
  }
  const api = await fetch(`${base}/api/tickers`);
  expect(api.status).toBe(401);
  expect((await api.json()).error).toContain("authentification");
  expect(await (await fetch(`${base}/health`)).text()).toBe("ok");
  const login = await fetch(`${base}/login?next=/app/reglages`);
  expect(login.status).toBe(200);
  const html = await login.text();
  expect(html).toContain('name="password"');
  expect(html).toContain('name="code"');
  expect(html).toContain('value="/app/reglages"');
  expect(login.headers.get("x-robots-tag")).toContain("noindex");
  expect(await (await fetch(`${base}/robots.txt`)).text()).toContain("Disallow: /");
});

test("connexion : mot de passe + code 2FA → cookie sécurisé, puis accès ; déconnexion", async () => {
  const r = await form({ user: "max", password: PASSWORD, code: code(), next: "/app/reglages", ip: "203.0.113.10" });
  expect(r.status).toBe(303);
  expect(r.headers.get("location")).toBe("/app/reglages");
  const set = r.headers.get("set-cookie")!;
  for (const flag of ["HttpOnly", "Secure", "SameSite=Strict", "Path=/", `Max-Age=${7 * 86400}`]) expect(set).toContain(flag);
  const cookie = set.split(";")[0]!;
  expect(cookie.startsWith(`${SESSION_COOKIE}=`)).toBe(true);
  const page = await fetch(`${base}/api/universe?kind=crypto&limit=1`, { headers: { Cookie: cookie } });
  expect(page.status).not.toBe(401);
  expect(page.headers.get("cache-control")).toContain("private");
  // Already logged in: the login page sends back to the app.
  expect((await fetch(`${base}/login`, { headers: { Cookie: cookie }, redirect: "manual" })).status).toBe(302);
  const out = await fetch(`${base}/logout`, { method: "POST", headers: { Cookie: cookie, Origin: base }, redirect: "manual" });
  expect(out.status).toBe(303);
  expect(out.headers.get("set-cookie")).toContain("Max-Age=0");
});

test("refus : mauvais mot de passe, mauvais code, code rejoué, redirection externe, autre site (CSRF)", async () => {
  const ip = "203.0.113.20";
  expect((await form({ user: "max", password: "mauvais", code: code(), ip })).status).toBe(401);
  expect((await form({ user: "max", password: PASSWORD, code: "000000", ip: "203.0.113.21" })).status).toBe(401);
  expect((await form({ user: "autre", password: PASSWORD, code: code(), ip: "203.0.113.22" })).status).toBe(401);
  // A code already used (the successful login above consumed the current step) is refused.
  const c = code(1);
  expect((await form({ user: "max", password: PASSWORD, code: c, ip: "203.0.113.23" })).status).toBe(303);
  expect((await form({ user: "max", password: PASSWORD, code: c, ip: "203.0.113.24" })).status).toBe(401);
  // Open redirect: an external "next" is replaced by /app.
  const r = await form({ user: "max", password: PASSWORD, code: code(-1) === c ? code() : code(-1), next: "//evil.example/x", ip: "203.0.113.25" });
  if (r.status === 303) expect(r.headers.get("location")).toBe("/app");
  // Form posted from another site.
  expect((await form({ user: "max", password: PASSWORD, code: code() }, { Origin: "https://evil.example" })).status).toBe(403);
  const bad = await form({ user: "max", password: "x", code: "123456", ip });
  expect(await bad.text()).toContain("Identifiant, mot de passe ou code incorrect.");
});

test("verrouillage après 5 échecs depuis la même adresse, même avec le bon mot de passe", async () => {
  const ip = "203.0.113.30";
  for (let i = 0; i < 5; i++) expect((await form({ user: "max", password: "faux", code: "111111", ip })).status).toBe(401);
  const locked = await form({ user: "max", password: PASSWORD, code: code(), ip });
  expect(locked.status).toBe(429);
  // Another address is not affected.
  expect((await form({ user: "max", password: "faux", code: "111111", ip: "203.0.113.31" })).status).toBe(401);
}, 20_000);

test("bots : jeton Bearer accepté sur l'API seulement", async () => {
  const ok = await fetch(`${base}/api/universe?kind=crypto&limit=1`, { headers: { Authorization: `Bearer ${API}` } });
  expect(ok.status).not.toBe(401);
  expect((await fetch(`${base}/api/universe?kind=crypto&limit=1`, { headers: { Authorization: `Bearer ${API}x` } })).status).toBe(401);
  expect((await fetch(`${base}/app`, { headers: { Authorization: `Bearer ${API}` }, redirect: "manual" })).status).toBe(302);
});

test("production sans configuration : rien n'est servi (échec fermé)", async () => {
  const s = createApp({ auth: { cfg: null, production: true } }).listen(0);
  await new Promise((r) => s.once("listening", r));
  const addr = s.address();
  const b = `http://127.0.0.1:${typeof addr === "object" && addr ? addr.port : 0}`;
  try {
    expect((await fetch(`${b}/app`, { redirect: "manual" })).status).toBe(302);
    expect((await fetch(`${b}/api/tickers`)).status).toBe(401);
    expect(await (await fetch(`${b}/login`)).text()).toContain("non configuré");
  } finally {
    s.close();
  }
});
