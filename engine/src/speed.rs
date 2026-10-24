//! The speed drill: one triple at a time, swipe right for a set and left for
//! not. Half the triples are sets. Most of the rest are near misses, where
//! three of the four attributes follow the rule and one breaks it, since
//! those are the ones that fool people. Every answer is logged with its
//! reaction time so the profile can report speed and accuracy by kind.

use crate::card::{is_set, third, Card, DECK_SIZE};
use crate::rng::Rng;
use crate::stats::Signature;

/// Triples in one round.
pub const ROUND_LEN: usize = 20;
/// Wrong swipes add this much to the round clock.
pub const WRONG_MS: u32 = 2000;
/// Answers kept for the profile; older ones fall off.
pub const LOG_CAP: usize = 600;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Trial {
    pub cards: [Card; 3],
    pub is_set: bool,
    /// A non-set where exactly one attribute breaks the rule.
    pub near: bool,
}

fn pick(rng: &mut Rng) -> Card {
    Card(rng.below(DECK_SIZE) as u8)
}

/// One triple: a set half the time, otherwise a near miss three times in five.
pub fn deal_trial(rng: &mut Rng) -> Trial {
    let a = pick(rng);
    let mut b = pick(rng);
    while b == a {
        b = pick(rng);
    }
    let completes = third(a, b);
    let roll = rng.below(10);
    let (c, set, near) = if roll < 5 {
        (completes, true, false)
    } else if roll < 8 {
        // Nudge one attribute of the completing card to another value. When the
        // first two cards differ in just that attribute the nudge lands on one
        // of them, so try again on another attribute.
        let c = loop {
            let mut attrs = completes.attrs();
            let i = rng.below(4);
            attrs[i] = (attrs[i] + 1 + rng.below(2) as u8) % 3;
            let c = Card::new(attrs[0], attrs[1], attrs[2], attrs[3]);
            if c != a && c != b {
                break c;
            }
        };
        (c, false, true)
    } else {
        let mut c = pick(rng);
        while c == a || c == b || c == completes {
            c = pick(rng);
        }
        // A random third card may still be a near miss by chance; label it honestly.
        let near = near_miss(a, b, c);
        (c, false, near)
    };
    let mut cards = [a, b, c];
    rng.shuffle(&mut cards);
    debug_assert_eq!(is_set(cards[0], cards[1], cards[2]), set);
    Trial {
        cards,
        is_set: set,
        near,
    }
}

/// Three of the four attributes follow the rule.
fn near_miss(a: Card, b: Card, c: Card) -> bool {
    let (x, y, z) = (a.attrs(), b.attrs(), c.attrs());
    let ok = (0..4)
        .filter(|&i| {
            let same = x[i] == y[i] && y[i] == z[i];
            let diff = x[i] != y[i] && y[i] != z[i] && x[i] != z[i];
            same || diff
        })
        .count();
    ok == 3
}

/// A full round of triples.
pub fn deal_round(rng: &mut Rng) -> Vec<Trial> {
    (0..ROUND_LEN).map(|_| deal_trial(rng)).collect()
}

/// One answered triple, as the profile keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// Category keys of the triple when it is a set (see `Signature::categories`).
    pub keys: Vec<String>,
    pub ms: u32,
    pub correct: bool,
    pub is_set: bool,
    pub near: bool,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct SpeedStats {
    pub log: Vec<Answer>,
    pub rounds: u32,
    /// Best round time in milliseconds, penalties included; 0 when none.
    pub best_ms: u32,
}

/// A speed and accuracy reading for one slice of answers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub n: u32,
    pub correct: u32,
    /// Median reaction time of the correct answers, when there are any.
    pub p50: Option<u32>,
}

impl Reading {
    pub fn accuracy(&self) -> Option<f64> {
        (self.n > 0).then(|| self.correct as f64 / self.n as f64)
    }
}

pub fn percentile(sorted: &[u32], q: f64) -> Option<u32> {
    if sorted.is_empty() {
        return None;
    }
    let pos = ((sorted.len() - 1) as f64 * q).round() as usize;
    Some(sorted[pos.min(sorted.len() - 1)])
}

fn reading<'a>(answers: impl Iterator<Item = &'a Answer>) -> Reading {
    let mut n = 0;
    let mut correct = 0;
    let mut rts = Vec::new();
    for a in answers {
        n += 1;
        if a.correct {
            correct += 1;
            rts.push(a.ms);
        }
    }
    rts.sort_unstable();
    Reading {
        n,
        correct,
        p50: percentile(&rts, 0.5),
    }
}

impl SpeedStats {
    pub fn record(&mut self, trial: &Trial, ms: u32, correct: bool) {
        let keys = if trial.is_set {
            Signature::of(trial.cards[0], trial.cards[1], trial.cards[2]).categories()
        } else {
            Vec::new()
        };
        self.log.push(Answer {
            keys,
            ms,
            correct,
            is_set: trial.is_set,
            near: trial.near,
        });
        if self.log.len() > LOG_CAP {
            let extra = self.log.len() - LOG_CAP;
            self.log.drain(..extra);
        }
    }

    pub fn finish_round(&mut self, total_ms: u32) {
        self.rounds += 1;
        if self.best_ms == 0 || total_ms < self.best_ms {
            self.best_ms = total_ms;
        }
    }

    pub fn overall(&self) -> Reading {
        reading(self.log.iter())
    }

    /// Correct answers per ten seconds of answering, over every logged swipe.
    /// Wrong answers count toward the time but not the tally.
    pub fn pace_per_10s(&self) -> Option<f64> {
        let ms: u64 = self.log.iter().map(|a| a.ms as u64).sum();
        (ms > 0).then(|| {
            let correct = self.log.iter().filter(|a| a.correct).count() as f64;
            correct * 10_000.0 / ms as f64
        })
    }

    /// Reaction time percentile over all correct answers.
    pub fn rt_percentile(&self, q: f64) -> Option<u32> {
        let mut rts: Vec<u32> = self
            .log
            .iter()
            .filter(|a| a.correct)
            .map(|a| a.ms)
            .collect();
        rts.sort_unstable();
        percentile(&rts, q)
    }

    /// Sets of one kind: how often they were recognised and how fast.
    pub fn for_key(&self, key: &str) -> Reading {
        reading(
            self.log
                .iter()
                .filter(|a| a.is_set && a.keys.iter().any(|k| k == key)),
        )
    }

    pub fn sets(&self) -> Reading {
        reading(self.log.iter().filter(|a| a.is_set))
    }

    pub fn near_misses(&self) -> Reading {
        reading(self.log.iter().filter(|a| !a.is_set && a.near))
    }

    pub fn far_misses(&self) -> Reading {
        reading(self.log.iter().filter(|a| !a.is_set && !a.near))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trials_are_labelled_correctly() {
        let mut rng = Rng::new(5);
        let mut sets = 0;
        let mut nears = 0;
        for _ in 0..2000 {
            let t = deal_trial(&mut rng);
            assert_eq!(is_set(t.cards[0], t.cards[1], t.cards[2]), t.is_set);
            assert!(
                t.cards[0] != t.cards[1] && t.cards[1] != t.cards[2] && t.cards[0] != t.cards[2]
            );
            if t.near {
                assert!(!t.is_set);
                assert!(near_miss(t.cards[0], t.cards[1], t.cards[2]));
            }
            sets += t.is_set as u32;
            nears += t.near as u32;
        }
        assert!((850..1150).contains(&sets), "sets {sets}");
        assert!(nears > 450, "near misses {nears}");
    }

    #[test]
    fn round_has_twenty() {
        assert_eq!(deal_round(&mut Rng::new(1)).len(), ROUND_LEN);
    }

    #[test]
    fn percentiles() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[7], 0.5), Some(7));
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 0.5), Some(3));
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 0.9), Some(5));
        assert_eq!(percentile(&[1, 2, 3, 4], 0.0), Some(1));
    }

    #[test]
    fn readings_and_cap() {
        let mut st = SpeedStats::default();
        let set = Trial {
            cards: [
                Card::new(0, 0, 0, 0),
                Card::new(0, 0, 0, 1),
                Card::new(0, 0, 0, 2),
            ],
            is_set: true,
            near: false,
        };
        let near = Trial {
            cards: [
                Card::new(0, 0, 0, 0),
                Card::new(0, 0, 0, 1),
                Card::new(1, 0, 0, 2),
            ],
            is_set: false,
            near: true,
        };
        st.record(&set, 900, true);
        st.record(&set, 1100, true);
        st.record(&set, 700, false);
        st.record(&near, 1500, false);
        assert_eq!(st.overall().n, 4);
        assert_eq!(st.overall().correct, 2);
        assert_eq!(st.rt_percentile(0.5), Some(1100));
        let d1 = st.for_key("d1");
        assert_eq!((d1.n, d1.correct, d1.p50), (3, 2, Some(1100)));
        assert_eq!(st.for_key("c0").n, 3);
        assert_eq!(st.near_misses().n, 1);
        assert_eq!(st.near_misses().accuracy(), Some(0.0));
        assert_eq!(st.far_misses().n, 0);
        // Two correct in 4.2 seconds of answering: 4.76 per ten seconds.
        let pace = st.pace_per_10s().unwrap();
        assert!((pace - 4.7619).abs() < 0.001, "{pace}");
        assert_eq!(SpeedStats::default().pace_per_10s(), None);
        st.finish_round(30_000);
        st.finish_round(40_000);
        assert_eq!((st.rounds, st.best_ms), (2, 30_000));
        for _ in 0..LOG_CAP + 10 {
            st.record(&set, 800, true);
        }
        assert_eq!(st.log.len(), LOG_CAP);
    }
}
