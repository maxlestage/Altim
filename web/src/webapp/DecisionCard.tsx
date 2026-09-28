import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { Kind } from "../engine/reliability";
import { api } from "./api";
import {
  cacheDecision, cachedDecision, count, decisionUrl, EXIT_KIND_LABEL, exitText, familyTone, hashRate, LEVEL_UI, longDate, modeText, nyDate, num,
  pct, recentVerdict, riskRewardText, SCENARIO_UI, shortDateTime, sortVetoes, STEP_UI, summaryFamilies, UNCERTAINTY_LABEL, usd, usdCompact,
  type CryptoFundamentals, type Decision, type Family, type PersonalInput, type StockFundamentals,
} from "./decision";
import { TrackDetails } from "./TrackDetails";
import { SimulateBuy } from "./PaperOrder";

// ---------- Small building blocks ----------

function Section({ title, badge, children, open }: { title: string; badge?: ReactNode; children: ReactNode; open?: boolean }) {
  return (
    <details className="dec-section" open={open}>
      <summary>
        <span>{title}</span>
        {badge != null && <small className="dec-section-badge">{badge}</small>}
      </summary>
      <div className="dec-section-body">{children}</div>
    </details>
  );
}

/** Label / value pairs; a missing value says "non disponible" rather than a blank. */
function Figures({ rows }: { rows: [string, string | null | undefined, string?][] }) {
  return (
    <dl className="dec-figures">
      {rows.map(([k, v, cls]) => (
        <div key={k}>
          <dt>{k}</dt>
          <dd className={v == null || v === "—" ? "muted" : `mono ${cls ?? ""}`}>{v == null || v === "—" ? "non disponible" : v}</dd>
        </div>
      ))}
    </dl>
  );
}

const upDown = (v: number | null | undefined) => (v == null ? "" : v >= 0 ? "up" : "down");

function Conditions({ title, items }: { title: string; items: Decision["toBuy"] }) {
  if (!items.length) return null;
  return (
    <div className="dec-block">
      <h3>{title}</h3>
      <ul className="dec-list">
        {items.map((c) => (
          <li key={c.text}>
            {c.text}
            {c.level != null && <> <span className="dec-level mono">niveau {usd(c.level)}</span></>}
          </li>
        ))}
      </ul>
    </div>
  );
}

function FamilyRow({ f }: { f: Family }) {
  const t = familyTone(f);
  const s = f.score;
  return (
    <li className="dec-family">
      <div className="dec-family-head">
        <b>{f.label}</b>
        <span className={`dec-tag tone-${t.tone}`}>
          <span aria-hidden>{t.icon}</span> {t.text}{s != null && <span className="mono"> · {s > 0 ? "+" : s < 0 ? "−" : ""}{Math.abs(Math.round(s))}</span>}
        </span>
      </div>
      {s != null ? (
        <div className="bar" role="img" aria-label={`Score ${Math.round(s)} sur une échelle de −100 à +100`}>
          <i className={s >= 0 ? "pos" : "neg"} style={{ width: `${Math.min(100, Math.abs(s)) / 2}%`, left: s >= 0 ? "50%" : `${50 - Math.min(100, Math.abs(s)) / 2}%` }} />
        </div>
      ) : (
        <p className="muted small">Non disponible : aucune source gratuite et vérifiable, rien n'est estimé.</p>
      )}
      <p className="small">{f.summary}</p>
      {f.points.length > 0 && <ul className="dec-list small">{f.points.map((p) => <li key={p}>{p}</li>)}</ul>}
      <small className="muted">Source : {f.source}</small>
    </li>
  );
}

function StockFund({ f }: { f: StockFundamentals }) {
  const e = f.nextEarnings;
  return (
    <>
      <p className="muted small">{f.period}</p>
      <Figures
        rows={[
          ["Chiffre d'affaires", f.revenue != null ? `${usdCompact(f.revenue)}${f.revenueGrowth != null ? ` (${pct(f.revenueGrowth, 1, true)} sur un an)` : ""}` : null],
          ["Résultat net", usdCompact(f.netIncome)],
          ["Bénéfice par action", f.eps != null ? `${usd(f.eps)}${f.epsGrowth != null ? ` (${pct(f.epsGrowth, 1, true)})` : ""}` : null],
          ["Marge brute", pct(f.grossMargin)],
          ["Marge opérationnelle", pct(f.operatingMargin)],
          ["Marge nette", pct(f.netMargin)],
          ["Flux de trésorerie disponible", f.freeCashFlow != null ? `${usdCompact(f.freeCashFlow)}${f.fcfMargin != null ? ` (${pct(f.fcfMargin)} du CA)` : ""}` : null],
          ["Dette", usdCompact(f.debt)],
          ["Trésorerie", usdCompact(f.cash)],
          ["Dette nette", usdCompact(f.netDebt)],
          ["Rentabilité des capitaux (ROE)", pct(f.roe, 0)],
          ["PER", num(f.per, 1)],
          ["PEG", num(f.peg, 2)],
          ["EV/EBITDA", num(f.evEbitda, 1)],
          ["Rendement du dividende", pct(f.dividendYield, 2)],
          ["Nombre d'actions sur un an", f.shareChange != null ? `${pct(f.shareChange, 1, true)}${f.shareChange < 0 ? " (rachats)" : ""}` : null],
        ]}
      />
      <p className="kv small">
        <span>Prochains résultats</span>
        <b>{e ? <>{nyDate(e.date)}{e.estimated && <span className="dec-estimated"> · date estimée</span>}</> : "non communiqués"}</b>
      </p>
      {f.surprises.length > 0 && (
        <div className="dec-block">
          <h3>Surprises sur les résultats</h3>
          <ul className="dec-list small">
            {f.surprises.map((s) => (
              <li key={s.quarter}>
                {s.quarter} : BPA {usd(s.eps)} contre {usd(s.consensus)} attendus, <span className={upDown(s.surprisePct)}>{pct(s.surprisePct, 1, true)}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {f.revisions && (
        <p className="small">
          Révisions des analystes (BPA de l'exercice) : {usd(f.revisions.monthAgo)} il y a un mois, {usd(f.revisions.now)} aujourd'hui (
          <span className={upDown(f.revisions.changePct)}>{pct(f.revisions.changePct, 1, true)}</span>).
        </p>
      )}
      <p className="muted small">{f.sectorNote}</p>
      <small className="muted">Source : {f.source}</small>
    </>
  );
}

function CryptoFund({ f }: { f: CryptoFundamentals }) {
  return (
    <>
      <Figures
        rows={[
          ["Capitalisation", usdCompact(f.marketCap)],
          ["Valorisation totale diluée (FDV)", usdCompact(f.fdv)],
          ["Capitalisation ÷ FDV", num(f.mcFdv, 2)],
          ["Offre en circulation", f.circulatingSupply != null ? `${count(f.circulatingSupply)}${f.circulatingPct != null ? ` (${pct(f.circulatingPct)} du maximum)` : ""}` : null],
          ["Offre totale", f.totalSupply != null ? count(f.totalSupply) : null],
          ["Offre maximale", f.maxSupply != null ? count(f.maxSupply) : null],
          ["Valeur bloquée (TVL)", usdCompact(f.tvl)],
          ["Frais sur 30 jours", usdCompact(f.fees30d)],
          ["Dominance du bitcoin", pct(f.btcDominance)],
          ["Taux de financement (funding)", f.fundingRate != null ? pct(f.fundingRate * 100, 4, true) : null],
          ["Positions ouvertes (OI)", usdCompact(f.openInterest)],
          ...(f.txPerDay != null ? [["Transactions par jour", count(f.txPerDay)] as [string, string]] : []),
          ...(f.hashRate != null ? [["Taux de hachage", hashRate(f.hashRate)] as [string, string]] : []),
        ]}
      />
      <p className="small"><span className="muted">Déblocages de jetons : </span>{f.unlocks}</p>
      <small className="muted">Source : {f.source}</small>
    </>
  );
}

// ---------- The card itself (pure: renders a decision) ----------

export type DecisionStatus = { kind: "fresh" } | { kind: "refreshing"; at: number } | { kind: "stale"; at: number; offline: boolean; error: string };

export function DecisionView({ d, status = { kind: "fresh" }, onRetry, simulate }: {
  d: Decision; status?: DecisionStatus; onRetry?: () => void;
  /** Shows "Simuler cet achat" (paper trading) with the live price when known. */
  simulate?: { livePrice: number | null };
}) {
  const lv = LEVEL_UI[d.level];
  const vetoes = sortVetoes(d.vetoes);
  const active = vetoes.filter((v) => v.active).length;
  const p = d.plan;
  const fam = summaryFamilies(d.families);
  return (
    <article className={`card decision lv-${d.level}`} aria-labelledby="dec-title">
      <p className="dec-date">
        <b className="mono">{d.symbol}</b> — {longDate(d.asOf)}
        <span className="muted"> · {d.name}</span>
      </p>

      {status.kind === "refreshing" && <p className="muted small dec-status">Décision du {shortDateTime(status.at)} · mise à jour…</p>}
      {status.kind === "stale" && (
        <p className="notice warn small dec-status" role="status">
          {status.offline ? "Hors ligne" : `Serveur injoignable (${status.error})`} : dernière décision connue, du {shortDateTime(status.at)}.
          {onRetry && <> <button className="link-btn" onClick={onRetry}>Réessayer</button></>}
        </p>
      )}

      <div className="dec-verdict">
        <h2 id="dec-title" className="dec-label">
          <span aria-hidden>{lv.icon}</span> {d.label}
        </h2>
        <span className="dec-level-label">{d.levelLabel || lv.label}</span>
        <div className="dec-confidence">
          <span>Confiance du modèle : <b className="mono">{Math.round(d.confidence)}/100</b></span>
          <div className="weight-track" role="img" aria-label={`Confiance ${Math.round(d.confidence)} sur 100`}>
            <i style={{ width: `${Math.max(2, Math.min(100, d.confidence))}%` }} />
          </div>
        </div>
      </div>
      <p className="muted small">{d.confidenceText}</p>
      <p className={`dec-mode ${d.mode === "personal" ? "personal" : ""}`}>{modeText(d.mode)}</p>

      <p className="dec-headline">{d.headline}</p>

      {fam.length > 0 && (
        <ul className="dec-chips" aria-label="Résumé par famille">
          {fam.map(({ key, name, family }) => {
            const t = familyTone(family);
            return (
              <li key={key} className={`dec-chip tone-${t.tone}`} title={family.summary}>
                <span>{name}</span> <span aria-hidden>{t.icon}</span> <small>{t.text}</small>
              </li>
            );
          })}
        </ul>
      )}

      {p ? (
        <div className="dec-plan">
          <div><small>Zone d'achat</small><b>{usd(p.zoneFrom)} – {usd(p.zoneTo)}</b></div>
          <div><small>Stop / invalidation</small><b className="sell">{usd(p.stop)}</b><small className="muted">{pct(-p.riskPct)}</small></div>
          <div><small>Objectif 1</small><b className="buy">{usd(p.target1)}</b><small className="muted">{pct(p.reward1Pct, 1, true)}</small></div>
          <div><small>Objectif 2</small>{p.target2 != null ? <><b className="buy">{usd(p.target2)}</b><small className="muted">{pct(p.reward2Pct, 1, true)}</small></> : <b className="muted">aucun</b>}</div>
          <div className={p.acceptable ? "rr-ok" : "rr-ko"}>
            <small>Gain/risque (entrée {usd(p.entry)})</small>
            <b>{riskRewardText(p)}</b>
            <small>{p.acceptable ? "✓ suffisant" : "✕ insuffisant"}</small>
          </div>
          <p className="muted small dec-horizon">Horizon : {p.horizon}</p>
        </div>
      ) : (
        <p className="muted small">Pas de plan d'entrée : aucun niveau net (zone, stop et objectifs) sur cet actif pour l'instant.</p>
      )}

      {d.position && (
        <div className="dec-block dec-position">
          <h3>Votre position</h3>
          <p className="kv small"><span>Prix d'achat moyen</span><b className="mono">{usd(d.position.cost)}</b></p>
          {d.position.pnlPct != null && <p className="kv small"><span>Plus-value latente</span><b className={`mono ${upDown(d.position.pnlPct)}`}>{pct(d.position.pnlPct, 1, true)}</b></p>}
          <p className="small">{d.position.advice}</p>
          {d.position.exits.length > 0 && (
            <ul className="dec-exits">
              {d.position.exits.map((e) => (
                <li key={`${e.kind}-${e.trigger}`} className={e.now ? "now" : ""}>
                  <div>
                    <b>{exitText(e)}</b>
                    <small className="muted">{EXIT_KIND_LABEL[e.kind]}{e.price != null ? ` · ${usd(e.price)}` : ""}</small>
                  </div>
                  {e.now && <span className="dec-now">▶ maintenant</span>}
                </li>
              ))}
            </ul>
          )}
          <p className="muted small">Sorties progressives : le reste de la position est conservé tant que le scénario tient.</p>
        </div>
      )}

      {d.exposure && (d.exposure.warning ? (
        <p className="notice warn small" role="note">
          ⚠ {d.exposure.warning} Actifs concernés : {d.exposure.assets.join(", ")}
          {d.exposure.correlation != null && ` ; corrélation de ${d.symbol} au ${d.exposure.factor} : ${num(d.exposure.correlation, 2)}`}.
        </p>
      ) : (
        <p className="muted small">
          Exposition au facteur {d.exposure.factor} : {pct(d.exposure.weight, 0)} du portefeuille
          {d.exposure.correlation != null && ` (corrélation ${num(d.exposure.correlation, 2)})`}.
        </p>
      ))}

      {d.whyWait.length > 0 && (
        <div className="dec-block">
          <h3>Pourquoi attendre ?</h3>
          <ul className="dec-list">{d.whyWait.map((w) => <li key={w}>{w}</li>)}</ul>
        </div>
      )}
      <Conditions title="Pour passer en ACHAT" items={d.toBuy} />
      <Conditions title="Pour passer en VENTE" items={d.toSell} />

      <div className="dec-sections">
        <Section title="Familles d'indices" badge={`${d.families.filter((f) => f.status !== "unavailable").length}/${d.families.length} disponibles`}>
          <ul className="dec-families">{d.families.map((f) => <FamilyRow key={f.key} f={f} />)}</ul>
        </Section>

        <Section title="Interdictions d'achat" badge={active ? `${active} active${active > 1 ? "s" : ""}` : "aucune active"}>
          <ul className="dec-vetoes">
            {vetoes.map((v) => {
              const st = v.active ? { cls: "active", icon: "⛔", word: "ACTIVE" } : v.verifiable ? { cls: "pass", icon: "✓", word: "vérifié" } : { cls: "unknown", icon: "?", word: "non vérifiable" };
              return (
                <li key={v.code} className={`veto-${st.cls}`}>
                  <span className="dec-veto-mark"><span aria-hidden>{st.icon}</span> {st.word}</span>
                  <div><b>{v.label}</b><small className="muted">{v.detail}</small></div>
                </li>
              );
            })}
          </ul>
        </Section>

        <Section title="Plan d'entrée recherché" badge={`${d.setup.met}/${d.setup.total} étapes`}>
          <p className="small"><b>{d.setup.name}</b></p>
          <ol className="dec-steps">
            {d.setup.steps.map((s) => (
              <li key={s.label} className={`step-${s.state}`}>
                <span className="dec-step-mark"><span aria-hidden>{STEP_UI[s.state].icon}</span> {STEP_UI[s.state].label}</span>
                <div><span>{s.label}</span>{s.detail && s.detail !== "—" && <small className="muted">{s.detail}</small>}</div>
              </li>
            ))}
          </ol>
        </Section>

        {d.scenarios.length > 0 && (
          <Section title="Scénarios">
            <ul className="dec-scenarios">
              {d.scenarios.map((s) => (
                <li key={s.kind} className={`sc-${s.kind}`}>
                  <b><span aria-hidden>{SCENARIO_UI[s.kind].icon}</span> {s.title}</b>
                  <p className="small">Si {s.condition.charAt(0).toLowerCase()}{s.condition.slice(1)} → {s.consequence}</p>
                  {s.level != null && <small className="muted mono">niveau {usd(s.level)}</small>}
                </li>
              ))}
            </ul>
            <p className="muted small">Ce qu'il faut surveiller, pas une prévision.</p>
          </Section>
        )}

        <Section title="Points favorables et défavorables" badge={`${d.pros.length} / ${d.cons.length}`}>
          <div className="dec-proscons">
            <div>
              <h3 className="up">＋ Points favorables</h3>
              <ul className="dec-list">{d.pros.map((x) => <li key={x}>{x}</li>)}</ul>
            </div>
            <div>
              <h3 className="down">− Points défavorables</h3>
              <ul className="dec-list">{d.cons.map((x) => <li key={x}>{x}</li>)}</ul>
            </div>
          </div>
        </Section>

        <Section title="Pourquoi pas ?" badge={`incertitude ${UNCERTAINTY_LABEL[d.whyNot.uncertainty]}`}>
          <p className="small muted">Ce qui pourrait rendre cette décision fausse, cherché exprès.</p>
          <ul className="dec-list">{d.whyNot.risks.map((r) => <li key={r}>{r}</li>)}</ul>
          <p className="kv small"><span>Incertitude</span><b>{UNCERTAINTY_LABEL[d.whyNot.uncertainty]}</b></p>
          {d.whyNot.invalidation.length > 0 && (
            <>
              <h3>Invalidation</h3>
              <ul className="dec-list">{d.whyNot.invalidation.map((r) => <li key={r}>{r}</li>)}</ul>
            </>
          )}
        </Section>

        <Section title={d.fundamentals?.kind === "crypto" ? "Fondamentaux du réseau" : "Fondamentaux de l'entreprise"}>
          {d.fundamentals == null ? <p className="muted small">Non disponibles pour cet actif.</p>
            : d.fundamentals.kind === "stock" ? <StockFund f={d.fundamentals} /> : <CryptoFund f={d.fundamentals} />}
        </Section>

        <Section title="Liquidité">
          {d.liquidity ? (
            <>
              <Figures
                rows={[
                  ["Écart achat/vente", d.liquidity.spreadPct != null ? pct(d.liquidity.spreadPct, 4) : null],
                  ["Montant échangé par jour (moyenne 20 j)", usdCompact(d.liquidity.dailyValue)],
                  ["Volume du jour ÷ moyenne", d.liquidity.relativeVolume != null ? `${num(d.liquidity.relativeVolume, 1)} ×` : null],
                ]}
              />
              <small className="muted">Source : {d.liquidity.source}</small>
            </>
          ) : <p className="muted small">Non disponible.</p>}
        </Section>

        <Section title="Historique du signal" badge={d.track ? `${d.track.trades} trades` : "non disponible"}>
          {d.track ? (
            <>
              <p className="muted small">{d.track.period}</p>
              <Figures
                rows={[
                  ["Trades", String(d.track.trades)],
                  ["Réussite", pct(d.track.winRate, 0)],
                  ["Gain moyen", pct(d.track.avgWin, 1, true), "up"],
                  ["Perte moyenne", pct(d.track.avgLoss, 1, true), "down"],
                  ["Profit factor", num(d.track.profitFactor, 2)],
                  ["Sharpe", num(d.track.sharpe, 2)],
                  ["Sortino", num(d.track.sortino, 2)],
                  ["Pire recul (drawdown)", pct(d.track.maxDrawdown, 1, true), "down"],
                  ["Rendement du signal", pct(d.track.totalReturn, 1, true), upDown(d.track.totalReturn)],
                  ["Simple détention", pct(d.track.buyAndHold, 1, true), upDown(d.track.buyAndHold)],
                  ["Frais et glissement par ordre", `${pct(d.track.feesPct, 2)} + ${pct(d.track.slippagePct, 2)}`],
                  ["Plus longue série perdante", `${d.track.losingStreak} trade${d.track.losingStreak > 1 ? "s" : ""}`],
                ]}
              />
              {d.track.trades < 30 && <p className="notice warn small">Moins de 30 trades : échantillon trop petit pour conclure.</p>}
              <p className="small">{d.track.note}</p>
              <TrackDetails track={d.track} />
            </>
          ) : <p className="muted small">Pas assez d'historique pour mesurer ce signal sur cet actif.</p>}
        </Section>

        <Section title="Sources" badge={`${d.sources.filter((s) => s.ok).length}/${d.sources.length} disponibles`}>
          <ul className="source-list">
            {d.sources.map((s) => (
              <li key={s.name}>
                <span className={s.ok ? "up" : "down"}>{s.ok ? "● ok" : "○ indisponible"}</span> {s.name}
                <small className="muted">{s.detail}</small>
              </li>
            ))}
          </ul>
        </Section>
      </div>

      {simulate && <SimulateBuy d={d} livePrice={simulate.livePrice} />}

      <p className="dec-disclaimer small">{d.disclaimer}</p>
    </article>
  );
}

// ---------- Loading, refresh, error and offline states ----------

/**
 * Loads the decision (every 5 minutes while visible and when the connection returns), shows the last one
 * cached in this browser while loading or when the server is unreachable.
 * `personal` is null for an asset not held; `ready` is false while the portfolio prices are loading.
 */
export function DecisionCard({ symbol, kind, personal, ready = true, livePrice = null }: {
  symbol: string; kind: Kind; personal: PersonalInput | null; ready?: boolean;
  /** Live price of the asset, used by "Simuler cet achat" (else the decision's price). */
  livePrice?: number | null;
}) {
  const isPersonal = !!personal && (personal.cost != null || personal.weights.length > 0);
  const url = decisionUrl(symbol, kind, isPersonal ? personal : null);
  const initial = () => {
    const c = cachedDecision(kind, symbol);
    return c && c.personal === isPersonal ? c : null;
  };
  const [d, setD] = useState<Decision | null>(() => initial()?.decision ?? null);
  const [status, setStatus] = useState<DecisionStatus>(() => {
    const c = initial();
    return c ? { kind: "refreshing", at: c.at } : { kind: "fresh" };
  });
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const retry = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    const c = initial();
    setD(c?.decision ?? null);
    setStatus(c ? { kind: "refreshing", at: c.at } : { kind: "fresh" });
    setError(null);
  }, [symbol, kind, isPersonal]);

  useEffect(() => {
    if (!ready) return;
    let alive = true;
    const load = () =>
      api.decision(symbol, kind, isPersonal ? personal : null)
        .then((x) => {
          if (!alive) return;
          cacheDecision(x, isPersonal);
          setD(x);
          setStatus({ kind: "fresh" });
          setError(null);
        })
        .catch((e: unknown) => {
          if (!alive) return;
          const raw = e instanceof Error ? e.message : "Données indisponibles";
          const msg = raw === "Erreur 404" ? "décision pas encore disponible sur ce serveur" : raw;
          const offline = typeof navigator !== "undefined" && navigator.onLine === false;
          const c = initial();
          if (c) {
            setD(c.decision);
            setStatus({ kind: "stale", at: c.at, offline, error: msg });
          } else setError(offline ? "Hors ligne : aucune décision enregistrée pour cet actif." : `Décision indisponible : ${msg}.`);
        });
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 300_000);
    window.addEventListener("online", load);
    return () => {
      alive = false;
      clearInterval(id);
      window.removeEventListener("online", load);
    };
    // The URL carries every input (symbol, kind, cost, weights).
  }, [url, ready, nonce]);

  if (d) return <DecisionView d={d} status={status} onRetry={retry} simulate={{ livePrice }} />;
  if (error) {
    return (
      <div className="card decision">
        <h2 className="card-title">Décision</h2>
        <p className="notice warn">⚠ {error}</p>
        <button className="link-btn" onClick={retry}>Réessayer</button>
        <p className="dec-disclaimer small">Pas un conseil en investissement réglementé ; Altim ne passe aucun ordre.</p>
      </div>
    );
  }
  return <div className="skeleton tall" aria-label="Chargement de la décision" role="status" />;
}

/**
 * Radar badge: the verdict last seen on the asset page (less than 12 h ago), read from this browser's cache.
 * No request per row: an asset never opened shows nothing.
 */
export function VerdictMini({ kind, symbol }: { kind: Kind; symbol: string }) {
  const v = recentVerdict(kind, symbol);
  if (!v) return null;
  return (
    <span className={`dec-mini lv-${v.level}`} title={`Décision Altim vue le ${shortDateTime(v.at)} : ${LEVEL_UI[v.level].label}`}>
      Décision : <span aria-hidden>{LEVEL_UI[v.level].icon}</span> {v.label}
    </span>
  );
}
