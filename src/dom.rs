//! Thin helpers over web-sys so the game code reads like the DOM it drives.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlButtonElement, HtmlElement, Storage, Window};

pub fn window() -> Window {
    web_sys::window().expect("no window")
}

pub fn document() -> Document {
    window().document().expect("no document")
}

/// An element by id, as an HtmlElement. Panics on a missing id: the page is ours.
pub fn el(id: &str) -> HtmlElement {
    document()
        .get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing element #{id}"))
        .dyn_into::<HtmlElement>()
        .unwrap_or_else(|_| panic!("#{id} is not an HtmlElement"))
}

pub fn btn(id: &str) -> HtmlButtonElement {
    el(id)
        .dyn_into::<HtmlButtonElement>()
        .unwrap_or_else(|_| panic!("#{id} is not a button"))
}

pub fn set_class(e: &Element, name: &str, on: bool) {
    let _ = e.class_list().toggle_with_force(name, on);
}

pub fn set_var(e: &HtmlElement, name: &str, value: &str) {
    let _ = e.style().set_property(name, value);
}

pub fn set_hidden(e: &HtmlElement, hidden: bool) {
    e.set_hidden(hidden);
}

/// Every child element of a container, in order.
pub fn children(e: &HtmlElement) -> Vec<HtmlElement> {
    let list = e.children();
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|c| c.dyn_into::<HtmlElement>().ok())
        .collect()
}

pub fn set_timeout(cb: &Closure<dyn FnMut()>, ms: i32) -> i32 {
    window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), ms)
        .expect("setTimeout")
}

pub fn clear_timeout(id: i32) {
    window().clear_timeout_with_handle(id);
}

pub fn set_interval(cb: &Closure<dyn FnMut()>, ms: i32) -> i32 {
    window()
        .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), ms)
        .expect("setInterval")
}

fn storage() -> Option<Storage> {
    window().local_storage().ok().flatten()
}

/// localStorage reads and writes never fail the game: a blocked store just means no persistence.
pub fn local_get(key: &str) -> Option<String> {
    storage().and_then(|s| s.get_item(key).ok().flatten())
}

pub fn local_set(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
}

/// Today's local calendar date as YYYY-MM-DD.
pub fn today_key() -> String {
    let d = js_sys::Date::new_0();
    tripos_engine::daily::date_key(d.get_full_year() as i32, d.get_month() + 1, d.get_date())
}

/// Milliseconds since the epoch; monotonic enough for a game clock.
pub fn now() -> f64 {
    js_sys::Date::now()
}

/// A 64-bit seed from the page's random source.
pub fn random_seed() -> u64 {
    let hi = (js_sys::Math::random() * 4294967296.0) as u64;
    let lo = (js_sys::Math::random() * 4294967296.0) as u64;
    (hi << 32) | lo
}

pub fn landscape() -> bool {
    let w = window()
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let h = window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    w > h
}

pub fn page_hidden() -> bool {
    document().hidden()
}

/// The page URL without query or hash, for the share text.
pub fn page_url() -> String {
    let loc = window().location();
    format!(
        "{}{}",
        loc.origin().unwrap_or_default(),
        loc.pathname().unwrap_or_default()
    )
}
