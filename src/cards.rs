//! Card faces as inline SVG. Each card is a 160 by 100 view box holding one,
//! two or three symbols side by side. Colours and stripe patterns come from the
//! stylesheet through the c0, c1 and c2 classes so themes can restyle them.

use tripos_engine::Card;

const SHAPE_NAMES: [&str; 3] = ["diamond", "pill", "squiggle"];
const SHADING_NAMES: [&str; 3] = ["solid", "striped", "open"];
const NUMBER_NAMES: [&str; 3] = ["one", "two", "three"];

/// A symbol centred on the origin, 34 wide and 64 tall.
fn symbol(shape: u8) -> &'static str {
    match shape {
        0 => r#"<polygon points="0,-32 17,0 0,32 -17,0"/>"#,
        1 => r#"<rect x="-17" y="-32" width="34" height="64" rx="17"/>"#,
        _ => {
            r#"<path d="M-13,-21 C-13,-30 -3,-35 6,-30 C14,-25 17,-16 13,-7 C10,1 15,9 14,19 C13,29 5,35 -4,31 C-12,27 -16,17 -12,7 C-9,-1 -13,-11 -13,-21 Z"/>"#
        }
    }
}

/// Human-readable name for screen readers, e.g. "two striped pills".
pub fn describe(card: Card, colors: [&str; 3]) -> String {
    let n = card.number() as usize;
    let plural = if n == 0 { "" } else { "s" };
    format!(
        "{} {} {} {}{}",
        NUMBER_NAMES[n],
        SHADING_NAMES[card.shading() as usize],
        colors[card.color() as usize],
        SHAPE_NAMES[card.shape() as usize],
        plural
    )
}

/// The SVG markup for one card face.
pub fn face(card: Card) -> String {
    let count = card.number() as usize + 1;
    let color = card.color();
    let shading = card.shading();
    let fill = match shading {
        0 => format!("fill-{color}"),
        1 => format!("stripe-{color}"),
        _ => "fill-none".to_string(),
    };
    let mut s = String::with_capacity(600);
    s.push_str(r#"<svg class="face" viewBox="0 0 160 100" preserveAspectRatio="xMidYMid meet" aria-hidden="true" focusable="false">"#);
    s.push_str(r#"<rect class="frame" x="2" y="2" width="156" height="96" rx="12"/>"#);
    s.push_str(&format!(r#"<g class="sym c{color} {fill}">"#));
    let step = 42.0;
    let sym = symbol(card.shape());
    for i in 0..count {
        let x = 80.0 + (i as f64 - (count as f64 - 1.0) / 2.0) * step;
        s.push_str(&format!(r#"<g transform="translate({x:.1},50)">{sym}</g>"#));
    }
    s.push_str("</g></svg>");
    s
}
