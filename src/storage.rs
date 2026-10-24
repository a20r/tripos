//! Saved-game shapes and localStorage keys.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tripos_engine::speed::{Answer, SpeedStats};
use tripos_engine::{Stats, Tally};

pub const THEME_KEY: &str = "tripos.theme";
pub const MODE_KEY: &str = "tripos.mode";
pub const SOLO_KEY: &str = "tripos.solo.state";
pub const SOLO_BEST_KEY: &str = "tripos.solo.best";
pub const DAILY_KEY: &str = "tripos.daily.state";
pub const DAILY_HIST_KEY: &str = "tripos.daily.history";
pub const PARTY_PLAYERS_KEY: &str = "tripos.party.players";
pub const STATS_KEY: &str = "tripos.stats";
pub const SPEED_KEY: &str = "tripos.speed";

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

/// One answered speed-drill triple.
#[derive(Clone, Serialize, Deserialize)]
pub struct AnswerSave {
    /// Comma-joined category keys; empty for a non-set.
    #[serde(default)]
    pub k: String,
    pub ms: u32,
    pub ok: bool,
    pub set: bool,
    #[serde(default)]
    pub near: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SpeedSave {
    pub v: u32,
    #[serde(default)]
    pub rounds: u32,
    #[serde(default)]
    pub best_ms: u32,
    #[serde(default)]
    pub log: Vec<AnswerSave>,
}

impl SpeedSave {
    pub fn from_stats(s: &SpeedStats) -> SpeedSave {
        SpeedSave {
            v: 1,
            rounds: s.rounds,
            best_ms: s.best_ms,
            log: s
                .log
                .iter()
                .map(|a| AnswerSave {
                    k: a.keys.join(","),
                    ms: a.ms,
                    ok: a.correct,
                    set: a.is_set,
                    near: a.near,
                })
                .collect(),
        }
    }

    pub fn into_stats(self) -> SpeedStats {
        SpeedStats {
            rounds: self.rounds,
            best_ms: self.best_ms,
            log: self
                .log
                .into_iter()
                .map(|a| Answer {
                    keys: if a.k.is_empty() {
                        Vec::new()
                    } else {
                        a.k.split(',').map(str::to_string).collect()
                    },
                    ms: a.ms,
                    correct: a.ok,
                    is_set: a.set,
                    near: a.near,
                })
                .collect(),
        }
    }
}

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
