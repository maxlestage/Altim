/**
 * « Validation du modèle »: the signal's backtest (the decision's own track record, same costs) on a basket fixed in
 * advance, pooled by asset class, by market regime and overall (/api/validation). Stacked cards, mobile first; the
 * per-asset list follows the basket's order by default, never ranked by performance.
 */
import { useEffect, useState } from "react";
import { api } from "./api";
import { onLink } from "./router";
import {
  beatText, CLASS_SHORT, monthYear, plain, pooledText, regimeDaysText, regimesOf, signedPct, SORTS, sortAssets, verdictTone, years,
  type AssetResult, type GroupStat, type RegimeGroup, type SortKey, type ValidationReport,
} from "./model-validation";

export function Validation() {
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = () =>
      api.validation()
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
        .catch((e) => alive && setError(e instanceof Error ? e.message : "Validation indisponible"));
    load();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, []);

  return (
    <section className="app-screen validation">
      <div className="screen-top">
        <div>
          <h1>Validation du modèle</h1>
          <p className="muted small">
            Le signal d'Altim rejoué jour par jour sur un panier d'actions, de bitcoin, d'ether et d'altcoins choisi à l'avance, avec les mêmes réglages et les mêmes coûts que
            l'historique de la carte Décision. Un test du passé, pas une promesse.
          </p>
        </div>
      </div>
      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && (
        <div className="card">
          <p className="muted">{pending ? "Calcul sur tout le panier en cours (environ 30 secondes la première fois)…" : "Chargement…"}</p>
          <div className="skeleton" />
        </div>
      )}
      {report && <ValidationView report={report} />}
    </section>
  );
}

export function ValidationView({ report: r }: { report: ValidationReport }) {
  const [regimeGroup, setRegimeGroup] = useState<string>("all");
  const [sort, setSort] = useState<SortKey>("class");
  const groups = [r.overall, ...r.classes];
  const shownRegimes = groups.find((g) => g.id === regimeGroup) ?? r.overall;
  return (
    <>
      <div className="card val-head">
        <p className="val-headline">{r.headline}</p>
        <div className="val-tiles">
          <Tile label="Actifs testés" value={`${r.overall.assets} / ${r.basket.length}`} />
          <Tile label="Trades" value={String(r.overall.trades)} />
          <Tile label="Historique" value={r.minYears != null && r.maxYears != null && r.maxYears - r.minYears >= 0.2 ? `${plain(r.minYears, 1)} à ${years(r.maxYears)}` : years(r.years)} />
        </div>
        <BeatMeter beat={r.overall.beatHold} n={r.overall.assets} share={r.overall.beatShare} />
        <p className="small"><VerdictChip g={r.overall} /></p>
        <p className="muted small">Tous les trades ensemble : {pooledText(r.overall)}</p>
        <p className="muted small">
          Période : {monthYear(r.from)} – {monthYear(r.to)} (bougies journalières). Calculé le {new Date(r.asOf).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}, mis à jour toutes les 12 h.
        </p>
        {r.failures.length > 0 && (
          <div className="notice warn small">
            {r.failures.length} actif{r.failures.length > 1 ? "s" : ""} non testé{r.failures.length > 1 ? "s" : ""} :
            <ul className="reasons">{r.failures.map((f) => <li key={f.symbol}><b>{f.symbol}</b> — {f.error}</li>)}</ul>
          </div>
        )}
      </div>

      <h2 className="section-label">Par classe d'actifs</h2>
      <div className="val-grid">{r.classes.map((g) => <GroupCard key={g.id} g={g} taxRate={r.parameters.taxRatePct} />)}</div>

      <h2 className="section-label">Par régime de marché</h2>
      <div className="val-chips" role="group" aria-label="Actifs pris en compte">
        {groups.map((g) => (
          <button key={g.id} type="button" className={`chip pick${g.id === regimeGroup ? " on" : ""}`} aria-pressed={g.id === regimeGroup} onClick={() => setRegimeGroup(g.id)}>
            {g.id === "all" ? "Tous" : CLASS_SHORT[g.id]}
          </button>
        ))}
      </div>
      <div className="val-grid">{regimesOf(shownRegimes).map((g) => <RegimeCard key={g.regime} g={g} />)}</div>
      <p className="muted small">{r.parameters.regimeRule} Régime lu la veille, sans données futures ; « inconnu » tant que l'historique est trop court pour le classer.</p>

      <h2 className="section-label">Actif par actif</h2>
      <div className="val-chips" role="group" aria-label="Ordre des actifs">
        {SORTS.map(([k, l]) => (
          <button key={k} type="button" className={`chip pick${k === sort ? " on" : ""}`} aria-pressed={k === sort} onClick={() => setSort(k)}>{l}</button>
        ))}
      </div>
      <ul className="val-assets">{sortAssets(r.assets, sort).map((a) => <AssetRow key={a.symbol} a={a} />)}</ul>

      <div className="card">
        <h2 className="card-title">Protections contre les biais</h2>
        <ul className="reasons">{r.protections.map((p) => <li key={p}>{p}</li>)}</ul>
        <p className="small"><b>Hors échantillon ?</b> {r.outOfSample.note}</p>
      </div>

      <div className="card">
        <h2 className="card-title">Biais et limites</h2>
        <ul className="reasons">{r.limits.map((p) => <li key={p}>{p}</li>)}</ul>
        <p className="muted small">
          Coûts par ordre : frais {plain(r.parameters.feesPct, 3)}{" "}%, glissement {plain(r.parameters.slippagePct, 3)}{" "}%, écart achat/vente supposé {plain(r.parameters.spreadStockPct, 3)}{" "}% (actions) ou{" "}
          {plain(r.parameters.spreadCryptoPct, 3)}{" "}% (cryptos), moitié payée à chaque ordre. Panier fixé le {r.basketFixedOn.split("-").reverse().join("/")}. Source : {r.source}.
        </p>
      </div>
    </>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="val-tile">
      <span className="muted small">{label}</span>
      <b>{value}</b>
    </div>
  );
}

/** Share of the assets where the signal beat buy-and-hold: one thin bar, the number written next to it. */
export function BeatMeter({ beat, n, share }: { beat: number; n: number; share: number | null }) {
  return (
    <div className="val-meter">
      <p className="kv small"><span>A battu la simple détention</span><b>{beatText(beat, n, share)}</b></p>
      <div className="val-bar" role="img" aria-label={`A battu la détention sur ${beat} actifs sur ${n}`}>
        <span style={{ width: `${Math.max(0, Math.min(100, share ?? 0))}%` }} />
      </div>
    </div>
  );
}

function VerdictChip({ g }: { g: { verdict: GroupStat["verdict"]; verdictLabel: string } }) {
  return <span className={`chip val-verdict ${verdictTone(g.verdict)}`}>{g.verdictLabel}</span>;
}

function KV({ label, value, tone }: { label: string; value: string; tone?: number | null }) {
  const cls = tone == null ? undefined : tone >= 0 ? "up" : "down";
  return <p className="kv small"><span>{label}</span><b className={cls}>{value}</b></p>;
}

export function GroupCard({ g, taxRate }: { g: GroupStat; taxRate: number }) {
  return (
    <div className="card val-card">
      <h3 className="val-title">{g.label}</h3>
      <p className="small"><VerdictChip g={g} /></p>
      <p className="muted small">{g.assets} actif{g.assets > 1 ? "s" : ""} · historique médian {years(g.years)}</p>
      <p className="small">{pooledText(g)}</p>
      <BeatMeter beat={g.beatHold} n={g.assets} share={g.beatShare} />
      <KV label="Rendement médian du signal" value={signedPct(g.medianReturn)} tone={g.medianReturn} />
      <KV label="Détention médiane" value={signedPct(g.medianHold)} tone={g.medianHold} />
      {g.worstReturn && <KV label={`Pire actif (${g.worstReturn.symbol})`} value={signedPct(g.worstReturn.value)} tone={g.worstReturn.value} />}
      <KV label="Recul max médian (signal / détention)" value={`${signedPct(g.medianDrawdown)} / ${signedPct(g.medianHoldDrawdown)}`} />
      {g.worstDrawdown && <KV label={`Pire recul (${g.worstDrawdown.symbol})`} value={signedPct(g.worstDrawdown.value)} />}
      <KV label="Sharpe / Sortino médians" value={`${plain(g.medianSharpe)} / ${plain(g.medianSortino)}`} />
      <KV label="Multiple de R moyen" value={g.avgR == null ? "—" : `${plain(g.avgR)} R`} />
      <KV label="Temps investi médian" value={g.medianExposure == null ? "—" : `${plain(g.medianExposure, 0)} %`} />
      <KV label={`Après impôt ${plain(taxRate, 0)} % (signal / détention)`} value={`${signedPct(g.medianAfterTax)} / ${signedPct(g.medianHoldAfterTax)}`} />
    </div>
  );
}

export function RegimeCard({ g }: { g: RegimeGroup }) {
  return (
    <div className="card val-card">
      <h3 className="val-title">{g.label}</h3>
      <p className="small"><VerdictChip g={g} /></p>
      <p className="small">{pooledText(g)}</p>
      <p className="muted small">{g.days.toLocaleString("fr-FR")} jours-actifs dans ce régime.</p>
      {g.regime !== "unknown" && <p className="small">{regimeDaysText(g)}</p>}
    </div>
  );
}

export function AssetRow({ a }: { a: AssetResult }) {
  const gap = a.totalReturn - a.buyAndHold;
  return (
    <li className="val-asset">
      <div className="val-asset-top">
        <a href={`/app/actif/${a.kind}/${a.symbol}`} onClick={onLink} className="val-sym"><b>{a.symbol}</b> <span className="muted small">{a.name}</span></a>
        <span className="chip muted">{CLASS_SHORT[a.class]} · {a.group}</span>
        {a.lowSample && <span className="chip muted">échantillon trop faible</span>}
      </div>
      <p className="small">
        Signal <b className={a.totalReturn >= 0 ? "up" : "down"}>{signedPct(a.totalReturn)}</b> · détention <b className={a.buyAndHold >= 0 ? "up" : "down"}>{signedPct(a.buyAndHold)}</b>
        {" "}<span className="muted">({a.beatHold ? "mieux" : "moins bien"}, écart {signedPct(gap)})</span>
      </p>
      <p className="muted small">
        {a.trades} trade{a.trades > 1 ? "s" : ""} · réussite {a.winRate == null ? "—" : `${plain(a.winRate, 0)} %`} · facteur de profit {plain(a.profitFactor)} · espérance {signedPct(a.expectancy, 2)} · recul max{" "}
        {signedPct(a.maxDrawdown)} (détention {signedPct(a.holdMaxDrawdown)}) · Sharpe {plain(a.sharpe)} · Sortino {plain(a.sortino)} · {years((a.to - a.from) / (365.25 * 86_400_000))} ({a.source})
      </p>
    </li>
  );
}
