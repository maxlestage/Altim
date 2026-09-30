//! Footer of the site (Footer.tsx).
use yew::prelude::*;

const AUTHOR: &str = "Maxime Nathan Lestage";

const COLUMNS: [(&str, &[(&str, &str)]); 3] = [
    (
        "Produit",
        &[
            ("/#apps", "Applications iPhone, Watch et Android"),
            ("/app", "Application web"),
            ("/#live", "Signal en direct"),
            ("/#features", "Fonctionnalités"),
            ("/#sources", "Sources de données"),
            ("/#how", "Le moteur"),
            ("/#bot", "Bot Altim"),
            ("/#security", "Confidentialité"),
        ],
    ),
    ("Ressources", &[("/#faq", "Questions fréquentes"), ("/#transparency", "Transparence"), ("/#download", "Ouvrir l'app")]),
    ("Légal", &[("/mentions-legales", "Mentions légales"), ("/confidentialite", "Confidentialité"), ("/risques", "Avertissement sur les risques")]),
];

const DATA: &str = "Binance · OKX · Coinbase · Kraken · KuCoin · Gate.io · Bitfinex · CoinGecko · Yahoo Finance · Nasdaq · Cboe · WSJ / MarketWatch · Financial Times · Fidelity · Robinhood · alternative.me · StockTwits";

#[component]
pub fn Footer() -> Html {
    let year = js_sys::Date::new_0().get_full_year();
    html! {
        <footer class="footer" aria-labelledby="footer-title">
            <h2 id="footer-title" class="sr-only">{ "Pied de page" }</h2>

            <div class="footer-top">
                <div class="footer-brand">
                    <a href="/" class="brand" aria-label="Altim, accueil">
                        <img src="/logo.svg" alt="" width="40" height="40" />
                        <span>{ "ALTIM" }</span>
                    </a>
                    <p class="footer-tagline">{ "Le marché, décodé." }</p>
                    <p class="muted">
                        { "Signaux d'achat et de vente pour la crypto et les actions, vérifiés sur 40 sources de prix. Applications iPhone, Apple Watch et Android, et application web." }
                    </p>
                    <ul class="footer-badges" aria-label="Caractéristiques">
                        <li>{ "iPhone · Watch · Android · Web" }</li>
                        <li>{ "Prix en direct" }</li>
                        <li>{ "40 sources" }</li>
                    </ul>
                </div>

                <nav class="footer-nav" aria-label="Liens du pied de page">
                    { for COLUMNS.iter().map(|(title, links)| html! {
                        <div key={*title} class="footer-col">
                            <h3>{ *title }</h3>
                            <ul>
                                { for links.iter().map(|(href, label)| html! { <li key={*label}><a href={*href}>{ *label }</a></li> }) }
                            </ul>
                        </div>
                    }) }
                </nav>
            </div>

            <section class="footer-credits" aria-label="Crédits">
                <h3>{ "Crédits" }</h3>
                <dl>
                    <div>
                        <dt>{ "Conception, design & développement" }</dt>
                        <dd><span class="author">{ AUTHOR }</span></dd>
                    </div>
                    <div>
                        <dt>{ "Données de marché" }</dt>
                        <dd>{ DATA }</dd>
                    </div>
                    <div>
                        <dt>{ "Technologies" }</dt>
                        <dd>{ "Rust · Yew (WebAssembly) · Axum · hébergé sur Heroku" }</dd>
                    </div>
                    <div>
                        <dt>{ "Typographies" }</dt>
                        <dd>{ "Orbitron, Space Grotesk, JetBrains Mono — SIL Open Font License" }</dd>
                    </div>
                </dl>
            </section>

            <p class="risk" id="risk">
                <b>{ "Avertissement sur les risques." }</b>
                { " Le trading de crypto-actifs et d'instruments financiers comporte un risque élevé de perte en capital. Les performances passées ne préjugent pas des performances futures. Altim est un outil d'aide à la décision : il ne fournit pas de conseil en investissement personnalisé et n'est pas un prestataire de services d'investissement. " }
                <a href="/risques">{ "En savoir plus" }</a>
            </p>

            <div class="footer-bottom">
                <p>{ format!("© {year} Altim · {AUTHOR}. Tous droits réservés.") }</p>
                <a href="#top" class="to-top" aria-label="Revenir en haut de la page">{ "Haut de page ↑" }</a>
            </div>
        </footer>
    }
}
