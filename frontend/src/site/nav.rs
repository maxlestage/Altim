//! Top navigation of the site (Nav.tsx): anchors of the home page, burger menu on mobile.
use yew::prelude::*;

const LINKS: [(&str, &str); 8] = [
    ("/#live", "Signal live"),
    ("/#features", "Fonctionnalités"),
    ("/#apps", "Apps"),
    ("/#sources", "Sources"),
    ("/#how", "Moteur"),
    ("/#bot", "Bot"),
    ("/#security", "Confidentialité"),
    ("/#faq", "FAQ"),
];

#[component]
pub fn Nav() -> Html {
    let open = use_state(|| false);
    let close = {
        let open = open.clone();
        Callback::from(move |_| open.set(false))
    };
    let toggle = {
        let open = open.clone();
        Callback::from(move |_| open.set(!*open))
    };
    html! {
        <header class="nav" id="top">
            <a href="/" class="brand" aria-label="Altim, accueil">
                <img src="/logo.svg" alt="" width="34" height="34" />
                <span>{ "ALTIM" }</span>
            </a>
            <nav class={if *open { "links open" } else { "links" }} onclick={close}>
                { for LINKS.iter().map(|(href, label)| html! { <a key={*href} href={*href}>{ *label }</a> }) }
                <a href="/app" class="btn btn-small">{ "Ouvrir l'app" }</a>
            </nav>
            <button class="burger" aria-label="Menu" aria-expanded={open.to_string()} onclick={toggle}>
                <span />
                <span />
            </button>
        </header>
    }
}
