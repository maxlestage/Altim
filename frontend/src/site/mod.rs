//! The presentation site (App.tsx `Home` + web/src/components), same markup and classes as the React one.
pub mod background;
pub mod bot_section;
pub mod footer;
pub mod hero;
pub mod legal;
pub mod live_signal;
pub mod nav;
pub mod sections;
pub mod ticker;

use yew::prelude::*;

use crate::hooks::use_reveal;

/// Home page: amounts in the app's display currency (euros by default), at the verified rate.
#[component]
pub fn Home() -> Html {
    let ticks = crate::hooks::use_ticks();
    let _app = crate::state::app::use_app_state();
    let _fx = crate::state::fx::use_fx();
    use_effect_with((), |_| crate::state::fx::start_fx());
    html! {
        <>
            <background::Background />
            <nav::Nav />
            <main>
                <hero::Hero ticks={ticks.clone()} />
                <ticker::Ticker ticks={ticks} />
                <live_signal::LiveSignal />
                <sections::Features />
                <sections::Apps />
                <sections::Sources />
                <sections::HowItWorks />
                <bot_section::BotSection />
                <sections::Transparency />
                <sections::Security />
                <sections::Faq />
                <sections::Download />
            </main>
            <footer::Footer />
            <sections::MobileCta />
        </>
    }
}

#[derive(Properties, PartialEq)]
pub struct SectionProps {
    pub id: AttrValue,
    pub eyebrow: AttrValue,
    pub title: Html,
    #[prop_or_default]
    pub intro: Option<Html>,
    #[prop_or_default]
    pub children: Html,
}

/// A titled section of the home page, revealed on scroll.
#[component]
pub fn Section(p: &SectionProps) -> Html {
    let r = use_reveal();
    html! {
        <section class="section reveal" id={p.id.clone()} ref={r}>
            <div class="section-head">
                <p class="eyebrow">{ p.eyebrow.clone() }</p>
                <h2>{ p.title.clone() }</h2>
                if let Some(intro) = &p.intro {
                    <p class="muted">{ intro.clone() }</p>
                }
            </div>
            { p.children.clone() }
        </section>
    }
}

/// The page of a site address.
pub fn page(r: crate::route::Route) -> Html {
    use crate::route::Route;
    match r {
        Route::MentionsLegales | Route::Confidentialite | Route::Risques => html! { <legal::LegalScreen route={r} /> },
        // Anything else: the home page (App.tsx).
        _ => html! { <Home /> },
    }
}

/// The presentation site's .wasm (frontend/bundles/site.rs).
pub static SITE: crate::part::Part = crate::part::Part { bundles: &["site"], app: crate::part::none, site: page };
