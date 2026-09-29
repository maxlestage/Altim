/**
 * More of the signal's track record (inside the "Historique du signal" section of the decision card): spread cost,
 * expectancy, R multiples, results by market regime, how the test avoids flattering itself, and the tax note.
 * Isolated so the card itself only renders it.
 */
import { num, pct, type Track } from "./decision";
import { onLink } from "./router";

/** French flat tax (PFU) on the net gain, as in the sale tool: an assumption, not the user's own situation. */
const FLAT_TAX = 30;

export function TrackDetails({ track: t }: { track: Track }) {
  // Older answers (before these fields) show nothing more.
  if (t.regimes === undefined && t.expectancy === undefined) return null;
  const afterTax = t.totalReturn > 0 ? t.totalReturn * (1 - FLAT_TAX / 100) : t.totalReturn;
  return (
    <div className="track-details">
      <p className="kv small"><span>Espérance par trade (coûts inclus)</span><b className={(t.expectancy ?? 0) >= 0 ? "up" : "down"}>{pct(t.expectancy, 2, true)}</b></p>
      <p className="kv small"><span>Multiple de R moyen (gain ÷ risque jusqu'au stop)</span><b>{t.avgR == null ? "—" : `${num(t.avgR, 2)} R`}</b></p>
      {t.spreadPct != null && (
        <p className="kv small"><span>Écart achat/vente {t.spreadMeasured ? "mesuré" : "supposé"}</span><b>{pct(t.spreadPct, 3)}</b></p>
      )}
      {t.spreadNote && <p className="muted small">{t.spreadNote}</p>}

      {t.regimes && t.regimes.length > 0 && (
        <>
          <p className="small"><b>Selon le régime de marché</b></p>
          <ul className="insights">
            {t.regimes.map((g) => (
              <li key={g.regime} className={`insight ${g.lowSample ? "info" : (g.avgReturn ?? 0) >= 0 ? "good" : "warning"}`}>
                <span aria-hidden>{g.regime === "bull" ? "↗" : g.regime === "bear" ? "↘" : g.regime === "crisis" ? "⚠" : "→"}</span>
                <span className="small">
                  <b>{g.label}</b>
                  <br />
                  {g.trades} trade{g.trades > 1 ? "s" : ""}
                  {g.trades > 0 && <> · réussite {pct(g.winRate, 0)} · moyenne {pct(g.avgReturn, 1, true)}</>}
                  {g.lowSample && <> · <span className="chip muted">échantillon trop faible</span></>}
                </span>
              </li>
            ))}
          </ul>
          <p className="muted small">
            Haussier : clôture au-dessus d'une moyenne 200 jours qui monte (sur 20 jours) ; baissier : sous une moyenne qui baisse ; crise : plus de 30 % sous le plus haut de l'année.
            Régime lu à la date du signal, sans données futures.
          </p>
        </>
      )}

      {t.biasNotes && t.biasNotes.length > 0 && (
        <>
          <p className="small"><b>Comment ce test évite de se flatter</b></p>
          <ul className="reasons">{t.biasNotes.map((n) => <li key={n}>{n}</li>)}</ul>
        </>
      )}

      <p className="muted small">
        Impôt (hypothèse : flat tax de {FLAT_TAX} % sur le gain net, payée à la fin, pertes compensées) : rendement du signal après impôt ≈ {pct(afterTax, 1, true)}. Votre situation
        fiscale peut différer.
      </p>
      <p className="small">
        <a href="/app/validation" onClick={onLink} className="link">Validation du modèle : le même test sur 34 actifs →</a>
      </p>
    </div>
  );
}
