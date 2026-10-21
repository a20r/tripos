//! Saved-game shapes and localStorage keys.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const THEME_KEY: &str = "tripos.theme";
pub const MODE_KEY: &str = "tripos.mode";
pub const SOLO_KEY: &str = "tripos.solo.state";
pub const SOLO_BEST_KEY: &str = "tripos.solo.best";
pub const DAILY_KEY: &str = "tripos.daily.state";
pub const DAILY_HIST_KEY: &str = "tripos.daily.history";
pub const PARTY_PLAYERS_KEY: &str = "tripos.party.players";

/// A solo or daily game in progress: deck order, table, and the clock.
#[derive(Clone, Serialize, Deserialize)]
pub struct TimedSave {
    pub v: u32,
    #[serde(default)]
    pub date: Option<String>,
    pub deck: Vec<u8>,
    pub table: Vec<u8>,
    #[serde(default)]
    pub found: u32,
    /// Play time in milliseconds, penalties included.
    #[serde(default)]
    pub elapsed: u32,
    #[serde(default)]
    pub penalty: u32,
    #[serde(default)]
    pub hints: u32,
    #[serde(default)]
    pub over: bool,
}

/// Best time in milliseconds per daily date key.
pub type History = HashMap<String, u32>;

pub fn parse<T: for<'a> Deserialize<'a>>(raw: Option<String>) -> Option<T> {
    raw.and_then(|s| serde_json::from_str::<T>(&s).ok())
}

pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
