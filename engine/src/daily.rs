//! The daily game: one deck order per calendar day, the same for everyone.

use crate::game::Game;
use crate::rng::Rng;

/// A YYYY-MM-DD key from calendar parts.
pub fn date_key(year: i32, month: u32, day: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since 1970-01-01 for a YYYY-MM-DD key, for streak arithmetic.
/// Malformed keys give i64::MIN so they never neighbour a real day.
pub fn day_number(key: &str) -> i64 {
    let mut parts = key.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return i64::MIN;
    };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i64>(), m.parse::<i64>(), d.parse::<i64>()) else {
        return i64::MIN;
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return i64::MIN;
    }
    // Days from civil, Howard Hinnant's algorithm.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn rng_for(key: &str) -> Rng {
    Rng::from_seed_str(&format!("tripos.daily.{key}"))
}

pub fn daily(key: &str) -> Game {
    Game::new(&mut rng_for(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers() {
        assert_eq!(day_number("1970-01-01"), 0);
        assert_eq!(day_number("1970-01-02"), 1);
        assert_eq!(day_number("2000-03-01"), 11017);
        assert_eq!(day_number("2026-10-21") - day_number("2026-10-20"), 1);
        assert_eq!(day_number("2026-03-01") - day_number("2026-02-28"), 1);
        assert_eq!(day_number("nonsense"), i64::MIN);
    }

    #[test]
    fn daily_is_deterministic_and_differs_by_day() {
        let a = daily("2026-10-21");
        let b = daily("2026-10-21");
        let c = daily("2026-10-22");
        assert_eq!(a.table(), b.table());
        assert_eq!(a.deck(), b.deck());
        assert_ne!(a.deck(), c.deck());
    }
}
