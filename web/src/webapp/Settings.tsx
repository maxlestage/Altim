import { useState } from "react";
import { DEFAULT_RISK, type RiskSettings } from "../engine/risk";
import { AssetPicker } from "./AssetPicker";
import { assetKey, DEFAULT_WATCHLIST, setState, useAppState, type HorizonPref, type WatchItem } from "./store";
import { Segmented } from "./ui";
import { HORIZONS } from "../engine/fibonacci";

const RISK_FIELDS: { key: keyof RiskSettings; label: string; min: number; max: number; step: number; unit: string }[] = [
  { key: "riskPerTradePercent", label: "Risque accepté par idée", min: 0.25, max: 5, step: 0.25, unit: " %" },
  { key: "maxPositionPercent", label: "Taille max d'une ligne", min: 5, max: 100, step: 5, unit: " %" },
  { key: "minRiskReward", label: "Gain/risque min", min: 1, max: 5, step: 0.25, unit: "" },
];

export function Settings() {
  const { risk, watchlist, horizon } = useAppState();
  const [picking, setPicking] = useState(false);

  const setRisk = (key: keyof RiskSettings, value: number) => setState((s) => ({ risk: { ...s.risk, [key]: value } }));
  const toggle = (item: WatchItem) =>
    setState((s) => ({
      watchlist: s.watchlist.some((w) => assetKey(w) === assetKey(item))
        ? s.watchlist.filter((w) => assetKey(w) !== assetKey(item))
        : [...s.watchlist, { symbol: item.symbol, kind: item.kind, name: item.name }],
    }));

  return (
    <section className="app-screen">
      <div className="screen-top"><h1>Réglages</h1></div>

      <div className="card">
        <h2 className="card-title">Radar</h2>
        <p className="muted small">Suivez autant d'actifs que vous voulez, parmi toutes les cryptos et toutes les actions et ETF cotés aux États-Unis.</p>
        <button className="btn btn-ghost" onClick={() => setPicking(true)}>+ Ajouter : rechercher une crypto ou une action</button>
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
      {picking && (
        <AssetPicker title="Actifs du radar" selected={new Set(watchlist.map(assetKey))} onToggle={toggle} onClose={() => setPicking(false)} />
      )}

      <div className="card">
        <h2 className="card-title">Mon horizon d'investissement</h2>
        <p className="muted small">
          Les zones d'achat ne sont pas les mêmes selon la durée pendant laquelle vous comptez garder l'actif. Votre horizon est mis en avant sur chaque fiche ; les autres restent consultables.
        </p>
        <Segmented<HorizonPref>
          label="Horizon d'investissement"
          value={horizon}
          onChange={(v) => setState({ horizon: v })}
          options={(["short", "medium", "long"] as HorizonPref[]).map((h) => [h, HORIZONS[h].label])}
        />
        <p className="muted small">{HORIZONS[horizon].label} : bougies {HORIZONS[horizon].unit}, détention de {HORIZONS[horizon].holding}.</p>
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
