//! One game of SET: a shuffled deck, the cards on the table, and the rules for
//! claiming a set and dealing more. The table tops up to twelve cards while
//! the deck lasts; three more are dealt only on request, so spotting that no
//! set is out stays part of the game.

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

    /// Deals up to the minimum table size.
    fn fill(&mut self) {
        while self.table.len() < TABLE_MIN && !self.deck.is_empty() {
            self.table.push(self.deck.pop().unwrap());
        }
    }

    /// Rearranges the cards on the table without changing which cards are there.
    pub fn shuffle_table(&mut self, rng: &mut Rng) {
        rng.shuffle(&mut self.table);
    }

    /// Deals three more cards on request. Returns false when the deck is short
    /// or the table is full. Callers decide whether the request was justified
    /// (see `has_set`), since the penalty is a scoring matter.
    pub fn deal_three(&mut self) -> bool {
        if self.deck.len() < 3 || self.table.len() + 3 > TABLE_MAX {
            return false;
        }
        for _ in 0..3 {
            self.table.push(self.deck.pop().unwrap());
        }
        true
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
    fn opening_table_is_twelve() {
        for seed in 0..200 {
            let g = Game::new(&mut Rng::new(seed));
            assert_eq!(g.table.len(), TABLE_MIN);
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
    fn claims_never_grow_the_table() {
        for seed in 0..100 {
            let mut g = Game::new(&mut Rng::new(seed));
            while !g.has_set() {
                assert!(g.deal_three());
            }
            let before = g.table.len();
            let sets = g.sets();
            assert_eq!(g.claim(sets[0]), Claim::Set);
            // Replaced in place at twelve, removed above it.
            let want = if before > TABLE_MIN {
                before - 3
            } else {
                TABLE_MIN
            };
            assert_eq!(g.table.len(), want);
        }
    }

    #[test]
    fn shuffle_keeps_the_same_cards() {
        let mut g = Game::new(&mut Rng::new(11));
        let mut before = g.table.clone();
        let deck = g.deck.clone();
        g.shuffle_table(&mut Rng::new(5));
        assert_ne!(g.table, before, "order should change");
        let mut after = g.table.clone();
        before.sort();
        after.sort();
        assert_eq!(before, after);
        assert_eq!(g.deck, deck);
    }

    #[test]
    fn oversized_table_shrinks_back() {
        let mut g = Game::new(&mut Rng::new(3));
        assert!(g.deal_three());
        assert!(g.deal_three());
        assert_eq!(g.table.len(), 18);
        let sets = g.sets();
        assert!(!sets.is_empty());
        assert_eq!(g.claim(sets[0]), Claim::Set);
        assert_eq!(g.table.len(), 15);
    }
}
