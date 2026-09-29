import { fxLine } from "../money";
import { useFx } from "./fx";
import { useAppState } from "./store";

/**
 * Discreet line under amounts: "1 $ = 0,881 € · Yahoo Finance, 14:05", or why they are still in dollars (euros asked
 * but no rate: never a made-up conversion).
 */
export function FxNote({ className = "muted small fx-note" }: { className?: string }) {
  const { currency } = useAppState();
  const { fx, error } = useFx();
  if (currency === "USD") return null;
  if (!fx) return <p className={className} title={error ?? undefined}>Taux EUR/USD indisponible : montants affichés en $.</p>;
  return <p className={className}>{fxLine(fx)}{fx.stale ? " (dernier taux connu)" : ""}</p>;
}
