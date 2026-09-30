//! Lexique (phase 2, batch D: port of web/src/webapp/Glossary.tsx): the words of Altim explained simply.
mod terms;

pub use terms::GLOSSARY;
use yew::prelude::*;

/// Glossary screen (/app/lexique).
#[component]
pub fn Glossary() -> Html {
    html! {
        <section class="app-screen">
            <div class="screen-top">
                <div>
                    <h1>{ "Lexique" }</h1>
                    <p class="muted small">{ "Les mots d'Altim, expliqués simplement." }</p>
                </div>
            </div>
            <div class="card">
                <dl class="glossary">
                    { for GLOSSARY.iter().map(|(term, text)| html! {
                        <div key={*term}>
                            <dt>{ *term }</dt>
                            <dd class="muted">{ *text }</dd>
                        </div>
                    }) }
                </dl>
            </div>
        </section>
    }
}
