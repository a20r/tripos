use tripos_engine::*;

#[test]
fn attributes_round_trip() {
    for n in 0..3 {
        for s in 0..3 {
            for c in 0..3 {
                for h in 0..3 {
                    let card = Card::new(n, s, c, h);
                    assert_eq!(card.attrs(), [n, s, c, h]);
                    assert!(card.is_valid());
                }
            }
        }
    }
    assert!(!Card(81).is_valid());
}

#[test]
fn third_completes_a_set() {
    let deck = card::full_deck();
    for &a in &deck {
        for &b in &deck {
            if a == b {
                continue;
            }
            let c = third(a, b);
            assert!(is_set(a, b, c));
            assert!(is_set(b, c, a));
            assert_ne!(c, a);
            assert_ne!(c, b);
        }
    }
}

#[test]
fn full_deck_has_1080_sets() {
    let deck = card::full_deck();
    assert_eq!(find_sets(&deck).len(), 1080);
    assert!(has_set(&deck));
}

/// Set membership by the rulebook definition, independent of `third`.
fn is_set_by_rule(a: Card, b: Card, c: Card) -> bool {
    let (x, y, z) = (a.attrs(), b.attrs(), c.attrs());
    (0..4).all(|i| {
        let same = x[i] == y[i] && y[i] == z[i];
        let diff = x[i] != y[i] && y[i] != z[i] && x[i] != z[i];
        same || diff
    })
}

#[test]
fn is_set_matches_the_rulebook() {
    let deck = card::full_deck();
    for &a in &deck {
        for &b in &deck {
            for &c in deck.iter().step_by(7) {
                if a == b || b == c || a == c {
                    continue;
                }
                assert_eq!(is_set(a, b, c), is_set_by_rule(a, b, c));
            }
        }
    }
}

#[test]
fn a_greedily_built_set_free_table() {
    // Add cards in shuffled order, skipping any that would complete a set.
    let mut rng = Rng::new(9);
    let mut deck = card::full_deck();
    rng.shuffle(&mut deck);
    let mut table: Vec<Card> = Vec::new();
    for c in deck {
        let completes = table
            .iter()
            .enumerate()
            .any(|(i, &a)| table[(i + 1)..].iter().any(|&b| third(a, b) == c));
        if !completes {
            table.push(c);
        }
        if table.len() == 12 {
            break;
        }
    }
    assert_eq!(table.len(), 12);
    for i in 0..12 {
        for j in (i + 1)..12 {
            for k in (j + 1)..12 {
                assert!(!is_set_by_rule(table[i], table[j], table[k]));
            }
        }
    }
    assert!(find_sets(&table).is_empty());
    assert!(!has_set(&table));
}

#[test]
fn has_set_agrees_with_find_sets() {
    let mut rng = Rng::new(42);
    for _ in 0..2000 {
        let mut deck = card::full_deck();
        rng.shuffle(&mut deck);
        let n = 3 + rng.below(18);
        let table = &deck[..n];
        assert_eq!(has_set(table), !find_sets(table).is_empty());
    }
}

#[test]
fn shuffle_is_seeded() {
    let mut a = card::full_deck();
    let mut b = card::full_deck();
    Rng::new(1).shuffle(&mut a);
    Rng::new(1).shuffle(&mut b);
    assert_eq!(a, b);
    Rng::new(2).shuffle(&mut b);
    assert_ne!(a, b);
    let mut sorted = a.clone();
    sorted.sort();
    assert_eq!(sorted, card::full_deck());
}
