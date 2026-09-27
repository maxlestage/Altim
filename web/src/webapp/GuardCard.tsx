import type { GuardFactor, FactorStatus } from "../engine/guard";
import type { GuardReport } from "./api";

const LEVEL = { calm: "Calme", agitated: "Agité", shock: "Choc" } as const;
const TREND = { up: "Haussière", down: "Baissière", range: "Sans direction" } as const;
const POLICY = { ok: "Autorisé", reduce: "Taille réduite", pause: "Suspendu" } as const;
const STATUS: Record<FactorStatus, string> = {
  verified: "vérifié sur cet actif",
  unproven: "peu d'historique : compté à moitié",
  rejected: "jamais prédictif ici : ignoré",
  unverifiable: "sans historique : non vérifié",
};

function Meter({ label, value, tone }: { label: string; value: number; tone: "ok" | "warn" | "bad" }) {
  return (
    <div className="guard-meter" role="img" aria-label={`${label} : ${value} sur 100`}>
      <div className="guard-meter-head"><span>{label}</span><b className="mono">{value}/100</b></div>
      <div className="weight-track"><i className={`guard-${tone}`} style={{ width: `${Math.max(2, value)}%` }} /></div>
    </div>
  );
}

function Factors({ factors }: { factors: GuardFactor[] }) {
  if (!factors.length) return <p className="muted small">Aucun signal.</p>;
  return (
    <ul className="guard-factors">
      {factors.map((f) => (
        <li key={f.code} className={f.status === "rejected" ? "muted" : ""}>
          <span>{f.text}</span>
          <small className="muted">
            {" "}+{f.points} · {STATUS[f.status]}
            {f.evidence && f.evidence.samples > 0 && ` (${Math.round(f.evidence.rate)} % des ${f.evidence.samples} cas passés contre ${Math.round(f.evidence.base)} % d'habitude)`}
          </small>
        </li>
      ))}
    </ul>
  );
}

/** Market guard: background regime, shock risk, counter-trend reversal risk, and what a short-term bot should do. */
export function GuardCard({ g }: { g: GuardReport }) {
  const shockTone = g.shock.level === "shock" ? "bad" : g.shock.level === "agitated" ? "warn" : "ok";
  const revTone = g.reversal.score >= 50 ? "bad" : g.reversal.score >= 25 ? "warn" : "ok";
  return (
    <div className="card guard">
      <h2 className="card-title">Garde-fou marché</h2>
      <p className="kv"><span>Tendance de fond</span><b>{TREND[g.regime.trend]} · force {g.regime.strength}/100</b></p>
      <p className="muted small">{g.regime.text}</p>
      <Meter label={`Risque de choc · ${LEVEL[g.shock.level]}`} value={g.shock.score} tone={shockTone} />
      <Factors factors={g.shock.factors} />
      <Meter
        label={`Risque de retournement${g.reversal.direction ? ` à la ${g.reversal.direction === "down" ? "baisse" : "hausse"}` : ""}`}
        value={g.reversal.score}
        tone={revTone}
      />
      <Factors factors={g.reversal.factors} />
      <div className="guard-policy">
        <p className="kv"><span>Trading court terme (bots)</span><b>{POLICY[g.policy.scalping]}</b></p>
        <p className="kv small"><span>Taille × {g.policy.sizeMultiplier.toLocaleString("fr-FR")}</span><span>Stop × {g.policy.stopMultiplier.toLocaleString("fr-FR")}</span></p>
        <ul>{g.policy.notes.map((n) => <li key={n}>{n}</li>)}</ul>
      </div>
      {g.inputs.headlines.length > 0 && (
        <details>
          <summary className="small">Dernières actualités ({g.inputs.news24h} en 24 h)</summary>
          <ul className="guard-news">
            {g.inputs.headlines.map((h) => (
              <li key={h.title} className="small">{h.title} <span className="muted">· {new Date(h.time).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" })}</span></li>
            ))}
          </ul>
        </details>
      )}
      <p className="muted small">
        Aucun outil ne prévoit une vraie surprise. Le garde-fou mesure les conditions où les grands mouvements et les retournements sont
        plus probables, et chaque signal technique est vérifié sur l'historique de l'actif. API pour vos bots : <code>/api/guard?symbol={g.symbol}&amp;kind={g.kind}</code>
      </p>
    </div>
  );
}
