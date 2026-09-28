import { useEffect, useState } from "react";

const KEY = "altim.notes.v1";
type Notes = Record<string, { text: string; updated: number }>;

function load(): Notes {
  try {
    const n = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Notes;
    return n && typeof n === "object" ? n : {};
  } catch {
    return {};
  }
}

/** Personal notes on an asset (why I bought, my plan, when I sell): kept in this browser only. */
export function NoteCard({ id, symbol }: { id: string; symbol: string }) {
  const [text, setText] = useState(() => load()[id]?.text ?? "");
  const [updated, setUpdated] = useState<number | null>(() => load()[id]?.updated ?? null);

  useEffect(() => {
    const n = load()[id];
    setText(n?.text ?? "");
    setUpdated(n?.updated ?? null);
  }, [id]);

  const save = () => {
    const all = load();
    const clean = text.trim().slice(0, 2000);
    if (clean) all[id] = { text: clean, updated: Date.now() };
    else delete all[id];
    try {
      localStorage.setItem(KEY, JSON.stringify(all));
      setUpdated(clean ? Date.now() : null);
    } catch {
      /* private browsing: not kept */
    }
  };

  return (
    <div className="card note-card">
      <h2 className="card-title">Mes notes · {symbol}</h2>
      <textarea
        value={text}
        maxLength={2000}
        rows={4}
        placeholder="Pourquoi j'achète, mon plan, quand je vends… (ex. « acheté pour 3 ans, je renforce sous 60 000 $, je vends si la thèse change »)"
        onChange={(e) => setText(e.target.value)}
        onBlur={save}
        aria-label={`Mes notes sur ${symbol}`}
      />
      <p className="muted small">
        {updated ? `Enregistré le ${new Date(updated).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}. ` : ""}
        Gardé dans ce navigateur seulement, jamais envoyé au serveur. Relire sa thèse avant d'acheter ou de vendre évite les décisions sur un coup de tête.
      </p>
    </div>
  );
}
