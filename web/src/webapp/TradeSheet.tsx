import { useEffect, useMemo, useRef, useState } from "react";
import type { Signal } from "../engine/signal";
import type { Reliability } from "../engine/reliability";
import { positionSize, preTradeIssues, type Plan } from "../engine/risk";
import { formatPrice } from "../market";
import { api } from "./api";
import { executeDemo, FEE_RATE, realizedPnLToday, useAppState, assetKey } from "./store";

type Asset = { symbol: string; kind: "crypto" | "stock"; name: string };

const num = (s: string) => Number(s.replace(/\s/g, "").replace(",", "."));
const fmtQty = (q: number) => (q >= 1 ? q.toFixed(4) : q.toPrecision(4)).replace(/\.?0+$/, "");
/** Suggested size rounded DOWN (never above the risk limits), 4 significant digits. */
export function floorQty(q: number): string {
  if (!(q > 0)) return "";
  const decimals = Math.max(0, 3 - Math.floor(Math.log10(q)));
  const f = 10 ** decimals;
  return (Math.floor(q * f + 1e-9) / f).toFixed(decimals).replace(/\.?0+$/, "");
}

/** Demo order: size suggested by the risk manager, same checks as the iOS app. */
export function TradeSheet({ side, asset, price, signal, reliability, onClose }: {
  side: "buy" | "sell"; asset: Asset; price: number; signal: Signal | null; reliability: Reliability | null; onClose: (done: boolean) => void;
}) {
  const state = useAppState();
  const position = state.positions.find((p) => assetKey(p) === assetKey(asset));
  const plan = useMemo<Plan | null>(() => {
    if (side !== "buy" || !signal?.hasPlan) return null;
    const dist = signal.price - signal.stopLoss;
    if (!(dist > 0)) return null;
    return { entry: price, stopLoss: price - dist, takeProfit: price + dist * 2 };
  }, [side, signal, price]);
  const suggestion = plan ? positionSize(state.risk, state.cash, plan) : null;

  const [qty, setQty] = useState(() => (side === "buy" ? (suggestion ? floorQty(suggestion.quantity) : "") : position ? String(position.quantity) : ""));
  const [stop, setStop] = useState(plan ? plan.stopLoss.toPrecision(6) : "");
  const [target, setTarget] = useState(plan ? plan.takeProfit.toPrecision(6) : "");
  const [protect, setProtect] = useState(side === "buy");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    dialog.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose(false);
    window.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
    };
  }, [onClose]);

  const quantity = num(qty);
  const notional = quantity * price;
  const issues: string[] = [];
  if (!(quantity > 0)) issues.push("Quantité invalide.");
  if (side === "buy") {
    const p: Plan | null = protect && num(stop) > 0 && num(target) > 0 ? { entry: price, stopLoss: num(stop), takeProfit: num(target) } : null;
    if (protect && !p) issues.push("Stop et objectif requis.");
    const risk = preTradeIssues(state.risk, "buy", notional, state.cash, p, realizedPnLToday(state.journal));
    issues.push(...(protect ? risk : risk.filter((i) => i !== "Aucun stop défini : achat refusé.")));
    if (notional * (1 + FEE_RATE) > state.cash) issues.push(`Solde insuffisant (${formatPrice(state.cash)} USDT).`);
    if (reliability?.level === "low") issues.push("Données de marché non fiables (sources absentes ou en désaccord) : achat bloqué.");
  } else if (!position || quantity > position.quantity + 1e-12) {
    issues.push(`Vous détenez ${position ? fmtQty(position.quantity) : "0"} ${asset.symbol}.`);
  }
  const against = signal && ((side === "buy" && signal.action.includes("ell")) || (side === "sell" && signal.action.includes("uy")));

  async function confirm() {
    setBusy(true);
    try {
      // Execution price = fresh multi-source consensus (never a single source's price).
      const [q] = await api.quotes([asset]);
      if (!q || q.agreeing < 1) throw new Error("Cours indisponible : ordre annulé.");
      if (Math.abs(q.price / price - 1) > 0.01) throw new Error(`Le cours a bougé de plus de 1 % (${formatPrice(q.price)}) : vérifiez puis réessayez.`);
      const err = executeDemo({
        ...asset, side, quantity, price: q.price,
        stopLoss: side === "buy" && protect ? num(stop) : undefined,
        takeProfit: side === "buy" && protect ? num(target) : undefined,
      });
      if (err) throw new Error(err);
      setResult(`${side === "buy" ? "Achat" : "Vente"} de ${fmtQty(quantity)} ${asset.symbol} à ${formatPrice(q.price)} $ exécuté (démo).`);
    } catch (e) {
      setResult(`✕ ${e instanceof Error ? e.message : "Erreur"}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="sheet-backdrop" onClick={() => onClose(false)}>
      <div className="sheet" role="dialog" aria-modal="true" aria-label={`${side === "buy" ? "Acheter" : "Vendre"} ${asset.name}`} tabIndex={-1} ref={dialog} onClick={(e) => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="sheet-head">
          <h2>{side === "buy" ? "Acheter" : "Vendre"} {asset.name}</h2>
          <span className="app-env">DÉMO</span>
        </div>

        {result ? (
          <div className="sheet-result">
            <p className={result.startsWith("✕") ? "down" : "up"}>{result}</p>
            {!result.startsWith("✕") && <p className="muted small">Stop et objectif enregistrés, non simulés automatiquement en démo.</p>}
            <button className="btn" onClick={() => onClose(!result.startsWith("✕"))}>{result.startsWith("✕") ? "Fermer" : "Voir le portefeuille"}</button>
          </div>
        ) : (
          <>
            {against && <p className="notice warn">⚠ Cet ordre va à l'encontre du signal Altim.</p>}
            <label className="field">
              <span>Quantité ({asset.symbol})</span>
              <input inputMode="decimal" value={qty} onChange={(e) => setQty(e.target.value)} />
            </label>
            <p className="kv small"><span>Montant estimé</span><b>≈ {formatPrice(notional || 0)} $</b></p>
            <p className="kv small"><span>Disponible</span><b>{side === "buy" ? `${formatPrice(state.cash)} USDT` : `${position ? fmtQty(position.quantity) : 0} ${asset.symbol}`}</b></p>

            {side === "buy" && (
              <>
                <label className="check-row">
                  <input type="checkbox" checked={protect} onChange={(e) => setProtect(e.target.checked)} />
                  <span>Stop et objectif</span>
                </label>
                {protect ? (
                  <div className="two">
                    <label className="field"><span>Stop</span><input inputMode="decimal" value={stop} onChange={(e) => setStop(e.target.value)} /></label>
                    <label className="field"><span>Objectif</span><input inputMode="decimal" value={target} onChange={(e) => setTarget(e.target.value)} /></label>
                  </div>
                ) : (
                  <p className="warn small">Sans stop, une chute brutale n'est pas limitée.</p>
                )}
                {suggestion && (
                  <p className="muted small">
                    Taille suggérée : risque {formatPrice(suggestion.riskAmount)} $ ({state.risk.riskPerTradePercent} % du capital) si le stop est touché, position {suggestion.percentOfEquity.toFixed(1)} % du capital{suggestion.capped ? " (plafonnée)" : ""}.
                  </p>
                )}
              </>
            )}

            {issues.length > 0 && <ul className="issues">{issues.map((i) => <li key={i}>✕ {i}</li>)}</ul>}

            <button className={`btn ${side === "buy" ? "buy-btn" : "sell-btn"}`} disabled={busy || issues.length > 0} onClick={confirm}>
              {busy ? "Vérification du cours…" : side === "buy" ? "Confirmer l'achat (démo)" : "Confirmer la vente (démo)"}
            </button>
            <p className="muted small center">Le prix d'exécution est revérifié auprès de plusieurs sources juste avant l'ordre.</p>
          </>
        )}
      </div>
    </div>
  );
}
