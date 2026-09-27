/**
 * Live prices (Server-Sent Events from /api/live): the page never freezes on a stale price.
 * One stream per 20 assets, closed while the tab is hidden, reopened (with the last prices) when it comes back.
 */
import { useEffect, useMemo, useState } from "react";
import type { Kind } from "../engine/reliability";

export type LiveTick = {
  symbol: string; kind: Kind; price: number; change: number | null;
  agreeing: number; total: number; sources: string[]; time: number; market?: "open" | "closed";
  /** Direction of the last move, for the flash. */
  dir?: "up" | "down";
  /** Reception time (the flash is keyed on it). */
  seq: number;
};
export type LiveStatus = "connecting" | "live" | "offline";

const key = (a: { symbol: string; kind: Kind }) => `${a.kind}:${a.symbol}`;
let seq = 0;

export function useLive(items: { symbol: string; kind: Kind }[]): { ticks: Record<string, LiveTick>; status: LiveStatus; last: number | null } {
  const [ticks, setTicks] = useState<Record<string, LiveTick>>({});
  const [status, setStatus] = useState<LiveStatus>("connecting");
  const [last, setLast] = useState<number | null>(null);
  // Stable identity: same assets → same streams, whatever the order of re-renders.
  const list = useMemo(() => [...new Set(items.map(key))].sort().join(","), [items]);

  useEffect(() => {
    if (!list || typeof EventSource === "undefined") return;
    const keys = list.split(",");
    const chunks: string[][] = [];
    for (let i = 0; i < keys.length; i += 20) chunks.push(keys.slice(i, i + 20));
    let sources: EventSource[] = [];

    const open = () => {
      close();
      setStatus("connecting");
      sources = chunks.map((c) => {
        const es = new EventSource(`/api/live?symbols=${encodeURIComponent(c.map((k) => `${k.split(":")[1]}:${k.split(":")[0]}`).join(","))}`);
        es.onopen = () => setStatus("live");
        es.onerror = () => setStatus(es.readyState === EventSource.CLOSED ? "offline" : "connecting");
        es.onmessage = (e) => {
          let t: LiveTick;
          try { t = JSON.parse(e.data); } catch { return; }
          if (!Number.isFinite(t.price)) return;
          setStatus("live");
          setLast(Date.now());
          setTicks((prev) => {
            const k = key(t);
            const before = prev[k]?.price;
            const dir = before === undefined || before === t.price ? prev[k]?.dir : t.price > before ? "up" : "down";
            return { ...prev, [k]: { ...t, dir, seq: before === t.price ? prev[k]!.seq : ++seq } };
          });
        };
        return es;
      });
    };
    const close = () => {
      sources.forEach((s) => s.close());
      sources = [];
    };
    const onVisibility = () => (document.visibilityState === "visible" ? open() : close());

    open();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      close();
    };
  }, [list]);

  return { ticks, status, last };
}

/** "EN DIRECT" pill: pulsing dot while ticks arrive, time of the last one. */
export function LiveBadge({ status, last }: { status: LiveStatus; last: number | null }) {
  const [, force] = useState(0);
  useEffect(() => {
    const id = setInterval(() => force((x) => x + 1), 1000);
    return () => clearInterval(id);
  }, []);
  const label = status === "live" ? "EN DIRECT" : status === "connecting" ? "CONNEXION…" : "HORS LIGNE";
  return (
    <span className={`live-badge ${status}`} role="status" aria-live="off">
      <i aria-hidden /> {label}
      {last && status === "live" ? <small> · {new Date(last).toLocaleTimeString("fr-FR")}</small> : null}
    </span>
  );
}

/** Price that flashes green / red on each move (the key restarts the animation). */
export function LivePrice({ tick, fallback, format }: { tick?: LiveTick; fallback: number | null | undefined; format: (v: number) => string }) {
  const v = tick?.price ?? fallback;
  if (v == null || !Number.isFinite(v)) return <>—</>;
  return <span key={tick?.seq ?? 0} className={`live-price${tick?.dir ? ` flash-${tick.dir}` : ""}`}>{format(v)}</span>;
}
