import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { api, type BuyAlert } from "./api";
import { onLink } from "./router";
import { assetKey, getHoldingAssets, useAppState, useHoldings } from "./store";
import { changeSince, journalSummary, rearm, setAlerts, targetLabel, useAlerts, type PriceTarget } from "./alerts-store";
import { askPermission, CHECK_MS, notificationsSupported, permission, runCheck, thresholdText } from "./notify";
import { convert, currencySymbol, displayCurrency, moneyPrice, toDisplay } from "../money";
import { useLive } from "./live";
import { Change, Segmented } from "./ui";
import type { Kind } from "../engine/reliability";

const frTime = (t: number) => new Date(t).toLocaleString("fr-FR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
const uniq = <T extends { symbol: string; kind: Kind }>(list: T[]) => [...new Map(list.map((a) => [assetKey(a), a])).values()];

/**
 * « Alertes » (the phones' Alerts tab): what can be bought now (same rule as the notifications), the price alerts
 * chosen on the asset pages, the notifications of this browser, and the journal of the alerts with what each gave.
 */
export function Alerts() {
  const s = useAlerts();
  const { watchlist } = useAppState();
  const { holdings } = useHoldings();
  const mine = useMemo(() => uniq([...watchlist, ...getHoldingAssets()]), [watchlist, holdings]);
  const [buyable, setBuyable] = useState<BuyAlert[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [perm, setPerm] = useState(permission());
  const [checkError, setCheckError] = useState<string | null>(null);
  const tracked = useMemo(() => uniq([...s.targets, ...s.journal]), [s.targets, s.journal]);
  const live = useLive(tracked);
  const [quotes, setQuotes] = useState<Record<string, number>>({});
  const price = (a: { symbol: string; kind: Kind }) => live.ticks[assetKey(a)]?.price ?? quotes[assetKey(a)] ?? null;

  useEffect(() => {
    if (!mine.length) return setBuyable([]);
    let alive = true;
    api.alerts(mine)
      .then((a) => alive && (setBuyable(a.filter((x) => x.buy).sort((x, y) => Number(!!y.strong) - Number(!!x.strong))), setError(null)))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Vérification impossible"));
    return () => {
      alive = false;
    };
  }, [mine.map(assetKey).join(",")]);

  useEffect(() => {
    if (!tracked.length) return;
    api.quotes(tracked).then((q) => setQuotes(Object.fromEntries(q.map((x) => [`${x.kind}:${x.symbol}`, x.price])))).catch(() => {});
  }, [tracked.map(assetKey).join(",")]);

  const prices = useMemo(() => {
    const p: Record<string, number> = { ...quotes };
    for (const a of tracked) {
      const t = live.ticks[assetKey(a)];
      if (t) p[assetKey(a)] = t.price;
    }
    return p;
  }, [quotes, live.ticks, tracked]);
  const summary = journalSummary(s.journal, prices, Date.now());

  const turnOn = async (key: "buy" | "news") => {
    const next = !s.notify[key];
    if (next && perm !== "granted") setPerm(await askPermission());
    setAlerts((cur) => ({ notify: { ...cur.notify, [key]: next } }));
    if (next) setCheckError(await runCheck());
  };

  return (
    <section className="app-screen">
      <a href="/app" onClick={onLink} className="back">← Radar</a>
      <div className="screen-top"><h1>Alertes</h1></div>

      <div className="card">
        <h2 className="card-title">Notifications de ce navigateur</h2>
        {!notificationsSupported() ? (
          <p className="notice warn small">Ce navigateur n'affiche pas de notifications : les alertes restent visibles ici et dans le journal.</p>
        ) : perm === "denied" ? (
          <p className="notice warn small">Notifications refusées pour Altim : autorisez-les dans les réglages du site de votre navigateur.</p>
        ) : null}
        <label className="check-row">
          <input type="checkbox" checked={s.notify.buy} onChange={() => void turnOn("buy")} />
          <span>Me prévenir quand je peux acheter (radar et avoirs)</span>
        </label>
        {s.notify.buy && (
          <label className="check-row">
            <input type="checkbox" checked={s.notify.strongOnly} onChange={() => setAlerts((cur) => ({ notify: { ...cur.notify, strongOnly: !cur.notify.strongOnly } }))} />
            <span>Seulement les achats conseillés (signal + zone)</span>
          </label>
        )}
        <label className="check-row">
          <input type="checkbox" checked={s.notify.news} onChange={() => void turnOn("news")} />
          <span>Me prévenir des actualités importantes</span>
        </label>
        <p className="muted small">
          Vérifié toutes les {CHECK_MS / 60_000} minutes tant qu'un onglet Altim est ouvert (même en arrière-plan) : un navigateur fermé ne peut pas prévenir,
          contrairement aux applications iPhone et Android. Les alertes de prix sont vérifiées de la même façon.
          {s.lastCheck ? ` Dernière vérification : ${frTime(s.lastCheck)}.` : ""}
        </p>
        {checkError && <p className="notice warn small">⚠ Vérification impossible : {checkError}. Nouvel essai à la prochaine vérification.</p>}
      </div>

      <div className="card">
        <h2 className="card-title">Achetables maintenant</h2>
        {error ? <p className="notice warn small">⚠ {error}</p>
          : !buyable ? <p className="muted small">Vérification du radar et des avoirs…</p>
          : !buyable.length ? <p className="muted small">Rien d'achetable pour l'instant : aucune décision complète ne dit ACHETER ou ZONE D'ACHAT.</p>
          : (
            <ul className="alert-list">
              {buyable.map((a) => (
                <li key={assetKey(a)}>
                  <a href={`/app/actif/${a.kind}/${a.symbol}`} onClick={onLink} className="alert-row">
                    <span><b className="mono">{a.symbol}</b> <span className={`badge ${a.strong ? "buy" : "hold"}`}>{a.strong ? "ACHETER" : "ZONE D'ACHAT"}</span></span>
                    <span className="mono">{a.price != null ? moneyPrice(a.price) : "—"}</span>
                  </a>
                  <ul className="reasons small">{[...(a.reasons ?? []), ...(a.cautions ?? [])].map((r) => <li key={r}>{r}</li>)}</ul>
                </li>
              ))}
            </ul>
          )}
      </div>

      <div className="card">
        <h2 className="card-title">Alertes de prix</h2>
        {!s.targets.length && <p className="muted small">Aucune alerte de prix. Sur la fiche d'un actif, bouton « Alerte de prix » : « préviens-moi si BTC passe sous 80 000 {currencySymbol()} ».</p>}
        <ul className="alert-list">
          {s.targets.map((t) => (
            <li key={t.id}>
              <TargetRow t={t} price={price(t)} />
              <div className="row-actions">
                {t.triggered != null && <button className="link-btn" onClick={() => setAlerts((cur) => ({ targets: cur.targets.map((x) => (x.id === t.id ? rearm(x, price(t)) : x)) }))}>Réarmer</button>}
                <button className="link-btn danger" onClick={() => setAlerts((cur) => ({ targets: cur.targets.filter((x) => x.id !== t.id) }))}>Supprimer</button>
              </div>
            </li>
          ))}
        </ul>
      </div>

      <div className="card">
        <h2 className="card-title">Journal des alertes</h2>
        {summary && (
          <>
            <p className="small">
              Depuis leur envoi, {summary.up} alerte(s) d'achat sur {summary.count} sont en hausse ({Math.round(summary.upShare)} %), variation moyenne{" "}
              <Change value={summary.average} />.
            </p>
            <p className="muted small">Mesure simple depuis chaque alerte, sans frais ni règle de sortie, sur vos propres alertes : une indication, pas une preuve. Les alertes de moins d'une heure et vos alertes de prix ne comptent pas.</p>
          </>
        )}
        {!s.journal.length && <p className="muted small">Les alertes reçues s'afficheront ici, avec ce qu'elles ont donné depuis.</p>}
        <ul className="alert-list">
          {s.journal.slice(0, 100).map((e) => (
            <li key={e.id}>
              <a href={`/app/actif/${e.kind}/${e.symbol}`} onClick={onLink} className="alert-row">
                <span className="small">{e.title}<br /><small className="muted">{frTime(e.date)} · {moneyPrice(e.price)}</small></span>
                <Change value={changeSince(e, price(e))} />
              </a>
            </li>
          ))}
        </ul>
        {s.journal.length > 0 && <button className="link-btn danger" onClick={() => confirm("Vider le journal des alertes ?") && setAlerts({ journal: [] })}>Vider le journal</button>}
      </div>
    </section>
  );
}

function TargetRow({ t, price }: { t: PriceTarget; price: number | null }) {
  const label = targetLabel(t, thresholdText);
  // Compared in the alert's own currency (the dollar price converted at the current rate).
  const inCur = price != null ? convert(price, "USD", t.currency) : NaN;
  const shown = Number.isFinite(inCur) ? inCur : null;
  return (
    <a href={`/app/actif/${t.kind}/${t.symbol}`} onClick={onLink} className="alert-row">
      <span className="small">
        <b>{t.symbol}</b> · {label.toLowerCase()}
        <br />
        <small className={t.triggered != null ? "up" : "muted"}>
          {t.triggered != null
            ? `Atteinte le ${frTime(t.triggered)}`
            : shown != null && t.move != null
              ? `Prix actuel ${thresholdText(shown, t.currency)} · variation ${((shown / t.price - 1) * 100).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} % sur ±${t.move} %`
              : shown != null
                ? `Prix actuel ${thresholdText(shown, t.currency)} · encore ${((t.price / shown - 1) * 100).toLocaleString("fr-FR", { maximumFractionDigits: 1, signDisplay: "always" })} %`
                : "En attente"}
        </small>
      </span>
    </a>
  );
}

/** « Alerte de prix » on an asset page: above / below a price, or a move of ±x %, typed in the display currency. */
export function PriceAlertButton({ symbol, kind, name, price }: { symbol: string; kind: Kind; name: string; price: number | null }) {
  const [open, setOpen] = useState(false);
  const { targets } = useAlerts();
  const count = targets.filter((t) => t.symbol === symbol && t.kind === kind && t.triggered == null).length;
  return (
    <>
      <button className="btn btn-ghost btn-small" onClick={() => setOpen(true)} aria-haspopup="dialog">
        🔔 Alerte de prix{count ? ` · ${count}` : ""}
      </button>
      {open && <PriceAlertSheet symbol={symbol} kind={kind} name={name} price={price} onClose={() => setOpen(false)} />}
    </>
  );
}

type Mode = "below" | "above" | "move";

function PriceAlertSheet({ symbol, kind, name, price, onClose }: { symbol: string; kind: Kind; name: string; price: number | null; onClose: () => void }) {
  const cur = displayCurrency();
  const [mode, setMode] = useState<Mode>("below");
  const [text, setText] = useState("");
  const [done, setDone] = useState(false);
  const v = Number(text.replace(/[\s  $€%]/g, "").replace(",", "."));
  const shownPrice = price != null ? toDisplay(price) : null;
  const valid = Number.isFinite(v) && v > 0 && (mode !== "move" || (v <= 100 && shownPrice != null));
  const save = async () => {
    if (!valid) return;
    const t: PriceTarget = {
      id: typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `t-${Date.now()}`,
      symbol, kind, name, above: mode === "above", currency: cur, created: Date.now(), triggered: null,
      price: mode === "move" ? shownPrice! : v, move: mode === "move" ? v : null,
    };
    setAlerts((s) => ({ targets: [...s.targets, t] }));
    if (permission() === "default") await askPermission();
    setDone(true);
  };
  return createPortal(
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="price-alert-title" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="sheet-head"><h2 id="price-alert-title">Alerte de prix · {symbol}</h2></div>
        {price != null && <p className="kv small"><span>Prix actuel</span><b className="mono">{moneyPrice(price)}</b></p>}
        <Segmented<Mode> label="Type d'alerte" value={mode} onChange={setMode} options={[["below", "En dessous"], ["above", "Au-dessus"], ["move", "Variation"]]} />
        <label className="field">
          <span>{mode === "move" ? "Variation (%, hausse ou baisse)" : `Prix (${currencySymbol(cur)})`}</span>
          <input inputMode="decimal" autoFocus value={text} placeholder={mode === "move" ? "ex. 5" : shownPrice ? thresholdText(shownPrice, cur).replace(/ [€$]$/, "") : ""} onChange={(e) => setText(e.target.value)} />
        </label>
        <p className="muted small">
          Vérifiée toutes les 5 minutes tant qu'un onglet Altim est ouvert, et notifiée si vous l'autorisez ; visible dans Alertes. Un seuil en € est comparé au cours converti au taux du jour.
        </p>
        {done ? (
          <p className="notice ok small" role="status">✔ Alerte enregistrée. <a href="/app/alertes" onClick={onLink} className="link">Voir les alertes</a></p>
        ) : (
          <button className="btn" disabled={!valid} onClick={() => void save()}>Créer l'alerte</button>
        )}
        <button className="btn btn-ghost" onClick={onClose}>{done ? "Fermer" : "Annuler"}</button>
      </div>
    </div>,
    document.body,
  );
}
