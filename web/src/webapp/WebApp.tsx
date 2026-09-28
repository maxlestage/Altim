import { useEffect, useState } from "react";
import { onLink, usePath } from "./router";
import { setState, useAppState } from "./store";
import { Radar } from "./Radar";
import { AssetScreen } from "./AssetScreen";
import { Settings } from "./Settings";
import { MyHoldings } from "./MyHoldings";
import { Selection } from "./Selection";
import { News } from "./News";
import { Glossary } from "./Glossary";
import { Simulation } from "./Simulation";
import { Journal } from "./Journal";

const TABS = [
  { href: "/app", label: "Radar", icon: "M3 12a9 9 0 1 0 18 0 9 9 0 1 0-18 0M12 12l6-6M7.5 12a4.5 4.5 0 0 0 9 0" },
  { href: "/app/selection", label: "Sélection", icon: "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z" },
  { href: "/app/avoirs", label: "Mes avoirs", icon: "M12 2 3 7l9 5 9-5-9-5zM3 12l9 5 9-5M3 17l9 5 9-5" },
  { href: "/app/actu", label: "Actu", icon: "M4 5h13v14H6a2 2 0 0 1-2-2zM17 9h3v8a2 2 0 0 1-2 2M8 9h6M8 13h6M8 17h4" },
  { href: "/app/reglages", label: "Réglages", icon: "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M14 4v4M8 10v4M16 16v4" },
];

function Icon({ d }: { d: string }) {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={d} />
    </svg>
  );
}

export function WebApp() {
  const path = usePath();
  const state = useAppState();

  useEffect(() => {
    document.title = "Altim — Application web";
  }, []);

  if (!state.acceptedDisclaimer) return <Disclaimer />;

  const asset = path.match(/^\/app\/actif\/(crypto|stock)\/([A-Za-z0-9.\-]{1,10})$/);
  let screen;
  if (asset) screen = <AssetScreen key={path} kind={asset[1] as "crypto" | "stock"} symbol={asset[2]!.toUpperCase()} />;
  else if (path === "/app/avoirs") screen = <MyHoldings />;
  else if (path === "/app/selection") screen = <Selection />;
  else if (path === "/app/reglages") screen = <Settings />;
  else if (path === "/app/actu") screen = <News />;
  else if (path === "/app/lexique") screen = <Glossary />;
  else if (path === "/app/simulation") screen = <Simulation />;
  else if (path === "/app/journal") screen = <Journal />;
  else screen = <Radar />;

  // The simulation and the journal sit next to the real holdings (tab "Mes avoirs").
  const active = asset ? "/app" : path === "/app/simulation" || path === "/app/journal" ? "/app/avoirs" : TABS.find((t) => t.href === path)?.href ?? "/app";

  return (
    <div className="webapp">
      <header className="app-bar">
        <a href="/app" onClick={onLink} className="brand" aria-label="Altim, radar">
          <img src="/logo.svg" alt="" width={30} height={30} />
          <span>ALTIM</span>
        </a>
        <span className="app-env">CONSEIL</span>
        <nav className="app-tabs-top" aria-label="Sections">
          {TABS.map((t) => (
            <a key={t.href} href={t.href} onClick={onLink} className={t.href === active ? "on" : ""} aria-current={t.href === active ? "page" : undefined}>
              {t.label}
            </a>
          ))}
        </nav>
        <a href="/" className="app-site">Site</a>
      </header>
      <main className="app-main">{screen}</main>
      <nav className="app-tabs" aria-label="Sections">
        {TABS.map((t) => (
          <a key={t.href} href={t.href} onClick={onLink} className={t.href === active ? "on" : ""} aria-current={t.href === active ? "page" : undefined}>
            <Icon d={t.icon} />
            <span>{t.label}</span>
          </a>
        ))}
      </nav>
    </div>
  );
}

function Disclaimer() {
  const [ok, setOk] = useState(false);
  return (
    <main className="app-disclaimer">
      <img src="/logo.svg" alt="" width={72} height={72} />
      <h1>
        Bienvenue sur <span className="gradient">Altim</span>
      </h1>
      <ul>
        <li>Les signaux sont des probabilités calculées sur l'historique, jamais des certitudes. Aucun outil ne garantit un gain.</li>
        <li>Chaque cours est recoupé sur plusieurs sources indépendantes ; si les données ne sont pas fiables, le signal est suspendu.</li>
        <li>Altim vous <b>conseille</b> : il ne passe aucun ordre et n'a jamais accès à vos comptes. Vous restez libre de suivre ou non ses conseils.</li>
        <li>Ce que vous renseignez dans « Mes avoirs » reste dans ce navigateur.</li>
        <li>Le trading comporte un risque de perte totale du capital investi.</li>
      </ul>
      <label className="check-row">
        <input type="checkbox" checked={ok} onChange={(e) => setOk(e.target.checked)} />
        <span>J'ai compris que je reste seul responsable de mes décisions d'investissement.</span>
      </label>
      <button className="btn" disabled={!ok} onClick={() => setState({ acceptedDisclaimer: true })}>
        Entrer dans l'app
      </button>
      <a href="/" className="muted back-site">
        ← Retour au site
      </a>
    </main>
  );
}
