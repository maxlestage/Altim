import { useEffect, useState } from "react";
import type { Kind } from "../engine/reliability";
import { api, type UniverseItem } from "./api";
import { assetKey } from "./store";
import { Segmented } from "./ui";

const PAGE = 50;
export const KIND_LABEL: Record<Kind, string> = { crypto: "Crypto", stock: "Actions & ETF" };

/**
 * Full catalogue (every crypto, every US-listed stock and ETF), browsable by category and searchable.
 * Several assets can be picked in one go.
 */
export function AssetPicker({
  kind: fixedKind,
  selected,
  onToggle,
  onClose,
  title = "Choisir des actifs",
}: {
  kind?: Kind;
  selected: Set<string>;
  onToggle: (item: UniverseItem) => void;
  onClose: () => void;
  title?: string;
}) {
  const [kind, setKind] = useState<Kind>(fixedKind ?? "crypto");
  const [q, setQ] = useState("");
  const [items, setItems] = useState<UniverseItem[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async (offset: number, alive: () => boolean = () => true) => {
    setLoading(true);
    try {
      const r = await api.universe(kind, q.trim(), offset, PAGE);
      if (!alive()) return;
      setItems((prev) => (offset === 0 ? r.items : [...prev, ...r.items]));
      setTotal(r.total);
      setError(null);
    } catch (e) {
      if (alive()) setError(e instanceof Error ? e.message : "Catalogue indisponible");
    } finally {
      if (alive()) setLoading(false);
    }
  };

  useEffect(() => {
    let alive = true;
    const t = setTimeout(() => load(0, () => alive), q ? 250 : 0);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [kind, q]);

  const count = [...selected].filter((k) => k.startsWith(`${kind}:`)).length;

  return (
    <div className="sheet-backdrop picker-backdrop" onClick={onClose}>
      <div className="sheet picker" role="dialog" aria-modal="true" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="sheet-head">
          <h2>{title}</h2>
          <button className="btn btn-small" onClick={onClose}>Terminé{selected.size ? ` (${selected.size})` : ""}</button>
        </div>
        {!fixedKind && (
          <Segmented label="Catégorie" value={kind} onChange={(k) => { setKind(k); setItems([]); setTotal(null); }} options={[["crypto", "Crypto"], ["stock", "Actions & ETF"]]} />
        )}
        <label className="field">
          <span>
            {total === null ? "Chargement du catalogue…" : q.trim()
              ? `${total.toLocaleString("fr-FR")} résultat${total > 1 ? "s" : ""}`
              : kind === "crypto" ? `Toutes les cryptos : ${total.toLocaleString("fr-FR")}` : `Toutes les actions et ETF cotés aux États-Unis : ${total.toLocaleString("fr-FR")}`}
            {count ? ` · ${count} sélectionné${count > 1 ? "s" : ""}` : ""}
          </span>
          <input type="search" placeholder={kind === "crypto" ? "Filtrer : BTC, Solana, PEPE…" : "Filtrer : Apple, NVDA, S&P 500…"} value={q} onChange={(e) => setQ(e.target.value)} />
        </label>
        {error && <p className="notice warn">⚠ {error}</p>}
        <ul className="search-results picker-list">
          {items.map((it) => {
            const on = selected.has(assetKey(it));
            return (
              <li key={assetKey(it)}>
                <button className={on ? "picked" : ""} aria-pressed={on} onClick={() => onToggle(it)}>
                  <span className="pick-box" aria-hidden>{on ? "✓" : "+"}</span>
                  <b>{it.symbol}</b>
                  <span className="muted pick-name">{it.name}</span>
                  <small className="tag">{it.rank ? `#${it.rank}` : it.kind === "stock" ? (it.etf ? "ETF" : "Action") : `${it.exchanges} plateforme${(it.exchanges ?? 0) > 1 ? "s" : ""}`}</small>
                </button>
              </li>
            );
          })}
        </ul>
        {total !== null && items.length < total && (
          <button className="btn btn-ghost" disabled={loading} onClick={() => load(items.length)}>
            {loading ? "Chargement…" : `Afficher plus (${(total - items.length).toLocaleString("fr-FR")} restants)`}
          </button>
        )}
        {total === 0 && <p className="muted small">Aucun actif trouvé. Les actions cotées hors des États-Unis (en euros) ne sont pas encore prises en charge.</p>}
      </div>
    </div>
  );
}
