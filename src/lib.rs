//! Tripos in the browser: the game, rendering and input live in `app`, the
//! rules in the `tripos-engine` crate. A few engine functions are exported
//! for tests and tinkering (`window.Tripos` in index.html).

mod app;
mod cards;
mod dom;
mod net;
mod storage;

use tripos_engine::Card;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    app::boot();
}

/// Whether three card codes (0..81) form a set.
#[wasm_bindgen]
pub fn is_set(a: u8, b: u8, c: u8) -> bool {
    tripos_engine::is_set(Card(a), Card(b), Card(c))
}

/// Every set among the given card codes, flattened as index triples.
#[wasm_bindgen]
pub fn find_sets(cards: &[u8]) -> Vec<u32> {
    let cards: Vec<Card> = cards.iter().map(|&c| Card(c)).collect();
    tripos_engine::find_sets(&cards)
        .into_iter()
        .flat_map(|t| t.into_iter().map(|i| i as u32))
        .collect()
}

/// The opening table of the daily game for a YYYY-MM-DD key.
#[wasm_bindgen]
pub fn daily_table(key: &str) -> Vec<u8> {
    tripos_engine::daily::daily(key)
        .table()
        .iter()
        .map(|c| c.0)
        .collect()
}

#[wasm_bindgen]
pub fn date_key() -> String {
    dom::today_key()
}
