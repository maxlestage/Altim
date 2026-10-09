//! The home page's scroll « film » (altim_core::web::film): a full-screen sticky scene, one caption per station.
//! This component draws the captions (real text) and maps the scroll to the scene: it writes the position in the film
//! on the stage (`data-s`), which frontend/motion.js, loaded after the first paint, reads to morph its particles.
//! Without motion.js (not loaded yet, no WebGL, data saver) the captions still follow the scroll over a still logo.
//! Reduced motion: no sticky scene, the captions as a plain list, each with a still drawing (motion.js).
use altim_core::js::to_fixed;
use altim_core::web::film::{STATIONS, caption_alpha, nearest, progress, station_at};
use yew::prelude::*;

/// `prefers-reduced-motion: reduce`.
pub fn reduced_motion() -> bool {
    gloo::utils::window().match_media("(prefers-reduced-motion: reduce)").ok().flatten().is_some_and(|m| m.matches())
}

#[component]
pub fn Film() -> Html {
    let still = use_memo((), |_| reduced_motion());
    let section = use_node_ref();
    let n = STATIONS.len();
    {
        let section = section.clone();
        use_effect_with(*still, move |still| {
            let mut keep = None;
            if let (false, Some(el)) = (*still, section.cast::<web_sys::HtmlElement>()) {
                let stage = el.query_selector(".film-stage").ok().flatten();
                let caps = el.query_selector_all(".film-cap").ok();
                let count = el.query_selector(".film-count b").ok().flatten();
                let last = std::cell::Cell::new(usize::MAX);
                let on_scroll = move || {
                    let w = gloo::utils::window();
                    let vh = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let r = el.get_bounding_client_rect();
                    let s = station_at(progress(r.top(), r.height(), vh), n);
                    if let Some(stage) = &stage {
                        let _ = stage.set_attribute("data-s", &to_fixed(s, 4));
                    }
                    if let Some(caps) = &caps {
                        for k in 0..caps.length() {
                            let Some(c) = caps.item(k).and_then(|c| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(c).ok()) else { continue };
                            let a = caption_alpha(s, k as usize);
                            let dy = ((k as f64 - s) * 28.0).clamp(-28.0, 28.0);
                            let _ = c.set_attribute("style", &format!("opacity:{};transform:translate3d(0,{}px,0)", to_fixed(a, 3), to_fixed(dy, 1)));
                            let _ = c.set_attribute("aria-current", if nearest(s, n) == k as usize { "step" } else { "false" });
                        }
                    }
                    let k = nearest(s, n);
                    if k != last.get() {
                        last.set(k);
                        if let Some(b) = &count {
                            b.set_text_content(Some(&format!("{:02}", k + 1)));
                        }
                    }
                };
                on_scroll();
                let on_scroll = std::rc::Rc::new(on_scroll);
                let s2 = on_scroll.clone();
                let w = gloo::utils::window();
                keep = Some((
                    gloo::events::EventListener::new(&w, "scroll", move |_| on_scroll()),
                    gloo::events::EventListener::new(&w, "resize", move |_| s2()),
                ));
            }
            move || drop(keep)
        });
    }
    let shapes = STATIONS.iter().map(|s| s.shape).collect::<Vec<_>>().join(",");
    html! {
        <section
            class={classes!("film", still.then_some("still"))}
            id="film"
            ref={section}
            aria-label="Altim en sept plans"
            data-film={shapes}
            style={format!("--n:{n}")}
        >
            <div class="film-stage" data-s="0">
                <canvas class="film-canvas" aria-hidden="true" />
                <svg class="film-poster" viewBox="0 0 1024 1024" aria-hidden="true">
                    <path d="M232 780 L512 214 L792 780" />
                    <path class="acc" d="M330 600 L430 520 L520 575 L700 420" />
                    <circle class="acc" cx="700" cy="420" r="26" />
                </svg>
                <ol class="film-caps">
                    { for STATIONS.iter().enumerate().map(|(k, s)| html! {
                        <li class="film-cap" key={k} data-shape={s.shape} style={if k == 0 || *still { "" } else { "opacity:0" }}>
                            <canvas class="film-still" aria-hidden="true" />
                            <p>
                                { for s.caption.iter().map(|(t, accent)| if *accent {
                                    html! { <em>{ *t }</em> }
                                } else {
                                    html! { { *t } }
                                }) }
                            </p>
                        </li>
                    }) }
                </ol>
                <p class="film-count" aria-hidden="true"><b>{ "01" }</b>{ format!(" / {n:02}") }</p>
            </div>
        </section>
    }
}
