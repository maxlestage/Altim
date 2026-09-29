import { useState } from "react";
import { createPortal } from "react-dom";
import { DEFAULT_CAPITAL, FEE_RATE, newPaper, openPosition, SLIPPAGE, valuation } from "../engine/paper";
import { onLink } from "./router";
import { currencySymbol, fromDisplay, money, toDisplay } from "../money";
import type { Decision } from "./decision";
import { getPaper, setPaper, usePaper } from "./paper-store";
import { record } from "./journal-store";
import { JournalNote } from "./Journal";
import { againstDecision, againstText, defaultAmount, frDate, inputPrice, levelWarnings, newId, parseAmount, price as fmtPrice, usd } from "./paper-ui";

/** "Simuler cet achat" on the Décision card: a virtual purchase, no real money, no order sent. */
export function SimulateBuy({ d, livePrice }: { d: Decision; livePrice?: number | null }) {
  const [open, setOpen] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const px = livePrice ?? d.price;
  const against = againstDecision(d.verdict);
  return (
    <div className="dec-block paper-cta">
      <button className="btn btn-ghost" onClick={() => { setDone(null); setOpen(true); }} disabled={!(px != null && px > 0)}>
        Simuler cet achat
      </button>
      <small className="muted">Portefeuille simulé : aucun argent réel, aucun ordre passé.{against && ` Décision actuelle : « ${d.label} ».`}</small>
      {done && (
        <p className="notice ok small" role="status">
          ✔ {done} <a href="/app/simulation" onClick={onLink} className="link">Voir la simulation</a>
        </p>
      )}
      {open && px != null && px > 0 && (
        <OrderSheet d={d} px={px} live={livePrice != null} onClose={() => setOpen(false)} onDone={(msg) => { setOpen(false); setDone(msg); }} />
      )}
    </div>
  );
}

function OrderSheet({ d, px, live, onClose, onDone }: { d: Decision; px: number; live: boolean; onClose: () => void; onDone: (msg: string) => void }) {
  const { state } = usePaper();
  // The simulated ledger is kept in dollars like the prices; amounts are typed and shown in the display currency.
  const startUsd = fromDisplay(DEFAULT_CAPITAL);
  const cash = state?.cash ?? startUsd;
  const equity = state ? valuation(state, {}).equity : startUsd;
  const shownPrice = (v: number | null | undefined) => (v == null ? v : toDisplay(v));
  const [amountText, setAmountText] = useState(() => inputPrice(toDisplay(defaultAmount(equity, cash))));
  const [stopText, setStopText] = useState(inputPrice(shownPrice(d.plan?.stop)));
  const [targetText, setTargetText] = useState(inputPrice(shownPrice(d.plan?.target1)));
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState("");

  const usdOf = (v: number | null) => (v == null ? v : fromDisplay(v));
  const amount = usdOf(parseAmount(amountText));
  const stop = usdOf(parseAmount(stopText));
  const target = usdOf(parseAmount(targetText));
  const fill = px * (1 + SLIPPAGE);
  const warnings = levelWarnings(fill, stop, target);
  const badNumber = [stop, target].some((v) => v != null && !Number.isFinite(v));
  const against = againstDecision(d.verdict);

  const confirm = () => {
    setError(null);
    if (badNumber) return setError("Stop ou objectif illisible : laissez vide ou saisissez un nombre.");
    const now = Date.now();
    const base = getPaper();
    const created = !base;
    const s = base ?? newPaper(startUsd, now);
    const res = openPosition(
      s,
      {
        id: newId(), symbol: d.symbol, kind: d.kind, name: d.name, price: px, amount: amount ?? NaN,
        stop, target, decision: { verdict: d.verdict, label: d.label, confidence: d.confidence, asOf: d.asOf },
      },
      now,
    );
    if (res.error) return setError(res.error);
    setPaper(res.state);
    const pos = res.state.positions[res.state.positions.length - 1]!;
    record({
      source: "paper", side: "buy", symbol: d.symbol, kind: d.kind, name: d.name, price: pos.entry, quantity: pos.quantity, amount: pos.invested,
      stop: pos.stop, targets: [pos.target], note, refId: pos.id, decision: d, now,
    });
    onDone(`Achat simulé : ${usd(amount!)} de ${d.name}${created ? ` (portefeuille simulé créé avec ${money(startUsd, 0, 0)})` : ""}. Inscrit au journal.`);
  };

  // Portal: the Décision card has a backdrop-filter, which would trap a position: fixed sheet inside it.
  return createPortal(
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="paper-order-title" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="sheet-head"><h2 id="paper-order-title">Simuler l'achat de {d.name}</h2></div>
        <p className="notice small">Simulation : aucun argent réel, aucun ordre passé. Le résultat sera suivi dans l'écran « Simulation ».</p>
        {against && <p className="notice warn small" role="alert">⚠ {againstText(d.label)} C'est permis : utile pour comparer.</p>}
        <p className="kv small"><span>Décision affichée</span><b>{d.label} · confiance {Math.round(d.confidence)}/100</b></p>
        <p className="kv small"><span>Cours utilisé</span><b className="mono">{fmtPrice(px)}</b></p>
        <p className="muted small">{live ? "Cours en direct." : `Cours de la décision du ${frDate(d.asOf, true)}.`} Achat simulé à {fmtPrice(fill)} (glissement 0,05 %), frais 0,1 %.</p>
        <label className="field">
          <span>Montant à investir ({currencySymbol()}, frais compris)</span>
          <input inputMode="decimal" autoFocus value={amountText} onChange={(e) => setAmountText(e.target.value)} aria-describedby="paper-cash" />
        </label>
        <small id="paper-cash" className="muted">
          Liquidités simulées : {usd(cash)}{!state && ` (nouveau portefeuille de ${money(startUsd, 0, 0)})`} · par défaut 10 % de la valeur simulée.
          {amount != null && amount > 0 && Number.isFinite(amount) && ` Frais : ${usd(amount * FEE_RATE)}.`}
        </small>
        <div className="grid-2">
          <label className="field">
            <span>Stop (optionnel)</span>
            <input inputMode="decimal" value={stopText} onChange={(e) => setStopText(e.target.value)} placeholder="aucun" />
          </label>
          <label className="field">
            <span>Objectif (optionnel)</span>
            <input inputMode="decimal" value={targetText} onChange={(e) => setTargetText(e.target.value)} placeholder="aucun" />
          </label>
        </div>
        {d.plan && <small className="muted">Préremplis avec le plan de la décision (stop et objectif 1). Vente automatique si une bougie journalière les atteint.</small>}
        {warnings.map((w) => <p key={w} className="notice warn small">⚠ {w}</p>)}
        <JournalNote value={note} onChange={setNote} />
        {error && <p className="notice danger small" role="alert">✕ {error}</p>}
        <button className="btn" onClick={confirm}>Confirmer l'achat simulé</button>
        <button className="btn btn-ghost" onClick={onClose}>Annuler</button>
      </div>
    </div>,
    document.body,
  );
}
