/**
 * Serveur de production (Heroku) : fichiers statiques de `dist/` et `public/`,
 * compression gzip, en-têtes de sécurité et cache long pour les fichiers versionnés.
 */
import { join, normalize } from "node:path";

const port = Number(process.env.PORT ?? 3000);
const root = import.meta.dir;
const dirs = [join(root, "dist"), join(root, "public")];
const compressible = /\.(html|js|css|svg|json|txt)$/;
const gzipCache = new Map<string, Uint8Array>();

const securityHeaders: Record<string, string> = {
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=()",
  "Strict-Transport-Security": "max-age=63072000; includeSubDomains",
  "Content-Security-Policy": [
    "default-src 'self'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com",
    "font-src https://fonts.gstatic.com",
    "img-src 'self' data:",
    "connect-src 'self' https://api.binance.com https://api.coingecko.com",
  ].join("; "),
};

async function findFile(pathname: string) {
  const safe = normalize(decodeURIComponent(pathname)).replace(/^(\.\.[/\\])+/, "");
  for (const dir of dirs) {
    const candidate = join(dir, safe);
    if (!candidate.startsWith(dir)) continue;
    const file = Bun.file(candidate);
    if ((await file.exists()) && !candidate.endsWith("/")) return file;
  }
  return null;
}

const server = Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url);
    if (url.pathname === "/health") return new Response("ok");

    // Redirection HTTPS derrière le routeur Heroku.
    if (req.headers.get("x-forwarded-proto") === "http") {
      url.protocol = "https:";
      return Response.redirect(url.toString(), 301);
    }

    let file = url.pathname === "/" ? null : await findFile(url.pathname);
    const isIndex = !file;
    if (!file) file = Bun.file(join(dirs[0]!, "index.html"));
    if (!(await file.exists())) return new Response("Build manquant : lancez `bun run build`.", { status: 500 });

    const headers = new Headers(securityHeaders);
    headers.set("Content-Type", file.type);
    headers.set(
      "Cache-Control",
      isIndex ? "no-cache" : /-[a-z0-9]{8,}\.(js|css)$/.test(url.pathname) ? "public, max-age=31536000, immutable" : "public, max-age=3600",
    );

    const name = isIndex ? "index.html" : url.pathname;
    if (compressible.test(name) && req.headers.get("accept-encoding")?.includes("gzip")) {
      headers.set("Content-Encoding", "gzip");
      headers.set("Vary", "Accept-Encoding");
      const key = file.name ?? name;
      let body = gzipCache.get(key);
      if (!body) {
        body = Bun.gzipSync(new Uint8Array(await file.arrayBuffer()));
        gzipCache.set(key, body);
      }
      return new Response(body as unknown as BodyInit, { headers });
    }
    return new Response(file, { headers });
  },
});

console.log(`Altim web en ligne sur http://localhost:${server.port}`);
