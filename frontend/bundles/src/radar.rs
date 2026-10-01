//! Entry point of the .wasm of the Radar group (/app, /app/alertes, any other /app address): see altim_web::part.
use wasm_bindgen::prelude::*;

/// Called by the wasm-bindgen loader of the page.
#[wasm_bindgen(start)]
pub fn start() {
    altim_web::part::run(&altim_web::app::RADAR);
}
