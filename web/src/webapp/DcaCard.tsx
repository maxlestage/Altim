import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { Segmented } from "./ui";
import { currencySymbol, displayCurrency, fromDisplay, money, moneyPrice } from "../money";
import { simulateDca } from "../engine/dca";
import type { Close } from "../engine/history";

const usd = (v: number) => money(v, 0, 0);
const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const date = (t: number) => new Date(t).toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });

/** One purchase repeats nothing: its "next purchase" is far beyond any period. */
const ONCE = 100_000;

/**
 * "If I had invested 1 000 $": one purchase by default (the user does not want to spend every month), regular
 * purchases as an option, replayed on the real daily closes of the asset.
 */
export function DcaCard({ symbol, kind }: { symbol: string; kind: "crypto" | "stock" }) {
  const [amountText, setAmountText] = useState("1000");
  const [every, setEvery] = useState<"once" | "7" | "30">("once");
  const [period, setPeriod] = useState<"182" | "365" | "730">("365");
  const once = every === "once";
  const [closes, setCloses] = useState<Close[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setCloses(null);
    api.history([{ symbol, kind }], period === "730" ? 730 : 365)
      .then((r) => alive && (setCloses(r.series.find((s) => s.symbol === symbol && s.kind === kind)?.closes ?? []), setError(null)))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Historique indisponible"));
    return () => {
      alive = false;
    };
  }, [symbol, kind, period]);

  // Typed in the display currency, replayed in dollars on the dollar closes.
  const amount = fromDisplay(Number(amountText.replace(/\s/g, "").replace(",", ".")));
  const r = useMemo(() => (closes && amount > 0 ? simulateDca(closes, amount, once ? ONCE : Number(every), Number(period)) : null), [closes, amount, every, period]);

  return (
    <div className="card dca-card">
      <h2 className="card-title">Si j'avais investi</h2>
      <label className="field">
        <span>{once ? `Montant investi (${currencySymbol()})` : `Montant par achat (${currencySymbol()})`}</span>
        <input inputMode="decimal" value={amountText} onChange={(e) => setAmountText(e.target.value)} />
      </label>
      <Segmented label="Achat" value={every} options={[["once", "Une fois"], ["7", "Chaque semaine"], ["30", "Chaque mois"]]} onChange={setEvery} />
      <Segmented label="Depuis" value={period} options={[["182", "6 mois"], ["365", "1 an"], ["730", "2 ans"]]} onChange={setPeriod} />
      {error && <p className="notice warn">⚠ {error}</p>}
      {!closes && !error && <p className="muted small">Chargement de l'historique…</p>}
      {closes && !r && <p className="muted small">{amount > 0 ? `Pas assez d'historique pour ${symbol} sur cette période.` : "Indiquez un montant."}</p>}
      {r && (
        <>
          <p className="kv">
            <span>{once ? `${usd(r.invested)} investis le ${date(r.first)}` : `${r.buys} achats · ${usd(r.invested)} investis`}</span>
            <b className={r.gain >= 0 ? "up" : "down"}>{usd(r.value)} ({pct(r.gain)})</b>
          </p>
          <DcaChart r={r} />
          {!once && <p className="kv small"><span>Tout investi le {date(r.first)}</span><b className={r.lumpSum.gain >= 0 ? "up" : "down"}>{usd(r.lumpSum.value)} ({pct(r.lumpSum.gain)})</b></p>}
          <p className="kv small"><span>{once ? "Prix d'achat" : "Prix moyen payé"}</span><b>{moneyPrice(r.averagePrice)}</b></p>
          <p className="kv small"><span>Prix à la dernière clôture</span><b>{moneyPrice(r.lastPrice)}</b></p>
          {displayCurrency() === "EUR" && (
            <p className="muted small">Rejoué en $ sur les cours en dollars, puis converti au taux du jour : l'effet de change passé (EUR/USD) n'est pas compté.</p>
          )}
          <p className="muted small">
            {once
              ? "Un seul achat, à la clôture de ce jour-là."
              : r.lumpSum.gain > r.gain
              ? "Sur cette période, tout acheter le premier jour a mieux rendu : le prix a surtout monté."
              : r.lumpSum.gain < r.gain
                ? "Sur cette période, étaler les achats a mieux rendu : ils ont profité des baisses."
                : "Sur cette période, les deux façons d'investir reviennent au même."}{" "}
            Rejoué sur les vraies clôtures journalières, sans frais ni impôts ; le passé ne dit pas ce qui arrivera.
          </p>
        </>
      )}
    </div>
  );
}

function DcaChart({ r }: { r: NonNullable<ReturnType<typeof simulateDca>> }) {
  const W = 320;
  const H = 110;
  const max = Math.max(...r.path.map((p) => Math.max(p.value, p.invested)), 1);
  const x = (i: number) => (i / (r.path.length - 1)) * W;
  const y = (v: number) => 6 + (1 - v / max) * (H - 12);
  const line = (vals: number[]) => vals.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join("");
  return (
    <div className="history-chart">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${usd(r.invested)} investis, valeur ${usd(r.value)}`}>
        <path d={line(r.path.map((p) => p.invested))} stroke="rgba(255,255,255,0.45)" strokeDasharray="4 3" className="bench" />
        <path d={line(r.path.map((p) => p.value))} stroke="#3987e5" className="mine" />
      </svg>
      <ul className="history-legend">
        <li><i style={{ background: "#3987e5" }} />Valeur</li>
        <li><i style={{ background: "rgba(255,255,255,0.45)" }} />Somme investie</li>
      </ul>
    </div>
  );
}
