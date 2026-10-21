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
            // An S: a thick curved stroke whose halves bow opposite ways, so it never reads as the oval.
            r#"<path d="M13.4,-25.9 C14.0,-24.5 14.5,-22.8 15.0,-20.9 C15.4,-19.1 16.0,-17.1 16.1,-15.1 C16.1,-13.0 16.0,-10.6 15.5,-8.5 C15.0,-6.4 14.0,-4.3 12.9,-2.5 C11.9,-0.7 10.4,0.8 9.2,2.2 C7.9,3.6 6.6,4.8 5.4,5.9 C4.3,7.0 3.2,7.9 2.4,8.8 C1.6,9.6 1.0,10.4 0.6,11.0 C0.2,11.6 0.1,11.9 0.0,12.4 C-0.1,12.9 -0.2,13.2 -0.1,13.9 C-0.0,14.6 0.1,15.5 0.4,16.7 C0.8,17.8 1.5,19.3 1.7,20.7 C2.0,22.0 2.3,23.5 2.1,24.8 C1.8,26.1 1.1,27.5 0.3,28.5 C-0.6,29.5 -1.9,30.4 -3.2,30.8 C-4.4,31.3 -6.0,31.4 -7.3,31.1 C-8.6,30.9 -10.0,30.2 -11.0,29.3 C-12.0,28.5 -12.7,27.3 -13.4,25.9 C-14.0,24.5 -14.5,22.8 -15.0,20.9 C-15.4,19.1 -16.0,17.1 -16.1,15.1 C-16.1,13.0 -16.0,10.6 -15.5,8.5 C-15.0,6.4 -14.0,4.3 -12.9,2.5 C-11.9,0.7 -10.4,-0.8 -9.2,-2.2 C-7.9,-3.6 -6.6,-4.8 -5.4,-5.9 C-4.3,-7.0 -3.2,-7.9 -2.4,-8.8 C-1.6,-9.6 -1.0,-10.4 -0.6,-11.0 C-0.2,-11.6 -0.1,-11.9 -0.0,-12.4 C0.1,-12.9 0.2,-13.2 0.1,-13.9 C0.0,-14.6 -0.1,-15.5 -0.4,-16.7 C-0.8,-17.8 -1.5,-19.3 -1.7,-20.7 C-2.0,-22.0 -2.3,-23.5 -2.1,-24.8 C-1.8,-26.1 -1.1,-27.5 -0.3,-28.5 C0.6,-29.5 1.9,-30.4 3.2,-30.8 C4.4,-31.3 6.0,-31.4 7.3,-31.1 C8.6,-30.9 10.0,-30.2 11.0,-29.3 C12.0,-28.5 12.7,-27.3 13.4,-25.9 Z"/>"#
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
