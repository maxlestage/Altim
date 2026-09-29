/**
 * « Bot Altim » v2: candidate models trained on long histories of the validation's basket and an extra universe
 * (/api/bot), chosen at each retraining on an inner validation and tested walk-forward on periods they had not seen,
 * saying ACHETER / ATTENDRE / VENDRE; today's view of the watched assets (/api/bot/views). Stacked cards, mobile
 * first; the per-asset list keeps the basket's order; each candidate is shown alone for information only.
 */
import { useEffect, useState } from "react";
import { api } from "./api";
import { onLink } from "./router";
import { useAppState } from "./store";
import { CLASS_SHORT, monthYear, plain, signedPct, verdictTone } from "./model-validation";
import {
  ACTION_UI, CANDIDATE_SHORT, buyText, calibrationRows, clusteredText, dataText, exitText, pct0, points, selectionRuns, sellText, skillText, waitText,
  type BotAction, type BotAssetRow, type BotGroupStat, type BotReport, type BotStats, type BotView, type BotViews, type Bucket, type CandidateStat,
} from "./model-bot";

export function Bot() {
  const [report, setReport] = useState<BotReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = () =>
      api.bot()
        .then((r) => {
          if (!alive) return;
          if ("pending" in r) {
            setPending(true);
            timer = setTimeout(load, 5_000);
          } else {
            setPending(false);
            setReport(r);
          }
        })
        .catch((e) => alive && setError(e instanceof Error ? e.message : "Bot indisponible"));
    load();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, []);

  return (
    <section className="app-screen validation bot">
      <div className="screen-top">
        <div>
          <h1>Bot Altim</h1>
          <p className="muted small">
            Des modèles appris sur de longs historiques (jusqu'à 20 ans), qui disent ACHETER, ATTENDRE ou VENDRE à 20 jours. Jugés seulement sur des périodes qu'ils
            n'avaient pas vues, sur 34 actifs fixés d'avance. Altim ne passe aucun ordre.
          </p>
        </div>
      </div>
      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && (
        <div className="card">
          <p className="muted">{pending ? "Téléchargement des historiques, entraînement et test en cours (une à deux minutes la première fois)…" : "Chargement…"}</p>
          <div className="skeleton" />
        </div>
      )}
      {report && <BotReportView report={report} />}
    </section>
  );
}

export function ActionChip({ action }: { action: BotAction | null }) {
  if (!action) return <span className="chip muted">pas d'avis</span>;
  const ui = ACTION_UI[action];
  return <span className={`chip bot-action ${ui.tone}`}>{ui.label}</span>;
}

function Verdict({ v, label }: { v: BotGroupStat["buy"]["verdict"]; label: string }) {
  return <span className={`chip val-verdict ${v ? verdictTone(v) : "info"}`}>{label}</span>;
}

const frDate = (s: string) => s.split("-").reverse().join("/");

function day(ms: number | null | undefined): string {
  return ms == null ? "?" : new Date(ms).toLocaleDateString("fr-FR", { timeZone: "UTC" });
}

export function BotReportView({ report: r }: { report: BotReport }) {
  const p = r.parameters;
  const extraFailures = r.extraFailures ?? [];
  const withCandidates = r.groups.filter((g) => g.candidates?.length);
  return (
    <>
      <div className="card val-head">
        <p className="val-headline">{r.headline}</p>
        <div className="val-tiles">
          <Tile label="Actifs testés" value={`${r.assets.length} / ${r.assets.length + r.failures.length}`} />
          <Tile label="Jours testés" value={r.overall.labelled.toLocaleString("fr-FR")} />
          <Tile label="Achats / ventes" value={`${r.overall.buy.signals} / ${r.overall.sell.signals}`} />
        </div>
        <p className="muted small">
          Horizon {p.horizonDays} jours · seuil : fréquence d'entraînement + {plain(p.thresholdMargin, 0)} points · coûts d'un aller-retour {plain(p.costStockPct, 2)}{NB}% (actions),{" "}
          {plain(p.costCryptoPct, 2)}{NB}% (cryptos). Calculé le {new Date(r.asOf).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}, réentraîné toutes les 12 h.
        </p>
        {r.failures.length > 0 && (
          <div className="notice warn small">
            {r.failures.length} actif{r.failures.length > 1 ? "s" : ""} non utilisé{r.failures.length > 1 ? "s" : ""} :
            <ul className="reasons">{r.failures.map((f) => <li key={f.symbol}><b>{f.symbol}</b> — {f.error}</li>)}</ul>
          </div>
        )}
        {extraFailures.length > 0 && (
          <p className="muted small">
            Univers élargi : {extraFailures.length} actif{extraFailures.length > 1 ? "s" : ""} indisponible{extraFailures.length > 1 ? "s" : ""} ({extraFailures.map((f) => f.symbol).join(", ")}).
          </p>
        )}
      </div>

      {r.changes && r.changes.length > 0 && (
        <div className="card">
          <h2 className="card-title">Ce qui change avec la v2</h2>
          <ul className="reasons">{r.changes.map((c) => <li key={c}>{c}</li>)}</ul>
        </div>
      )}

      <WatchedViews />

      <div className="card">
        <h2 className="card-title">Comment il apprend et comment il est jugé</h2>
        <ul className="reasons">{r.method.map((m) => <li key={m}>{m}</li>)}</ul>
        <details className="small">
          <summary>Les {r.features.length} mesures lues à chaque clôture</summary>
          <ul className="reasons">{r.features.map((f) => <li key={f.id}><b>{f.label}</b> — {f.help}</li>)}</ul>
        </details>
      </div>

      <h2 className="section-label">Résultats hors échantillon</h2>
      <p className="muted small">Modèle choisi à chaque réentraînement sur une validation interne (jamais sur le test), testé sur les actifs du panier.</p>
      <div className="val-grid">{r.groups.map((g) => <GroupCard key={g.id} g={g} />)}</div>

      {withCandidates.length > 0 && (
        <>
          <h2 className="section-label">Chaque modèle seul</h2>
          <p className="muted small">À titre d'information, non utilisé pour choisir : ce qu'aurait donné chaque candidat retenu partout, sur les mêmes jours.</p>
          <div className="val-grid">{withCandidates.map((g) => <CandidatesCard key={g.id} g={g} />)}</div>
        </>
      )}

      {r.groups.some((g) => g.holdout || g.extra) && (
        <>
          <h2 className="section-label">Dernière année et univers élargi</h2>
          <div className="val-grid">
            {r.groups.flatMap((g) => [
              g.holdout ? (
                <SubResult
                  key={`${g.id}-h`}
                  title={`${g.label} · 12 derniers mois`}
                  note={`Du ${day(g.holdout.from)} au ${day(g.holdout.to)}, présentés à part (même modèle choisi ; rien n'est choisi sur cette période).`}
                  s={g.holdout}
                />
              ) : null,
              g.extra ? (
                <SubResult
                  key={`${g.id}-x`}
                  title={`${g.label} · actifs d'entraînement hors panier`}
                  note={`${g.universe?.extra ?? 0} actifs fixés d'avance, hors du test principal ; eux aussi jugés hors échantillon.`}
                  s={g.extra}
                />
              ) : null,
            ])}
          </div>
        </>
      )}

      <h2 className="section-label">Calibration</h2>
      <p className="muted small">
        Quand le bot annonce une probabilité, la fréquence observée ensuite devrait être proche. Chaque ligne : jours de test dont la probabilité tombait dans la tranche.
      </p>
      <div className="val-grid">
        {r.groups.flatMap((g) => [
          <Calibration key={`${g.id}-up`} title={`${g.label} · hausse`} buckets={g.calibrationUp} skill={g.brierSkillUp} />,
          <Calibration key={`${g.id}-down`} title={`${g.label} · baisse`} buckets={g.calibrationDown} skill={g.brierSkillDown} />,
        ])}
      </div>

      <h2 className="section-label">Actif par actif</h2>
      <p className="muted small">Dans l'ordre du panier, jamais classés par performance. « Aujourd'hui » : avis du modèle retenu, entraîné sur tout l'historique connu.</p>
      <ul className="val-assets">{r.assets.map((a) => <AssetRow key={a.symbol} a={a} />)}</ul>

      <div className="card">
        <h2 className="card-title">Limites</h2>
        <ul className="reasons">{r.limits.map((l) => <li key={l}>{l}</li>)}</ul>
        <p className="muted small">
          Panier fixé le {frDate(r.basketFixedOn)}{r.extraFixedOn ? `, univers élargi le ${frDate(r.extraFixedOn)}` : ""}. Source : {r.source}.
          {r.timing ? ` Calcul : ${Math.round(r.timing.fetchMs / 1000)} s de téléchargement, ${Math.round(r.timing.computeMs / 1000)} s d'entraînement et de test.` : ""}{" "}
          <a href="/app/validation" onClick={onLink} className="link">Voir la validation du signal →</a>
        </p>
      </div>
    </>
  );
}

const NB = " ";

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="val-tile">
      <span className="muted small">{label}</span>
      <b>{value}</b>
    </div>
  );
}

export function GroupCard({ g }: { g: BotGroupStat }) {
  const data = dataText(g);
  const exit = exitText(g.sell.exit);
  const live = g.model?.candidate;
  return (
    <div className="card val-card">
      <h3 className="val-title">{g.label}</h3>
      <p className="muted small">
        {g.assets} actif{g.assets > 1 ? "s" : ""} testé{g.assets > 1 ? "s" : ""} · test du {monthYear(g.testFrom)} au {monthYear(g.testTo)} · {g.trainedBlocks} réentraînement{g.trainedBlocks > 1 ? "s" : ""}
      </p>
      {data && <p className="muted small">Données : {data}.</p>}
      <div className="bot-side">
        <p className="small bot-side-head"><ActionChip action="buy" /> <Verdict v={g.buy.verdict} label={g.buy.verdictLabel} /></p>
        <p className="small">{buyText(g.buy)}</p>
        {g.buy.clustered && <p className="muted small">{clusteredText(g.buy.clustered, g.buy.tStat)}</p>}
        <p className="kv small"><span>Achats gagnants / jours gagnants</span><b>{pct0(g.buy.hitRate)} / {pct0(g.buy.baselineHitRate)}</b></p>
        <p className="kv small"><span>Achats cumulés / détention (médianes)</span><b>{signedPct(g.buy.medianBotReturn)} / {signedPct(g.buy.medianHoldReturn)}</b></p>
      </div>
      <div className="bot-side">
        <p className="small bot-side-head"><ActionChip action="sell" /> <Verdict v={g.sell.verdict} label={g.sell.verdictLabel} /></p>
        <p className="small">{sellText(g.sell)}</p>
        {g.sell.clustered && <p className="muted small">{clusteredText(g.sell.clustered, g.sell.tStat)}</p>}
        <p className="kv small"><span>Suivies d'une baisse / tous les jours</span><b>{pct0(g.sell.fallRate)} / {pct0(g.sell.baselineFallRate)}</b></p>
        <p className="kv small"><span>Pire recul moyen ensuite / au hasard</span><b>{signedPct(g.sell.meanDrawdown)} / {signedPct(g.sell.baselineDrawdown)}</b></p>
        {exit && <p className="small">{exit}</p>}
      </div>
      <div className="bot-side">
        <p className="small bot-side-head"><ActionChip action="wait" /></p>
        <p className="small">{waitText(g.wait)}</p>
      </div>
      {g.selection && g.selection.length > 0 && (
        <div className="bot-side">
          <p className="small"><b>Modèle retenu à chaque réentraînement</b></p>
          <ul className="bot-runs">
            {selectionRuns(g.selection).map((x) => (
              <li key={x.from} className="small">
                <span className="muted">{monthYear(x.from)}{x.count > 1 ? ` → ${monthYear(x.to)} (${x.count} fois)` : ""}</span>{" "}
                <b>{x.chosen ? CANDIDATE_SHORT[x.chosen] : "aucun (trop peu de données)"}</b>
              </li>
            ))}
          </ul>
          {live && <p className="muted small">Aujourd'hui : {CANDIDATE_SHORT[live]}, choisi de la même façon sur la dernière année connue.</p>}
        </div>
      )}
    </div>
  );
}

/** Each candidate's own out-of-sample result (for information, never used to choose): stacked rows, no table. */
export function CandidatesCard({ g }: { g: BotGroupStat }) {
  return (
    <div className="card val-card">
      <h3 className="val-title">{g.label}</h3>
      <ul className="bot-cands">{(g.candidates ?? []).map((c) => <CandidateRow key={c.id} c={c} />)}</ul>
    </div>
  );
}

function CandidateRow({ c }: { c: CandidateStat }) {
  return (
    <li>
      <p className="small bot-side-head">
        <b>{c.label}</b> <span className="chip muted">retenu {c.chosenBlocks} fois sur {c.trainedBlocks}</span>
      </p>
      <p className="muted small">{c.description}</p>
      <p className="kv small"><span>{c.buy.signals} achats · écart au hasard</span><b>{points(c.buy.excess)} (t {plain(c.buy.tStat, 1)})</b></p>
      <p className="kv small"><span>{c.sell.signals} ventes · baisse évitée</span><b>{points(c.sell.avoided)} (t {plain(c.sell.tStat, 1)})</b></p>
      <p className="kv small"><span>Précision hausse / baisse</span><b>{plain(c.brierSkillUp, 1)}{NB}% / {plain(c.brierSkillDown, 1)}{NB}%</b></p>
      <p className="small bot-side-head">
        <Verdict v={c.buy.verdict} label={`Achats : ${c.buy.verdictLabel.charAt(0).toLowerCase()}${c.buy.verdictLabel.slice(1)}`} />
        <Verdict v={c.sell.verdict} label={`Ventes : ${c.sell.verdictLabel.charAt(0).toLowerCase()}${c.sell.verdictLabel.slice(1)}`} />
      </p>
    </li>
  );
}

/** The last 12 months, or the extra training assets: the same figures, shorter. */
function SubResult({ title, note, s }: { title: string; note: string; s: BotStats }) {
  const exit = exitText(s.sell.exit);
  return (
    <div className="card val-card">
      <h3 className="val-title">{title}</h3>
      <p className="muted small">{note}</p>
      <p className="small bot-side-head"><ActionChip action="buy" /> <Verdict v={s.buy.verdict} label={s.buy.verdictLabel} /></p>
      <p className="small">{buyText(s.buy)}</p>
      <p className="small bot-side-head"><ActionChip action="sell" /> <Verdict v={s.sell.verdict} label={s.sell.verdictLabel} /></p>
      <p className="small">{sellText(s.sell)}</p>
      {exit && <p className="muted small">{exit}</p>}
    </div>
  );
}

/** Predicted vs realised per probability bucket: two thin bars per row, the numbers written next to them. */
export function Calibration({ title, buckets, skill }: { title: string; buckets: Bucket[]; skill: number | null }) {
  const rows = calibrationRows(buckets);
  return (
    <div className="card val-card">
      <h3 className="val-title">{title}</h3>
      <p className="muted small">Précision : {skillText(skill)}.</p>
      <ul className="bot-calib">
        {rows.map((b) => (
          <li key={b.label}>
            <p className="small"><b>{b.label}</b> <span className="muted">· {b.rows.toLocaleString("fr-FR")} jours</span></p>
            <p className="small">prévu {pct0(b.predicted)} · observé <b>{pct0(b.realised)}</b></p>
            <div className="bot-bars" role="img" aria-label={`Prévu ${pct0(b.predicted)}, observé ${pct0(b.realised)}`}>
              <span className="pred" style={{ width: `${Math.max(0, Math.min(100, b.predicted ?? 0))}%` }} />
              <span className="real" style={{ width: `${Math.max(0, Math.min(100, b.realised ?? 0))}%` }} />
            </div>
          </li>
        ))}
      </ul>
      <p className="muted small"><span className="bot-key pred" /> prévu · <span className="bot-key real" /> observé</p>
    </div>
  );
}

export function AssetRow({ a }: { a: BotAssetRow }) {
  return (
    <li className="val-asset">
      <div className="val-asset-top">
        <a href={`/app/actif/${a.kind}/${a.symbol}`} onClick={onLink} className="val-sym"><b>{a.symbol}</b> <span className="muted small">{a.name}</span></a>
        <span className="chip muted">{CLASS_SHORT[a.class]}</span>
        <ActionChip action={a.now.action} />
      </div>
      <p className="small">
        Aujourd'hui : hausse {pct0(a.now.up)}, baisse {pct0(a.now.down)}
      </p>
      <p className="muted small">
        Test : {a.buys} achat{a.buys > 1 ? "s" : ""}{a.buyExcess != null && ` (${points(a.buyExcess)} vs hasard)`} · {a.sells} vente{a.sells > 1 ? "s" : ""}
        {a.sellAvoided != null && ` (cours ensuite ${points(-a.sellAvoided)} vs hasard)`} · attente {pct0(a.waitShare)} · achats cumulés {signedPct(a.botReturn)}, détention{" "}
        {signedPct(a.holdReturn)}
        {a.outShare != null && ` · hors marché après VENDRE ${pct0(a.outShare)} du temps, pire baisse ${signedPct(a.botMaxDrawdown)} contre ${signedPct(a.holdMaxDrawdown)} en gardant`}
        {" "}({a.source}{a.years != null ? `, ${plain(a.years, 1)} ans` : ""})
      </p>
    </li>
  );
}

/** Today's view of the watched assets (cached report only; nothing is trained here). */
function WatchedViews() {
  const { watchlist } = useAppState();
  const [data, setData] = useState<BotViews | null>(null);
  const [error, setError] = useState<string | null>(null);
  const key = watchlist.map((w) => `${w.symbol}:${w.kind}`).join(",");
  useEffect(() => {
    if (!watchlist.length) return;
    let alive = true;
    api.botViews(watchlist)
      .then((d) => alive && setData(d))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "indisponible"));
    return () => {
      alive = false;
    };
  }, [key]);
  if (!watchlist.length) return null;
  return (
    <div className="card">
      <h2 className="card-title">Vos actifs aujourd'hui</h2>
      {error && <p className="muted small">Avis indisponibles ({error}).</p>}
      {!data && !error && <p className="muted small">Chargement…</p>}
      {data && (
        <ul className="bot-views">
          {data.views.map((v) => <ViewRow key={`${v.kind}:${v.symbol}`} v={v} />)}
        </ul>
      )}
    </div>
  );
}

function ViewRow({ v }: { v: BotView & { symbol: string; kind: string } }) {
  return (
    <li>
      <div className="val-asset-top">
        <a href={`/app/actif/${v.kind}/${v.symbol}`} onClick={onLink} className="val-sym"><b>{v.symbol}</b></a>
        <ActionChip action={v.action} />
        <span className={`chip ${v.counts ? "" : "muted"}`}>{v.counts ? "compte" : "ne compte pas"}</span>
      </div>
      <p className="small muted">
        {v.available && v.action
          ? `Hausse ${pct0(v.up)} (seuil ${pct0(v.thresholdUp)}), baisse ${pct0(v.down)} (seuil ${pct0(v.thresholdDown)})${v.modelLabel ? ` · ${v.modelLabel}` : ""}${v.inBasket ? "" : " · hors du panier testé"}`
          : v.text}
      </p>
    </li>
  );
}
