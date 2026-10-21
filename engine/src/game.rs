//! One game of SET: a shuffled deck, the cards on the table, and the rules for
//! claiming a set and dealing more. The table always shows at least twelve
//! cards and at least one set while the deck can provide it; three more cards
//! are dealt whenever a set runs out.

use crate::card::{find_sets, full_deck, has_set, is_set, Card};
use crate::rng::Rng;

pub const TABLE_MIN: usize = 12;
/// 20 cards can be set-free; 21 never are.
pub const TABLE_MAX: usize = 21;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    /// The three cards were a set; they have been replaced or removed.
    Set,
    NotSet,
    BadIndices,
}

#[derive(Clone, Debug)]
pub struct Game {
    deck: Vec<Card>,
    table: Vec<Card>,
    found: u32,
}

impl Game {
    /// A fresh game from a shuffled full deck with the opening table dealt.
    pub fn new(rng: &mut Rng) -> Game {
        let mut deck = full_deck();
        rng.shuffle(&mut deck);
        Game::from_deck(deck)
    }

    /// A game from an explicit deck order; the top of the deck is the end of the vector.
    pub fn from_deck(deck: Vec<Card>) -> Game {
        let mut g = Game {
            deck,
            table: Vec::new(),
            found: 0,
        };
        g.fill();
        g
    }

    /// Rebuilds a game from saved parts without re-dealing.
    pub fn restore(deck: Vec<Card>, table: Vec<Card>, found: u32) -> Game {
        Game { deck, table, found }
    }

    pub fn deck(&self) -> &[Card] {
        &self.deck
    }
    pub fn table(&self) -> &[Card] {
        &self.table
    }
    pub fn found(&self) -> u32 {
        self.found
    }
    pub fn remaining(&self) -> usize {
        self.deck.len()
    }

    pub fn sets(&self) -> Vec<[usize; 3]> {
        find_sets(&self.table)
    }
    pub fn has_set(&self) -> bool {
        has_set(&self.table)
    }

    /// The game is over when no set is on the table and the deck cannot add one.
    pub fn is_over(&self) -> bool {
        self.deck.is_empty() && !self.has_set()
    }

    /// Deals up to the minimum and then three at a time until a set is present.
    fn fill(&mut self) {
        while self.table.len() < TABLE_MIN && !self.deck.is_empty() {
            self.table.push(self.deck.pop().unwrap());
        }
        while !self.has_set() && self.table.len() < TABLE_MAX && self.deck.len() >= 3 {
            self.deal_three_unchecked();
        }
    }

    fn deal_three_unchecked(&mut self) -> bool {
        if self.deck.len() < 3 || self.table.len() + 3 > TABLE_MAX {
            return false;
        }
        for _ in 0..3 {
            self.table.push(self.deck.pop().unwrap());
        }
        true
    }

    /// Deals three more cards on request. Returns false when the deck is short
    /// or the table is full. Callers decide whether the request was justified
    /// (see `has_set`), since the penalty is a scoring matter.
    pub fn deal_three(&mut self) -> bool {
        self.deal_three_unchecked()
    }

    /// Claims three table positions as a set. On success the cards leave the
    /// table: replaced in place from the deck when the table would fall below
    /// twelve, removed otherwise (so an oversized table shrinks back).
    pub fn claim(&mut self, idx: [usize; 3]) -> Claim {
        let n = self.table.len();
        let [a, b, c] = idx;
        if a >= n || b >= n || c >= n || a == b || b == c || a == c {
            return Claim::BadIndices;
        }
        if !is_set(self.table[a], self.table[b], self.table[c]) {
            return Claim::NotSet;
        }
        self.found += 1;
        let mut sorted = idx;
        sorted.sort_unstable();
        if n - 3 < TABLE_MIN && self.deck.len() >= 3 {
            for i in sorted {
                self.table[i] = self.deck.pop().unwrap();
            }
        } else {
            for i in sorted.iter().rev() {
                self.table.remove(*i);
            }
        }
        self.fill();
        Claim::Set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_table_has_a_set() {
        for seed in 0..200 {
            let g = Game::new(&mut Rng::new(seed));
            assert!(g.table.len() >= TABLE_MIN);
            assert!(g.has_set() || g.deck.is_empty());
            assert_eq!(g.table.len() + g.deck.len(), 81);
        }
    }

    #[test]
    fn play_through_whole_deck() {
        for seed in 0..50 {
            let mut g = Game::new(&mut Rng::new(seed));
            let mut found = 0;
            while !g.is_over() {
                let sets = g.sets();
                if sets.is_empty() {
                    assert!(g.deal_three(), "no set and cannot deal");
                    continue;
                }
                assert_eq!(g.claim(sets[0]), Claim::Set);
                found += 1;
                assert!(g.table.len() >= TABLE_MIN || g.deck.is_empty());
                assert_eq!(g.table.len() + g.deck.len() + 3 * found, 81);
            }
            assert_eq!(g.found() as usize, found);
            assert!(g.deck.is_empty());
        }
    }

    #[test]
    fn not_a_set_leaves_table() {
        let mut g = Game::new(&mut Rng::new(7));
        let before = g.table.clone();
        // Find three cards that are not a set.
        let mut tri = None;
        'o: for i in 0..12 {
            for j in (i + 1)..12 {
                for k in (j + 1)..12 {
                    if !is_set(before[i], before[j], before[k]) {
                        tri = Some([i, j, k]);
                        break 'o;
                    }
                }
            }
        }
        assert_eq!(g.claim(tri.unwrap()), Claim::NotSet);
        assert_eq!(g.table, before);
        assert_eq!(g.claim([0, 0, 1]), Claim::BadIndices);
        assert_eq!(g.claim([0, 1, 99]), Claim::BadIndices);
    }

    #[test]
    fn oversized_table_shrinks_back() {
        // Deal three extra, then claim a set: the table should return to its earlier size.
        for seed in 0..100 {
            let mut g = Game::new(&mut Rng::new(seed));
            let before = g.table.len();
            assert!(g.deal_three());
            assert_eq!(g.table.len(), before + 3);
            let sets = g.sets();
            assert_eq!(g.claim(sets[0]), Claim::Set);
            // Back to the earlier size, unless those cards held no set and three more were dealt.
            if g.table.len() != before {
                assert_eq!(g.table.len(), before + 3);
                assert!(!has_set(&g.table[..before]));
            }
        }
    }
}
