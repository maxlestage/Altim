/**
 * « Bot Altim » v3 on the Bot screen: what changed, the pre-registration date, the corrected threshold (K tests), the
 * 4 headline results (2 families × horizons 20 / 60) with raw and required t, the forward test, each configuration
 * alone for information, the volatility-managed trend vs holding, and the time / memory of the computation. Stacked
 * cards and rows (no table), mobile first.
 */
import { monthYear, plain, signedPct, verdictTone } from "./model-validation";
import {
  FAMILY_LABEL, V3_SHORT, computeText, judged, pendingText, proven, tLine, forwardSignals, forwardText, frIso, headlineConfigs, points, tVsRequired, v3SideText, v3VerdictLabel, volRows,
  type BotReport, type SideStats, type V3BlockOut, type V3Candidate, type V3Config, type V3Group, type V3Report, type VolManaged,
} from "./model-bot";

function Chip({ s, required }: { s: SideStats; required: number }) {
  return <span className={`chip val-verdict ${s.verdict ? verdictTone(s.verdict) : "info"}`}>{v3VerdictLabel(s, required)}</span>;
}

/** Runs of identical choices over the retrainings of one family. */
function choiceRuns(blocks: V3BlockOut[], pick: (b: V3BlockOut) => V3Candidate | null) {
  const out: { from: number; to: number; chosen: V3Candidate | null; count: number }[] = [];
  for (const b of blocks) {
    const c = pick(b);
    const last = out[out.length - 1];
    if (last && last.chosen === c) {
      last.to = b.start;
      last.count++;
    } else out.push({ from: b.start, to: b.start, chosen: c, count: 1 });
  }
  return out;
}

export function BotV3Section({ v3, timing }: { v3: V3Report; timing: BotReport["timing"] }) {
  const req = v3.tRequired;
  const fwd = forwardSignals(v3);
  const compute = computeText(timing);
  return (
    <>
      <div className="card">
        <h2 className="card-title">Bot v3 · pré-enregistré le {frIso(v3.preregDate)}</h2>
        <div className="val-tiles">
          <div className="val-tile"><span className="muted small">Seuil corrigé</span><b>t ≥ {plain(req, 2)}</b></div>
          <div className="val-tile"><span className="muted small">Tests comptés (v1 à v3)</span><b>{v3.k.total}</b></div>
          <div className="val-tile"><span className="muted small">Test sur l'avenir</span><b>{fwd} signal{fwd > 1 ? "s" : ""}</b></div>
        </div>
        <p className="muted small">
          Protocole écrit avant tout calcul et figé : {v3.k.v1} tests en v1, {v3.k.v2} en v2, {v3.k.v3} en v3. Avec autant d'essais, un t de 2 arrive par hasard ;
          un avantage n'est dit démontré qu'au-delà de t = {plain(req, 2)} (Bonferroni, 5 %). Horizons jugés à part : {v3.parameters.horizons.join(" et ")} jours de bourse.
        </p>
        {v3.afterPrereg.length > 0 && (
          <div className="notice warn small">
            Modifié après le pré-enregistrement :
            <ul className="reasons">{v3.afterPrereg.map((c) => <li key={c}>{c}</li>)}</ul>
          </div>
        )}
        <details className="small">
          <summary>Ce qui change avec la v3</summary>
          <ul className="reasons">{v3.changes.map((c) => <li key={c}>{c}</li>)}</ul>
        </details>
      </div>

      <h2 className="section-label">Résultats v3 hors échantillon</h2>
      <p className="muted small">Panier fixe, signaux datés jusqu'au {frIso(v3.preregDate)} ; modèle choisi à chaque réentraînement sur une validation interne, jamais sur le test.</p>
      <div className="val-grid">{v3.groups.map((g) => <V3GroupCard key={g.id} g={g} req={req} />)}</div>

      <h2 className="section-label">Depuis le {frIso(v3.preregDate)} (test sur l'avenir)</h2>
      <div className="card">
        <p className="small">{v3.forwardHeadline}</p>
        <p className="muted small">
          Le seul test vraiment neuf : ces signaux n'existaient pas quand le protocole a été figé. Même calcul, même seuil ; un avantage passé qu'il contredit (après au
          moins {v3.parameters.minSignals} signaux) cesse de compter.
        </p>
        <ul className="bot-cands">
          {v3.groups.flatMap((g) =>
            headlineConfigs(g).map(({ horizon, c }) => (
              <li key={`${g.id}-${horizon}-${c.id}`}>
                <p className="small"><b>{g.label}</b> · {FAMILY_LABEL[c.family]} · {horizon} jours</p>
                <p className="muted small">{forwardText(c.forward, req)}</p>
              </li>
            )),
          )}
        </ul>
      </div>

      <h2 className="section-label">Chaque configuration seule</h2>
      <p className="muted small">À titre d'information, non utilisé pour choisir : chaque candidat retenu partout, et la sélection v2 refaite sur les nouvelles données.</p>
      <div className="val-grid">
        {v3.groups.flatMap((g) => g.horizons.map((h) => <ConfigsCard key={`${g.id}-${h.horizon}`} title={`${g.label} · ${h.horizon} jours`} configs={h.configs} req={req} />))}
      </div>

      {v3.groups.some((g) => g.volManaged) && (
        <>
          <h2 className="section-label">Tendance à volatilité gérée</h2>
          <p className="muted small">
            Règle publiée, sans apprentissage : investi seulement au-dessus de la moyenne 200 jours, exposition réduite quand la volatilité dépasse sa médiane passée ;
            comparée à la simple détention, sur les mêmes jours de test.
          </p>
          <div className="val-grid">{v3.groups.map((g) => (g.volManaged ? <VolCard key={g.id} label={g.label} v={g.volManaged} /> : null))}</div>
        </>
      )}

      <div className="card">
        <h2 className="card-title">Comment la v3 est jugée</h2>
        <ul className="reasons">{v3.method.map((m) => <li key={m}>{m}</li>)}</ul>
        <details className="small">
          <summary>Les {v3.candidates.length} candidats</summary>
          <ul className="reasons">{v3.candidates.map((c) => <li key={c.id}><b>{c.label}</b> ({FAMILY_LABEL[c.family]}) — {c.description}</li>)}</ul>
        </details>
        <details className="small">
          <summary>Limites de la v3</summary>
          <ul className="reasons">{v3.limits.map((l) => <li key={l}>{l}</li>)}</ul>
        </details>
        {compute && (
          <p className="muted small">
            Calcul : {compute} ; {v3.compute.blocks} réentraînements, jusqu'à {v3.compute.maxRows.toLocaleString("fr-FR")} jours × actifs par groupe et horizon.
          </p>
        )}
      </div>
    </>
  );
}

export function V3GroupCard({ g, req }: { g: V3Group; req: number }) {
  return (
    <div className="card val-card">
      <h3 className="val-title">{g.label}</h3>
      <p className="muted small">
        {g.universe.basket} actifs testés, {g.universe.extra} de plus à l'entraînement · historique médian {plain(g.universe.medianYears, 1)} ans
        {g.peersFrom != null ? ` · classement entre pairs possible depuis ${monthYear(g.peersFrom)}` : ""}
      </p>
      {g.horizons.map((h) => (
        <div key={h.horizon} className="bot-v3-horizon">
          <p className="small"><b>À {h.horizon} jours</b> <span className="muted">· test de {monthYear(h.testFrom)} à {monthYear(h.testTo)} · {h.blocks} réentraînements</span></p>
          {h.configs.filter((c) => c.headline).map((c) => (
            <HeadlineConfig key={c.id} c={c} req={req} runs={choiceRuns(h.selection, (b) => (c.family === "peers" ? b.peers : b.absolute))} />
          ))}
        </div>
      ))}
    </div>
  );
}

function HeadlineConfig({ c, req, runs }: { c: V3Config; req: number; runs: ReturnType<typeof choiceRuns> }) {
  const m = c.main;
  return (
    <div className="bot-side">
      <p className="small"><b>{FAMILY_LABEL[c.family]}</b></p>
      <p className="small">{v3SideText(c.family, "buy", m.buy, req)}</p>
      <p className="small bot-side-head"><span className="muted">ACHETER{m.buyVsMean ? " · face à la médiane" : ""}</span> <Chip s={m.buy} required={req} /></p>
      {m.buyVsMean && <Control s={m.buyVsMean} req={req} proven={proven(m, "buy")} />}
      <p className="small">{v3SideText(c.family, "sell", m.sell, req)}</p>
      <p className="small bot-side-head"><span className="muted">VENDRE{m.sellVsMean ? " · face à la médiane" : ""}</span> <Chip s={m.sell} required={req} /></p>
      {m.sellVsMean && <Control s={m.sellVsMean} req={req} proven={proven(m, "sell")} />}
      {(["buy", "sell"] as const).map((side) => {
        const p = pendingText(c, side, req);
        return p ? <p key={side} className="notice warn small">{p}</p> : null;
      })}
      {m.holdAssets > 0 && (
        <p className="kv small"><span>Achats cumulés / détention (médianes)</span><b>{signedPct(m.medianBotReturn, 0)} / {signedPct(m.medianHoldReturn, 0)}</b></p>
      )}
      <details className="small">
        <summary>Modèle retenu à chaque réentraînement</summary>
        <p className="muted small">{runs.map((r) => `${r.chosen ? V3_SHORT[r.chosen] : "aucun"}${r.count > 1 ? ` (${r.count} fois)` : ""}`).join(" → ")}</p>
      </details>
    </div>
  );
}

/** Family B against the group's equal-weight mean (control added after the first real run), and what counts. */
function Control({ s, req, proven: ok }: { s: SideStats; req: number; proven: boolean }) {
  return (
    <div className="bot-v3-control">
      <p className="small bot-side-head">
        <span className="muted">Face à la moyenne du groupe (contrôle ajouté après coup) : {points(s.excess)} ({tLine(s, req)})</span>
        <Chip s={s} required={req} />
      </p>
      <p className="small"><b>{ok ? "Sur le passé : avantage face aux deux références." : "Retenu : non démontré (il faut les deux références) ; ne compte pas."}</b></p>
    </div>
  );
}

/** Every configuration of a group and horizon (headline included), compact rows. */
export function ConfigsCard({ title, configs, req }: { title: string; configs: V3Config[]; req: number }) {
  return (
    <div className="card val-card">
      <h3 className="val-title">{title}</h3>
      <ul className="bot-cands">
        {configs.map((c) => (
          <li key={c.id}>
            <p className="small bot-side-head">
              <b>{c.label}</b>
              {c.headline && <span className="chip">principal</span>}
              {c.candidate && <span className="chip muted">retenu {c.chosenBlocks} fois sur {c.trainedBlocks}</span>}
            </p>
            <p className="kv small"><span>{c.main.buy.signals} achats</span><b>{points(c.main.buy.excess)} ({tVsRequired(c.main.buy.t, req)})</b></p>
            <p className="kv small"><span>{c.main.sell.signals} ventes</span><b>{points(c.main.sell.excess)} ({tVsRequired(c.main.sell.t, req)})</b></p>
            {c.main.buyVsMean && (
              <p className="kv small"><span>Face à la moyenne</span><b>{points(judged(c.main, "buy").excess)} / {points(judged(c.main, "sell").excess)} (t {plain(judged(c.main, "buy").t, 1)} / {plain(judged(c.main, "sell").t, 1)})</b></p>
            )}
            <p className="small bot-side-head"><Chip s={c.main.buy} required={req} /><Chip s={c.main.sell} required={req} /></p>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function VolCard({ label, v }: { label: string; v: VolManaged }) {
  return (
    <div className="card val-card">
      <h3 className="val-title">{label}</h3>
      <p className="muted small">{v.assets} actifs du panier, de {monthYear(v.from)} à {monthYear(v.to)} · portefeuille à parts égales</p>
      <ul className="bot-vol">
        {volRows(v).map((r) => (
          <li key={r.label} className="small">
            <span>{r.label}</span>
            <span>gérée <b>{r.managed}</b></span>
            <span className="muted">détention {r.hold}</span>
          </li>
        ))}
      </ul>
      <p className="small">{v.text}</p>
    </div>
  );
}
