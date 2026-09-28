/**
 * Last "positions devenues dangereuses" computed on Mes avoirs, kept in this browser so the Radar can show them
 * with the time they were measured (the web app has no notifications). Versioned key, validated on read.
 */
import type { Danger } from "../engine/portfolio-risk";

const KEY = "altim.dangers.v1";
export interface DangerState { version: 1; at: number; items: Danger[] }

type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;
const local = (): Storage | null => {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
};

const CODES = ["stop_broken", "near_stop", "loss_over_risk"];
const isDanger = (v: unknown): v is Danger => {
  const x = v as Danger;
  return !!x && typeof x.id === "string" && typeof x.symbol === "string" && (x.kind === "crypto" || x.kind === "stock") && typeof x.name === "string" &&
    Array.isArray(x.reasons) && x.reasons.every((r) => !!r && CODES.includes(r.code) && typeof r.text === "string");
};

export function parseDangers(raw: string | null): DangerState | null {
  try {
    const p = raw ? (JSON.parse(raw) as DangerState) : null;
    if (!p || p.version !== 1 || !Number.isFinite(p.at) || !Array.isArray(p.items)) return null;
    return { version: 1, at: p.at, items: p.items.filter(isDanger) };
  } catch {
    return null;
  }
}

export function saveDangers(items: Danger[], now = Date.now(), s: Storage | null = local()) {
  try {
    s?.setItem(KEY, JSON.stringify({ version: 1, at: now, items }));
  } catch {
    /* quota or private browsing: not remembered */
  }
}

export function readDangers(s: Storage | null = local()): DangerState | null {
  try {
    return parseDangers(s?.getItem(KEY) ?? null);
  } catch {
    return null;
  }
}
