//! A SET card is four attributes, each with three values. Cards are stored as
//! a single byte 0..81 in base 3: number, shape, colour, shading.

pub const DECK_SIZE: usize = 81;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Card(pub u8);

impl Card {
    pub fn new(number: u8, shape: u8, color: u8, shading: u8) -> Card {
        debug_assert!(number < 3 && shape < 3 && color < 3 && shading < 3);
        Card(number * 27 + shape * 9 + color * 3 + shading)
    }

    /// How many symbols are on the card: 0, 1 or 2 meaning one, two or three.
    pub fn number(self) -> u8 {
        self.0 / 27
    }
    /// 0 diamond, 1 pill, 2 squiggle.
    pub fn shape(self) -> u8 {
        (self.0 / 9) % 3
    }
    /// 0, 1, 2: the theme decides the hues.
    pub fn color(self) -> u8 {
        (self.0 / 3) % 3
    }
    /// 0 solid, 1 striped, 2 open.
    pub fn shading(self) -> u8 {
        self.0 % 3
    }

    pub fn attrs(self) -> [u8; 4] {
        [self.number(), self.shape(), self.color(), self.shading()]
    }

    pub fn is_valid(self) -> bool {
        (self.0 as usize) < DECK_SIZE
    }
}

/// The value that completes a line in one attribute: equal when the two agree,
/// the remaining one when they differ.
fn third_attr(a: u8, b: u8) -> u8 {
    if a == b {
        a
    } else {
        3 - a - b
    }
}

/// The unique card that forms a set with the two given.
pub fn third(a: Card, b: Card) -> Card {
    let x = a.attrs();
    let y = b.attrs();
    Card::new(
        third_attr(x[0], y[0]),
        third_attr(x[1], y[1]),
        third_attr(x[2], y[2]),
        third_attr(x[3], y[3]),
    )
}

/// Three cards are a set when every attribute is all the same or all different.
pub fn is_set(a: Card, b: Card, c: Card) -> bool {
    a != b && third(a, b) == c
}

/// Every set on a board, as sorted index triples in lexicographic order.
pub fn find_sets(board: &[Card]) -> Vec<[usize; 3]> {
    let mut out = Vec::new();
    for i in 0..board.len() {
        for j in (i + 1)..board.len() {
            let want = third(board[i], board[j]);
            for (k, &c) in board.iter().enumerate().skip(j + 1) {
                if c == want {
                    out.push([i, j, k]);
                }
            }
        }
    }
    out
}

pub fn has_set(board: &[Card]) -> bool {
    for i in 0..board.len() {
        for j in (i + 1)..board.len() {
            let want = third(board[i], board[j]);
            if board[(j + 1)..].contains(&want) {
                return true;
            }
        }
    }
    false
}

/// All 81 cards in byte order.
pub fn full_deck() -> Vec<Card> {
    (0..DECK_SIZE as u8).map(Card).collect()
}
