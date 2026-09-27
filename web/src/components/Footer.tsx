const AUTHOR = "Maxime Nathan Lestage";
const GITHUB = "https://github.com/maxlestage";

const COLUMNS = [
  {
    title: "Produit",
    links: [
      ["/app", "Application web"],
      ["/#live", "Signal en direct"],
      ["/#features", "Fonctionnalités"],
      ["/#sources", "Sources de données"],
      ["/#how", "Le moteur"],
      ["/#security", "Confidentialité"],
    ],
  },
  {
    title: "Ressources",
    links: [
      ["/#faq", "Questions fréquentes"],
      ["/#transparency", "Transparence"],
      ["/#download", "Bêta TestFlight"],
      [GITHUB, "GitHub"],
    ],
  },
  {
    title: "Légal",
    links: [
      ["/mentions-legales", "Mentions légales"],
      ["/confidentialite", "Confidentialité"],
      ["/risques", "Avertissement sur les risques"],
    ],
  },
] as const;

const DATA = "Binance · OKX · Coinbase · Kraken · KuCoin · Gate.io · Bitfinex · CoinGecko · Yahoo Finance · Nasdaq · Cboe · alternative.me · StockTwits";

export function Footer() {
  const year = new Date().getFullYear();
  return (
    <footer className="footer" aria-labelledby="footer-title">
      <h2 id="footer-title" className="sr-only">Pied de page</h2>

      <div className="footer-top">
        <div className="footer-brand">
          <a href="/" className="brand" aria-label="Altim, accueil">
            <img src="/logo.svg" alt="" width={40} height={40} />
            <span>ALTIM</span>
          </a>
          <p className="footer-tagline">Le marché, décodé.</p>
          <p className="muted">
            Signaux d'achat et de vente pour la crypto et les actions, vérifiés sur 16 sources de prix. Application
            iOS native.
          </p>
          <ul className="footer-badges" aria-label="Caractéristiques">
            <li>iOS 17+</li>
            <li>Swift natif</li>
            <li>16 sources</li>
          </ul>
        </div>

        <nav className="footer-nav" aria-label="Liens du pied de page">
          {COLUMNS.map((col) => (
            <div key={col.title} className="footer-col">
              <h3>{col.title}</h3>
              <ul>
                {col.links.map(([href, label]) => (
                  <li key={label}>
                    <a href={href} {...(href.startsWith("http") ? { target: "_blank", rel: "noreferrer" } : {})}>
                      {label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </nav>
      </div>

      <section className="footer-credits" aria-label="Crédits">
        <h3>Crédits</h3>
        <dl>
          <div>
            <dt>Conception, design & développement</dt>
            <dd>
              <a href={GITHUB} target="_blank" rel="noreferrer" className="author">
                {AUTHOR}
              </a>
            </dd>
          </div>
          <div>
            <dt>Données de marché</dt>
            <dd>{DATA}</dd>
          </div>
          <div>
            <dt>Technologies</dt>
            <dd>Swift · SwiftUI · React · TypeScript · Bun · hébergé sur Heroku</dd>
          </div>
          <div>
            <dt>Typographies</dt>
            <dd>Orbitron, Space Grotesk, JetBrains Mono — SIL Open Font License</dd>
          </div>
        </dl>
      </section>

      <p className="risk" id="risk">
        <b>Avertissement sur les risques.</b> Le trading de crypto-actifs et d'instruments financiers comporte un
        risque élevé de perte en capital. Les performances passées ne préjugent pas des performances futures. Altim
        est un outil d'aide à la décision : il ne fournit pas de conseil en investissement personnalisé et n'est pas un
        prestataire de services d'investissement. <a href="/risques">En savoir plus</a>
      </p>

      <div className="footer-bottom">
        <p>
          © {year} Altim · {AUTHOR}. Tous droits réservés.
        </p>
        <a href="#top" className="to-top" aria-label="Revenir en haut de la page">
          Haut de page ↑
        </a>
      </div>
    </footer>
  );
}
