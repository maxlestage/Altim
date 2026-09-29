/**
 * EUR/USD rate of the app (/api/fx: Yahoo Finance, else ECB, else Frankfurter), refreshed every 10 minutes. The last
 * valid rate is kept in this browser (7 days at most, shown with its source and time) so a server hiccup does not
 * switch the amounts back to dollars; older, or never read, the app says "taux indisponible" and shows dollars.
 */
import { useSyncExternalStore } from "react";
import type { FxRate } from "../money";
import { fxApi } from "./api";

export const FX_KEY = "altim.fx.v1";
export const FX_REFRESH_MS = 10 * 60_000;
const KEEP_MS = 7 * 86_400_000;

/** `/api/fx` body (rate null with `error` when no source answered). */
export type FxResponse = {
  base: "USD"; quote: "EUR"; rate: number | null; usdPerEur: number | null; time: number | null; source: string | null;
  fetchedAt: number | null; stale: boolean; error?: string;
};

export type FxState = { fx: FxRate | null; error: string | null; loading: boolean };

/** A valid rate from the response, else null (never a default). */
export function parseFx(r: unknown): FxRate | null {
  const x = r as FxResponse | null;
  if (!x || typeof x.rate !== "number" || !Number.isFinite(x.rate) || x.rate <= 0 || x.rate > 5) return null;
  if (typeof x.time !== "number" || typeof x.source !== "string") return null;
  return {
    rate: x.rate, usdPerEur: typeof x.usdPerEur === "number" ? x.usdPerEur : 1 / x.rate, time: x.time, source: x.source,
    fetchedAt: typeof x.fetchedAt === "number" ? x.fetchedAt : Date.now(), stale: !!x.stale,
  };
}

/** The saved rate when younger than 7 days (read by Altim, not quoted: a weekend's Friday close stays valid). */
export function savedFx(raw: string | null, now = Date.now()): FxRate | null {
  try {
    const r = parseFx(raw ? JSON.parse(raw) : null);
    return r && now - r.fetchedAt < KEEP_MS ? { ...r, stale: true } : null;
  } catch {
    return null;
  }
}

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

let state: FxState = { fx: savedFx(storage()?.getItem(FX_KEY) ?? null), error: null, loading: false };
const listeners = new Set<() => void>();
const emit = (s: FxState) => {
  state = s;
  listeners.forEach((l) => l());
};

export async function refreshFx(): Promise<void> {
  if (state.loading) return;
  emit({ ...state, loading: true });
  try {
    const body = await fxApi();
    const fx = parseFx(body);
    if (fx) {
      try {
        storage()?.setItem(FX_KEY, JSON.stringify(body));
      } catch {}
      emit({ fx, error: null, loading: false });
    } else emit({ ...state, error: body.error ?? "taux indisponible", loading: false });
  } catch (e) {
    emit({ ...state, error: `taux indisponible (${(e as Error).message})`, loading: false });
  }
}

let timer: ReturnType<typeof setInterval> | null = null;
/** Starts the refresh loop once (first read now, then every 10 minutes). */
export function startFx() {
  if (timer || typeof window === "undefined") return;
  void refreshFx();
  timer = setInterval(() => void refreshFx(), FX_REFRESH_MS);
}

export function useFx(): FxState {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => state,
    () => state,
  );
}
