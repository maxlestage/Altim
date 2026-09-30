/**
 * « Bot Altim » on the Radar: the report's headline (read from /api/bot, never written here), the forward test's
 * counter and a link to the Bot screen. Loading, pending (first training) and error states; one compact card.
 */
import { useEffect, useState } from "react";
import { api } from "./api";
import { onLink } from "./router";
import { forwardSignals, type BotReport } from "./model-bot";

export function BotRadarCard() {
  const [report, setReport] = useState<BotReport | null>(null);
  const [state, setState] = useState<"loading" | "pending" | "error" | "ready">("loading");
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = () =>
      api.bot()
        .then((r) => {
          if (!alive) return;
          if ("pending" in r) {
            setState("pending");
            timer = setTimeout(load, 15_000);
          } else {
            setReport(r);
            setState("ready");
          }
        })
        .catch(() => alive && setState("error"));
    load();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, []);
  return <BotRadarView state={state} report={report} />;
}

/** The headline's first sentence (the verdict; the details are on the Bot screen). */
export function firstSentence(s: string): string {
  const i = s.indexOf(". ");
  return i < 0 ? s : s.slice(0, i + 1);
}

/** The card itself (pure, tested). */
export function BotRadarView({ state, report }: { state: "loading" | "pending" | "error" | "ready"; report: BotReport | null }) {
  const n = report?.v3 ? forwardSignals(report.v3) : null;
  return (
    <div className="card bot-radar">
      <h2 className="card-title">Bot Altim</h2>
      {state === "loading" && <p className="muted small">Chargement…</p>}
      {state === "pending" && <p className="muted small">Entraînement et test en cours sur le serveur (plusieurs minutes la première fois)…</p>}
      {state === "error" && <p className="muted small">Résultats indisponibles pour le moment.</p>}
      {state === "ready" && report && (
        <>
          <p className="small bot-radar-headline">{firstSentence(report.v3?.headline ?? report.headline)}</p>
          {n != null && <p className="small"><b>Test sur l'avenir : {n} {n > 1 ? "signaux" : "signal"} sur 30</b></p>}
        </>
      )}
      <a href="/app/bot" onClick={onLink} className="link small">Voir le bot →</a>
    </div>
  );
}
