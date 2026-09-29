/**
 * Checks of the Alertes screen while an Altim tab is open (every 5 minutes, and at start): buy alerts of the radar
 * and holdings (/api/alerts, same rule as the phones), price alerts (consensus quotes), important news. New ones
 * are shown with the browser's Notification API when allowed, and written to the alerts journal. The browser cannot
 * run this when every Altim tab is closed (no push server): the phones' background checks have no web equivalent.
 */
import { api, type BuyAlert } from "./api";
import { navigate } from "./router";
import { addToJournal, evaluateTargets, getAlerts, newBuyAlerts, newNews, setAlerts, targetLabel, type AlertEntry } from "./alerts-store";
import { getAppState, getHoldingAssets } from "./store";
import { currencySymbol, moneyPrice, type Currency } from "../money";
import type { Kind } from "../engine/reliability";

export const CHECK_MS = 5 * 60_000;
const DISCLAIMER = " Conseil indicatif : Altim ne passe aucun ordre.";

export const notificationsSupported = () => typeof window !== "undefined" && "Notification" in window;
export const permission = (): NotificationPermission | "unsupported" => (notificationsSupported() ? Notification.permission : "unsupported");

/** Asks the browser's permission (only on a click: browsers refuse it otherwise). */
export async function askPermission(): Promise<NotificationPermission | "unsupported"> {
  if (!notificationsSupported()) return "unsupported";
  try {
    return await Notification.requestPermission();
  } catch {
    return Notification.permission;
  }
}

function show(title: string, body: string, tag: string, open: string | null) {
  if (permission() !== "granted") return;
  try {
    const n = new Notification(title, { body, tag, icon: "/logo.svg" });
    n.onclick = () => {
      window.focus();
      if (open) navigate(open);
      n.close();
    };
  } catch {
    // Some mobile browsers only allow notifications from a service worker: the journal still keeps them.
  }
}

const assetPath = (a: { symbol: string; kind: Kind }) => `/app/actif/${a.kind}/${a.symbol}`;
const uniq = <T extends { symbol: string; kind: Kind }>(list: T[]) => [...new Map(list.map((a) => [`${a.kind}:${a.symbol}`, a])).values()];
/** A threshold shown in its own currency ("80 000,00 €"). */
export const thresholdText = (v: number, c: Currency) => `${v.toLocaleString("fr-FR", { minimumFractionDigits: v >= 1 ? 2 : 4, maximumFractionDigits: v >= 1 ? 2 : 8 })} ${currencySymbol(c)}`;

/** Up to 3 buy alerts: one notification each; beyond, a single summary (the first check can find many at once). */
function postBuys(alerts: BuyAlert[]) {
  if (alerts.length > 3) {
    const strong = alerts.filter((a) => a.strong).map((a) => a.symbol);
    const others = alerts.filter((a) => !a.strong).map((a) => a.symbol);
    const body = `${strong.length ? `Achat conseillé : ${strong.join(", ")}. ` : ""}${others.length ? `Achat possible : ${others.join(", ")}. ` : ""}Ouvrez Altim pour le détail de chacun.`;
    show(`${alerts.length} actifs achetables`, body + DISCLAIMER, "altim.summary", "/app/alertes");
  } else for (const a of alerts) show(a.title ?? `${a.symbol} : achetable`, (a.body ?? "") + DISCLAIMER, `altim.${a.kind}:${a.symbol}`, assetPath(a));
}

let running = false;

/** One check; returns an error text when the server could not be reached (the next check retries). */
export async function runCheck(now = Date.now()): Promise<string | null> {
  if (running) return null;
  running = true;
  try {
    const s = getAlerts();
    const mine = uniq([...getAppState().watchlist, ...getHoldingAssets()]);
    const entries: AlertEntry[] = [];
    let tracker = s.tracker;
    let targets = s.targets;
    let newsSeen = s.newsSeen;
    const armed = uniq(s.targets.filter((t) => t.triggered == null));
    if (armed.length) {
      const quotes = await api.quotes(armed);
      const r = evaluateTargets(s.targets, Object.fromEntries(quotes.map((q) => [`${q.kind}:${q.symbol}`, q.price])), now);
      targets = r.targets;
      for (const { target: t, price } of r.fired) {
        const label = targetLabel(t, thresholdText);
        entries.push({ id: `${t.id}:${now}`, symbol: t.symbol, kind: t.kind, name: t.name, source: "target", title: `${t.symbol} : ${label.toLowerCase()}`, price, date: now });
        show(`${t.symbol} : alerte de prix atteinte`, `${label}. Prix actuel ${moneyPrice(price)}. Réarmez-la dans Alertes si besoin.`, `altim.target.${t.id}`, assetPath(t));
      }
    }
    if (s.notify.buy && mine.length) {
      const items = await api.alerts(mine);
      const r = newBuyAlerts(s.tracker, items, s.notify.strongOnly, now);
      tracker = r.tracker;
      for (const a of r.fresh) {
        if (a.price != null) entries.push({ id: `${a.kind}:${a.symbol}:${now}`, symbol: a.symbol, kind: a.kind, name: a.name, source: a.strong ? "strongBuy" : "buy", title: a.title ?? a.symbol, price: a.price, date: now });
      }
      postBuys(r.fresh);
    }
    if (s.notify.news) {
      // A feed that fails does not stop the buy and price alerts.
      const report = await api.news(mine).catch(() => null);
      if (report) {
        const r = newNews(s.newsSeen, report.items, new Set(mine.map((a) => `${a.kind}:${a.symbol}`)), now);
        newsSeen = r.seen;
        const list = r.fresh.length <= 2 ? r.fresh : [];
        for (const n of list) {
          const who = n.assets.map((a) => a.split(":").pop()).join(", ");
          show(n.alert ? "Alerte actualité" : `Actualité : ${who}`, `${n.title} (${n.source}${n.alsoIn.length ? ` +${n.alsoIn.length}` : ""})`, `altim.news.${n.id}`, "/app/actu");
        }
        if (r.fresh.length > 2) show(`${r.fresh.length} actualités importantes`, r.fresh.slice(0, 3).map((n) => n.title).join(" · "), "altim.news.summary", "/app/actu");
      }
    }
    setAlerts((cur) => ({ tracker, targets: mergeTargets(cur.targets, targets), newsSeen, journal: entries.length ? addToJournal(entries, cur.journal) : cur.journal, lastCheck: now }));
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : "vérification impossible";
  } finally {
    running = false;
  }
}

/** Targets changed during the check (added, removed, re-armed by the user) are kept; only the triggers are applied. */
function mergeTargets(current: import("./alerts-store").PriceTarget[], checked: import("./alerts-store").PriceTarget[]) {
  const done = new Map(checked.filter((t) => t.triggered != null).map((t) => [t.id, t.triggered]));
  return current.map((t) => (t.triggered == null && done.has(t.id) ? { ...t, triggered: done.get(t.id)! } : t));
}

let timer: ReturnType<typeof setInterval> | null = null;
/** Starts the checks once per tab (they only query the server when something is to check). */
export function startChecks() {
  if (timer || typeof window === "undefined") return;
  const tick = () => {
    const s = getAlerts();
    if (s.notify.buy || s.notify.news || s.targets.some((t) => t.triggered == null)) void runCheck();
  };
  setTimeout(tick, 5_000);
  timer = setInterval(tick, CHECK_MS);
}
