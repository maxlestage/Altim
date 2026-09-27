import { useEffect, useRef, useState } from "react";
import { api, type UniverseItem } from "./api";
import { assetKey } from "./store";

/**
 * One search box for everything: cryptos and stocks / ETF together, by symbol or name
 * (BTC, Solana, Apple, NVDA, S&P 500…). No list to scroll through.
 */
export function AssetSearch({
  selected,
  onToggle,
  autoFocus = false,
  clearOnPick = false,
  label = "Rechercher une crypto ou une action",
}: {
  selected: Set<string>;
  onToggle: (item: UniverseItem) => void;
  autoFocus?: boolean;
  clearOnPick?: boolean;
  label?: string;
}) {
  const [q, setQ] = useState("");
  const [results, setResults] = useState<UniverseItem[]>([]);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const query = q.trim();
    if (!query) return setResults([]);
    let alive = true;
    setSearching(true);
    const t = setTimeout(() => {
      api.search(query, 20)
        .then((r) => alive && (setResults(r), setError(null)))
        .catch((e) => alive && setError(e instanceof Error ? e.message : "Recherche indisponible"))
        .finally(() => alive && setSearching(false));
    }, 200);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [q]);

  const pick = (it: UniverseItem) => {
    onToggle(it);
    if (clearOnPick) {
      setQ("");
      input.current?.focus();
    }
  };

  return (
    <div className="asset-search">
      <label className="field">
        <span>{label}</span>
        <input
          ref={input}
          type="search"
          autoFocus={autoFocus}
          enterKeyHint="search"
          autoComplete="off"
          placeholder="BTC, Solana, Apple, NVDA, S&P 500…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </label>
      {error && <p className="notice warn">⚠ {error}</p>}
      {q.trim() && !searching && !results.length && !error && (
        <p className="muted small">Aucun résultat pour « {q.trim()} ». Les actions cotées en euros ne sont pas encore prises en charge.</p>
      )}
      {results.length > 0 && (
        <ul className="search-results picker-list">
          {results.map((it) => {
            const on = selected.has(assetKey(it));
            return (
              <li key={assetKey(it)}>
                <button className={on ? "picked" : ""} aria-pressed={on} onClick={() => pick(it)}>
                  <span className="pick-box" aria-hidden>{on ? "✓" : "+"}</span>
                  <b>{it.symbol}</b>
                  <span className="muted pick-name">{it.name}</span>
                  <small className="tag">{it.kind === "crypto" ? "Crypto" : it.etf ? "ETF" : "Action"}</small>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
