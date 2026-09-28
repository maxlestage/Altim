import { useEffect, useMemo, useState } from "react";
import {
  journalProfile, MIN_SAMPLE, REVIEW_DAYS, reviewEntry, type ClosedInfo, type EntryReview, type GroupStat, type HorizonReview, type JournalEntry, type ReviewDays,
} from "../engine/journal";
import type { Candle } from "../engine/signal";
import { api } from "./api";
import { deleteJournalEntry, setJournalNote, useJournal } from "./journal-store";
import { usePaper } from "./paper-store";
import { frDate, price as fmtPrice, REASON_LABEL } from "./paper-ui";
import { onLink } from "./router";
import { PortfolioTabs } from "./Simulation";

const fr = (v: number, d = 1) => v.toLocaleString("fr-FR", { maximumFractionDigits: d });
const signed = (v: number, d = 1) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${fr(Math.abs(v), d)}`;
const rText = (v: number | null) => (v == null ? "—" : `${signed(v, 2)} R`);
const pctText = (v: number | null) => (v == null ? "—" : `${signed(v)} %`);
const STEP_ICON: Record<string, string> = { ok: "✓", no: "✕", unknown: "?" };
const MACRO_LABEL: Record<string, string> = { calm: "calme", tense: "tendu", high: "très tendu" };

/** "Pourquoi je suis entré" (optional), in the order sheets. */
export function JournalNote({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <label className="field">
      <span>Pourquoi j'entre (facultatif, pour le journal)</span>
      <textarea rows={2} maxLength={1000} value={value} onChange={(e) => onChange(e.target.value)} placeholder="ex. rebond sur le support, objectif 1 visé" />
    </label>
  );
}

/** Checkbox "inscrire au journal" + the note, in « Mes avoirs ». */
export function JournalToggle({ checked, onChange, note, onNote, text }: { checked: boolean; onChange: (v: boolean) => void; note: string; onNote: (v: string) => void; text: string }) {
  return (
    <div className="journal-toggle">
      <label className="check-row">
        <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
        <span>{text}</span>
      </label>
      {checked && <JournalNote value={note} onChange={onNote} />}
    </div>
  );
}

type CandleState = Record<string, Candle[] | null | undefined>;

/** Journal screen (/app/journal): every entry with its automatic review, and the profile of the results. */
export function Journal() {
  const { state, error } = useJournal();
  const { state: paper } = usePaper();
  const entries = state.entries;
  const [candles, setCandles] = useState<CandleState>({});
  const [horizon, setHorizon] = useState<ReviewDays>(10);
  const assetsKey = [...new Set(entries.map((e) => `${e.kind}:${e.symbol}`))].sort().join(",");

  // Daily candles of every asset of the journal, once per opening (cached by the server).
  useEffect(() => {
    let alive = true;
    const keys = assetsKey ? assetsKey.split(",") : [];
    for (const k of keys) {
      const [kind, symbol] = k.split(":") as ["crypto" | "stock", string];
      api.candles(symbol, kind, "1d")
        .then((s) => alive && setCandles((m) => ({ ...m, [k]: s.candles })))
        .catch(() => alive && setCandles((m) => ({ ...m, [k]: null })));
    }
    return () => {
      alive = false;
    };
  }, [assetsKey]);

  const closedById = useMemo(() => {
    const m = new Map<string, ClosedInfo>();
    for (const t of paper?.trades ?? []) m.set(t.id, { at: t.closedAt, price: t.exit, reason: REASON_LABEL[t.reason] });
    return m;
  }, [paper]);

  const now = Date.now();
  const reviews = useMemo(
    () => entries.map((e) => reviewEntry(e, candles[`${e.kind}:${e.symbol}`] ?? [], now, e.source === "paper" && e.refId ? closedById.get(e.refId) ?? null : null)),
    [entries, candles, closedById],
  );
  const profile = useMemo(() => journalProfile(reviews, horizon), [reviews, horizon]);
  const newest = [...reviews].sort((a, b) => b.entry.createdAt - a.entry.createdAt);

  return (
    <section className="app-screen journal-screen">
      <PortfolioTabs active="journal" />
      <div className="screen-top">
        <div>
          <h1>Journal</h1>
          <p className="muted small">Enregistré uniquement dans ce navigateur · {entries.length} entrée{entries.length > 1 ? "s" : ""}</p>
        </div>
      </div>
      <p className="muted small">
        Chaque achat simulé et chaque achat ou vente réel enregistré dans « Mes avoirs » est noté ici avec la décision affichée à ce moment-là, puis revu
        automatiquement à 3, 10 et 30 jours sur les bougies journalières suivantes. Des faits, pas des jugements : un bon trade peut perdre, un mauvais peut gagner.
      </p>
      {error && <p className="notice warn" role="alert">⚠ {error}</p>}

      {!entries.length ? (
        <div className="card empty-card">
          <h2>Aucune entrée pour l'instant</h2>
          <p className="muted">
            Simulez un achat depuis la carte « Décision » d'un actif, ou enregistrez un achat ou une vente dans{" "}
            <a href="/app/avoirs" onClick={onLink} className="link">Mes avoirs</a> : l'entrée apparaîtra ici.
          </p>
        </div>
      ) : (
        <>
          <ProfileCard profile={profile} horizon={horizon} onHorizon={setHorizon} />
          <h2 className="section-label">Entrées</h2>
          <ul className="journal-list">
            {newest.map((r) => (
              <EntryCard key={r.entry.id} r={r} loaded={candles[`${r.entry.kind}:${r.entry.symbol}`]} />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

function ProfileCard({ profile, horizon, onHorizon }: { profile: ReturnType<typeof journalProfile>; horizon: ReviewDays; onHorizon: (h: ReviewDays) => void }) {
  return (
    <div className="card">
      <h2 className="card-title">Votre profil</h2>
      <div className="agenda-chips" role="group" aria-label="Horizon">
        {REVIEW_DAYS.map((d) => (
          <button key={d} className={`chip pick ${d === horizon ? "on" : ""}`} aria-pressed={d === horizon} onClick={() => onHorizon(d)}>À {d} jours</button>
        ))}
      </div>
      <p className="muted small">
        {profile.reviewed} achat{profile.reviewed > 1 ? "s" : ""} sur {profile.purchases} revu{profile.reviewed > 1 ? "s" : ""} à {horizon} jours
        {profile.pending > 0 && ` · ${profile.pending} pas encore à ${horizon} jours`}. Résultats en R (multiples du risque pris, entrée − stop) et en pire recul :
        un gain obtenu en prenant plus de risque ne compte pas davantage. Sortie supposée au premier niveau touché (stop ou objectif 1), sinon au dernier cours.
      </p>
      {profile.all ? (
        <>
          <GroupRows title="Ensemble" groups={[profile.all]} />
          <GroupRows title="Par note à l'entrée" groups={profile.byRating} />
          <GroupRows title="Plan respecté ou non" groups={profile.byPlan} />
          <GroupRows title="Par régime de marché" groups={profile.byRegime} />
        </>
      ) : (
        <p className="muted small">Aucun achat n'a encore {horizon} jours d'historique après l'entrée.</p>
      )}
    </div>
  );
}

function GroupRows({ title, groups }: { title: string; groups: GroupStat[] }) {
  if (!groups.length) return null;
  return (
    <div className="journal-groups">
      <h3 className="small">{title}</h3>
      <ul>
        {groups.map((g) => (
          <li key={g.key} className="journal-group">
            <div className="journal-group-head">
              <b>{g.label}</b>
              <span className="muted small">{g.n} achat{g.n > 1 ? "s" : ""}{g.withStop < g.n ? ` (${g.withStop} avec stop)` : ""}</span>
              {g.lowSample && <span className="chip muted">échantillon trop faible (&lt; {MIN_SAMPLE})</span>}
            </div>
            <div className="journal-figures">
              <div><small>Résultat moyen</small><b className={(g.avgR ?? 0) > 0 ? "up" : (g.avgR ?? 0) < 0 ? "down" : ""}>{rText(g.avgR)}</b></div>
              <div><small>Médiane</small><b>{rText(g.medianR)}</b></div>
              <div><small>Positifs</small><b>{g.winRate == null ? "—" : `${fr(g.winRate, 0)} %`}</b></div>
              <div><small>Recul moyen</small><b>{g.avgMaeR != null ? rText(g.avgMaeR) : pctText(g.avgMaePct)}</b></div>
              <div><small>Pire recul</small><b>{rText(g.worstMaeR)}</b></div>
              <div><small>Variation moy.</small><b>{pctText(g.avgReturnPct)}</b></div>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

function HorizonRow({ h, buy }: { h: HorizonReview; buy: boolean }) {
  if (h.status === "pending") return <li className="muted small"><b>{h.days} j</b> : disponible le {frDate(h.availableAt)}.</li>;
  if (h.status === "noData") return <li className="muted small"><b>{h.days} j</b> : pas de bougie journalière après l'entrée dans l'historique disponible.</li>;
  const first = h.first === "stop" ? `stop touché en ${h.firstDays} j` : h.first === "target1" ? `objectif 1 atteint en ${h.firstDays} j${h.target2 ? ", puis objectif 2" : ""}` : "ni stop ni objectif";
  return (
    <li className="small">
      <b>{h.days} j</b> : {pctText(h.returnPct)} au dernier cours · plus haut {pctText(h.mfePct)}{h.mfeR != null && ` (${rText(h.mfeR)})`} · plus bas {pctText(h.maePct)}
      {h.maeR != null && ` (${rText(h.maeR)})`}
      {buy && <> · {first}{h.resultR != null && <> · résultat selon le plan <b>{rText(h.resultR)}</b></>}</>}
    </li>
  );
}

function EntryCard({ r, loaded }: { r: EntryReview; loaded: Candle[] | null | undefined }) {
  const e: JournalEntry = r.entry;
  const d = e.decision;
  const [editing, setEditing] = useState(false);
  const [note, setNote] = useState(e.note);
  const buy = e.side === "buy";
  const m = e.market;
  const cls = r.coherent == null ? "" : r.coherent ? "buy" : "hold";
  return (
    <li className={`card journal-entry ${cls}`}>
      <div className="journal-head">
        <a href={`/app/actif/${e.kind}/${e.symbol}`} onClick={onLink} className="holding-name">
          <b>{buy ? "Achat" : "Vente"} · {e.name}</b>
          <small className="muted mono">{e.symbol} · {frDate(e.createdAt, true)}</small>
        </a>
        <span className={`chip ${e.source === "paper" ? "" : "muted"}`}>{e.source === "paper" ? "simulé" : "réel"}</span>
      </div>
      <div className="journal-figures">
        <div><small>Prix</small><b className="mono">{fmtPrice(e.price)}</b></div>
        {buy && <div><small>Stop{r.levels.stopSource === "plan" ? " (plan)" : ""}</small><b className="mono">{r.levels.stop != null ? fmtPrice(r.levels.stop) : "aucun"}</b></div>}
        {buy && <div><small>Objectifs{r.levels.targetsSource === "plan" ? " (plan)" : ""}</small><b className="mono">{r.levels.targets.length ? r.levels.targets.map(fmtPrice).join(" · ") : "aucun"}</b></div>}
        {buy && <div><small>Gain visé</small><b>{rText(r.planR)}</b></div>}
      </div>
      <p className="small"><span className="muted">Signal utilisé : </span>{e.signal}</p>

      <details className="journal-details">
        <summary>Pourquoi, et le contexte de marché</summary>
        {d ? (
          <>
            <p className="small"><b>{d.ratingLabel ?? d.label}</b> · confiance {Math.round(d.confidence)}/100{d.score != null && ` · score ${Math.round(d.score)}/100`} · décision du {frDate(d.asOf, true)}</p>
            <p className="small">{d.headline}</p>
            {d.degraded && <p className="notice warn small">⚠ Signal dégradé : {d.degradedHeadline}</p>}
            {d.vetoes.length > 0 && <p className="notice danger small">⛔ Interdictions d'achat actives : {d.vetoes.join(", ")}</p>}
            {d.pros.length > 0 && <ul className="reasons">{d.pros.map((p) => <li key={p}>＋ {p}</li>)}</ul>}
            {d.cons.length > 0 && <ul className="reasons">{d.cons.map((c) => <li key={c}>－ {c}</li>)}</ul>}
            <p className="small muted">Configuration « {d.setup.name} » : {d.setup.met}/{d.setup.total}</p>
            <ul className="dec-chips">{d.setup.steps.map((s) => <li key={s.label} className="dec-chip"><span aria-hidden>{STEP_ICON[s.state] ?? "?"}</span> {s.label}</li>)}</ul>
            {d.plan && (
              <p className="small muted">
                Plan : zone {fmtPrice(Math.min(d.plan.zoneFrom, d.plan.zoneTo))} – {fmtPrice(Math.max(d.plan.zoneFrom, d.plan.zoneTo))}, stop {fmtPrice(d.plan.stop)}, objectif 1 {fmtPrice(d.plan.target1)}
                {d.plan.target2 != null && `, objectif 2 ${fmtPrice(d.plan.target2)}`}, rapport gain / risque {fr(d.plan.riskReward)}.
              </p>
            )}
          </>
        ) : (
          <p className="small muted">Aucune décision chargée pour cet actif à ce moment-là.</p>
        )}
        <ul className="dec-chips">
          <li className="dec-chip">Régime : {m.regimeLabel ?? "inconnu"}</li>
          <li className="dec-chip">Stress macro : {m.macroScore != null ? `${Math.round(m.macroScore)}/100 (${MACRO_LABEL[m.macroLevel ?? ""] ?? m.macroLevel})` : "inconnu"}</li>
          <li className="dec-chip">ATR : {m.atrPct != null ? `${fr(m.atrPct, 2)} %/jour` : "inconnu"}</li>
          <li className="dec-chip">Volume relatif : {m.relativeVolume != null ? `${fr(m.relativeVolume, 2)}×` : "inconnu"}</li>
          <li className="dec-chip">Événements à 7 j : {m.events ?? "inconnu"}</li>
        </ul>
      </details>

      {editing ? (
        <div>
          <JournalNote value={note} onChange={setNote} />
          <div className="row-actions">
            <button className="btn btn-small" onClick={() => { setJournalNote(e.id, note); setEditing(false); }}>Enregistrer</button>
            <button className="btn btn-small btn-ghost" onClick={() => { setNote(e.note); setEditing(false); }}>Annuler</button>
          </div>
        </div>
      ) : (
        <p className="small">
          <span className="muted">{buy ? "Pourquoi je suis entré" : "Pourquoi j'ai vendu"} : </span>{e.note || <i className="muted">pas de note</i>}{" "}
          <button className="link-btn" onClick={() => setEditing(true)}>{e.note ? "Modifier" : "Ajouter"}</button>
        </p>
      )}

      <div className="journal-review">
        <h3 className="small">Revue automatique</h3>
        {loaded === null && <p className="notice warn small">⚠ Cours journaliers indisponibles pour l'instant : revue non calculée.</p>}
        {loaded === undefined && <p className="muted small">Chargement des cours…</p>}
        <ul className="reasons">{r.horizons.map((h) => <HorizonRow key={h.days} h={h} buy={buy} />)}</ul>
        {r.closed && (
          <p className="small">Clôturée le {frDate(r.closed.at)} ({r.closed.reason}) à {fmtPrice(r.closed.price)}{r.realizedR != null && <> : <b>{rText(r.realizedR)}</b> réalisé</>}.</p>
        )}
        {r.worked.length > 0 && (
          <>
            <h4 className="small">Qu'est-ce qui a fonctionné ?</h4>
            <ul className="insights">{r.worked.map((t) => <li key={t} className="insight good"><span aria-hidden>✔</span><span>{t}</span></li>)}</ul>
          </>
        )}
        {r.failed.length > 0 && (
          <>
            <h4 className="small">Qu'est-ce qui n'a pas fonctionné ?</h4>
            <ul className="insights">{r.failed.map((t) => <li key={t} className="insight warning"><span aria-hidden>⚠</span><span>{t}</span></li>)}</ul>
          </>
        )}
        <h4 className="small">Le signal était-il cohérent avec les données disponibles à ce moment-là ?</h4>
        <p className="small"><b>{r.coherent == null ? "Non vérifiable" : r.coherent ? "Oui" : "Non"}</b>{r.coherent === false && " : au moins un point ci-dessous ne l'était pas."}</p>
        <ul className="journal-checks">
          {r.coherence.map((c) => (
            <li key={c.code} className="small">
              <span aria-hidden>{c.ok == null ? "?" : c.ok ? "✓" : "✕"}</span>
              <span><b>{c.label}</b> — {c.detail}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className="holding-actions">
        <button className="link-btn danger" onClick={() => confirm(`Supprimer cette entrée du journal (${e.name}) ?`) && deleteJournalEntry(e.id)}>Supprimer</button>
      </div>
    </li>
  );
}
