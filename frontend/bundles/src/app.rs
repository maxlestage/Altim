//! Entry point of the .wasm of the whole app (every group of screens), which a group's page fetches in the background
//! and hands the page over to: see altim_web::part.
use wasm_bindgen::prelude::*;

/// Called by the loader of the page, at the first move to another group's screen.
#[wasm_bindgen(start)]
pub fn start() {
    altim_web::part::run(&altim_web::app::APP);
}
