import { money } from "../money";
import type { PortfolioAnalysis } from "../engine/holdings";
import { onLink } from "./router";
import { BENCHMARK, MIN_BETA_DAYS, type BetaEstimate, type LimitCheck, type StressResult } from "../engine/portfolio-risk";

const usd = (v: number) => money(Math.abs(v), 0, 0);
const pc = (v: number) => `${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const LIMIT_ICON = { danger: "⛔", warning: "⚠", ok: "✔", na: "?" } as const;
const LIMIT_CLASS = { danger: "danger", warning: "warning", ok: "good", na: "info" } as const;

/** Market shocks passed through each line's beta: loss in $ and %, worst line, what the cash cushions. */
export function StressCard({ analysis, results, betas }: { analysis: PortfolioAnalysis; results: StressResult[]; betas: Record<string, BetaEstimate | undefined> }) {
  const assets = [...new Map(analysis.lines.map((l) => [`${l.kind}:${l.symbol}`, l])).values()];
  const estimated = assets.filter((l) => betas[`${l.kind}:${l.symbol}`]?.estimated);
  const reference = assets.filter((l) => betas[`${l.kind}:${l.symbol}`]?.reference);
  const fallback = assets.filter((l) => !betas[`${l.kind}:${l.symbol}`]?.estimated && !betas[`${l.kind}:${l.symbol}`]?.reference);
  const days = estimated.map((l) => betas[`${l.kind}:${l.symbol}`]!.days);
  return (
    <div className="card">
      <h2 className="card-title">Scénarios de crise</h2>
      <p className="muted small">
        Ce que perdrait votre portefeuille si les marchés chutaient d'un coup : chaque ligne bouge selon son bêta face à sa référence ({BENCHMARK.crypto.label} pour les cryptos,{" "}
        {BENCHMARK.stock.label} pour les actions). Vos liquidités ({usd(analysis.cash)}) ne bougent pas.
      </p>
      <ul className="insights">
        {results.map((r) => (
          <li key={r.scenario.key} className={`insight ${r.lossPercent >= 10 ? "danger" : r.loss > 0 ? "warning" : "good"}`}>
            <span aria-hidden>{r.loss > 0 ? "↘" : "→"}</span>
            <span>
              <b>{r.scenario.label}</b>
              <br />
              {r.loss >= 0 ? "Perte" : "Gain"} ≈ <b className={r.loss > 0 ? "down" : "up"}>{r.loss > 0 ? "−" : "+"}{usd(r.loss)}</b>, soit {pc(r.lossPercent)} du patrimoine
              {analysis.cash > 0 && r.loss > 0 && <> ({pc(r.investedLossPercent)} de vos placements : les liquidités amortissent {pc(r.investedLossPercent - r.lossPercent)})</>}.
              {r.worst && (
                <>
                  <br />
                  <small className="muted">Ligne la plus touchée : {r.worst.symbol} ({r.worst.movePercent > 0 ? "+" : "−"}{pc(r.worst.movePercent)}, −{usd(r.worst.loss)})</small>
                </>
              )}
            </span>
          </li>
        ))}
      </ul>
      <p className="muted small">
        Hypothèse : choc instantané, bêta constant.{" "}
        {estimated.length > 0 && <>Bêta estimé sur {Math.min(...days)}{Math.max(...days) !== Math.min(...days) ? ` à ${Math.max(...days)}` : ""} jours de rendements journaliers ({estimated.map((l) => `${l.symbol} ${betas[`${l.kind}:${l.symbol}`]!.beta.toFixed(2)}`).join(", ")}). </>}
        {reference.length > 0 && <>{reference.map((l) => l.symbol).join(", ")} : référence elle-même, bêta 1. </>}
        {fallback.length > 0 && <>Historique trop court (moins de {MIN_BETA_DAYS} jours communs) ou indisponible pour {fallback.map((l) => l.symbol).join(", ")} : bêta 1 retenu. </>}
        Une vraie crise peut aller plus loin : les corrélations montent quand tout baisse.
      </p>
    </div>
  );
}

/** The portfolio against the user's own limits (Réglages). */
export function LimitsCard({ checks }: { checks: LimitCheck[] }) {
  if (!checks.length) return null;
  return (
    <div className="card">
      <h2 className="card-title">Vos limites de risque</h2>
      <ul className="insights">
        {checks.map((c) => (
          <li key={c.code} className={`insight ${LIMIT_CLASS[c.level]}`}>
            <span aria-hidden>{LIMIT_ICON[c.level]}</span>
            <span><b>{c.label}</b><br />{c.detail}</span>
          </li>
        ))}
      </ul>
      <p className="muted small">
        Limites réglables dans <a href="/app/reglages" onClick={onLink} className="link">Réglages → Prudence des conseils</a>. Hypothèse : stop que vous avez saisi, sinon stop de protection à 2 × la volatilité
        journalière ; variation du jour mesurée depuis la clôture de la veille (journée UTC pour les cryptos, dernière séance pour les actions).
      </p>
    </div>
  );
}
