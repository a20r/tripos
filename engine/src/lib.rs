//! SET rules, deck handling and daily seeding with no dependencies.
//!
//! A card has four attributes (number, shape, colour, shading) with three
//! values each. Three cards are a set when, for every attribute, the three
//! values are all the same or all different.

pub mod card;
pub mod daily;
pub mod game;
pub mod rng;

pub use card::{find_sets, has_set, is_set, third, Card, DECK_SIZE};
pub use game::{Claim, Game, TABLE_MAX, TABLE_MIN};
pub use rng::Rng;
