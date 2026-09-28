import { useState } from "react";
import { DEFAULT_RISK, type RiskSettings } from "../engine/risk";
import { AssetPicker } from "./AssetPicker";
import { assetKey, DEFAULT_WATCHLIST, setState, useAppState, type HorizonPref, type WatchItem } from "./store";
import { Segmented } from "./ui";
import { onLink } from "./router";
import { HORIZONS } from "../engine/fibonacci";
import { DEFAULT_SCORE_WEIGHTS, NNBSP, SCORE_FACTORS } from "./decision";

const RISK_FIELDS: { key: keyof RiskSettings; label: string; min: number; max: number; step: number; unit: string }[] = [
  { key: "riskPerTradePercent", label: "Risque accepté par idée", min: 0.25, max: 5, step: 0.25, unit: " %" },
  { key: "maxPositionPercent", label: "Taille max d'une ligne", min: 5, max: 100, step: 5, unit: " %" },
  { key: "minRiskReward", label: "Gain/risque min", min: 1, max: 5, step: 0.25, unit: "" },
  { key: "dailyLossLimitPercent", label: "Perte max du jour", min: 0.5, max: 10, step: 0.5, unit: " %" },
  { key: "maxCryptoPercent", label: "Part crypto max", min: 0, max: 100, step: 5, unit: " %" },
];

export function Settings() {
  const { risk, watchlist, horizon, scoreWeights } = useAppState();
  const [picking, setPicking] = useState(false);
  const weightTotal = SCORE_FACTORS.reduce((a, f) => a + scoreWeights[f.key], 0);

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
        <p className="muted small">
          Perte max du jour : si votre patrimoine a déjà perdu ce pourcentage depuis la clôture de la veille, Mes avoirs vous conseille de ne plus ouvrir de position aujourd'hui.
          Part crypto max : au-delà, Mes avoirs signale une surexposition aux cryptos, qui peuvent perdre 50 % ou plus ensemble (60 % par défaut ; 10 à 30 % est plus courant pour un patrimoine prudent).
        </p>
        <button className="link-btn" onClick={() => setState({ risk: DEFAULT_RISK })}>Valeurs recommandées</button>
      </div>

      <div className="card">
        <h2 className="card-title">Score composite</h2>
        <p className="muted small">
          Poids de chaque famille dans le score de −100 à +100 de la carte Décision. Seuls les facteurs mesurés comptent : leurs poids sont ramenés à 100 %. Le verdict, lui, ne change pas.
        </p>
        <div className="score-weights">
          {SCORE_FACTORS.map((f) => (
            <label key={f.key} className="score-weight">
              <span><b>{f.label}</b> <small className="muted">{f.hint}</small></span>
              <b className="mono">{scoreWeights[f.key]}{NNBSP}%</b>
              <input
                type="range" min={0} max={100} step={1} value={scoreWeights[f.key]}
                aria-label={`Poids ${f.label}`}
                onChange={(e) => setState((s) => ({ scoreWeights: { ...s.scoreWeights, [f.key]: Math.min(100, Math.max(0, Math.round(Number(e.target.value)) || 0)) } }))}
              />
            </label>
          ))}
        </div>
        <p className="muted small">
          Total : <span className="mono">{weightTotal}</span> (ramené à 100 %).{weightTotal === 0 ? " Tous à 0 : les poids par défaut sont utilisés." : ""}
        </p>
        <button className="link-btn" onClick={() => setState({ scoreWeights: DEFAULT_SCORE_WEIGHTS })}>Poids par défaut (32 / 18 / 20 / 10 / 10 / 10)</button>
      </div>

      <div className="card">
        <h2 className="card-title">Simulation (sans argent réel)</h2>
        <p className="muted small">Un portefeuille virtuel pour tester les décisions d'Altim : achats simulés depuis la carte Décision, ventes au stop ou à l'objectif, résultats par décision. Aucun argent réel, aucun ordre passé, tout reste dans ce navigateur.</p>
        <a href="/app/simulation" onClick={onLink} className="btn btn-ghost">Ouvrir la simulation</a>
      </div>

      <div className="card">
        <h2 className="card-title">Comprendre</h2>
        <p className="muted small">Signal, zone d'achat, stop, volatilité, flat tax… les mots d'Altim expliqués simplement.</p>
        <a href="/app/lexique" onClick={onLink} className="btn btn-ghost">Ouvrir le lexique</a>
      </div>

      <div className="card">
        <h2 className="card-title">Accès privé</h2>
        <p className="muted small">Votre session reste ouverte 7 jours sur cet appareil. Déconnectez-vous sur un appareil partagé.</p>
        <form method="post" action="/logout">
          <button type="submit" className="btn btn-ghost">Se déconnecter</button>
        </form>
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
