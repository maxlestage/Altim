import { useState } from "react";

const LINKS = [
  ["#live", "Signal live"],
  ["#features", "Fonctionnalités"],
  ["#how", "Moteur"],
  ["#security", "Sécurité"],
  ["#faq", "FAQ"],
] as const;

export function Nav() {
  const [open, setOpen] = useState(false);
  return (
    <header className="nav">
      <a href="#top" className="brand" aria-label="Altim, accueil">
        <img src="/logo.svg" alt="" width={34} height={34} />
        <span>ALTIM</span>
      </a>
      <nav className={open ? "links open" : "links"} onClick={() => setOpen(false)}>
        {LINKS.map(([href, label]) => (
          <a key={href} href={href}>
            {label}
          </a>
        ))}
        <a href="#download" className="btn btn-small">
          Télécharger
        </a>
      </nav>
      <button className="burger" aria-label="Menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span />
        <span />
      </button>
    </header>
  );
}
