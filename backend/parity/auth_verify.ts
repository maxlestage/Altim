/**
 * Does the TypeScript accept this session cookie? `bun parity/auth_verify.ts <cookie>` → "valide" | "refusé".
 * Same configuration as parity/auth.ts (used by tests/auth.rs to check cookies signed by the Rust port).
 */
import { readSession, type AuthConfig } from "../../web/server/auth";

const cfg: AuthConfig = { user: "max", passwordHash: "$argon2id$", totpSecret: null, sessionSecret: "s".repeat(128), apiToken: null };
console.log(readSession(cfg, process.argv[2]) ? "valide" : "refusé");
