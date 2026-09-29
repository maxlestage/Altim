import { useEffect, useState } from "react";
import { api } from "../webapp/api";
import { plain } from "../webapp/model-validation";
import { FAMILY_LABEL, forwardSignals, frIso, headlineConfigs, judged, proven, type BotReport, type ConfigStats, type SideStats } from "../webapp/model-bot";
import { Section } from "./Section";

/** "non démontré (t = −1,2)" for a side at the corrected threshold (family B: both references must agree). */
function verdictShort(c: ConfigStats, side: "buy" | "sell"): string {
  const s: SideStats = judged(c, side);
  const t = s.t == null ? "t non calculable" : `t = ${plain(s.t, 1)}`;
  if (proven(c, side)) return `avantage démontré (${t})`;
  if (c[side].verdict === "edge") return `non démontré face à la moyenne du groupe (${t})`;
  switch (s.verdict) {
    case "negative": return `moins bien que la référence (${t})`;
    case "insufficient": return "trop peu de signaux";
    default: return `non démontré (${t})`;
  }
}

/**
 * « Bot Altim » on the presentation site: what it is and how it is judged (fixed text), and its current results read
 * live from /api/bot (never written here: they change with each retraining and with the forward test).
 */
export function BotSection() {
  const [report, setReport] = useState<BotReport | null>(null);
  const [state, setState] = useState<"loading" | "pending" | "error" | "ready">("loading");

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let tries = 0;
    const load = () =>
      api.bot()
        .then((r) => {
          if (!alive) return;
          if ("pending" in r) {
            setState("pending");
            if (++tries < 60) timer = setTimeout(load, 10_000);
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

  const v3 = report?.v3;
  return (
    <Section
      id="bot"
      eyebrow="Bot Altim"
      title={
        <>
          Un bot qui doit <span className="gradient">prouver</span> avant de compter
        </>
      }
      intro="Des modèles appris sur de longs historiques disent ACHETER, ATTENDRE ou VENDRE. Ils ne pèsent dans une décision que s'ils ont battu le hasard sur des périodes qu'ils n'avaient jamais vues, avec une marge qui tient compte de tous les essais faits."
    >
      <div className="bot-site">
        <div className="card bot-site-how">
          <h3>Comment il est construit</h3>
          <ul>
            <li><b>ACHETER, ATTENDRE ou VENDRE</b> à 20 et 60 jours de bourse, frais compris ; VENDRE veut dire sortir ou alléger, jamais vendre à découvert.</li>
            <li><b>Plus de 20 ans de données</b> : actions et ETF américains depuis 1990, cryptos depuis leur cotation ; 125 actifs, dont 34 fixés d'avance pour le test.</li>
            <li><b>Plusieurs modèles candidats</b> : régressions, arbres de décision entraînés longuement avec arrêt anticipé, règles publiées ; et un classement entre pairs (faire mieux que la médiane de son groupe).</li>
            <li><b>Walk-forward</b> : réentraîné chaque année sur le seul passé, jugé sur l'année suivante qu'il n'a jamais vue.</li>
            <li><b>Pré-enregistré le 30/09/2026</b> : le protocole est figé avant les calculs ; tout ce qui arrive ensuite forme un <b>test sur l'avenir</b>, le seul vraiment neuf.</li>
            <li><b>Seuil corrigé</b> : chaque essai compte ; plus on essaie de modèles, plus la preuve exigée est forte.</li>
          </ul>
        </div>

        <div className="card bot-site-live" aria-live="polite">
          <h3>Ses résultats aujourd'hui</h3>
          {state === "loading" && <p className="muted">Chargement des résultats…</p>}
          {state === "pending" && <p className="muted">Entraînement et test en cours sur le serveur (plusieurs minutes la première fois)…</p>}
          {state === "error" && <p className="muted">Résultats indisponibles pour le moment.</p>}
          {state === "ready" && report && (
            <>
              <p className="bot-site-headline">{v3?.headline ?? report.headline}</p>
              {v3 && (
                <>
                  <dl className="bot-site-tiles">
                    <div><dt>t ≥ {plain(v3.tRequired, 2)}</dt><dd>seuil corrigé</dd></div>
                    <div><dt>{v3.k.total}</dt><dd>essais comptés depuis la v1</dd></div>
                    <div><dt>{forwardSignals(v3)}</dt><dd>signaux jugés depuis le {frIso(v3.preregDate)}</dd></div>
                  </dl>
                  <p className="muted small">{v3.forwardHeadline}</p>
                  <ul className="bot-site-rows">
                    {v3.groups.flatMap((g) =>
                      headlineConfigs(g).map(({ horizon, c }) => (
                        <li key={`${g.id}-${horizon}-${c.id}`}>
                          <span className="muted">{g.id === "stock" ? "Actions" : "Cryptos"} · {FAMILY_LABEL[c.family].toLowerCase()} · {horizon} j</span>
                          <span>achats : {verdictShort(c.main, "buy")}</span>
                          <span>ventes : {verdictShort(c.main, "sell")}</span>
                        </li>
                      )),
                    )}
                  </ul>
                </>
              )}
              <p className="muted small">Calculé le {new Date(report.asOf).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}, réentraîné toutes les 12 h.</p>
            </>
          )}
          <a href="/app/bot" className="btn btn-small">Voir le détail</a>
        </div>
      </div>
    </Section>
  );
}
