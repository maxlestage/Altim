//! Static sections of the home page (Features, Apps, Sources, HowItWorks, Security, Transparency, FAQ, Download,
//! MobileCTA), text for text with the React components.
use yew::prelude::*;

use super::Section;
use crate::hooks::use_reveal;

fn icon(d: &'static str) -> Html {
    html! {
        <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d={d} />
        </svg>
    }
}

const FEATURES: [(&str, &str, &str); 7] = [
    (
        "M3 12h4l3-8 4 16 3-8h4",
        "Signaux par confluence",
        "Tendance (EMA 50/200 + ADX), MACD, RSI, Stochastique, Bollinger et volume (OBV) votent ensemble. Un achat n'est proposé que s'ils s'accordent.",
    ),
    (
        "M4 20V10M10 20V4M16 20v-7M22 20H2",
        "Multi-unités de temps",
        "Un signal 1 h est confirmé par la tendance 4 h. Acheter contre la tendance de fond ? Altim rétrograde automatiquement le signal.",
    ),
    (
        "M12 3v18M5 8l7-5 7 5M5 16l7 5 7-5",
        "Des conseils concrets",
        "Chaque conseil d'achat arrive avec un plan : zone d'entrée, stop à 2 ATR, objectif à 2× le risque, et le montant prudent à y consacrer selon votre patrimoine.",
    ),
    (
        "M12 2a10 10 0 1 0 10 10M12 6v6l4 2",
        "Backtest intégré",
        "Voyez comment les signaux se sont comportés sur chaque actif : rendement, taux de réussite, pire baisse — comparés à l'achat-conservation.",
    ),
    (
        "M12 2 3 7l9 5 9-5-9-5zM3 12l9 5 9-5M3 17l9 5 9-5",
        "Vos avoirs analysés",
        "Renseignez ce que vous possédez déjà : patrimoine, plus-values, répartition, risque d'une mauvaise journée et, ligne par ligne, quoi faire (conserver, alléger, protéger, renforcer).",
    ),
    (
        "M12 2 3 6v6c0 5 4 9 9 10 5-1 9-5 9-10V6z",
        "Prudence intégrée",
        "Montants conseillés pour ne risquer que 1 % de votre patrimoine par idée, plafond par ligne, et aucun conseil quand les données ne sont pas fiables.",
    ),
    (
        "M7 11V7a5 5 0 0 1 10 0v4M5 11h14v10H5z",
        "Conseil, jamais d'ordre",
        "Altim ne passe aucun ordre et n'accède à aucun compte. Vos avoirs restent sur votre appareil (navigateur ou application), jamais sur un serveur.",
    ),
];

#[component]
pub fn Features() -> Html {
    html! {
        <Section id="features" eyebrow="Fonctionnalités" title={html! { <>{ "Tout ce qu'il faut pour " }<span class="gradient">{ "décider vite et bien" }</span></> }}>
            <div class="features">
                { for FEATURES.iter().map(|(d, title, text)| html! {
                    <article key={*title} class="card feature">
                        <div class="feature-icon">{ icon(d) }</div>
                        <h3>{ *title }</h3>
                        <p>{ *text }</p>
                    </article>
                }) }
            </div>
        </Section>
    }
}

struct App {
    id: &'static str,
    name: &'static str,
    tag: &'static str,
    icon: &'static str,
    points: &'static [&'static str],
}

const APPS: [App; 4] = [
    App {
        id: "iphone",
        name: "iPhone",
        tag: "App native SwiftUI · iOS 17+",
        icon: "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zM11 18h2",
        points: &[
            "Notification « achat possible » dès qu'un actif de votre radar ou de vos avoirs devient achetable.",
            "Suivi en direct sur l'écran verrouillé et dans la Dynamic Island : prix et verdict d'achat.",
            "Face ID à l'ouverture, mot de passe chiffré dans le trousseau de l'iPhone.",
        ],
    },
    App {
        id: "watch",
        name: "Apple Watch",
        tag: "App watchOS",
        icon: "M9 2h6l1 4H8zM9 22h6l1-4H8zM7 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zM12 9v3l2 1",
        points: &[
            "Les actifs achetables d'un coup d'œil, avec leurs raisons.",
            "Les notifications d'achat de l'iPhone arrivent au poignet.",
            "Ni mot de passe ni session sur la montre : elle affiche ce que l'iPhone a calculé.",
        ],
    },
    App {
        id: "android",
        name: "Android",
        tag: "App native Kotlin · Android 11+",
        icon: "M7 9h10v8a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1zM7 9a5 5 0 0 1 10 0M9 5 8 3M15 5l1-2M10 7h.01M14 7h.01",
        points: &[
            "Notification « achat possible » toutes les 15 minutes, sans répétition.",
            "Empreinte, visage ou code de l'écran ; mot de passe chiffré par le Keystore Android.",
            "Mêmes écrans que sur iPhone : radar en direct, zones, sélection, avoirs.",
        ],
    },
    App {
        id: "web",
        name: "Navigateur",
        tag: "Application web",
        icon: "M3 5h18v14H3zM3 9h18M7 7h.01M10 7h.01",
        points: &[
            "Sur ordinateur comme sur téléphone, sans installation.",
            "Les mêmes chiffres partout : tout est calculé sur votre serveur Altim privé.",
        ],
    },
];

/// The native apps (iPhone, Apple Watch, Android) and the web app, all clients of the same private server.
#[component]
pub fn Apps() -> Html {
    html! {
        <Section id="apps" eyebrow="Applications" title={html! { <>{ "Sur " }<span class="gradient">{ "iPhone, Apple Watch et Android" }</span></> }}>
            <p class="muted apps-intro">
                { "Des applications natives, pas un site déguisé. Elles se connectent toutes à votre serveur Altim privé : 40 sources de prix, mêmes signaux, mêmes zones d'achat, mêmes chiffres qu'ici. Altim ne passe jamais d'ordre." }
            </p>
            <div class="apps">
                { for APPS.iter().map(|a| html! {
                    <article key={a.id} class="card feature app-card" id={format!("app-{}", a.id)}>
                        <div class="feature-icon">{ icon(a.icon) }</div>
                        <h3>{ a.name }</h3>
                        <p class="app-tag">{ a.tag }</p>
                        <ul>{ for a.points.iter().map(|p| html! { <li key={*p}>{ *p }</li> }) }</ul>
                    </article>
                }) }
            </div>
            <p class="muted small apps-note">
                { "Accès privé : les apps iPhone et Apple Watch s'installent par TestFlight, l'app Android par un APK signé (sans Google Play). La marche à suivre est dans le guide de déploiement." }
            </p>
        </Section>
    }
}

const CRYPTO: [&str; 23] = [
    "Binance",
    "OKX",
    "Coinbase",
    "Kraken",
    "KuCoin",
    "Gate.io",
    "Bitfinex",
    "Binance.US",
    "Bitstamp",
    "Gemini",
    "Crypto.com",
    "Bitget",
    "MEXC",
    "HTX",
    "Poloniex",
    "HitBTC",
    "WhiteBIT",
    "CoinEx",
    "XT",
    "WOO X",
    "BingX",
    "LBank",
    "CoinGecko",
];
const STOCKS: [&str; 17] = [
    "Yahoo Finance",
    "Nasdaq",
    "Robinhood",
    "Cboe",
    "StockAnalysis",
    "Webull",
    "TradingView",
    "Zacks",
    "WSJ / MarketWatch",
    "Financial Times",
    "Fidelity",
    "Finviz",
    "AlphaQuery",
    "eToro",
    "StockCharts",
    "TipRanks",
    "Public.com",
];
const CONTEXT: [&str; 2] = ["Fear & Greed (alternative.me)", "StockTwits"];

fn chips(items: &[&'static str]) -> Html {
    html! { <ul class="chips">{ for items.iter().map(|s| html! { <li key={*s}>{ *s }</li> }) }</ul> }
}

#[component]
pub fn Sources() -> Html {
    html! {
        <Section
            id="sources"
            eyebrow="Fiabilité"
            title={html! { <><span class="gradient">{ "40 sources de prix" }</span>{ " recoupées en permanence" }</> }}
            intro={html! { "Chaque prix est vérifié auprès de plusieurs places de marché indépendantes. Une source qui diverge est écartée ; si les données ne sont pas fiables, Altim suspend le signal au lieu de deviner. Ce site applique le même consensus à chaque cours affiché." }}
        >
            <div class="sources-grid">
                <div class="card"><h3>{ "Crypto" }</h3>{ chips(&CRYPTO) }</div>
                <div class="card"><h3>{ "Actions & ETF" }</h3>{ chips(&STOCKS) }</div>
                <div class="card"><h3>{ "Contexte" }</h3>{ chips(&CONTEXT) }</div>
            </div>
            <ol class="pipeline">
                <li><b>{ "Consensus" }</b>{ " jusqu'à 23 sources par crypto et 17 par action interrogées ensemble ; médiane bougie par bougie, écart toléré 0,5 % (crypto) / 1 % (actions) ; une source en retard est écartée" }</li>
                <li><b>{ "Contrôle qualité" }</b>{ " trous, pics aberrants, données périmées, volume absent" }</li>
                <li><b>{ "Réseau résilient" }</b>{ " nouvelles tentatives, disjoncteur par source, bascule automatique" }</li>
                <li><b>{ "Garde-fou" }</b>{ " données douteuses ou sources en désaccord : aucun conseil plutôt qu'un mauvais conseil" }</li>
            </ol>
        </Section>
    }
}

const STEPS: [(&str, &str, &str); 3] = [
    (
        "01",
        "Analyse",
        "Toutes les 60 s, Altim récupère les bougies clôturées (Binance, Yahoo Finance), écarte les données invalides et calcule 7 indicateurs.",
    ),
    (
        "02",
        "Décision",
        "Chaque indicateur vote de −1 à +1. Le score pondéré (−100 à +100) et l'accord entre indicateurs donnent l'action et la confiance.",
    ),
    (
        "03",
        "Conseil",
        "Altim traduit le tout en conseil clair — acheter, attendre, éviter, alléger, protéger — adapté à ce que vous possédez déjà. Vous décidez.",
    ),
];

#[component]
pub fn HowItWorks() -> Html {
    html! {
        <Section
            id="how"
            eyebrow="Le moteur"
            title={html! { <>{ "Transparent. " }<span class="gradient">{ "Explicable." }</span>{ " Vérifié." }</> }}
            intro={html! { "Pas de boîte noire : chaque signal affiche la contribution de chaque indicateur." }}
        >
            <ol class="steps">
                { for STEPS.iter().map(|(n, title, text)| html! {
                    <li key={*n} class="card step">
                        <span class="step-n">{ *n }</span>
                        <h3>{ *title }</h3>
                        <p>{ *text }</p>
                    </li>
                }) }
            </ol>
        </Section>
    }
}

const SECURITY: [(&str, &str); 6] = [
    ("Aucun accès à vos comptes", "Altim conseille uniquement : aucune clé, aucun identifiant de courtier, aucun ordre."),
    (
        "Vos avoirs restent chez vous",
        "Sur votre appareil (navigateur ou application, dans un stockage chiffré et exclu des sauvegardes), jamais sur un serveur.",
    ),
    ("Aucun traceur", "Pas de cookie, pas de mesure d'audience, pas de publicité."),
    ("Export à tout moment", "Vos avoirs s'exportent en un fichier, à réimporter sur un autre appareil."),
    ("Données vérifiées", "Chaque cours est recoupé sur plusieurs sources ; en cas de doute, aucun conseil."),
    ("Transparent", "Chaque conseil affiche ses raisons, les indicateurs et les sources utilisés."),
];

#[component]
pub fn Security() -> Html {
    html! {
        <Section id="security" eyebrow="Confidentialité" title={html! { <>{ "Vos données, " }<span class="gradient">{ "chez vous" }</span></> }}>
            <div class="security">
                { for SECURITY.iter().map(|(title, text)| html! {
                    <div key={*title} class="sec-item">
                        <span class="check" aria-hidden="true">{ "◆" }</span>
                        <div>
                            <h3>{ *title }</h3>
                            <p>{ *text }</p>
                        </div>
                    </div>
                }) }
            </div>
        </Section>
    }
}

#[component]
pub fn Transparency() -> Html {
    html! {
        <Section id="transparency" eyebrow="Honnêteté" title={html! { <>{ "Ce qu'Altim " }<span class="gradient">{ "ne vous promettra jamais" }</span></> }}>
            <div class="card transparency">
                <p>
                    { "Aucun algorithme ne prédit le marché à coup sûr. Nos backtests le montrent : en marché fortement haussier, conserver simplement l'actif rapporte souvent plus que suivre des signaux. En revanche, en marché baissier, le moteur limite nettement les pertes — sur BTC en journalier, " }
                    <b>{ "−3,8 %" }</b>{ " pour la stratégie contre" }{ " " }<b>{ "−29,6 %" }</b>{ " pour l'achat-conservation sur la même période." }
                </p>
                <p>
                    { "C'est pourquoi l'app affiche, pour chaque actif, la performance de la stratégie " }<em>{ "et" }</em>
                    { " celle de l'achat-conservation, frais inclus. Vous décidez en connaissance de cause." }
                </p>
            </div>
        </Section>
    }
}

const QA: [(&str, &str); 8] = [
    (
        "Altim garantit-il des gains ?",
        "Non. Altim est un outil d'aide à la décision. Les signaux sont des probabilités basées sur l'historique. Investissez uniquement ce que vous pouvez vous permettre de perdre.",
    ),
    (
        "Quels marchés sont couverts ?",
        "Toutes les cryptos cotées en dollar sur Binance, OKX, Coinbase, Kraken, KuCoin et Gate (environ 2 000), et toutes les actions et ETF cotés aux États-Unis (plus de 11 000, y compris de nombreuses sociétés européennes via leur cotation américaine). Les actions cotées en euros ne sont pas encore prises en charge.",
    ),
    (
        "Utilisez-vous Bloomberg ?",
        "Les données Bloomberg (Terminal, B-PIPE) nécessitent une licence professionnelle. Altim recoupe à la place 40 sources de prix : jusqu'à 23 pour une crypto (Binance, OKX, Coinbase, Kraken, Bitstamp, Gemini…) et 17 pour une action (Nasdaq, Cboe, WSJ / MarketWatch, Financial Times, Fidelity, Robinhood…), et son architecture permet de brancher un flux Bloomberg si vous disposez d'une licence.",
    ),
    (
        "Que se passe-t-il si une source donne un mauvais prix ?",
        "Elle est comparée aux autres bougie par bougie et écartée si elle s'éloigne de la médiane. Si les sources sont en désaccord ou trop peu nombreuses, le signal est suspendu et Altim ne donne aucun conseil sur cet actif.",
    ),
    (
        "Altim achète-t-il ou vend-il à ma place ?",
        "Non. Altim est un conseiller : il ne passe aucun ordre et ne demande aucun accès à vos comptes. Vous suivez ou non ses conseils chez votre courtier habituel.",
    ),
    (
        "Comment Altim tient-il compte de ce que je possède ?",
        "Renseignez vos avoirs (actif, quantité, prix d'achat moyen, liquidités) : Altim calcule votre patrimoine, vos plus-values, vos risques, et adapte chaque conseil — par exemple le montant prudent pour un nouvel achat, ou s'il faut alléger une ligne trop lourde.",
    ),
    (
        "Altim peut-il protéger mon bot de trading des grands mouvements imprévus ?",
        "Oui, via le garde-fou marché : tendance de fond, risque de choc (volatilité anormale, sauts de prix, rafales d'actualités, VIX) et risque de retournement contre la tendance (excès techniques, foule surendettée, sentiment extrême, ton des actualités). Chaque signal est vérifié sur l'historique de l'actif avant de compter. Votre bot l'interroge via /api/guard et applique la consigne : continuer, réduire la taille ou suspendre. Aucun outil ne prévoit une vraie surprise, mais on peut éviter d'y être exposé à pleine taille.",
    ),
    (
        "Où sont stockés mes avoirs ?",
        "Sur votre appareil : dans le navigateur pour l'app web, dans un stockage protégé et exclu des sauvegardes pour les apps iPhone et Android. Jamais sur nos serveurs. Un export permet de les transférer d'un navigateur à l'autre.",
    ),
];

#[component]
pub fn Faq() -> Html {
    html! {
        <Section id="faq" eyebrow="FAQ" title={html! { "Questions fréquentes" }}>
            <div class="faq">
                { for QA.iter().map(|(q, a)| html! {
                    <details key={*q} class="card">
                        <summary>{ *q }</summary>
                        <p>{ *a }</p>
                    </details>
                }) }
            </div>
        </Section>
    }
}

#[component]
pub fn Download() -> Html {
    let r = use_reveal();
    html! {
        <section class="section download reveal" id="download" ref={r}>
            <div class="card download-card">
                <img src="/logo.svg" alt="" width="96" height="96" />
                <h2>{ "Prêt à voir le " }<span class="gradient">{ "signal" }</span>{ " ?" }</h2>
                <p class="muted">
                    { "Sur iPhone et Apple Watch, sur Android, ou tout de suite dans votre navigateur : les mêmes conseils partout, et une notification quand vous pouvez acheter." }
                </p>
                <div class="download-actions">
                    <a class="btn" href="/#apps">{ "Voir les applications" }</a>
                    <a class="btn btn-ghost" href="/app">{ "Ouvrir l'app web" }</a>
                </div>
            </div>
        </section>
    }
}

/// Sticky bar at the bottom of the screen on mobile, shown once the hero has been scrolled past.
#[component]
pub fn MobileCta() -> Html {
    let visible = use_state(|| false);
    {
        let visible = visible.clone();
        use_effect_with((), move |_| {
            let on_scroll = move || {
                let w = gloo::utils::window();
                let ih = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let near_end = gloo::utils::document().get_element_by_id("download").is_some_and(|d| d.get_bounding_client_rect().top() < ih);
                visible.set(w.scroll_y().unwrap_or(0.0) > ih * 0.8 && !near_end);
            };
            on_scroll();
            let l = gloo::events::EventListener::new(&gloo::utils::window(), "scroll", move |_| on_scroll());
            move || drop(l)
        });
    }
    html! {
        <div class={if *visible { "mobile-cta show" } else { "mobile-cta" }} aria-hidden={(!*visible).to_string()}>
            <div>
                <b>{ "Altim" }</b>
                <small>{ "Signaux crypto & actions" }</small>
            </div>
            <a href="/app" class="btn btn-small" tabindex={if *visible { "0" } else { "-1" }}>{ "Ouvrir l'app" }</a>
        </div>
    }
}
