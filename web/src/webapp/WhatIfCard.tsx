import { currencySymbol, fromDisplay, money } from "../money";
import { useEffect, useMemo, useState } from "react";
import type { PortfolioAnalysis } from "../engine/holdings";
import { FACTOR_SHOCKS, FACTORS, factorBeta, MIN_BETA_DAYS, WEAK_CORRELATION, WHATIF_BETA_DAYS, whatIf, type FactorBeta, type FactorKey } from "../engine/portfolio-risk";
import type { Candle } from "../engine/signal";
import { api } from "./api";
import { parseAmount } from "./paper-ui";

const fr = (v: number, d = 1) => v.toLocaleString("fr-FR", { maximumFractionDigits: d });
const usd = (v: number) => money(v, 0, 0);
const signedUsd = (loss: number) => `${loss > 0 ? "−" : loss < 0 ? "+" : ""}${usd(Math.abs(loss))}`;
const signedPct = (v: number) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${fr(Math.abs(v))} %`;

/**
 * « Et si… ? » on Mes avoirs: how the portfolio would move if one market factor (Nasdaq-100, S&P 500, Bitcoin) fell
 * by a given shock, each line through its beta to that factor. `daily`: the daily candles already loaded
 * (`kind:symbol`); the factor's own candles are fetched when missing.
 */
export function WhatIfCard({ analysis, daily }: { analysis: PortfolioAnalysis; daily: Record<string, Candle[] | undefined> }) {
  const [factor, setFactor] = useState<FactorKey>("qqq");
  const [shock, setShock] = useState(-10);
  const [custom, setCustom] = useState("");
  const [amountText, setAmountText] = useState("");
  const [fetched, setFetched] = useState<Record<string, Candle[] | null>>({});
  const f = FACTORS[factor];
  const fKey = `${f.kind}:${f.symbol}`;
  const factorCandles = daily[fKey]?.length ? daily[fKey]! : fetched[fKey] ?? null;

  useEffect(() => {
    if (daily[fKey]?.length || fKey in fetched) return;
    let alive = true;
    api.candles(f.symbol, f.kind, "1d")
      .then((s) => alive && setFetched((m) => ({ ...m, [fKey]: s.candles })))
      .catch(() => alive && setFetched((m) => ({ ...m, [fKey]: null })));
    return () => {
      alive = false;
    };
  }, [fKey]);

  const customShock = parseAmount(custom.replace(/^[−-]/, ""));
  const effShock = custom.trim() && customShock != null && Number.isFinite(customShock) && customShock > 0 && customShock <= 100 ? -customShock : shock;
  // Typed in the display currency; the engine works in dollars.
  const typedAmount = parseAmount(amountText);
  const amount = typedAmount == null ? null : fromDisplay(typedAmount);
  const betas = useMemo(() => {
    const b: Record<string, FactorBeta> = {};
    if (!factorCandles?.length) return b;
    for (const l of analysis.lines) {
      const k = `${l.kind}:${l.symbol}`;
      if (!(k in b)) b[k] = factorBeta(daily[k] ?? [], factorCandles, WHATIF_BETA_DAYS);
    }
    return b;
  }, [analysis.lines, daily, factorCandles]);
  const loading = !factorCandles && !(fKey in fetched);
  const r = useMemo(() => whatIf(analysis, factor, effShock, betas, amount != null && Number.isFinite(amount) ? amount : null), [analysis, factor, effShock, betas, amount]);

  return (
    <div className="card whatif">
      <h2 className="card-title">Et si… ?</h2>
      <p className="muted small">
        Comment le risque de ce portefeuille évolue si {f.name} baisse de {fr(Math.abs(effShock))} % ? Chaque ligne bouge selon son bêta face à ce marché.
      </p>
      <div className="whatif-row">
        <span className="small muted">Marché</span>
        <div className="agenda-chips" role="group" aria-label="Marché">
          {(Object.keys(FACTORS) as FactorKey[]).map((k) => (
            <button key={k} className={`chip pick ${k === factor ? "on" : ""}`} aria-pressed={k === factor} onClick={() => setFactor(k)}>{FACTORS[k].label}</button>
          ))}
        </div>
      </div>
      <div className="whatif-row">
        <span className="small muted">Choc</span>
        <div className="agenda-chips" role="group" aria-label="Choc">
          {FACTOR_SHOCKS.map((s) => (
            <button key={s} className={`chip pick ${!custom.trim() && s === shock ? "on" : ""}`} aria-pressed={!custom.trim() && s === shock} onClick={() => { setShock(s); setCustom(""); }}>
              {signedPct(s)}
            </button>
          ))}
        </div>
      </div>
      <div className="grid-2">
        <label className="field">
          <span>Autre baisse (%)</span>
          <input inputMode="decimal" value={custom} placeholder="ex. 15" onChange={(e) => setCustom(e.target.value)} />
        </label>
        <label className="field">
          <span>Montant simulé ({currencySymbol()}, facultatif)</span>
          <input inputMode="decimal" value={amountText} placeholder={usd(analysis.total)} onChange={(e) => setAmountText(e.target.value)} />
        </label>
      </div>
      {r.scaled && <p className="muted small">Simulé sur {usd(r.base)} répartis selon les poids actuels (liquidités comprises : {usd(r.cash)}).</p>}

      {loading ? (
        <p className="muted small">Chargement des cours de {f.label}…</p>
      ) : !factorCandles?.length ? (
        <p className="notice warn small">⚠ Cours journaliers de {f.label} indisponibles : simulation non couverte pour l'instant.</p>
      ) : (
        <>
          <p className="whatif-total">
            <span>Perte estimée</span>
            <b className={r.loss > 0 ? "down" : "up"}>{signedUsd(r.loss)}</b>
            <small className="muted">soit {signedPct(-r.lossPercent)} de {r.scaled ? "ce montant" : "votre patrimoine"}</small>
          </p>
          {r.worst && <p className="small">Ligne la plus touchée : <b>{r.worst.symbol}</b> ({signedPct(r.worst.movePercent!)}, {signedUsd(r.worst.loss!)}).</p>}
          <ul className="whatif-lines">
            {r.lines.map((l) => (
              <li key={l.key}>
                <div className="whatif-line-head">
                  <b>{l.symbol}</b>
                  <span className={l.loss == null ? "muted" : l.loss > 0 ? "down" : "up"}>{l.loss == null ? "non couvert" : signedUsd(l.loss)}</span>
                </div>
                <small className="muted">
                  {usd(l.value)} · {fr(l.weight)} %
                  {l.reference
                    ? " · c'est ce marché lui-même (bêta 1)"
                    : l.beta == null
                      ? ` · moins de ${MIN_BETA_DAYS} jours communs avec ${f.label} : bêta non mesurable`
                      : ` · bêta ${l.beta.toFixed(2)}${l.correlation != null ? `, corrélation ${l.correlation.toFixed(2)}` : ""} · ${signedPct(l.movePercent!)}${
                          l.correlation != null && Math.abs(l.correlation) < WEAK_CORRELATION ? ` · lien faible avec ${f.label} : ce bêta explique mal les mouvements de la ligne, résultat peu fiable` : ""
                        }`}
                </small>
              </li>
            ))}
          </ul>
          {r.uncovered.length > 0 && (
            <p className="muted small">Non couvert : {r.uncovered.join(", ")} ({usd(r.uncoveredValue)}) — laissé hors du total, jamais estimé.</p>
          )}
          <p className="muted small">
            {r.minDays != null ? `Bêta estimé sur ${r.minDays}${r.maxDays !== r.minDays ? ` à ${r.maxDays}` : ""} jours de rendements journaliers communs` : "Aucun bêta mesuré"} ; hypothèse : choc instantané,
            relation stable — rarement vrai en crise (les corrélations montent quand tout baisse). Les liquidités ne bougent pas. Ce n'est pas une prévision.
          </p>
        </>
      )}
    </div>
  );
}
