//! Saved-game shapes and localStorage keys.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tripos_engine::{Stats, Tally};

pub const THEME_KEY: &str = "tripos.theme";
pub const MODE_KEY: &str = "tripos.mode";
pub const SOLO_KEY: &str = "tripos.solo.state";
pub const SOLO_BEST_KEY: &str = "tripos.solo.best";
pub const DAILY_KEY: &str = "tripos.daily.state";
pub const DAILY_HIST_KEY: &str = "tripos.daily.history";
pub const PARTY_PLAYERS_KEY: &str = "tripos.party.players";
pub const STATS_KEY: &str = "tripos.stats";

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

/// Lifetime play profile: per-category found, seen and search time.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StatsSave {
    pub v: u32,
    #[serde(default)]
    pub found: u32,
    #[serde(default)]
    pub seen: u32,
    #[serde(default)]
    pub hints: u32,
    #[serde(default)]
    pub wrong: u32,
    #[serde(default)]
    pub bad_deals: u32,
    /// Category key to [found, seen, total search ms].
    #[serde(default)]
    pub cats: HashMap<String, [u64; 3]>,
}

impl StatsSave {
    pub fn from_stats(s: &Stats) -> StatsSave {
        StatsSave {
            v: 1,
            found: s.found,
            seen: s.seen,
            hints: s.hints,
            wrong: s.wrong,
            bad_deals: s.bad_deals,
            cats: s
                .cats
                .iter()
                .map(|(k, t)| (k.clone(), [t.found as u64, t.seen as u64, t.ms]))
                .collect(),
        }
    }

    pub fn into_stats(self) -> Stats {
        Stats {
            cats: self
                .cats
                .into_iter()
                .map(|(k, [f, s, ms])| {
                    (
                        k,
                        Tally {
                            found: f as u32,
                            seen: s as u32,
                            ms,
                        },
                    )
                })
                .collect(),
            found: self.found,
            seen: self.seen,
            hints: self.hints,
            wrong: self.wrong,
            bad_deals: self.bad_deals,
        }
    }
}

pub fn parse<T: for<'a> Deserialize<'a>>(raw: Option<String>) -> Option<T> {
    raw.and_then(|s| serde_json::from_str::<T>(&s).ok())
}

pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
