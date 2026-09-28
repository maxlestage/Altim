import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { Segmented } from "./ui";
import { formatPrice } from "../market";
import { compareAssets, PROJECTION_RATES, positionSize, projection, rebalance, saleTotal, type AssetClass } from "../engine/tools";
import type { Close } from "../engine/history";
import type { PortfolioAnalysis } from "../engine/holdings";

const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 0 })} $`;
const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const num = (s: string) => Number(s.replace(/[\s ]/g, "").replace(",", "."));
const plain = (v: number) => (v >= 1 ? v.toFixed(2) : v.toPrecision(4)).replace(".", ",");
// Categorical palette (fixed order, validated on the dark surface). The 4th is close to the 1st for deuteranopes
// (ΔE 6,4, allowed with a second cue): its line is dashed.
const COLORS = ["#3987e5", "#d95926", "#199e70", "#c24ec9"];

// ---------- Comparison ----------

/** 2 to 4 assets of the radar over the same days: change, volatility, worst fall, correlation. */
export function CompareCard({ assets }: { assets: { symbol: string; kind: "crypto" | "stock" }[] }) {
  const [picked, setPicked] = useState<string[]>(() => assets.slice(0, 2).map((a) => `${a.kind}:${a.symbol}`));
  const [days, setDays] = useState<"30" | "90" | "365">("90");
  const [series, setSeries] = useState<Record<string, Close[]> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const key = [...picked].sort().join(",");

  useEffect(() => {
    if (picked.length < 2) return setSeries(null);
    let alive = true;
    setSeries(null);
    const items = picked.map((id) => ({ kind: id.split(":")[0] as "crypto" | "stock", symbol: id.split(":")[1]! }));
    api.history(items, Number(days) as 30 | 90 | 365)
      .then((r) => alive && (setSeries(Object.fromEntries(r.series.map((s) => [`${s.kind}:${s.symbol}`, s.closes]))), setError(null)))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Historique indisponible"));
    return () => {
      alive = false;
    };
  }, [key, days]);

  const c = useMemo(() => (series ? compareAssets(series, picked, Number(days)) : null), [series, key, days]);
  const toggle = (id: string) =>
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : p.length >= 4 ? p : [...p, id]));
  const name = (id: string) => id.split(":")[1]!;

  return (
    <div className="card compare-card">
      <h2 className="card-title">Comparer</h2>
      <p className="muted small">Choisissez 2 à 4 actifs de votre radar.</p>
      <div className="news-tags">
        {assets.map((a) => {
          const id = `${a.kind}:${a.symbol}`;
          const i = picked.indexOf(id);
          return (
            <button key={id} className={`chip pick ${i >= 0 ? "on" : ""}`} aria-pressed={i >= 0} onClick={() => toggle(id)} style={i >= 0 ? { borderColor: COLORS[i] } : undefined}>
              {a.symbol}
            </button>
          );
        })}
      </div>
      <Segmented label="Période" value={days} options={[["30", "30 j"], ["90", "90 j"], ["365", "1 an"]]} onChange={setDays} />
      {picked.length < 2 && <p className="muted small">Sélectionnez au moins 2 actifs.</p>}
      {error && <p className="notice warn">⚠ {error}</p>}
      {picked.length >= 2 && !series && !error && <p className="muted small">Chargement de l'historique…</p>}
      {c && (
        <>
          {/* One colour per asset, in the order of the stats (an asset without history is left out). */}
          <CompareChart c={c} colors={c.stats.map((s) => COLORS[picked.indexOf(s.id)]!)} />
          <div className="table-scroll">
            <table className="compare-table small">
              <thead>
                <tr><th>Actif</th><th>Variation</th><th>Volatilité</th><th>Pire recul</th></tr>
              </thead>
              <tbody>
                {c.stats.map((s) => (
                  <tr key={s.id}>
                    <td><i className="dot" style={{ background: COLORS[picked.indexOf(s.id)] }} />{name(s.id)}</td>
                    <td className={s.change >= 0 ? "up" : "down"}>{pct(s.change)}</td>
                    <td>{Math.round(s.volatility)}&nbsp;%/an</td>
                    <td className="down">{pct(s.maxDrawdown)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {c.stats.length >= 2 && (
            <p className="muted small">
              Corrélation :{" "}
              {c.stats.flatMap((a, i) => c.stats.slice(i + 1).map((b, j) => {
                const v = c.correlation[i]![i + 1 + j];
                return `${name(a.id)}/${name(b.id)} ${v == null ? "—" : v.toLocaleString("fr-FR", { maximumFractionDigits: 2 })}`;
              })).join(" · ")}
              . Proche de 1 : ils montent et baissent ensemble (peu de diversification) ; proche de 0 : indépendants.
            </p>
          )}
          <p className="muted small">
            Mêmes jours pour tous{c.missing.length ? ` (sans historique : ${c.missing.map(name).join(", ")})` : ""}. Volatilité = écart type annualisé des variations journalières. Le passé ne dit pas ce qui arrivera.
          </p>
        </>
      )}
    </div>
  );
}

function CompareChart({ c, colors }: { c: NonNullable<ReturnType<typeof compareAssets>>; colors: string[] }) {
  const W = 320;
  const H = 130;
  const all = [0, ...c.stats.flatMap((s) => s.pct)];
  const lo = Math.min(...all);
  const hi = Math.max(...all);
  const span = hi - lo || 1;
  const x = (i: number) => 34 + (i / Math.max(1, c.days.length - 1)) * (W - 40);
  const y = (v: number) => 6 + (1 - (v - lo) / span) * (H - 12);
  return (
    <div className="history-chart">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={c.stats.map((s) => `${s.id.split(":")[1]} ${pct(s.change)}`).join(", ")}>
        <line x1={34} x2={W - 6} y1={y(0)} y2={y(0)} className="zero" />
        <text x={30} y={y(hi) + 4} className="axis" textAnchor="end">{pct(hi)}</text>
        <text x={30} y={y(lo)} className="axis" textAnchor="end">{pct(lo)}</text>
        {c.stats.map((s, k) => (
          <path key={s.id} d={s.pct.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join("")} stroke={colors[k]} className="mine" strokeDasharray={colors[k] === COLORS[3] ? "6 4" : undefined} />
        ))}
      </svg>
    </div>
  );
}

// ---------- Position size ----------

/** How much to buy so that hitting the stop costs the chosen share of the capital. */
export function PositionCard({ symbol, price, stop, stopSource, target, capital, riskPct }: {
  symbol: string;
  price: number | null;
  stop: number | null;
  stopSource: string;
  target: number | null;
  capital: number;
  riskPct: number;
}) {
  const [capitalText, setCapitalText] = useState(capital > 0 ? String(Math.round(capital)) : "");
  const [riskText, setRiskText] = useState(String(riskPct).replace(".", ","));
  const [stopText, setStopText] = useState(stop ? plain(stop) : "");
  const [targetText, setTargetText] = useState(target ? plain(target) : "");
  useEffect(() => {
    if (!stopText && stop) setStopText(plain(stop));
    if (!targetText && target) setTargetText(plain(target));
    if (!capitalText && capital > 0) setCapitalText(String(Math.round(capital)));
  }, [stop, target, capital]);

  const p = price ? positionSize({ capital: num(capitalText), riskPct: num(riskText), entry: price, stop: num(stopText), target: targetText ? num(targetText) : null }) : null;
  return (
    <div className="card position-card">
      <h2 className="card-title">Taille de position</h2>
      <div className="grid-2">
        <label className="field"><span>Capital ($)</span><input inputMode="decimal" value={capitalText} onChange={(e) => setCapitalText(e.target.value)} /></label>
        <label className="field"><span>Risque accepté (%)</span><input inputMode="decimal" value={riskText} onChange={(e) => setRiskText(e.target.value)} /></label>
        <label className="field"><span>Stop ($)</span><input inputMode="decimal" value={stopText} onChange={(e) => setStopText(e.target.value)} /></label>
        <label className="field"><span>Objectif ($, facultatif)</span><input inputMode="decimal" value={targetText} onChange={(e) => setTargetText(e.target.value)} /></label>
      </div>
      <p className="muted small">Entrée au prix actuel {price ? `${formatPrice(price)} $` : "…"} ; stop proposé : {stopSource}.</p>
      {!p && price && <p className="muted small">Le stop doit être sous le prix d'entrée, et le capital et le risque positifs.</p>}
      {p && (
        <>
          <p className="kv"><span>Acheter</span><b>{p.quantity.toLocaleString("fr-FR", { maximumFractionDigits: p.quantity >= 1 ? 2 : 6 })} {symbol} · {usd(p.amount)}</b></p>
          <p className="kv small"><span>Part du capital</span><b>{p.capitalShare.toLocaleString("fr-FR", { maximumFractionDigits: 1 })}&nbsp;%</b></p>
          <p className="kv small"><span>Perte si le stop est touché ({pct(-p.stopDistance)})</span><b className="down">−{usd(p.risk)}</b></p>
          {p.reward != null && <p className="kv small"><span>Gain à l'objectif · rapport gain/risque</span><b className="up">+{usd(p.reward)} · {p.ratio!.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} R</b></p>}
          {p.capped && <p className="notice warn small">Stop très proche : la taille est limitée à votre capital, la perte au stop reste sous le risque choisi.</p>}
          {p.ratio != null && p.ratio < 1.5 && <p className="notice warn small">Rapport gain/risque sous 1,5 : l'idée rapporte peu au regard du risque.</p>}
          <p className="muted small">Calcul, pas conseil : un écart de prix (gap) peut faire perdre plus que prévu au stop. Altim ne passe aucun ordre.</p>
        </>
      )}
    </div>
  );
}

// ---------- Sale after fees and tax ----------

/** What selling would leave once the fees and the flat tax on the gains are paid. */
export function SaleCard({ analysis }: { analysis: PortfolioAnalysis }) {
  const [taxText, setTaxText] = useState("30");
  const [feeText, setFeeText] = useState("0,1");
  const lines = analysis.lines.filter((l) => l.value > 0).map((l) => ({ id: l.id, value: l.value, cost: l.invested > 0 ? l.invested : null }));
  const t = saleTotal(lines, num(taxText) || 0, num(feeText) || 0);
  const symbol = (id: string) => analysis.lines.find((l) => l.id === id)?.symbol ?? id;
  if (!lines.length) return null;
  return (
    <div className="card sale-card">
      <h2 className="card-title">Si je vendais</h2>
      <div className="grid-2">
        <label className="field"><span>Impôt sur la plus-value (%)</span><input inputMode="decimal" value={taxText} onChange={(e) => setTaxText(e.target.value)} /></label>
        <label className="field"><span>Frais de vente (%)</span><input inputMode="decimal" value={feeText} onChange={(e) => setFeeText(e.target.value)} /></label>
      </div>
      <div className="table-scroll">
        <table className="compare-table small">
          <thead>
            <tr><th>Ligne</th><th>Valeur</th><th>Plus-value</th><th>Impôt</th><th>Net</th></tr>
          </thead>
          <tbody>
            {t.lines.map((l) => (
              <tr key={l.id}>
                <td>{symbol(l.id)}</td>
                <td>{usd(l.gross)}</td>
                <td className={l.gain == null ? "muted" : l.gain >= 0 ? "up" : "down"}>{l.gain == null ? "—" : `${l.gain >= 0 ? "+" : "−"}${usd(Math.abs(l.gain))}`}</td>
                <td>{l.tax > 0 ? `−${usd(l.tax)}` : "0 $"}</td>
                <td><b>{usd(l.net)}</b></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="kv"><span>Tout vendre : vous garderiez</span><b>{usd(t.net)}</b></p>
      <p className="kv small">
        <span>Frais {usd(t.fees)} · impôt {usd(t.tax)}</span>
        <b className={t.gain == null ? "" : t.gain >= 0 ? "up" : "down"}>{t.gain == null ? "" : `plus-value nette ${t.gain >= 0 ? "+" : "−"}${usd(Math.abs(t.gain))}`}</b>
      </p>
      <p className="muted small">
        30 % = prélèvement forfaitaire unique en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux) ; les pertes de l'année compensent les gains. Cryptos : l'impôt se calcule sur
        l'ensemble du portefeuille à chaque cession et les cessions de moins de 305 € par an sont exonérées, donc ce calcul ligne par ligne est une estimation.
        {t.unknownCost ? ` ${t.unknownCost} ligne(s) sans prix d'achat : plus-value non calculée.` : ""} Vérifiez votre situation (PEA, assurance-vie, option barème…) ; Altim ne passe aucun ordre.
      </p>
    </div>
  );
}

// ---------- Projection ----------

/** What the portfolio plus a monthly contribution would become, under three yearly returns (hypotheses). */
export function ProjectionCard({ start }: { start: number }) {
  const [monthlyText, setMonthlyText] = useState("200");
  const [years, setYears] = useState<"5" | "10" | "20">("10");
  const monthly = num(monthlyText) || 0;
  const runs = PROJECTION_RATES.map((rate) => ({ rate, points: projection(start, monthly, Number(years), rate) }));
  const paid = runs[0]!.points.at(-1)!.paid;
  return (
    <div className="card projection-card">
      <h2 className="card-title">Projection</h2>
      <label className="field"><span>Versement chaque mois ($)</span><input inputMode="decimal" value={monthlyText} onChange={(e) => setMonthlyText(e.target.value)} /></label>
      <Segmented label="Durée" value={years} options={[["5", "5 ans"], ["10", "10 ans"], ["20", "20 ans"]]} onChange={setYears} />
      <p className="kv small"><span>Aujourd'hui {usd(start)} + versements</span><b>{usd(paid)} versés</b></p>
      {runs.map((r) => {
        const end = r.points.at(-1)!.value;
        return (
          <p key={r.rate} className="kv">
            <span>Si {r.rate} % par an</span>
            <b>{usd(end)} <small className={end - paid >= 0 ? "up" : "down"}>({end - paid >= 0 ? "+" : "−"}{usd(Math.abs(end - paid))})</small></b>
          </p>
        );
      })}
      <p className="muted small">
        Trois hypothèses de rendement à comparer, pas des prévisions : une année peut perdre 30 % ou plus (cryptos : davantage), et l'inflation réduit ce que ces montants
        achèteront. Sans frais ni impôts.
      </p>
    </div>
  );
}

// ---------- Rebalancing ----------

const CLASS_LABEL: Record<AssetClass, string> = { crypto: "Cryptos", stock: "Actions", cash: "Liquidités" };

/** Buys and sells to reach a target split crypto / stocks / cash. */
export function RebalanceCard({ analysis }: { analysis: PortfolioAnalysis }) {
  const [target, setTarget] = useState<Record<AssetClass, string>>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem("altim.rebalance") ?? "null") as Record<AssetClass, string> | null;
      if (saved) return saved;
    } catch {
      /* default below */
    }
    return { crypto: "40", stock: "50", cash: "10" };
  });
  const set = (k: AssetClass, v: string) => {
    const next = { ...target, [k]: v };
    setTarget(next);
    try {
      localStorage.setItem("altim.rebalance", JSON.stringify(next));
    } catch {
      /* private browsing: not remembered */
    }
  };
  const t = { crypto: num(target.crypto) || 0, stock: num(target.stock) || 0, cash: num(target.cash) || 0 };
  const sum = t.crypto + t.stock + t.cash;
  const r = rebalance(analysis.lines.map((l) => ({ id: l.id, kind: l.kind, value: l.value })), analysis.cash, t);
  const small = (v: number) => Math.abs(v) < Math.max(10, (r?.total ?? 0) * 0.01);

  return (
    <div className="card rebalance-card">
      <h2 className="card-title">Rééquilibrer</h2>
      <div className="grid-3">
        {(["crypto", "stock", "cash"] as AssetClass[]).map((k) => (
          <label key={k} className="field">
            <span>{CLASS_LABEL[k]} (%)</span>
            <input inputMode="decimal" value={target[k]} onChange={(e) => set(k, e.target.value)} />
          </label>
        ))}
      </div>
      {Math.abs(sum - 100) > 0.01 && <p className="notice warn small">La cible fait {sum.toLocaleString("fr-FR")} % : elle doit faire 100 %.</p>}
      {r && (
        <>
          {(["crypto", "stock", "cash"] as AssetClass[]).map((k) => (
            <p key={k} className="kv small">
              <span>{CLASS_LABEL[k]} : {r.current[k].toLocaleString("fr-FR", { maximumFractionDigits: 0 })} % → {t[k].toLocaleString("fr-FR")} %</span>
              <b className={small(r.moves[k]) ? "" : r.moves[k] > 0 ? "up" : "down"}>
                {small(r.moves[k]) ? "rien à faire" : `${r.moves[k] > 0 ? (k === "cash" ? "mettre de côté" : "acheter") : k === "cash" ? "investir" : "vendre"} ${usd(Math.abs(r.moves[k]))}`}
              </b>
            </p>
          ))}
          {r.lines.some((l) => !small(l.amount)) && (
            <ul className="rebalance-lines small">
              {r.lines.filter((l) => !small(l.amount)).map((l) => (
                <li key={l.id}>{l.amount > 0 ? "Acheter" : "Vendre"} {usd(Math.abs(l.amount))} de {analysis.lines.find((x) => x.id === l.id)?.symbol}</li>
              ))}
            </ul>
          )}
          <p className="muted small">
            Réparti au prorata de vos lignes actuelles{r.moves.crypto > 0 && !analysis.lines.some((l) => l.kind === "crypto") ? " (aucune crypto détenue : à répartir vous-même)" : ""}. Avant de vendre, pensez aux frais et à l'impôt sur les plus-values. Altim ne passe aucun ordre.
          </p>
        </>
      )}
    </div>
  );
}
