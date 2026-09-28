import { useEffect, useState } from "react";
import type { Kind } from "../engine/reliability";

/** `/api/why` (backend/src/engine/why.rs, serde camelCase). */
export type WhyDirection = "up" | "down" | "neutral";
export type WhyMagnitude = "low" | "medium" | "high";
export type WhyCertainty = "observed" | "possibleCorrelation" | "unverifiable";
export interface WhyFactor { key: string; label: string; direction: WhyDirection; magnitude: WhyMagnitude; detail: string; source: string; certainty: WhyCertainty; certaintyLabel: string; brief?: string }
export interface WhyReport {
  symbol: string; kind: Kind; name: string; asOf: number;
  change: { pct: number; price: number; previousClose: number; previousCloseTime: number; atrPct: number | null } | null;
  volume: { ratio: number; day: number; volume: number; average: number } | null;
  factors: WhyFactor[];
  notCovered: { key: string; label: string; reason: string }[];
  summary: string; disclaimer: string; askEnabled: boolean;
  sources: { name: string; ok: boolean; detail: string }[];
}
export interface AskAnswer { answer: string; model: string; question: string; data: unknown; disclaimer: string }

async function call<T>(url: string, init?: RequestInit): Promise<T> {
  const r = await fetch(url, init);
  if (r.status === 401 && typeof window !== "undefined") {
    window.location.assign(`/login?next=${encodeURIComponent(window.location.pathname + window.location.search)}`);
    throw new Error("Session expirée");
  }
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error((body as { error?: string }).error ?? `Erreur ${r.status}`);
  return body as T;
}

export const whyUrl = (symbol: string, kind: Kind) => `/api/why?symbol=${encodeURIComponent(symbol)}&kind=${kind}`;
export const fetchWhy = (symbol: string, kind: Kind) => call<WhyReport>(whyUrl(symbol, kind));
export const askWhy = (symbol: string, kind: Kind, question: string) =>
  call<AskAnswer>("/api/ask", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ symbol, kind, question }) });

const DIR_ICON: Record<WhyDirection, string> = { up: "↗", down: "↘", neutral: "→" };
const DIR_WORD: Record<WhyDirection, string> = { up: "haussier", down: "baissier", neutral: "neutre" };
const MAG_WORD: Record<WhyMagnitude, string> = { low: "faible", medium: "moyen", high: "fort" };
const CERT_CLASS: Record<WhyCertainty, string> = { observed: "good", possibleCorrelation: "info", unverifiable: "warning" };

/** « Pourquoi ça bouge ? » on the asset screen: co-occurring observations, never presented as causes. */
export function WhyCard({ symbol, kind }: { symbol: string; kind: Kind }) {
  const [report, setReport] = useState<WhyReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [question, setQuestion] = useState("");
  const [asking, setAsking] = useState(false);
  const [answer, setAnswer] = useState<AskAnswer | null>(null);
  const [askError, setAskError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setReport(null);
    setError(null);
    fetchWhy(symbol, kind)
      .then((r) => alive && setReport(r))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Indisponible"));
    return () => {
      alive = false;
    };
  }, [symbol, kind]);

  const ask = async () => {
    if (question.trim().length < 3 || asking) return;
    setAsking(true);
    setAskError(null);
    setAnswer(null);
    try {
      setAnswer(await askWhy(symbol, kind, question.trim()));
    } catch (e) {
      setAskError(e instanceof Error ? e.message : "Indisponible");
    } finally {
      setAsking(false);
    }
  };

  return (
    <div className="card why">
      <h2 className="card-title">Pourquoi ça bouge ?</h2>
      {error && <p className="notice warn small">⚠ {error}</p>}
      {!report && !error && <div className="skeleton" aria-label="Chargement des observations" />}
      {report && (
        <>
          <p className="why-summary">{report.summary}</p>
          <ul className="why-factors">
            {report.factors.map((f) => (
              <li key={f.key} className={`insight ${CERT_CLASS[f.certainty]}`}>
                <span aria-label={DIR_WORD[f.direction]}>{DIR_ICON[f.direction]}</span>
                <span>
                  <b>{f.label}</b> <span className="chip muted">{f.certaintyLabel}</span>
                  <br />
                  {f.detail}
                  <br />
                  <small className="muted">Sens {DIR_WORD[f.direction]}, ampleur {MAG_WORD[f.magnitude]} · source : {f.source}</small>
                </span>
              </li>
            ))}
          </ul>
          {report.notCovered.length > 0 && (
            <p className="muted small">Non couvert : {report.notCovered.map((n) => `${n.label.toLowerCase()} (${n.reason.replace(/\.$/, "")})`).join(" ; ")}.</p>
          )}
          <p className="muted small">{report.disclaimer}</p>

          {report.askEnabled && (
            <div className="why-ask">
              <label className="field">
                <span>Poser une question sur ces données</span>
                <textarea rows={2} maxLength={500} value={question} placeholder="ex. La baisse vient-elle du marché ou de l'actif ?" onChange={(e) => setQuestion(e.target.value)} />
              </label>
              <button className="btn btn-ghost" onClick={ask} disabled={asking || question.trim().length < 3}>{asking ? "Réponse en cours…" : "Demander"}</button>
              <small className="muted">
                Réponse d'un modèle d'IA (Claude, Anthropic) à partir des seules données ci-dessus, envoyées à Anthropic avec votre question (jamais vos avoirs).
                Elle peut se tromper ; ce n'est pas un conseil. Quelques questions par minute au plus.
              </small>
              {askError && <p className="notice warn small">⚠ {askError}</p>}
              {answer && (
                <div className="why-answer" role="status">
                  <p>{answer.answer}</p>
                  <small className="muted">Données utilisées : les observations ci-dessus{(answer.data as { derniereDecision?: unknown })?.derniereDecision ? " et la dernière décision calculée" : ""}.</small>
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
