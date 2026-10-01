//! Entry point of the .wasm of the holdings screen (/app/avoirs): see altim_web::part.
use wasm_bindgen::prelude::*;

/// Called by the wasm-bindgen loader of the page.
#[wasm_bindgen(start)]
pub fn start() {
    altim_web::part::run(&altim_web::app::AVOIRS);
}
