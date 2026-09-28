/**
 * Golden exchanges of the TypeScript access control (web/server/auth.ts inside the real Express app), replayed
 * against the Rust port by tests/auth_parity.rs: bun parity/auth.ts
 * Writes tests/golden/auth.json: configuration, a session cookie signed by the TypeScript, login pages and HTTP
 * exchanges (status, the headers the auth code sets, body).
 */
import { request } from "node:http";
import type { AddressInfo } from "node:net";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { loginPage, makeSession, type AuthConfig } from "../../web/server/auth";
import { createApp } from "../../web/server/app";

const PASSWORD = "Tr3s-Long-Mot-De-Passe-Pour-Test";
const HASH = "$argon2id$v=19$m=4096,t=2,p=1$mvKaT8nUCtubj3KYDvqbvLgnT6Kp89ChCixjoKPLZP8$J1VZYdJ9FeRYG48G0GNvS+OM0TV6Kp3h4KR0ex+Kdno";
const cfg: AuthConfig = { user: "max", passwordHash: HASH, totpSecret: null, sessionSecret: "s".repeat(128), apiToken: `altim_${"t".repeat(43)}` };

/** A session issued "in 2100": still valid when the Rust tests run. */
const FUTURE = 4_102_444_800_000;
const tsCookie = makeSession(cfg, FUTURE);

const pages = [
  { error: undefined, next: undefined, totp: false, configured: true },
  { error: "Identifiant, mot de passe ou code incorrect.", next: "/app/x?a=1&b=<\"'>", totp: true, configured: true },
  { error: undefined, next: "/app", totp: false, configured: false },
].map((opts) => ({ opts, html: loginPage(opts) }));

type Req = { method: string; path: string; headers: Record<string, string>; body?: string; etagOf?: number };
const HOST = "altim.test";
const ORIGIN = `http://${HOST}`;
const form = (fields: Record<string, string>) => new URLSearchParams(fields).toString();
const FORM = { "content-type": "application/x-www-form-urlencoded", origin: ORIGIN };
const ip = (n: number) => ({ "x-forwarded-for": `198.51.100.7, 203.0.113.${n}` });
const good = { user: "max", password: PASSWORD };

const reqs: Req[] = [
  { method: "GET", path: "/", headers: ip(1) },
  { method: "GET", path: "/app/actif/crypto/BTC?x=1&y=%C3%A9", headers: { ...ip(1), accept: "text/html,application/xhtml+xml,*/*;q=0.8" } },
  { method: "GET", path: "/mentions-legales", headers: { ...ip(1), accept: "application/json" } },
  { method: "HEAD", path: "/app", headers: ip(1) },
  { method: "GET", path: "/api/tickers", headers: ip(1) },
  { method: "POST", path: "/app", headers: ip(1) },
  { method: "GET", path: "/logout", headers: ip(1) },
  { method: "GET", path: "/health", headers: ip(1) },
  { method: "HEAD", path: "/health", headers: ip(1) },
  { method: "GET", path: "/robots.txt", headers: ip(1) },
  { method: "GET", path: "/login?next=/app/reglages", headers: ip(1) },
  { method: "GET", path: "/login?next=//evil.example", headers: ip(1) },
  { method: "GET", path: "/Login/?next=%2Fapp%2Fx&next=b", headers: ip(1) },
  { method: "GET", path: "/login?next=/app/reglages", headers: ip(1), etagOf: 10 },
  { method: "GET", path: "/login?next=/app/x", headers: { ...ip(1), cookie: `altim_session=${tsCookie}` } },
  { method: "GET", path: "/api/tickers", headers: { ...ip(1), cookie: "altim_session=%E0%A4%A" } },
  { method: "GET", path: "/app", headers: { ...ip(1), authorization: `Bearer ${cfg.apiToken}` } },
  { method: "POST", path: "/login", headers: { ...ip(2), "content-type": "application/x-www-form-urlencoded" }, body: form(good) },
  { method: "POST", path: "/login", headers: { ...ip(2), ...FORM, origin: "https://evil.example" }, body: form(good) },
  { method: "POST", path: "/login", headers: { ...ip(2), ...FORM, origin: "", referer: ORIGIN }, body: form(good) },
  { method: "POST", path: "/login", headers: { ...ip(3), "content-type": "application/x-www-form-urlencoded", referer: `${ORIGIN}/login` }, body: form({ ...good, password: "faux", next: "/app/x" }) },
  { method: "POST", path: "/login", headers: { ...ip(4), ...FORM, "content-type": "application/json" }, body: JSON.stringify(good) },
  { method: "POST", path: "/login", headers: { ...ip(4), ...FORM }, body: form({ ...good, next: "x".repeat(3000) }) },
  { method: "POST", path: "/login", headers: { ...ip(4), ...FORM, "content-type": "application/x-www-form-urlencoded; charset=latin2" }, body: form(good) },
  { method: "POST", path: "/login", headers: { ...ip(5), ...FORM, accept: "text/html" }, body: form({ ...good, next: "/app/x" }) },
  { method: "POST", path: "/login", headers: { ...ip(5), ...FORM }, body: form({ ...good, next: "//evil.example/x" }) },
  { method: "POST", path: "/login", headers: { ...ip(5), ...FORM }, body: `user=max&user=max&password=${encodeURIComponent(PASSWORD)}` },
  { method: "POST", path: "/logout", headers: { ...ip(6), cookie: `altim_session=${tsCookie}` } },
  { method: "POST", path: "/logout", headers: { ...ip(6), origin: ORIGIN, cookie: `altim_session=${tsCookie}` } },
  { method: "GET", path: "/login", headers: { ...ip(6), cookie: `altim_session=${tsCookie}` } },
  // Lock-out: 5 failures, then even the right password is refused; another address is not affected.
  ...Array.from({ length: 5 }, () => ({ method: "POST", path: "/login", headers: { ...ip(7), ...FORM }, body: form({ ...good, password: "faux" }) })),
  { method: "POST", path: "/login", headers: { ...ip(7), ...FORM }, body: form(good) },
  { method: "POST", path: "/login", headers: { ...ip(8), ...FORM }, body: form({ ...good, password: "faux" }) },
  // Rate limit of POST /login: 10 per minute per address (the 7th already sent 6).
  ...Array.from({ length: 5 }, () => ({ method: "POST", path: "/login", headers: { ...ip(7), ...FORM }, body: form(good) })),
];

const KEEP = ["content-type", "content-length", "location", "cache-control", "x-robots-tag", "vary", "etag", "retry-after", "set-cookie"];

function send(port: number, r: Req, extra: Record<string, string>) {
  return new Promise<{ status: number; headers: Record<string, string>; body: string }>((resolve, reject) => {
    const req = request({ host: "127.0.0.1", port, method: r.method, path: r.path, headers: { host: HOST, ...r.headers, ...extra } }, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (c) => (body += c));
      res.on("end", () => {
        const headers: Record<string, string> = {};
        for (const k of KEEP) {
          const v = res.headers[k];
          if (v !== undefined) headers[k] = Array.isArray(v) ? v.join("\n") : String(v);
        }
        resolve({ status: res.statusCode!, headers, body });
      });
    });
    req.on("error", reject);
    if (r.body !== undefined) req.end(r.body);
    else req.end();
  });
}

const server = createApp({ auth: { cfg, production: true } }).listen(0);
await new Promise((r) => server.once("listening", r));
const port = (server.address() as AddressInfo).port;
const exchanges: { req: Req; res: Awaited<ReturnType<typeof send>> }[] = [];
for (const r of reqs) {
  const extra = r.etagOf !== undefined ? { "if-none-match": exchanges[r.etagOf]!.res.headers.etag! } : {};
  const res = await send(port, r, extra);
  exchanges.push({ req: r, res });
}
server.close();

writeFileSync(
  join(import.meta.dir, "../tests/golden/auth.json"),
  JSON.stringify({ user: cfg.user, password: PASSWORD, passwordHash: HASH, sessionSecret: cfg.sessionSecret, apiToken: cfg.apiToken, tsCookie, pages, exchanges }, null, 1),
);
console.log("auth", exchanges.length, "échanges");
