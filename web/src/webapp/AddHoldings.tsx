import { useEffect, useState } from "react";
import type { Kind } from "../engine/reliability";
import { formatPrice } from "../market";
import { api, type UniverseItem } from "./api";
import { AssetPicker, KIND_LABEL } from "./AssetPicker";
import { addHoldings, assetKey, useHoldings } from "./store";

type Line = { asset: UniverseItem; qty: string; pru: string };

const usd = (v: number) => `${v.toLocaleString("fr-FR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })} $`;
const num = (s: string) => Number(s.replace(/\s/g, "").replace(",", "."));

/** Several holdings entered at once, in two sections: cryptos and stocks / ETF. */
export function AddHoldings({ onClose }: { onClose: () => void }) {
  const { holdings } = useHoldings();
  const [lines, setLines] = useState<Line[]>([]);
  const [picking, setPicking] = useState<Kind | null>(null);
  const [prices, setPrices] = useState<Record<string, number>>({});
  const keys = lines.map((l) => assetKey(l.asset));

  // Current consensus price of every chosen asset (default average cost, amount check).
  useEffect(() => {
    const missing = lines.map((l) => l.asset).filter((a) => !(assetKey(a) in prices));
    if (!missing.length) return;
    api.quotes(missing)
      .then((r) => setPrices((p) => ({ ...p, ...Object.fromEntries(missing.map((a) => [assetKey(a), r.find((q) => q.symbol === a.symbol && q.kind === a.kind)?.price ?? 0])) })))
      .catch(() => setPrices((p) => ({ ...p, ...Object.fromEntries(missing.map((a) => [assetKey(a), 0])) })));
  }, [keys.join(",")]);

  const toggle = (asset: UniverseItem) =>
    setLines((ls) => (ls.some((l) => assetKey(l.asset) === assetKey(asset)) ? ls.filter((l) => assetKey(l.asset) !== assetKey(asset)) : [...ls, { asset, qty: "", pru: "" }]));
  const update = (key: string, patch: Partial<Line>) => setLines((ls) => ls.map((l) => (assetKey(l.asset) === key ? { ...l, ...patch } : l)));

  const parsed = lines.map((l) => {
    const price = prices[assetKey(l.asset)] || 0;
    const quantity = num(l.qty);
    const averagePrice = l.pru.trim() === "" ? price : num(l.pru);
    const valid = quantity > 0 && Number.isFinite(averagePrice) && averagePrice > 0;
    return { l, price, quantity, averagePrice, valid };
  });
  const ready = parsed.filter((p) => p.valid);
  const incomplete = parsed.length - ready.length;
  const invested = ready.reduce((a, p) => a + p.quantity * p.averagePrice, 0);

  const save = () => {
    addHoldings(ready.map((p) => ({ symbol: p.l.asset.symbol, kind: p.l.asset.kind, name: p.l.asset.name, quantity: p.quantity, averagePrice: p.averagePrice })));
    onClose();
  };

  const section = (kind: Kind) => {
    const rows = parsed.filter((p) => p.l.asset.kind === kind);
    return (
      <fieldset className="entry-section">
        <legend className="section-label">{KIND_LABEL[kind]}{rows.length ? ` · ${rows.length}` : ""}</legend>
        {rows.map(({ l, price, valid }) => {
          const key = assetKey(l.asset);
          const held = holdings.some((h) => h.symbol === l.asset.symbol && h.kind === l.asset.kind);
          return (
            <div key={key} className={`entry-line card ${valid ? "" : "incomplete"}`}>
              <div className="entry-head">
                <div className="holding-name">
                  <b>{l.asset.name}</b>
                  <small className="muted mono">{l.asset.symbol}{price ? ` · cours ${formatPrice(price)} $` : ""}</small>
                </div>
                <button className="link-btn danger" aria-label={`Retirer ${l.asset.name}`} onClick={() => toggle(l.asset)}>Retirer</button>
              </div>
              <div className="entry-fields">
                <label className="field">
                  <span>Quantité</span>
                  <input inputMode="decimal" value={l.qty} placeholder="ex. 0,5" onChange={(e) => update(key, { qty: e.target.value })} />
                </label>
                <label className="field">
                  <span>Prix moyen payé ($)</span>
                  <input inputMode="decimal" value={l.pru} placeholder={price ? formatPrice(price) : "ex. 100"} onChange={(e) => update(key, { pru: e.target.value })} />
                </label>
              </div>
              {held && <p className="muted small">Déjà dans vos avoirs : la quantité sera ajoutée et le prix moyen recalculé.</p>}
            </div>
          );
        })}
        <button className="btn btn-ghost" onClick={() => setPicking(kind)}>
          + {kind === "crypto" ? "Ajouter des cryptos" : "Ajouter des actions / ETF"}
        </button>
      </fieldset>
    );
  };

  return (
    <>
      <div className="sheet-backdrop" onClick={onClose}>
        <div className="sheet" role="dialog" aria-modal="true" aria-label="Ajouter des avoirs" onClick={(e) => e.stopPropagation()}>
          <div className="sheet-handle" />
          <div className="sheet-head"><h2>Ajouter des avoirs</h2></div>
          <p className="muted small">
            Choisissez autant de cryptos et d'actions que vous voulez, puis indiquez pour chacune la quantité et le prix d'achat moyen
            (laissé vide = cours actuel).
          </p>
          {section("crypto")}
          {section("stock")}
          {ready.length > 0 && <p className="kv small"><span>Montant investi</span><b>{usd(invested)}</b></p>}
          {incomplete > 0 && <p className="muted small">{incomplete} ligne{incomplete > 1 ? "s" : ""} sans quantité : ignorée{incomplete > 1 ? "s" : ""}.</p>}
          <button className="btn" disabled={!ready.length} onClick={save}>
            {ready.length ? `Enregistrer ${ready.length} avoir${ready.length > 1 ? "s" : ""}` : "Enregistrer"}
          </button>
          <button className="btn btn-ghost" onClick={onClose}>Annuler</button>
        </div>
      </div>
      {picking && (
        <AssetPicker
          kind={picking}
          title={picking === "crypto" ? "Toutes les cryptos" : "Toutes les actions et ETF"}
          selected={new Set(keys)}
          onToggle={toggle}
          onClose={() => setPicking(null)}
        />
      )}
    </>
  );
}
