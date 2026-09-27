import { useState } from "react";

const LINKS = [
  ["/#live", "Signal live"],
  ["/#features", "Fonctionnalités"],
  ["/#sources", "Sources"],
  ["/#how", "Moteur"],
  ["/#security", "Confidentialité"],
  ["/#faq", "FAQ"],
] as const;

export function Nav() {
  const [open, setOpen] = useState(false);
  return (
    <header className="nav" id="top">
      <a href="/" className="brand" aria-label="Altim, accueil">
        <img src="/logo.svg" alt="" width={34} height={34} />
        <span>ALTIM</span>
      </a>
      <nav className={open ? "links open" : "links"} onClick={() => setOpen(false)}>
        {LINKS.map(([href, label]) => (
          <a key={href} href={href}>
            {label}
          </a>
        ))}
        <a href="/app" className="btn btn-small">
          Ouvrir l'app
        </a>
      </nav>
      <button className="burger" aria-label="Menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span />
        <span />
      </button>
    </header>
  );
}
