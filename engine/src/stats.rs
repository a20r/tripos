//! Play profiling: which kinds of set a player takes and which they pass over.
//!
//! Every claim is an observation. The set taken counts as found; every other
//! set on the table at that moment counts as passed over. Over many claims
//! each kind of set gets a take rate (found over seen), and comparing a kind's
//! rate with the overall rate shows a lean toward it or a blind spot for it.
//! Dealing while a set is out also marks every set on the table as passed over.

use std::collections::BTreeMap;

use crate::card::Card;

/// Attribute order used throughout: number, shape, colour, shading.
pub const ATTR_KEYS: [char; 4] = ['n', 's', 'c', 'h'];

/// What the three cards of a set share: `Some(value)` where the attribute is
/// the same on all three, `None` where it is all different.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Signature {
    pub same: [Option<u8>; 4],
}

impl Signature {
    pub fn of(a: Card, b: Card, c: Card) -> Signature {
        let (x, y, z) = (a.attrs(), b.attrs(), c.attrs());
        let mut same = [None; 4];
        for i in 0..4 {
            if x[i] == y[i] && y[i] == z[i] {
                same[i] = Some(x[i]);
            }
        }
        Signature { same }
    }

    /// How many attributes are all different: 1 (easiest to spot) to 4.
    pub fn differing(&self) -> u8 {
        self.same.iter().filter(|s| s.is_none()).count() as u8
    }

    /// The category keys this set belongs to: one difficulty key `d1`..`d4`
    /// and one per attribute, `n0`..`n2` for all-same-with-that-value or `nx`
    /// for all-different (likewise `s`, `c`, `h`).
    pub fn categories(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(5);
        out.push(format!("d{}", self.differing()));
        for (i, k) in ATTR_KEYS.iter().enumerate() {
            match self.same[i] {
                Some(v) => out.push(format!("{k}{v}")),
                None => out.push(format!("{k}x")),
            }
        }
        out
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Tally {
    pub found: u32,
    pub seen: u32,
    /// Total search time for the found sets, in milliseconds.
    pub ms: u64,
}

impl Tally {
    pub fn rate(&self) -> Option<f64> {
        (self.seen > 0).then(|| self.found as f64 / self.seen as f64)
    }
    pub fn avg_ms(&self) -> Option<u32> {
        (self.found > 0).then(|| (self.ms / self.found as u64) as u32)
    }
}

/// One category's standing against the player's overall take rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Bias {
    pub key: String,
    pub tally: Tally,
    pub rate: f64,
    /// Rate minus the overall rate, in the range -1..1.
    pub bias: f64,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Stats {
    pub cats: BTreeMap<String, Tally>,
    pub found: u32,
    pub seen: u32,
    pub hints: u32,
    pub wrong: u32,
    pub bad_deals: u32,
}

impl Stats {
    fn tally(&mut self, key: String) -> &mut Tally {
        self.cats.entry(key).or_default()
    }

    /// A claim: `sets` is every set on the table at that moment, `claimed`
    /// the one taken, `ms` how long it sat there before being found.
    pub fn record_claim(&mut self, sets: &[[Card; 3]], claimed: [Card; 3], ms: u32) {
        let mut hit = false;
        for set in sets {
            let same = sorted(*set) == sorted(claimed);
            hit |= same;
            self.observe(*set, same, ms);
        }
        if !hit {
            self.observe(claimed, true, ms);
        }
    }

    /// Every set on the table was passed over, with nothing taken.
    pub fn record_passed(&mut self, sets: &[[Card; 3]]) {
        for set in sets {
            self.observe(*set, false, 0);
        }
    }

    fn observe(&mut self, set: [Card; 3], found: bool, ms: u32) {
        self.seen += 1;
        if found {
            self.found += 1;
        }
        let sig = Signature::of(set[0], set[1], set[2]);
        for key in sig.categories() {
            let t = self.tally(key);
            t.seen += 1;
            if found {
                t.found += 1;
                t.ms += ms as u64;
            }
        }
    }

    pub fn overall_rate(&self) -> Option<f64> {
        (self.seen > 0).then(|| self.found as f64 / self.seen as f64)
    }

    pub fn avg_ms(&self) -> Option<u32> {
        let ms: u64 = self
            .cats
            .iter()
            .filter(|(k, _)| k.starts_with('d'))
            .map(|(_, t)| t.ms)
            .sum();
        (self.found > 0).then(|| (ms / self.found as u64) as u32)
    }

    /// Every category seen at least `min_seen` times, strongest lean first.
    pub fn biases(&self, min_seen: u32) -> Vec<Bias> {
        let Some(overall) = self.overall_rate() else {
            return Vec::new();
        };
        let mut out: Vec<Bias> = self
            .cats
            .iter()
            .filter(|(_, t)| t.seen >= min_seen)
            .filter_map(|(k, t)| {
                t.rate().map(|rate| Bias {
                    key: k.clone(),
                    tally: *t,
                    rate,
                    bias: rate - overall,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            b.bias
                .partial_cmp(&a.bias)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }
}

fn sorted(mut s: [Card; 3]) -> [Card; 3] {
    s.sort_unstable();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: u8, s: u8, col: u8, h: u8) -> Card {
        Card::new(n, s, col, h)
    }

    #[test]
    fn signature_and_categories() {
        // Same number and shading, different shape and colour.
        let sig = Signature::of(c(1, 0, 0, 2), c(1, 1, 1, 2), c(1, 2, 2, 2));
        assert_eq!(sig.same, [Some(1), None, None, Some(2)]);
        assert_eq!(sig.differing(), 2);
        assert_eq!(sig.categories(), vec!["d2", "n1", "sx", "cx", "h2"]);
        // Everything different.
        let sig = Signature::of(c(0, 0, 0, 0), c(1, 1, 1, 1), c(2, 2, 2, 2));
        assert_eq!(sig.differing(), 4);
        assert_eq!(sig.categories()[0], "d4");
    }

    #[test]
    fn claims_count_found_and_passed_over() {
        let mut st = Stats::default();
        let a = [c(0, 0, 0, 0), c(0, 0, 0, 1), c(0, 0, 0, 2)]; // d1, all one pink diamond
        let b = [c(0, 0, 0, 0), c(1, 1, 1, 1), c(2, 2, 2, 2)]; // d4
        st.record_claim(&[a, b], [a[2], a[0], a[1]], 4000);
        assert_eq!((st.found, st.seen), (1, 2));
        assert_eq!(
            st.cats["d1"],
            Tally {
                found: 1,
                seen: 1,
                ms: 4000
            }
        );
        assert_eq!(
            st.cats["d4"],
            Tally {
                found: 0,
                seen: 1,
                ms: 0
            }
        );
        assert_eq!(st.cats["c0"].found, 1);
        assert_eq!(st.cats["cx"].seen, 1);
        st.record_passed(&[b]);
        assert_eq!(st.cats["d4"].seen, 2);
        assert_eq!(st.seen, 3);
        assert_eq!(st.avg_ms(), Some(4000));
        assert_eq!(st.overall_rate(), Some(1.0 / 3.0));
    }

    #[test]
    fn claim_outside_the_list_still_counts() {
        let mut st = Stats::default();
        let a = [c(0, 0, 0, 0), c(0, 0, 0, 1), c(0, 0, 0, 2)];
        st.record_claim(&[], a, 100);
        assert_eq!((st.found, st.seen), (1, 1));
    }

    #[test]
    fn biases_rank_leans_first_and_respect_min_seen() {
        let mut st = Stats::default();
        let easy = [c(0, 0, 0, 0), c(0, 0, 0, 1), c(0, 0, 0, 2)];
        let hard = [c(0, 0, 0, 0), c(1, 1, 1, 1), c(2, 2, 2, 2)];
        for _ in 0..10 {
            st.record_claim(&[easy, hard], easy, 1000);
        }
        let b = st.biases(10);
        let d1 = b.iter().find(|x| x.key == "d1").unwrap();
        let d4 = b.iter().find(|x| x.key == "d4").unwrap();
        assert!(d1.bias > 0.0 && d4.bias < 0.0);
        assert_eq!(d1.rate, 1.0);
        assert_eq!(d4.rate, 0.0);
        assert_eq!(st.biases(11).len(), 1);
        assert!(st.biases(21).is_empty());
        assert!(b.windows(2).all(|w| w[0].bias >= w[1].bias));
    }
}
