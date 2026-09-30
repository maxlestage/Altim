//! Legal pages (Legal.tsx): mentions légales, confidentialité, risques.
use yew::prelude::*;

use crate::route::Route;

const AUTHOR: &str = "Maxime Nathan Lestage";

fn title(r: &Route) -> &'static str {
    match r {
        Route::MentionsLegales => "Mentions légales",
        Route::Confidentialite => "Politique de confidentialité",
        _ => "Avertissement sur les risques",
    }
}

fn body(r: &Route) -> Html {
    match r {
        Route::MentionsLegales => html! {
            <>
                <h2>{ "Éditeur" }</h2>
                <p>
                    { "Le site et l'application Altim sont édités par " }<b>{ AUTHOR }</b>{ ", à titre personnel." }
                    <br />
                    { format!("Directeur de la publication : {AUTHOR}.") }
                </p>
                <h2>{ "Hébergement" }</h2>
                <p>
                    { "Salesforce, Inc. (Heroku) — 415 Mission Street, Suite 300, San Francisco, CA 94105, États-Unis —" }{ " " }
                    <a href="https://www.heroku.com" target="_blank" rel="noreferrer">{ "heroku.com" }</a>
                </p>
                <h2>{ "Propriété intellectuelle" }</h2>
                <p>
                    { format!("La marque Altim, le logo, les textes, le design et le code source sont la propriété de {AUTHOR}. Toute reproduction sans autorisation est interdite. Les noms des plateformes citées (Binance, Coinbase, Nasdaq…) appartiennent à leurs propriétaires respectifs et sont mentionnés à titre de sources de données uniquement.") }
                </p>
                <h2>{ "Responsabilité" }</h2>
                <p>
                    { "Les informations et signaux fournis le sont à titre indicatif, sans garantie d'exactitude ni d'exhaustivité. Voir l'" }
                    <a href="/risques">{ "avertissement sur les risques" }</a>{ "." }
                </p>
            </>
        },
        Route::Confidentialite => html! {
            <>
                <h2>{ "En bref" }</h2>
                <p>
                    { "Ce site ne dépose " }<b>{ "aucun cookie" }</b>{ ", n'utilise " }<b>{ "aucun outil de mesure d'audience" }</b>
                    { " ni de publicité, et ne demande aucune donnée personnelle." }
                </p>
                <h2>{ "Données techniques" }</h2>
                <p>
                    { "Comme tout site web, l'hébergeur (Heroku) enregistre des journaux techniques (adresse IP, date, page demandée) conservés pour une durée limitée à des fins de sécurité. La démonstration en direct interroge le serveur Altim, qui relaie des données de marché publiques sans rien conserver vous concernant." }
                </p>
                <h2>{ "Vos avoirs" }</h2>
                <p>
                    { "Les avoirs que vous renseignez sont enregistrés uniquement sur votre appareil, dans le navigateur (localStorage). Altim ne passe aucun ordre et ne demande aucun accès à vos comptes. Seuls les symboles des actifs sont envoyés au serveur pour obtenir leurs cours, sans les quantités." }
                </p>
                <h2>{ "Vos droits" }</h2>
                <p>
                    { format!("Conformément au RGPD, vous pouvez exercer vos droits d'accès, de rectification et d'effacement en contactant l'éditeur, {AUTHOR}. Vous pouvez aussi saisir la CNIL (cnil.fr).") }
                </p>
            </>
        },
        _ => html! {
            <>
                <p>
                    { "Investir dans des crypto-actifs, des actions ou tout autre instrument financier comporte un " }
                    <b>{ "risque élevé de perte, pouvant aller jusqu'à la totalité du capital investi" }</b>
                    { ". Les crypto-actifs sont particulièrement volatils." }
                </p>
                <h2>{ "Nature d'Altim" }</h2>
                <p>
                    { "Altim est un outil d'analyse technique et d'aide à la décision. Il ne fournit pas de conseil en investissement personnalisé, n'est pas un prestataire de services d'investissement ni un prestataire de services sur actifs numériques : il ne passe aucun ordre et ne détient jamais vos fonds. Vous restez seul responsable de vos décisions." }
                </p>
                <h2>{ "Limites des signaux" }</h2>
                <p>
                    { "Les signaux sont calculés à partir de l'historique des prix : ce sont des probabilités, pas des certitudes. Les résultats des backtests sont hypothétiques et les performances passées ne préjugent pas des performances futures. En marché fortement haussier, conserver un actif peut rapporter davantage que suivre les signaux." }
                </p>
                <h2>{ "Bonnes pratiques" }</h2>
                <p>
                    { "N'investissez que ce que vous pouvez vous permettre de perdre, diversifiez vos lignes, fixez-vous un stop et considérez les conseils d'Altim comme un avis parmi d'autres." }
                </p>
            </>
        },
    }
}

#[derive(Properties, PartialEq)]
pub struct LegalProps {
    pub route: Route,
}

#[component]
pub fn LegalScreen(p: &LegalProps) -> Html {
    crate::hooks::set_title(&format!("{} — Altim", title(&p.route)));
    html! {
        <>
            <super::background::Background />
            <super::nav::Nav />
            <main class="legal">
                <a href="/" class="back">{ "← Retour à l'accueil" }</a>
                <h1>{ title(&p.route) }</h1>
                <div class="legal-body">{ body(&p.route) }</div>
                <p class="muted updated">{ "Dernière mise à jour : 27 septembre 2026" }</p>
            </main>
            <super::footer::Footer />
        </>
    }
}
