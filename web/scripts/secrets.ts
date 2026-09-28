/**
 * Generates the private-access values to put in Heroku → Settings → Config Vars (checked by backend/src/auth.rs).
 *   bun run secrets                     random password (shown once, only its hash goes to Heroku)
 *   bun run secrets -- --user max       choose the login name
 *   bun run secrets -- --password "…"   hash a password of your choice (16 characters minimum)
 * Nothing is written to disk: copy the values, keep the password in a password manager.
 */
import { randomBytes } from "node:crypto";

/** RFC 4648 base32 without padding (the 2FA secret, as authenticator apps expect it). */
const B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
function toBase32(buf: Buffer): string {
  let bits = "", out = "";
  for (const b of buf) bits += b.toString(2).padStart(8, "0");
  for (let i = 0; i < bits.length; i += 5) out += B32[parseInt(bits.slice(i, i + 5).padEnd(5, "0"), 2)];
  return out;
}

const arg = (name: string) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
};

// Password: 24 characters from a 56-symbol alphabet without look-alikes (≈ 139 bits of entropy).
const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
function password(length = 24): string {
  let out = "";
  while (out.length < length) {
    for (const b of randomBytes(64)) if (b < 224 && out.length < length) out += ALPHABET[b % 56]; // 224 = 4 × 56: no modulo bias
  }
  return out.match(/.{1,6}/g)!.join("-");
}

const user = arg("user") ?? "max";
const chosen = arg("password");
if (chosen !== undefined && chosen.length < 16) {
  console.error("Mot de passe trop court : 16 caractères minimum.");
  process.exit(1);
}
const pass = chosen ?? password();
const hash = await Bun.password.hash(pass, { algorithm: "argon2id", memoryCost: 65536, timeCost: 3 });
const totp = toBase32(randomBytes(20)); // 160 bits, RFC 4226 recommendation
const session = randomBytes(64).toString("hex");
const api = `altim_${randomBytes(32).toString("base64url")}`;
const uri = `otpauth://totp/Altim:${encodeURIComponent(user)}?secret=${totp}&issuer=Altim&algorithm=SHA1&digits=6&period=30`;

console.log(`
À mettre dans Heroku → Settings → Config Vars :

  ALTIM_USER            = ${user}
  ALTIM_PASSWORD_HASH   = ${hash}
  ALTIM_TOTP_SECRET     = ${totp}
  ALTIM_SESSION_SECRET  = ${session}
  ALTIM_API_TOKEN       = ${api}

À garder pour vous (gestionnaire de mots de passe), jamais dans Heroku ni dans le code :

  Mot de passe          : ${pass}
  Application 2FA       : ajoutez le secret ${totp} (ou ce lien : ${uri})
  Jeton pour vos bots   : Authorization: Bearer ${api}
`);
