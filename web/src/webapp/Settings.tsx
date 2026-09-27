import { useEffect, useState } from "react";
import { DEFAULT_RISK, type RiskSettings } from "../engine/risk";
import { api } from "./api";
import { assetKey, DEFAULT_WATCHLIST, setState, useAppState, type WatchItem } from "./store";

const RISK_FIELDS: { key: keyof RiskSettings; label: string; min: number; max: number; step: number; unit: string }[] = [
  { key: "riskPerTradePercent", label: "Risque accepté par idée", min: 0.25, max: 5, step: 0.25, unit: " %" },
  { key: "maxPositionPercent", label: "Taille max d'une ligne", min: 5, max: 100, step: 5, unit: " %" },
  { key: "minRiskReward", label: "Gain/risque min", min: 1, max: 5, step: 0.25, unit: "" },
];

export function Settings() {
  const { risk, watchlist } = useAppState();
  const [q, setQ] = useState("");
  const [results, setResults] = useState<WatchItem[]>([]);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (q.trim().length < 2) return setResults([]);
    let alive = true;
    setSearching(true);
    const t = setTimeout(() => {
      api.search(q.trim())
        .then((r) => alive && setResults(r))
        .catch(() => alive && setResults([]))
        .finally(() => alive && setSearching(false));
    }, 350);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [q]);

  const setRisk = (key: keyof RiskSettings, value: number) => setState((s) => ({ risk: { ...s.risk, [key]: value } }));
  const add = (item: WatchItem) => {
    if (!watchlist.some((w) => assetKey(w) === assetKey(item))) setState((s) => ({ watchlist: [...s.watchlist, item] }));
    setQ("");
  };

  return (
    <section className="app-screen">
      <div className="screen-top"><h1>Réglages</h1></div>

      <div className="card">
        <h2 className="card-title">Radar</h2>
        <label className="field">
          <span>Ajouter un actif (crypto ou action)</span>
          <input type="search" placeholder="BTC, SOL, Apple, NVDA…" value={q} onChange={(e) => setQ(e.target.value)} />
        </label>
        {searching && <p className="muted small">Recherche…</p>}
        {results.length > 0 && (
          <ul className="search-results">
            {results.map((r) => (
              <li key={assetKey(r)}>
                <button onClick={() => add(r)}>
                  <b>{r.symbol}</b> <span className="muted">{r.name}</span> <small className="tag">{r.kind === "crypto" ? "Crypto" : "Action"}</small>
                </button>
              </li>
            ))}
          </ul>
        )}
        <ul className="watch-edit">
          {watchlist.map((w) => (
            <li key={assetKey(w)}>
              <span><b>{w.symbol}</b> <span className="muted">{w.name}</span></span>
              <button aria-label={`Retirer ${w.name}`} onClick={() => setState((s) => ({ watchlist: s.watchlist.filter((x) => assetKey(x) !== assetKey(w)) }))}>Retirer</button>
            </li>
          ))}
        </ul>
        <button className="link-btn" onClick={() => setState({ watchlist: DEFAULT_WATCHLIST })}>Liste par défaut</button>
      </div>

      <div className="card">
        <h2 className="card-title">Prudence des conseils</h2>
        <p className="muted small">Ces réglages déterminent les montants conseillés : la part de votre patrimoine qu'un conseil d'achat accepte de risquer si le stop est touché, et la taille maximale d'une ligne.</p>
        {RISK_FIELDS.map((f) => (
          <div className="stepper" key={f.key}>
            <span>{f.label}</span>
            <div>
              <button aria-label={`Diminuer ${f.label}`} onClick={() => setRisk(f.key, Math.max(f.min, +(risk[f.key] - f.step).toFixed(2)))}>−</button>
              <b className="mono">{risk[f.key]}{f.unit}</b>
              <button aria-label={`Augmenter ${f.label}`} onClick={() => setRisk(f.key, Math.min(f.max, +(risk[f.key] + f.step).toFixed(2)))}>+</button>
            </div>
          </div>
        ))}
        <p className="muted small">Règle professionnelle : ne jamais risquer plus de 1 à 2 % de son patrimoine sur une seule idée.</p>
        <button className="link-btn" onClick={() => setState({ risk: DEFAULT_RISK })}>Valeurs recommandées</button>
      </div>

      <div className="card">
        <h2 className="card-title">À propos</h2>
        <p className="kv small"><span>Sources crypto</span><b>8 recoupées</b></p>
        <p className="kv small"><span>Sources actions</span><b>Yahoo, Nasdaq, Cboe</b></p>
        <p className="muted small">
          Altim est un outil d'aide à la décision, pas un conseil en investissement. <a href="/risques" className="link">Avertissement sur les risques</a> ·{" "}
          <a href="/confidentialite" className="link">Confidentialité</a>
        </p>
        <p className="muted small">Conception & développement : Maxime Nathan Lestage</p>
      </div>
    </section>
  );
}
