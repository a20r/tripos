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
            // A fat S: the pill bent along an S curve, so it stays smooth and chunky.
            r#"<path d="M13.2,20.0 C14.1,19.0 14.8,18.1 15.5,17.1 C16.1,16.2 16.8,15.2 17.2,14.3 C17.6,13.3 17.9,12.4 18.0,11.4 C18.1,10.5 18.0,9.5 17.7,8.6 C17.5,7.6 17.0,6.7 16.5,5.7 C15.9,4.8 15.2,3.8 14.5,2.9 C13.7,1.9 12.8,1.0 12.0,0.0 C11.2,-1.0 10.3,-1.9 9.5,-2.9 C8.8,-3.8 8.1,-4.8 7.5,-5.7 C7.0,-6.7 6.5,-7.6 6.3,-8.6 C6.0,-9.5 5.9,-10.5 6.0,-11.4 C6.1,-12.4 6.4,-13.3 6.8,-14.3 C7.2,-15.2 7.9,-16.2 8.5,-17.1 C9.2,-18.1 10.1,-19.0 10.8,-20.0 C11.6,-21.0 12.6,-22.1 13.1,-23.1 C13.7,-24.1 14.1,-25.1 14.2,-26.0 C14.3,-26.9 14.1,-27.8 13.7,-28.5 C13.3,-29.2 12.6,-29.9 11.8,-30.4 C11.1,-30.9 10.1,-31.3 9.1,-31.6 C8.1,-31.9 7.0,-32.0 6.0,-32.0 C5.0,-32.0 3.9,-31.9 2.9,-31.6 C1.9,-31.3 0.9,-30.9 -0.2,-30.4 C-1.2,-29.9 -2.2,-29.2 -3.3,-28.5 C-4.3,-27.8 -5.5,-26.9 -6.6,-26.0 C-7.7,-25.1 -8.9,-24.1 -10.0,-23.1 C-11.1,-22.1 -12.3,-21.0 -13.2,-20.0 C-14.1,-19.0 -14.8,-18.1 -15.5,-17.1 C-16.1,-16.2 -16.8,-15.2 -17.2,-14.3 C-17.6,-13.3 -17.9,-12.4 -18.0,-11.4 C-18.1,-10.5 -18.0,-9.5 -17.7,-8.6 C-17.5,-7.6 -17.0,-6.7 -16.5,-5.7 C-15.9,-4.8 -15.2,-3.8 -14.5,-2.9 C-13.7,-1.9 -12.8,-1.0 -12.0,0.0 C-11.2,1.0 -10.3,1.9 -9.5,2.9 C-8.8,3.8 -8.1,4.8 -7.5,5.7 C-7.0,6.7 -6.5,7.6 -6.3,8.6 C-6.0,9.5 -5.9,10.5 -6.0,11.4 C-6.1,12.4 -6.4,13.3 -6.8,14.3 C-7.2,15.2 -7.9,16.2 -8.5,17.1 C-9.2,18.1 -10.1,19.0 -10.8,20.0 C-11.6,21.0 -12.6,22.1 -13.1,23.1 C-13.7,24.1 -14.1,25.1 -14.2,26.0 C-14.3,26.9 -14.1,27.8 -13.7,28.5 C-13.3,29.2 -12.6,29.9 -11.8,30.4 C-11.1,30.9 -10.1,31.3 -9.1,31.6 C-8.1,31.9 -7.0,32.0 -6.0,32.0 C-5.0,32.0 -3.9,31.9 -2.9,31.6 C-1.9,31.3 -0.9,30.9 0.2,30.4 C1.2,29.9 2.2,29.2 3.3,28.5 C4.3,27.8 5.5,26.9 6.6,26.0 C7.7,25.1 8.9,24.1 10.0,23.1 C11.1,22.1 12.3,21.0 13.2,20.0 Z"/>"#
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
